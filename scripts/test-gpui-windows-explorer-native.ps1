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
[CaptionGuest]::ShowWindow($window,9) | Out-Null
[CaptionGuest]::MoveWindow($window,20,20,1100,850,$true) | Out-Null
[CaptionGuest]::SetForegroundWindow($window) | Out-Null
Start-Sleep -Milliseconds 700
Explorer-Keys 'PerfPack' ' '
Explorer-Keys 'Files' '{ENTER}'
Explorer-Wait 'config' | Out-Null
# Find an unopened descendant through the actual native text field.
Explorer-Keys 'Search instance files' 'file-04999'
Explorer-Wait 'config/file-04999.cfg' | Out-Null
Explorer-Keys 'Minecraft files' '{HOME}{ENTER}'
Explorer-Wait 'Advanced' | Out-Null
Explorer-Keys 'Clear file search' '{ENTER}'
Explorer-Wait 'config' | Out-Null
$report.nativeSearchOpenedCollapsedFile = $true
Explorer-Keys 'Minecraft files' '{HOME}{RIGHT}'
Explorer-Wait 'file-00000.cfg' | Out-Null
$root = [Windows.Automation.AutomationElement]::FromHandle($window)
$treeItems = $root.FindAll([Windows.Automation.TreeScope]::Descendants,
  (New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::ControlTypeProperty),([Windows.Automation.ControlType]::TreeItem)))
if ($treeItems.Count -gt 100) { throw "Explorer realized off-screen tree items: $($treeItems.Count)" }
$report.realizedRows = $treeItems.Count
$timer = [Diagnostics.Stopwatch]::StartNew()
Explorer-Keys 'Minecraft files' '{END}'
$last = Explorer-Wait 'file-04999.cfg'
$timer.Stop()
$report.keyboardEndMilliseconds = $timer.ElapsedMilliseconds
$bounds = $last.Current.BoundingRectangle
$treeBounds = (Explorer-Control 'Minecraft files').Current.BoundingRectangle
if (-not $treeBounds.Contains($bounds.X+$bounds.Width/2,$bounds.Y+$bounds.Height/2)) { throw 'Last file is outside the actual explorer viewport' }
Explorer-Keys 'Minecraft files' '{ENTER}'
Explorer-Wait 'Easy' | Out-Null
Explorer-Wait 'Advanced' | Out-Null
Start-Sleep -Milliseconds 250
$last = Explorer-Wait 'file-04999.cfg'
$bounds = $last.Current.BoundingRectangle
$treeBounds = (Explorer-Control 'Minecraft files').Current.BoundingRectangle
if (-not $treeBounds.Contains($bounds.X+$bounds.Width/2,$bounds.Y+$bounds.Height/2)) { throw 'Opening a file scrolled keyboard selection out of the viewport' }
$rect = New-Object CaptionGuest+Rect
[CaptionGuest]::GetWindowRect($window,[ref]$rect) | Out-Null
$capture = [Drawing.Rectangle]::FromLTRB($rect.Left,$rect.Top,$rect.Right,$rect.Bottom)
if (-not [Windows.Forms.SystemInformation]::VirtualScreen.Contains($capture)) { throw 'Guest screenshot would be clipped' }
$bitmap = New-Object Drawing.Bitmap $capture.Width,$capture.Height
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
  $graphics.CopyFromScreen($capture.Location,[Drawing.Point]::Empty,$capture.Size)
  $bitmap.Save((Join-Path $outputRoot 'explorer-last-file.png'),[Drawing.Imaging.ImageFormat]::Png)
} finally { $graphics.Dispose(); $bitmap.Dispose() }
$report.explorerFileCount = 5000
$report.openedLastFile = $true
$report.passed = $true
