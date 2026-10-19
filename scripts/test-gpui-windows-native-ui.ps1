# Dot-source only after the parent has verified the disposable guest and app.
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
    $marker.kind -ne 'gpui-expendable-squirrel-acceptance-v1') { throw 'Disposable guest required for native UI actions' }

function Wait-GuestCondition([scriptblock]$Test, [string]$Message) {
  $deadline = [DateTime]::UtcNow.AddSeconds(30)
  while (-not (& $Test)) {
    if ([DateTime]::UtcNow -gt $deadline) {
      $focused = [Windows.Automation.AutomationElement]::FocusedElement
      if ($null -ne $focused) {
        @{ name=$focused.Current.Name; type=$focused.Current.ControlType.ProgrammaticName;
           processId=$focused.Current.ProcessId; message=$Message } |
          ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'native-focus-failure.json') -Encoding UTF8
      }
      throw $Message
    }
    Start-Sleep -Milliseconds 100
  }
}
function Guest-UiElement([IntPtr]$Window, [string]$Name) {
  $root = [Windows.Automation.AutomationElement]::FromHandle($Window)
  # Background editors can expose the same Save label as a modal. Resolve
  # actions inside the active dialog, matching the application's focus guard.
  $dialogCondition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),'Launcher dialog'
  $dialog = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$dialogCondition)
  if ($null -ne $dialog) { $root = $dialog }
  $condition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),$Name
  $element = $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
  return $element
}
function Wait-GuestUiElement([IntPtr]$Window, [string]$Name) {
  try {
    Wait-GuestCondition { $null -ne (Guest-UiElement $Window $Name) } ('Native UI control missing: ' + $Name)
  } catch {
    [Windows.Automation.AutomationElement]::FromHandle($Window).FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) |
      ForEach-Object { @{ name=$_.Current.Name; type=$_.Current.ControlType.ProgrammaticName; id=$_.Current.AutomationId } } |
      ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-ui-last-tree.json') -Encoding UTF8
    throw
  }
  return (Guest-UiElement $Window $Name)
}
function Guest-UiAction([IntPtr]$Window, [string]$Name, [switch]$Keyboard, [switch]$Space) {
  $element = Wait-GuestUiElement $Window $Name
  [uint32]$owner = 0
  [GpuiGuestUi]::GetWindowThreadProcessId($Window,[ref]$owner) | Out-Null
  if ($owner -ne $guestUiProcessId -or [GpuiGuestUi]::GetForegroundWindow() -ne $Window) { throw 'Owned foreground guest window required for UI action' }
  if ($Keyboard -or $Space) {
    $element.SetFocus()
    Wait-GuestCondition { [Windows.Automation.Automation]::Compare([Windows.Automation.AutomationElement]::FocusedElement,$element) } ('Native keyboard focus did not reach: ' + $Name)
    [Windows.Forms.SendKeys]::SendWait($(if ($Space) { ' ' } else { '{ENTER}' }))
  } else {
    $pattern = $null
    if ($element.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern,[ref]$pattern)) {
      $pattern.Invoke()
    } elseif ($element.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern,[ref]$pattern)) {
      $pattern.Select()
    } else { throw ('Control has no native activation pattern: ' + $Name) }
  }
  Start-Sleep -Milliseconds 200
}
function Guest-Keys([IntPtr]$Window, [string]$Keys) {
  [uint32]$owner = 0
  [GpuiGuestUi]::GetWindowThreadProcessId($Window,[ref]$owner) | Out-Null
  if ($owner -ne $guestUiProcessId -or [GpuiGuestUi]::GetForegroundWindow() -ne $Window) { throw 'Owned foreground guest window required for keyboard input' }
  [Windows.Forms.SendKeys]::SendWait($Keys)
}
function Guest-Buttons([IntPtr]$Window) {
  [Windows.Automation.AutomationElement]::FromHandle($Window).FindAll(
    [Windows.Automation.TreeScope]::Descendants,
    (New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty),([Windows.Automation.ControlType]::Button)))
}
function Guest-Picker([string]$Value, [switch]$Cancel) {
  $dialog = [IntPtr]::Zero
  Wait-GuestCondition {
    $items = @([GpuiGuestUi]::Windows($guestUiProcessId,'#32770'))
    if ($items.Count -eq 1) { $script:guestPickerWindow = $items[0]; return $true }
    return $false
  } 'Owned native file picker did not appear'
  $dialog = $script:guestPickerWindow
  if ([GpuiGuestUi]::GetForegroundWindow() -ne $dialog) { throw 'Owned foreground native picker required' }
  $root = [Windows.Automation.AutomationElement]::FromHandle($dialog)
  $root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) |
    ForEach-Object { @{ name=$_.Current.Name; id=$_.Current.AutomationId; type=$_.Current.ControlType.ProgrammaticName } } |
    ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-picker-tree.json') -Encoding UTF8
  if (-not $Cancel) {
    # The native common dialog exposes these HWND controls as UIA Panes in
    # this VM, without Value/Invoke patterns. Address its owned controls by ID.
    $field = [GpuiGuestUi]::Control($dialog,$guestUiProcessId,1148,'Edit')
    [GpuiGuestUi]::SetText($field,$Value)
  }
  $button = [GpuiGuestUi]::Control($dialog,$guestUiProcessId,$(if ($Cancel) { 2 } else { 1 }))
  [GpuiGuestUi]::ClickButton($button)
  Wait-GuestCondition { @([GpuiGuestUi]::Windows($guestUiProcessId,'#32770')).Count -eq 0 } 'Native picker did not close'
}
function Guest-Folder([IntPtr]$Window, [string]$Control, [string]$Expected) {
  Guest-UiAction $Window $Control
  $shell = New-Object -ComObject Shell.Application
  try {
    Wait-GuestCondition {
      foreach ($entry in $shell.Windows()) {
        try {
          if ([IO.Path]::GetFullPath(([Uri]$entry.LocationURL).LocalPath) -eq [IO.Path]::GetFullPath($Expected)) {
            $entry.Quit()
            return $true
          }
        } catch { }
      }
      return $false
    } ('OS folder action did not open expected directory: ' + $Control)
  } finally { [Runtime.InteropServices.Marshal]::ReleaseComObject($shell) | Out-Null }
  [GpuiGuestUi]::SetForegroundWindow($Window) | Out-Null
  Wait-GuestCondition { [GpuiGuestUi]::GetForegroundWindow() -eq $Window } 'Owned GPUI window did not return to foreground'
}
function Invoke-GuestNativeUi($Process, [IntPtr]$Window, [string]$GuestDataDirectory, $Report) {
  if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
      $marker.kind -ne 'gpui-expendable-squirrel-acceptance-v1' -or $Process.Path -ne $installed) { throw 'Verified owned guest app required' }
  $guestUiProcessId = [uint32]$Process.Id
  Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Windows.Forms
  Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class GpuiGuestUi {
  delegate bool Callback(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumWindows(Callback callback, IntPtr parameter);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, Callback callback, IntPtr parameter);
  [DllImport("user32.dll")] static extern int GetDlgCtrlID(IntPtr window);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr SendMessageTimeout(IntPtr window, uint message, IntPtr wParam, string lParam, uint flags, uint timeout, out IntPtr result);
  public static IntPtr Control(IntPtr parent, uint pid, int id) {
    return Control(parent,pid,id,null);
  }
  public static IntPtr Control(IntPtr parent, uint pid, int id, string kind) {
    uint owner; GetWindowThreadProcessId(parent,out owner);
    if (owner != pid) throw new InvalidOperationException("Owned native dialog required");
    var matches = new List<IntPtr>();
    var classes = new List<string>();
    EnumChildWindows(parent,(window, parameter) => {
      GetWindowThreadProcessId(window,out owner);
      if (owner == pid && IsWindowVisible(window) && GetDlgCtrlID(window) == id) {
        var name = new StringBuilder(256); GetClassName(window,name,name.Capacity);
        classes.Add(name.ToString());
        if (kind == null || name.ToString() == kind) matches.Add(window);
      }
      return true;
    },IntPtr.Zero);
    if (matches.Count != 1) throw new InvalidOperationException("Native dialog control is missing or ambiguous: " + id + " " + kind + "; found " + String.Join(", ",classes));
    return matches[0];
  }
  public static void SetText(IntPtr control, string text) {
    IntPtr result;
    if (SendMessageTimeout(control,0x000C,IntPtr.Zero,text,2,5000,out result) == IntPtr.Zero || result == IntPtr.Zero)
      throw new InvalidOperationException("Native filename control rejected text");
  }
  public static void ClickButton(IntPtr control) {
    var name = new StringBuilder(256); GetClassName(control,name,name.Capacity);
    if (name.ToString() != "Button") throw new InvalidOperationException("Expected native dialog Button, got " + name);
    if (!PostMessage(control,0x00F5,IntPtr.Zero,IntPtr.Zero)) throw new InvalidOperationException("Native dialog button activation failed");
  }
  public static IntPtr[] Windows(uint pid, string kind) {
    var result = new List<IntPtr>();
    EnumWindows((window, parameter) => {
      uint owner; GetWindowThreadProcessId(window,out owner);
      var name = new StringBuilder(256); GetClassName(window,name,name.Capacity);
      if (owner == pid && IsWindowVisible(window) && name.ToString() == kind) result.Add(window);
      return true;
    },IntPtr.Zero);
    return result.ToArray();
  }
}
'@
  Progress 'Inspecting the installed GPUI accessibility tree and native keyboard activation'
  $root = [Windows.Automation.AutomationElement]::FromHandle($Window)
  $tree = @($root.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) |
    ForEach-Object { @{ name=$_.Current.Name; type=$_.Current.ControlType.ProgrammaticName; focusable=$_.Current.IsKeyboardFocusable; enabled=$_.Current.IsEnabled } })
  $tree | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-ui-tree.json') -Encoding UTF8
  if ($tree.Count -lt 10) { throw 'Installed GPUI did not expose its native control tree' }
  $Report.nativeAccessibleControls = $tree.Count
  $tile = Wait-GuestUiElement $Window 'GPUI Sandbox fixture'
  $tilePattern = $null
  $Report.nativeTileSelectionPattern = $tile.TryGetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern,[ref]$tilePattern)
  if ($Report.nativeTileSelectionPattern) { Guest-UiAction $Window 'GPUI Sandbox fixture' }
  Guest-UiAction $Window 'GPUI Sandbox fixture' -Space
  Guest-UiAction $Window 'Mods' -Keyboard
  Wait-GuestUiElement $Window 'Add mods' | Out-Null
  $Report.nativeKeyboardTabActivation = $true
  $instance = Join-Path $GuestDataDirectory 'instances/FixtureOne'
  $overlays = Join-Path $instance 'persistent-minecraft/mods'
  $active = Join-Path $instance '.minecraft/mods'
  Guest-UiAction $Window 'Add mods'
  Guest-Picker -Cancel
  if ((Test-Path -LiteralPath $overlays) -and @(Get-ChildItem -LiteralPath $overlays -File).Count -ne 0) { throw 'Picker cancellation imported a mod' }
  $one = Join-Path $outputRoot 'acceptance-one.jar'
  $two = Join-Path $outputRoot 'acceptance-two.zip'
  [IO.File]::WriteAllText($one,'Generated native picker JAR fixture')
  [IO.File]::WriteAllText($two,'Generated native picker ZIP fixture')
  Guest-UiAction $Window 'Add mods'
  Guest-Picker ('"' + $one + '" "' + $two + '"')
  Wait-GuestCondition { (Test-Path -LiteralPath (Join-Path $overlays 'acceptance-one.jar')) -and (Test-Path -LiteralPath (Join-Path $overlays 'acceptance-two.zip')) } 'Native multi-file import did not persist both mods'
  foreach ($source in @($one,$two)) {
    $name = [IO.Path]::GetFileName($source)
    foreach ($directory in @($overlays,$active)) {
      if ((Get-FileHash -LiteralPath (Join-Path $directory $name)).Hash -ne (Get-FileHash -LiteralPath $source).Hash) { throw 'Native mod import changed bytes' }
    }
  }
  Guest-Folder $Window 'Open mods folder' $overlays
  Guest-UiAction $Window 'Remove'
  Guest-UiAction $Window 'Remove mod' -Keyboard
  Wait-GuestCondition { -not (Test-Path -LiteralPath (Join-Path $overlays 'acceptance-one.jar')) } 'Confirmed native removal did not persist'
  if ([IO.File]::ReadAllText((Join-Path $active 'pack-owned.jar')) -ne 'Generated pack mod bytes' -or
      -not (Test-Path -LiteralPath (Join-Path $overlays 'acceptance-two.zip'))) { throw 'Native removal modified an unrelated mod' }
  $Report.nativeModPickerCancelMultiImportRemoveFolder = $true
  Progress 'Exercising native icon import, selection, reset and library folder dispatch'
  $beforeMenu = @(Guest-Buttons $Window | ForEach-Object { $_.Current.Name })
  Guest-UiAction $Window 'Instance actions: GPUI Sandbox fixture'
  $ellipsis = [char]0x2026
  $menuLabels = @('Open folder','Settings','Launch / Stop',('Copy instance' + $ellipsis),
    ('Update pack' + $ellipsis),('Clean reinstall' + $ellipsis),'Delete instance',
    'Change icon...','Change group...','Move earlier','Move later')
  try {
    Wait-GuestCondition {
      $buttons = @(Guest-Buttons $Window)
      foreach ($name in $menuLabels) {
        # The detail footer also contains Delete instance. Require one newly
        # exposed button per menu action rather than a globally unique name.
        $previous = @($beforeMenu | Where-Object { $_ -eq $name }).Count
        if (@($buttons | Where-Object { $_.Current.Name -eq $name }).Count -ne ($previous + 1)) { return $false }
      }
      return $true
    } 'Native context menu did not expose every distinct action'
  } finally {
    @(Guest-Buttons $Window | ForEach-Object { @{ name=$_.Current.Name; id=$_.Current.AutomationId } }) |
      ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-menu-tree.json') -Encoding UTF8
  }
  $Report.nativeInstanceMenuActions = $menuLabels.Count
  Guest-UiAction $Window 'Change icon...'
  Guest-UiAction $Window 'Import image...'
  Guest-Picker -Cancel
  $iconLibrary = Join-Path $GuestDataDirectory 'icons'
  if (@(Get-ChildItem -LiteralPath $iconLibrary -Filter 'custom-*' -File).Count -ne 0) { throw 'Cancelled icon picker imported an image' }
  Guest-UiAction $Window 'Import image...'
  Guest-Picker 'C:\GpuiAcceptanceInput\acceptance.png'
  $importedIcon = Join-Path $iconLibrary 'custom-acceptance.png'
  Wait-GuestCondition { Test-Path -LiteralPath $importedIcon } 'Native icon import did not populate the library'
  if ((Get-FileHash -LiteralPath $importedIcon).Hash -ne (Get-FileHash -LiteralPath 'C:\GpuiAcceptanceInput\acceptance.png').Hash) { throw 'Native icon import changed bytes' }
  Guest-Folder $Window 'Icons folder' $iconLibrary
  Guest-UiAction $Window 'acceptance' -Keyboard
  $instanceSettings = Join-Path $instance 'instance.json'
  Wait-GuestCondition {
    $saved = Get-Content -LiteralPath $instanceSettings -Raw | ConvertFrom-Json
    return $saved.custom_icon -and (Get-FileHash -LiteralPath (Join-Path $instance $saved.custom_icon)).Hash -eq (Get-FileHash -LiteralPath $importedIcon).Hash
  } 'Native icon selection did not persist the selected image'
  Guest-UiAction $Window 'Instance actions: GPUI Sandbox fixture'
  Guest-UiAction $Window 'Change icon...'
  Guest-UiAction $Window 'Reset icon' -Keyboard
  Wait-GuestCondition { $null -eq (Get-Content -LiteralPath $instanceSettings -Raw | ConvertFrom-Json).custom_icon } 'Native reset did not clear the selected custom icon'
  if (@(Get-ChildItem -LiteralPath $instance -Filter 'instance-icon*' -File).Count -ne 0) { throw 'Reset left an old generated instance icon' }
  if (-not (Test-Path -LiteralPath $importedIcon)) { throw 'Reset deleted the reusable library image' }
  $Report.nativeIconPickerCancelImportSelectResetFolder = $true
  Guest-UiAction $Window 'Accounts' -Keyboard
  Wait-GuestUiElement $Window 'Set GpuiAcceptance as default account' | Out-Null
  $accountTree = @([Windows.Automation.AutomationElement]::FromHandle($Window).FindAll(
    [Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) |
    ForEach-Object { @{ name=$_.Current.Name; type=$_.Current.ControlType.ProgrammaticName; focusable=$_.Current.IsKeyboardFocusable } })
  $accountTree | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-account-tree.json') -Encoding UTF8
  $Report.nativeAccountHasNamedInput = @($accountTree | Where-Object { $_.type -eq 'ControlType.Edit' -and $_.name }).Count -gt 0
  Guest-UiAction $Window 'Instances' -Keyboard
  Progress 'Exercising OS default-editor dispatch and atomic external-save mirroring'
  $association = 'HKCU:\Software\Classes\GpuiAcceptance.Config\shell\open\command'
  New-Item -Path 'HKCU:\Software\Classes\.cfg' -Force | Out-Null
  Set-Item -LiteralPath 'HKCU:\Software\Classes\.cfg' -Value 'GpuiAcceptance.Config'
  New-Item -Path $association -Force | Out-Null
  Set-Item -LiteralPath $association -Value ('"' + (Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe') + '" -NoProfile -STA -WindowStyle Hidden -ExecutionPolicy Bypass -File "C:\GpuiAcceptanceInput\editor.ps1" "%1"')
  Guest-UiAction $Window 'Files' -Keyboard
  Guest-UiAction $Window '> config'
  Guest-UiAction $Window ' acceptance.cfg'
  Guest-UiAction $Window 'External editor'
  $gameFile = Join-Path $instance '.minecraft/config/acceptance.cfg'
  $overlay = Join-Path $instance 'persistent-minecraft/config/acceptance.cfg'
  try {
    Wait-GuestCondition {
      if (Test-Path -LiteralPath (Join-Path $outputRoot 'editor-error.json')) {
        $errorReceipt = Get-Content -LiteralPath (Join-Path $outputRoot 'editor-error.json') -Raw | ConvertFrom-Json
        if ($errorReceipt.nonce -ne $marker.nonce) { throw 'Guest editor error receipt identity failed' }
        throw ('Controlled editor failed: ' + $errorReceipt.error)
      }
      Test-Path -LiteralPath (Join-Path $outputRoot 'editor-result.json')
    } 'OS default editor was not dispatched'
  } catch {
    $foreground = [GpuiGuestUi]::GetForegroundWindow()
    [uint32]$foregroundOwner = 0
    [GpuiGuestUi]::GetWindowThreadProcessId($foreground,[ref]$foregroundOwner) | Out-Null
    $foregroundProcess = Get-Process -Id $foregroundOwner -ErrorAction SilentlyContinue
    @{ processName=$foregroundProcess.ProcessName; processPath=$foregroundProcess.Path; window=$foreground.ToString();
       association=(Get-Item -LiteralPath $association).GetValue('') } |
      ConvertTo-Json | Set-Content -LiteralPath (Join-Path $outputRoot 'editor-dispatch-diagnostic.json') -Encoding UTF8
    [Windows.Automation.AutomationElement]::FromHandle($Window).FindAll(
      [Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) |
      ForEach-Object { @{ name=$_.Current.Name; type=$_.Current.ControlType.ProgrammaticName } } |
      ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $outputRoot 'native-editor-tree.json') -Encoding UTF8
    throw
  }
  $editorReceipt = Get-Content -LiteralPath (Join-Path $outputRoot 'editor-result.json') -Raw | ConvertFrom-Json
  if ($editorReceipt.nonce -ne $marker.nonce -or $editorReceipt.passed -ne $true -or $editorReceipt.nativeWindow -eq '0') { throw 'Native editor receipt identity failed' }
  Wait-GuestCondition { (Test-Path -LiteralPath $overlay) -and (Get-FileHash -LiteralPath $overlay).Hash -eq $editorReceipt.fileSha256 } 'Atomic native editor save was not mirrored'
  $Report.nativeDefaultEditorAtomicSaveMirrored = $true
  [GpuiGuestUi]::SetForegroundWindow($Window) | Out-Null
  Wait-GuestCondition { [GpuiGuestUi]::GetForegroundWindow() -eq $Window } 'Owned GPUI did not regain foreground'
  Guest-UiAction $Window 'Reload / discard draft'
  Guest-UiAction $Window 'Pop out'
  Wait-GuestCondition { @([GpuiGuestUi]::Windows($guestUiProcessId,'Zed::Window')).Count -eq 2 } 'Native separate editor window did not appear'
  $editorWindow = @([GpuiGuestUi]::Windows($guestUiProcessId,'Zed::Window') | Where-Object { $_ -ne $Window })[0]
  [GpuiGuestUi]::SetForegroundWindow($Window) | Out-Null
  Wait-GuestCondition { [GpuiGuestUi]::GetForegroundWindow() -eq $Window } 'Owned launcher did not regain foreground after editor popout'
  Guest-UiAction $Window '> config'
  Guest-UiAction $Window ' acceptance.cfg  (persistent)'
  Wait-GuestCondition { [GpuiGuestUi]::GetForegroundWindow() -eq $editorWindow } 'Repeated file open did not foreground its existing editor'
  if (@([GpuiGuestUi]::Windows($guestUiProcessId,'Zed::Window')).Count -ne 2) { throw 'Repeated editor open created a duplicate' }
  [GpuiGuestUi]::PostMessage($editorWindow,0x0010,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
  Wait-GuestCondition { @([GpuiGuestUi]::Windows($guestUiProcessId,'Zed::Window')).Count -eq 1 } 'Owned editor did not close normally'
  if ($Process.HasExited -or (Get-FileHash -LiteralPath $overlay).Hash -ne $editorReceipt.fileSha256) { throw 'Native editor close lost its saved override or launcher' }
  $Report.nativeEditorRepeatFocusAndIndependentClose = $true
  [GpuiGuestUi]::SetForegroundWindow($Window) | Out-Null
  Wait-GuestCondition { [GpuiGuestUi]::GetForegroundWindow() -eq $Window } 'Owned launcher did not regain foreground after editor close'
  Guest-UiAction $Window ' acceptance.cfg  (persistent)'
  Guest-UiAction $Window 'Remove override'
  Wait-GuestCondition { -not (Test-Path -LiteralPath $overlay) } 'Native override removal failed'
  [IO.File]::WriteAllText($gameFile,'Generated edit after intentional override removal')
  Start-Sleep -Seconds 2
  if (Test-Path -LiteralPath $overlay) { throw 'External watcher recreated an intentionally removed override' }
  $Report.nativeRemovedOverrideNotRecreated = $true
  Progress 'Exercising native account, preferences, theme and modal keyboard navigation'
  $settingsPath = Join-Path $GuestDataDirectory 'launcher-settings.json'
  Guest-UiAction $Window 'Accounts' -Keyboard
  $username = Wait-GuestUiElement $Window 'Offline account username'
  $usernameValue = $null
  if (-not $username.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern,[ref]$usernameValue)) { throw 'Account field is missing native editable value semantics' }
  $create = Wait-GuestUiElement $Window 'Create'
  $Report.nativeDisabledCreateReportedEnabled = $create.Current.IsEnabled
  $accountsPath = Join-Path $GuestDataDirectory 'accounts.json'
  $accountsBefore = (Get-FileHash -LiteralPath $accountsPath).Hash
  Guest-UiAction $Window 'Create' -Keyboard
  if ((Get-FileHash -LiteralPath $accountsPath).Hash -ne $accountsBefore) { throw 'Blank disabled creation changed the generated accounts' }
  $Report.nativeDisabledCreateGuard = $true
  $usernameValue.SetValue('CreatedByKeys')
  Wait-GuestCondition { (Guest-UiElement $Window 'Offline account username').GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -eq 'CreatedByKeys' } 'Accessible value replacement did not update the account field'
  Guest-UiAction $Window 'Create' -Keyboard
  Wait-GuestUiElement $Window 'Remove account CreatedByKeys' | Out-Null
  Guest-UiAction $Window 'Remove account CreatedByKeys' -Keyboard
  Wait-GuestCondition { $null -eq (Guest-UiElement $Window 'Remove account CreatedByKeys') } 'Keyboard did not remove the generated test account'
  Wait-GuestUiElement $Window 'Set GpuiAcceptance as default account' | Out-Null
  Guest-UiAction $Window 'Set GpuiKeyboard as default account' -Keyboard
  Wait-GuestCondition { (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).default_account_id -eq '20000000-0000-4000-8000-000000000003' } 'Keyboard did not persist the second account selection'
  Guest-UiAction $Window 'Set GpuiAcceptance as default account' -Space
  Wait-GuestCondition { (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).default_account_id -eq '20000000-0000-4000-8000-000000000001' } 'Space did not restore the original generated default account'
  Wait-GuestUiElement $Window 'Default account: GpuiAcceptance' | Out-Null
  Guest-UiAction $Window 'Default account: GpuiAcceptance' -Keyboard
  Guest-UiAction $Window ('Manage accounts' + [char]0x2026) -Space
  Wait-GuestUiElement $Window 'Offline account username' | Out-Null
  $originalPreset = (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).theme_preset
  if (-not $originalPreset) { $originalPreset = 'industrialis' }
  Guest-UiAction $Window 'Launcher preferences' -Space
  Guest-UiAction $Window 'Java' -Keyboard
  Wait-GuestUiElement $Window 'Refresh Java installations' | Out-Null
  Guest-UiAction $Window 'Browse...' -Keyboard
  Guest-Picker -Cancel
  Guest-UiAction $Window 'Appearance' -Keyboard
  Guest-UiAction $Window 'Midnight' -Space
  Wait-GuestCondition { (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).theme_preset -eq 'midnight' } 'Space did not save the selected theme'
  Guest-UiAction $Window 'Monochrome' -Keyboard
  Wait-GuestCondition { (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).theme_preset -eq 'monochrome' } 'Enter did not save the selected theme'
  $presetNames = @{ industrialis='Industrialis'; monochrome='Monochrome'; midnight='Midnight'; sandstone='Sandstone' }
  if (-not $presetNames.ContainsKey($originalPreset)) { throw 'Generated fixture contains an unexpected theme preset' }
  Guest-UiAction $Window $presetNames[$originalPreset] -Keyboard
  Wait-GuestCondition { (Get-Content -LiteralPath $settingsPath -Raw | ConvertFrom-Json).theme_preset -eq $originalPreset } 'Keyboard theme restoration did not persist'
  Guest-UiAction $Window 'Backups' -Space
  Wait-GuestUiElement $Window 'Save retention' | Out-Null
  Guest-UiAction $Window 'About' -Keyboard
  Wait-GuestUiElement $Window 'View on GitHub' | Out-Null
  $Report.nativeKeyboardAccountsPreferencesTheme = $true
  Guest-UiAction $Window 'Instances' -Keyboard
  (Wait-GuestUiElement $Window 'Instances').SetFocus()
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Instances' } 'Native focus did not reach Instances'
  Guest-Keys $Window '{TAB}'
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Processes' } 'Tab did not reach the next launcher tab'
  Guest-Keys $Window '+{TAB}'
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Instances' } 'Shift+Tab did not return to the preceding tab'
  (Wait-GuestUiElement $Window 'Instance library').SetFocus()
  Guest-Keys $Window '+{F10}'
  Guest-UiAction $Window 'New group' -Keyboard
  (Wait-GuestUiElement $Window 'Group name').SetFocus()
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Group name' } 'Native focus did not reach Group name'
  Guest-Keys $Window '{TAB}'
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Cancel' } 'Modal Tab did not stay inside the group dialog'
  Guest-Keys $Window '+{TAB}'
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Group name' } 'Modal Shift+Tab did not return to its field'
  Guest-Keys $Window '{ESC}'
  Wait-GuestCondition { $null -eq (Guest-UiElement $Window 'Group name') } 'Escape did not dismiss the active dialog'
  (Wait-GuestUiElement $Window 'Instance library').SetFocus()
  Guest-Keys $Window '+{F10}'
  Guest-UiAction $Window 'New group' -Space
  Guest-UiAction $Window 'Cancel' -Keyboard
  Wait-GuestCondition { $null -eq (Guest-UiElement $Window 'Group name') } 'Enter did not activate the dialog Cancel action'
  (Wait-GuestUiElement $Window 'Instance library').SetFocus()
  Guest-Keys $Window '+{F10}'
  Guest-UiAction $Window 'New group' -Keyboard
  (Wait-GuestUiElement $Window 'Group name').SetFocus()
  Wait-GuestCondition { [Windows.Automation.AutomationElement]::FocusedElement.Current.Name -eq 'Group name' } 'Native focus did not reach the recreated group field'
  Guest-Keys $Window 'KeyboardGroup'
  Wait-GuestCondition { (Guest-UiElement $Window 'Group name').GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern).Current.Value -eq 'KeyboardGroup' } 'Native keyboard text did not update the group field'
  Guest-UiAction $Window 'Save' -Keyboard
  Wait-GuestUiElement $Window 'Collapse group KEYBOARDGROUP' | Out-Null
  Wait-GuestUiElement $Window 'Collapse group UNGROUPED' | Out-Null
  Guest-UiAction $Window 'Collapse group UNGROUPED' -Space
  Wait-GuestUiElement $Window 'Expand group UNGROUPED' | Out-Null
  Guest-UiAction $Window 'Expand group UNGROUPED' -Keyboard
  # Rename is now a group context-menu action, reachable by Shift+F10.
  $groupHeader = Wait-GuestUiElement $Window 'Collapse group KEYBOARDGROUP'
  $groupHeader.SetFocus()
  Guest-Keys $Window '+{F10}'
  Guest-UiAction $Window 'Rename group' -Keyboard
  Wait-GuestUiElement $Window 'Group name' | Out-Null
  Guest-Keys $Window '{ESC}'
  Guest-UiAction $Window 'Delete group KEYBOARDGROUP' -Space
  Guest-UiAction $Window 'Save' -Keyboard
  Wait-GuestCondition { $null -eq (Guest-UiElement $Window 'Collapse group KEYBOARDGROUP') } 'Keyboard group deletion did not remove the empty group'
  $Report.nativeKeyboardDistinctAccountAndGroupControls = $true
  $Report.nativeKeyboardTabShiftTabModalEscapeEnter = $true
  if (-not $Report.nativeAccountHasNamedInput) { throw 'Native account input is missing its accessible name/role' }
  if (-not $Report.nativeTileSelectionPattern) { throw 'Instance tile is missing native selection activation semantics' }
  Progress 'Installed native keyboard, picker, folder and editor scenarios passed'
}
