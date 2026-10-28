# Dot-sourced only inside the verified disposable caption guest.
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $marker.kind -ne 'launcher-caption-acceptance-v1') { throw 'Disposable appearance guest required' }
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
function Appearance-Element([string]$Name) {
  $root = [Windows.Automation.AutomationElement]::FromHandle($window)
  $condition = New-Object Windows.Automation.PropertyCondition ([Windows.Automation.AutomationElement]::NameProperty),$Name
  return $root.FindFirst([Windows.Automation.TreeScope]::Descendants,$condition)
}
function Appearance-Action([string]$Name) {
  [uint32]$owner = 0
  [CaptionGuest]::GetWindowThreadProcessId($window,[ref]$owner) | Out-Null
  if ($owner -ne $app.Id -or [CaptionGuest]::GetForegroundWindow() -ne $window) { throw 'Owned foreground guest required' }
  $element = Appearance-Element $Name
  if ($null -eq $element) { throw "Appearance control missing: $Name" }
  $element.SetFocus()
  [Windows.Forms.SendKeys]::SendWait('{ENTER}')
  Start-Sleep -Milliseconds 350
}
function Appearance-Capture([string]$Name) {
  $rect = New-Object CaptionGuest+Rect
  if (-not [CaptionGuest]::GetWindowRect($window,[ref]$rect)) { throw 'Guest window bounds unavailable' }
  $bounds = [Drawing.Rectangle]::FromLTRB($rect.Left,$rect.Top,$rect.Right,$rect.Bottom)
  if (-not [Windows.Forms.SystemInformation]::VirtualScreen.Contains($bounds)) { throw 'Guest screenshot would be clipped by the desktop' }
  $bitmap = New-Object Drawing.Bitmap $bounds.Width,$bounds.Height
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($bounds.Location,[Drawing.Point]::Empty,$bounds.Size)
    $bitmap.Save((Join-Path $outputRoot $Name),[Drawing.Imaging.ImageFormat]::Png)
  } finally { $graphics.Dispose(); $bitmap.Dispose() }
}
[CaptionGuest]::ShowWindow($window,9) | Out-Null
[CaptionGuest]::MoveWindow($window,20,20,1100,850,$true) | Out-Null
[CaptionGuest]::SetForegroundWindow($window) | Out-Null
Start-Sleep -Milliseconds 700
Appearance-Action 'Launcher preferences'
if ($marker.scenario -eq 'about') {
  Appearance-Action 'About'
  Appearance-Capture 'about-actions.png'
  $rect = New-Object CaptionGuest+Rect
  [CaptionGuest]::GetWindowRect($window,[ref]$rect) | Out-Null
  $bitmap = [Drawing.Bitmap]::FromFile((Join-Path $outputRoot 'about-actions.png'))
  function Color-Luminance($color) {
    $channels = @($color.R,$color.G,$color.B) | ForEach-Object {
      $v = $_ / 255.0
      if ($v -le 0.04045) { $v / 12.92 } else { [Math]::Pow(($v+0.055)/1.055,2.4) }
    }
    return 0.2126*$channels[0]+0.7152*$channels[1]+0.0722*$channels[2]
  }
  $ratios = @{}
  try {
    foreach ($name in @('Open launcher data folder','View on GitHub','Launcher releases')) {
      $button = Appearance-Element $name
      if ($null -eq $button) { throw "About action is missing: $name" }
      $bounds = $button.Current.BoundingRectangle
      $x = [int][Math]::Round($bounds.X-$rect.Left)
      $y = [int][Math]::Round($bounds.Y-$rect.Top+$bounds.Height/2)
      $border = Color-Luminance ($bitmap.GetPixel($x,$y))
      $fill = Color-Luminance ($bitmap.GetPixel($x+4,$y))
      $ratio = ([Math]::Max($border,$fill)+0.05)/([Math]::Min($border,$fill)+0.05)
      if ($ratio -lt 2.9) { throw "About action boundary blends into its fill: $name ($ratio)" }
      $ratios[$name] = [Math]::Round($ratio,2)
    }
  } finally { $bitmap.Dispose() }
  [CaptionGuest]::MoveWindow($window,20,20,800,600,$true) | Out-Null
  Start-Sleep -Milliseconds 500
  Appearance-Capture 'about-minimum.png'
  $report.aboutActionsVisible = $true
  $report.actionBorderContrast = $ratios
  $report.passed = $true
  return
}
Appearance-Action 'Appearance'
foreach ($name in @('Light color scheme','Dark color scheme','Monochrome','Industrialis','Midnight','Sandstone','Create theme','Browse Open VSX themes')) {
  if ($null -eq (Appearance-Element $name)) { throw "Appearance option missing: $name" }
}
Appearance-Capture 'appearance-dark.png'
Appearance-Action 'Light color scheme'
Appearance-Capture 'appearance-light.png'
Appearance-Action 'Dark color scheme'
Appearance-Action 'Create theme'
$nameField = Appearance-Element 'Theme name'
if ($null -eq $nameField) { throw 'Theme editor name field is missing' }
$nameField.SetFocus()
[Windows.Forms.SendKeys]::SendWait('Sandbox theme')
Appearance-Action 'Save as new theme'
$deadline = [DateTime]::UtcNow.AddSeconds(5)
do {
  Start-Sleep -Milliseconds 100
  $saved = Get-Content -LiteralPath "$env:INDUSTRIALIS_DATA_DIR/launcher-settings.json" -Raw | ConvertFrom-Json
  if ([DateTime]::UtcNow -gt $deadline) { throw 'Created theme did not persist' }
} while (@($saved.custom_theme_presets | Where-Object {$_.name -eq 'Sandbox theme'}).Count -ne 1)
Appearance-Action 'Close editor'
Appearance-Capture 'appearance-saved.png'
[CaptionGuest]::MoveWindow($window,20,20,800,600,$true) | Out-Null
Start-Sleep -Milliseconds 500
Appearance-Capture 'appearance-minimum.png'
foreach ($name in @('Light color scheme','Dark color scheme','Create theme','Browse Open VSX themes')) {
  $element = Appearance-Element $name
  $bounds = $element.Current.BoundingRectangle
  $rect = New-Object CaptionGuest+Rect
  [CaptionGuest]::GetWindowRect($window,[ref]$rect) | Out-Null
  if ($bounds.X -lt $rect.Left -or $bounds.Right -gt $rect.Right) { throw "Control extends outside minimum window: $name" }
}
$report.appearanceNative = $true
$report.themeCreationPersisted = $true
$report.minimumWidth = 800
$report.passed = $true
