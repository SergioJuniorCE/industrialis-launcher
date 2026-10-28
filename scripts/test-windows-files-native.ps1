# Native interaction only inside the verified, disposable guest profile.
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $marker.kind -ne 'launcher-caption-acceptance-v1') { throw 'Disposable explorer guest required' }
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
function Explorer-Control([string]$Name) {
  $root = [Windows.Automation.AutomationElement]::FromHandle($window)
  $condition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),$Name
  return $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
}
function Explorer-Wait([string]$Name) {
  $deadline = [DateTime]::UtcNow.AddSeconds(20)
  do {
    $element = Explorer-Control $Name
    if ($null -ne $element) { return $element }
    if ([DateTime]::UtcNow -gt $deadline) { throw "Explorer control missing: $Name" }
    Start-Sleep -Milliseconds 50
  } while ($true)
}
function Explorer-Keys([string]$Name, [string]$Keys) {
  [uint32]$owner=0
  [CaptionGuest]::GetWindowThreadProcessId($window,[ref]$owner) | Out-Null
  if ($owner -ne $app.Id -or [CaptionGuest]::GetForegroundWindow() -ne $window) { throw 'Owned foreground guest required' }
  (Explorer-Wait $Name).SetFocus()
  [Windows.Forms.SendKeys]::SendWait($Keys)
}
function Files-Click([string]$Name) {
  Explorer-Keys $Name ''
  $bounds = (Explorer-Wait $Name).Current.BoundingRectangle
  [CaptionGuest]::SetCursorPos([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2)) | Out-Null
  [CaptionGuest]::mouse_event(2,0,0,0,[UIntPtr]::Zero)
  [CaptionGuest]::mouse_event(4,0,0,0,[UIntPtr]::Zero)
}
Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class FilesWindows {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc proc, IntPtr p);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint p);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder b, int n);
  public static IntPtr[] Owned(uint pid) {
    var result = new List<IntPtr>();
    EnumWindows((h,p) => { uint owner; GetWindowThreadProcessId(h,out owner); if(owner==pid && IsWindowVisible(h)) result.Add(h); return true; },IntPtr.Zero);
    return result.ToArray();
  }
  public static string Title(IntPtr h) { var b = new StringBuilder(1024); GetWindowText(h,b,b.Capacity); return b.ToString(); }
}
'@
function Files-Capture([string]$Name = 'files-desktop.png') {
  $bounds = [Windows.Forms.SystemInformation]::VirtualScreen
  $bitmap = New-Object Drawing.Bitmap $bounds.Width,$bounds.Height
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try { $graphics.CopyFromScreen($bounds.Location,[Drawing.Point]::Empty,$bounds.Size); $bitmap.Save("$outputRoot/$Name",[Drawing.Imaging.ImageFormat]::Png) }
  finally { $graphics.Dispose(); $bitmap.Dispose() }
}
function Files-Activate {
  [CaptionGuest]::SetForegroundWindow($window) | Out-Null
  Start-Sleep -Milliseconds 200
}
function Files-Context {
  Explorer-Keys 'Minecraft files' ''
  $bounds = (Explorer-Wait 'serverutilities/server/ranks.txt').Current.BoundingRectangle
  [CaptionGuest]::SetCursorPos([int]($bounds.X+$bounds.Width/2),[int]($bounds.Y+$bounds.Height/2)) | Out-Null
  [CaptionGuest]::mouse_event(8,0,0,0,[UIntPtr]::Zero)
  [CaptionGuest]::mouse_event(16,0,0,0,[UIntPtr]::Zero)
  Explorer-Wait 'Copy relative path' | Out-Null
}
function Files-CheckFolder([string]$Expected) {
  $shell = New-Object -ComObject Shell.Application
  $deadline = [DateTime]::UtcNow.AddSeconds(12)
  do {
    foreach ($folder in @($shell.Windows())) {
      try {
        if ($folder.LocationURL -and ([Uri]$folder.LocationURL).IsFile -and ([Uri]$folder.LocationURL).LocalPath.TrimEnd('\') -eq $Expected.TrimEnd('\')) {
          $folder.Quit()
          Start-Sleep -Milliseconds 350
          Files-Activate
          return
        }
      } catch { }
    }
    if ([DateTime]::UtcNow -gt $deadline) { throw "Explorer did not navigate inside $Expected" }
    Start-Sleep -Milliseconds 100
  } while ($true)
}
# The disposable image has no provisioned default text editor. Register an owned
# GUI fixture so the real shell association path is exercised without installing
# anything or changing host associations.
$editorFixture = "$env:INDUSTRIALIS_DATA_DIR/external-editor-fixture.exe"
Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing -OutputAssembly $editorFixture -OutputType WindowsApplication -TypeDefinition @'
using System;
using System.IO;
using System.Windows.Forms;
public static class ExternalEditorFixture {
  [STAThread] public static void Main(string[] args) {
    if (args.Length != 1) throw new Exception("One file path required");
    File.WriteAllText("C:/LauncherCaptionOutput/external-opened.txt", args[0]);
    var form = new Form { Text = "External Editor Fixture", Width = 640, Height = 480 };
    form.Controls.Add(new TextBox { Multiline = true, Dock = DockStyle.Fill, Text = File.ReadAllText(args[0]) });
    Application.Run(form);
  }
}
'@
New-Item -Path 'HKCU:/Software/Classes/.txt' -Force | Out-Null
Set-Item -Path 'HKCU:/Software/Classes/.txt' -Value 'LauncherAcceptance.Text'
New-Item -Path 'HKCU:/Software/Classes/LauncherAcceptance.Text/shell/open/command' -Force | Out-Null
Set-Item -Path 'HKCU:/Software/Classes/LauncherAcceptance.Text/shell/open/command' -Value ('"'+$editorFixture.Replace('/','\')+'" "%1"')
[CaptionGuest]::ShowWindow($window,9) | Out-Null
[CaptionGuest]::MoveWindow($window,20,20,1100,850,$true) | Out-Null
Files-Activate
$fullLibrary = (Explorer-Wait 'Instance library').Current.BoundingRectangle
if ((Explorer-Control 'Close instance details') -ne $null) { throw 'Unselected library has a details panel' }
$windowBounds = ([Windows.Automation.AutomationElement]::FromHandle($window)).Current.BoundingRectangle
if ($fullLibrary.Width -lt $windowBounds.Width - 24) { throw 'Unselected library does not fill the window' }
Explorer-Keys 'PerfPack' ' '
$splitLibrary = (Explorer-Wait 'Instance library').Current.BoundingRectangle
Explorer-Wait 'Close instance details' | Out-Null
if ($splitLibrary.Width -ge $fullLibrary.Width - 100) { throw 'Selecting an instance did not open details' }
Files-Click 'Close instance details'
$deadline = [DateTime]::UtcNow.AddSeconds(5)
do {
  $closedLibrary = (Explorer-Wait 'Instance library').Current.BoundingRectangle
  if ((Explorer-Control 'Close instance details') -eq $null -and [Math]::Abs($closedLibrary.Width - $fullLibrary.Width) -lt 2) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Closing details did not restore the full-width library' }
  Start-Sleep -Milliseconds 50
} while ($true)
Files-Capture 'workspace-closed.png'
Explorer-Keys 'PerfPack' ' '
Explorer-Wait 'Close instance details' | Out-Null
$reopenedLibrary = (Explorer-Wait 'Instance library').Current.BoundingRectangle
if ([Math]::Abs($reopenedLibrary.Width - $splitLibrary.Width) -ge 2) { throw 'Reopening details lost the previous panel width' }
$report.detailsCloseExpandsLibraryAndReopenRestoresSplit = $true
Explorer-Keys 'Open instance folder' '{ENTER}'
Files-CheckFolder ($instance.Replace('/','\'))
$report.instanceFolderOpenedInside = $true
Explorer-Keys 'Files' '{ENTER}'
Explorer-Wait 'config' | Out-Null
Explorer-Keys 'Search instance files' 'server ranks'
Explorer-Wait 'serverutilities/server/ranks.txt' | Out-Null
$report.multiwordPathSearch = $true
Explorer-Keys 'Minecraft files' '{HOME}{ENTER}'
Explorer-Wait 'Advanced' | Out-Null
Files-Context
Explorer-Keys 'Copy relative path' '{ENTER}'
if ([Windows.Forms.Clipboard]::GetText() -ne 'serverutilities/server/ranks.txt') { throw 'Wrong relative clipboard path' }
Files-Context
Explorer-Keys 'Copy absolute path' '{ENTER}'
$expected = "$instance/.minecraft/serverutilities/server/ranks.txt".Replace('/','\')
$deadline = [DateTime]::UtcNow.AddSeconds(5)
do {
  if ([Windows.Forms.Clipboard]::GetText() -eq $expected) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Wrong absolute clipboard path' }
  Start-Sleep -Milliseconds 50
} while ($true)
$report.relativeAndAbsoluteClipboard = $true
Files-Context
Explorer-Keys 'Open file location' '{ENTER}'
Files-CheckFolder ([IO.Path]::GetDirectoryName($expected))
$report.fileLocationOpened = $true
Files-Context
Explorer-Keys 'Open in external editor' '{ENTER}'
$deadline = [DateTime]::UtcNow.AddSeconds(15)
do {
  $editors = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -eq 'External Editor Fixture' })
  if ($editors.Count -gt 0) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Default external text editor did not open' }
  Start-Sleep -Milliseconds 100
} while ($true)
$report.externalEditorOpened = $true
if ([IO.File]::ReadAllText('C:/LauncherCaptionOutput/external-opened.txt') -ne $expected) { throw 'External editor received the wrong path' }
foreach ($external in $editors) { $external.CloseMainWindow() | Out-Null }
Start-Sleep -Milliseconds 500
Files-Activate
Files-Click 'Pop out file editor'
$deadline = [DateTime]::UtcNow.AddSeconds(12)
do {
  Start-Sleep -Milliseconds 100
  $app.Refresh()
  if ($app.HasExited) { throw "Pop-out terminated the launcher: exit $($app.ExitCode)" }
  $handles = @([FilesWindows]::Owned([uint32]$app.Id))
  $report.popoutWindowNames = @($handles | ForEach-Object { [FilesWindows]::Title($_) })
  Files-Capture
  if ($handles.Count -ge 2) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Native editor pop-out window did not appear' }
} while ($true)
$report.nativePopoutOpened = $true
$mainWindow = $window
$window = @($handles | Where-Object { $_ -ne $mainWindow })[0]
Files-Activate
[Windows.Forms.Clipboard]::SetText("# ranks popout`nplayer=edited`n")
Explorer-Keys 'File contents' '^a^v'
Explorer-Keys 'Save' '{ENTER}'
$deadline = [DateTime]::UtcNow.AddSeconds(10)
do {
  $saved = [IO.File]::ReadAllText($expected)
  if ($saved.Contains('player=edited')) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Pop-out edit did not save' }
  Start-Sleep -Milliseconds 50
} while ($true)
if ($saved -ne [IO.File]::ReadAllText("$instance/persistent-minecraft/serverutilities/server/ranks.txt")) { throw 'Pop-out persistent copy differs' }
$report.nativePopoutEditedAndSaved = $true
Files-Capture
[CaptionGuest]::SendMessage($window,0x0010,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 500
$window = $mainWindow
Files-Activate
Explorer-Wait 'Search instance files' | Out-Null
$report.mainWindowSurvivedEditorClose = $true
$report.passed = $true
