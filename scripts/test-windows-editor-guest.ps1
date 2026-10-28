param([Parameter(Mandatory=$true)][string]$Path)
# A controlled native default editor, registered only inside the disposable guest.
$ErrorActionPreference = 'Stop'
$marker = Get-Content -LiteralPath 'C:/LauncherAcceptanceInput/marker.json' -Raw | ConvertFrom-Json
if ($env:USERNAME -ne 'WDAGUtilityAccount' -or $env:COMPUTERNAME -eq $marker.hostComputer -or
    $marker.kind -ne 'launcher-expendable-squirrel-acceptance-v1') { throw 'Disposable guest editor required' }
try {
$expected = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'industrialis-launcher/instances/FixtureOne/.minecraft/config/acceptance.cfg'
@{ nonce=$marker.nonce; guest=$true; pid=$PID; argumentPath=$Path; expectedPath=$expected } |
  ConvertTo-Json | Set-Content -LiteralPath 'C:/LauncherAcceptanceOutput/editor-startup.json' -Encoding UTF8
if ([IO.Path]::GetFullPath($Path) -ne [IO.Path]::GetFullPath($expected)) { throw 'Owned guest editor file required' }
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$form = New-Object Windows.Forms.Form
$form.Text = 'Launcher acceptance default editor'
$form.Size = New-Object Drawing.Size 500,300
$box = New-Object Windows.Forms.TextBox
$box.Multiline = $true
$box.Dock = 'Fill'
$box.Text = [IO.File]::ReadAllText($Path)
$form.Controls.Add($box)
$timer = New-Object Windows.Forms.Timer
$timer.Interval = 2000
$timer.Add_Tick({
  $timer.Stop()
  try {
    $box.Text = $box.Text.Replace('I:amount=1','I:amount=2').Replace('original','external')
    $temporary = $Path + '.owned-editor-save'
    [IO.File]::WriteAllText($temporary,$box.Text)
    [IO.File]::Replace($temporary,$Path,[NullString]::Value)
    @{ nonce=$marker.nonce; guest=$true; pid=$PID; nativeWindow=$form.Handle.ToString(); atomicSave=$true;
       fileSha256=(Get-FileHash -LiteralPath $Path).Hash; passed=$true } |
      ConvertTo-Json | Set-Content -LiteralPath 'C:/LauncherAcceptanceOutput/editor-result.json' -Encoding UTF8
  } catch {
    @{ nonce=$marker.nonce; guest=$true; pid=$PID; error=$_.Exception.Message; passed=$false } |
      ConvertTo-Json | Set-Content -LiteralPath 'C:/LauncherAcceptanceOutput/editor-error.json' -Encoding UTF8
  } finally { $form.Close() }
})
$form.Add_Shown({ $timer.Start() })
try { [Windows.Forms.Application]::Run($form) } finally { $timer.Dispose(); $form.Dispose() }
} catch {
  @{ nonce=$marker.nonce; guest=$true; pid=$PID; error=$_.Exception.Message; passed=$false } |
    ConvertTo-Json | Set-Content -LiteralPath 'C:/LauncherAcceptanceOutput/editor-error.json' -Encoding UTF8
  throw
}
