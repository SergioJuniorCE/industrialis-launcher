# Native interaction only inside the verified, disposable guest profile.
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $marker.kind -ne 'gpui-caption-acceptance-v1') { throw 'Disposable explorer guest required' }
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
function Easy-Capture {
$rect = New-Object CaptionGuest+Rect
[CaptionGuest]::GetWindowRect($window,[ref]$rect) | Out-Null
$capture = [Drawing.Rectangle]::FromLTRB($rect.Left,$rect.Top,$rect.Right,$rect.Bottom)
if (-not [Windows.Forms.SystemInformation]::VirtualScreen.Contains($capture)) { throw 'Guest screenshot would be clipped' }
$bitmap = New-Object Drawing.Bitmap $capture.Width,$capture.Height
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
  $graphics.CopyFromScreen($capture.Location,[Drawing.Point]::Empty,$capture.Size)
  $bitmap.Save((Join-Path $outputRoot 'easy-last-setting.png'),[Drawing.Imaging.ImageFormat]::Png)
} finally { $graphics.Dispose(); $bitmap.Dispose() }
}
[CaptionGuest]::ShowWindow($window,9) | Out-Null
[CaptionGuest]::MoveWindow($window,20,20,1100,850,$true) | Out-Null
[CaptionGuest]::SetForegroundWindow($window) | Out-Null
Start-Sleep -Milliseconds 700
Explorer-Keys 'PerfPack' ' '
Explorer-Keys 'Files' '{ENTER}'
Explorer-Wait 'config' | Out-Null
Explorer-Keys 'Search instance files' 'easy-benchmark'
Explorer-Wait 'config/easy-benchmark.cfg' | Out-Null
Explorer-Keys 'Minecraft files' '{HOME}{ENTER}'
Explorer-Keys 'Easy' '{ENTER}'
Explorer-Wait 'general / setting1' | Out-Null
Explorer-Keys 'true' '{ENTER}'
Explorer-Wait 'false' | Out-Null
Explorer-Keys 'general / setting1' '^a43'
$root = [Windows.Automation.AutomationElement]::FromHandle($window)
$edits = $root.FindAll([Windows.Automation.TreeScope]::Descendants,
  (New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty),([Windows.Automation.ControlType]::Edit)))
if ($edits.Count -gt 40) { throw "Off-screen Easy inputs realized: $($edits.Count)" }
$report.realizedEditControls = $edits.Count
# Real wheel input over the owned form, moving to its last setting.
$bounds = (Explorer-Control 'general / setting1').Current.BoundingRectangle
[CaptionGuest]::SetCursorPos([int]($bounds.X+20),[int]($bounds.Y+10)) | Out-Null
$wheel = [BitConverter]::ToUInt32([BitConverter]::GetBytes([int]-12000),0)
for ($index=0; $index -lt 320; $index++) {
  if ([CaptionGuest]::GetForegroundWindow() -ne $window) { throw 'Owned guest foreground required for wheel input' }
  [CaptionGuest]::mouse_event(0x0800,0,0,$wheel,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
  if ($index % 10 -eq 0 -and $null -ne (Explorer-Control 'general / setting499')) { break }
}
Easy-Capture
$root = [Windows.Automation.AutomationElement]::FromHandle($window)
$edits = $root.FindAll([Windows.Automation.TreeScope]::Descendants,
  (New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty),([Windows.Automation.ControlType]::Edit)))
$report.visibleEditsAfterWheel = @($edits | ForEach-Object { $_.Current.Name })
Explorer-Keys 'general / setting499' '^a1234'
Explorer-Keys 'Save' '{ENTER}'
$active = "$instance/.minecraft/config/easy-benchmark.cfg"
$overlay = "$instance/persistent-minecraft/config/easy-benchmark.cfg"
$deadline = [DateTime]::UtcNow.AddSeconds(10)
do {
  $saved = [IO.File]::ReadAllText($active)
  if ($saved.Contains('B:enabled=false') -and $saved.Contains('I:setting1=43') -and $saved.Contains('I:setting499=1234') -and (Test-Path -LiteralPath $overlay)) { break }
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Easy changes did not save to both active and persistent config' }
  Start-Sleep -Milliseconds 50
} while ($true)
if ($saved -ne [IO.File]::ReadAllText($overlay) -or -not $saved.StartsWith('# Preserve this comment')) { throw 'Persistent file or lossless comments changed' }
$report.savedLastSetting = $true
$report.persistentCopyMatches = $true
Explorer-Wait 'config/easy-benchmark.cfg, persistent override' | Out-Null
$report.easyFieldCount = 500
Easy-Capture
$report.passed = $true
