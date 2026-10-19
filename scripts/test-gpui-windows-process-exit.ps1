# Exercise the guest's actual installer wait function without running guest setup.
$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'test-gpui-windows-squirrel-guest.ps1'),[ref]$tokens,[ref]$parseErrors)
if ($parseErrors.Count -gt 0) { throw 'Guest script could not be parsed' }
$nativeTokens = $null
$nativeParseErrors = $null
$nativeAst = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'test-gpui-windows-native-ui.ps1'),[ref]$nativeTokens,[ref]$nativeParseErrors)
if ($nativeParseErrors.Count -gt 0) { throw 'Native guest UI script could not be parsed' }
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
foreach ($call in $nativeAst.FindAll({ param($node)
  $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Static -and
  $node.Expression -is [Management.Automation.Language.TypeExpressionAst] -and
  $node.Expression.TypeName.FullName -like 'Windows.Automation.*'
},$true)) {
  # Resolve actual native-script static methods by reflection, without invoking
  # UI Automation. This catches unsupported API/class combinations before a VM.
  $apiType = $call.Expression.TypeName.FullName -as [type]
  if ($null -eq $apiType -or @($apiType.GetMethods() | Where-Object Name -eq $call.Member.Value).Count -eq 0) {
    throw ('Unsupported guest UI Automation method: ' + $call.Expression.TypeName.FullName + '::' + $call.Member.Value)
  }
}
foreach ($command in @($ast.FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type' },$true)) + @($nativeAst.FindAll({ param($node) $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Add-Type' },$true))) {
  for ($index=1; $index -lt $command.CommandElements.Count; $index++) {
    if ($command.CommandElements[$index] -is [Management.Automation.Language.CommandParameterAst] -and
        $command.CommandElements[$index].ParameterName -eq 'TypeDefinition') {
      $definition = $command.CommandElements[$index+1]
      if ($definition -isnot [Management.Automation.Language.StringConstantExpressionAst]) { throw 'Guest interop definition must be a literal' }
      # Compile the actual declarations only; never call a window/desktop API.
      Add-Type -TypeDefinition $definition.Value
    }
  }
}
foreach ($name in @('Progress','Run-Installer')) {
  $function = $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name },$true)
  if ($function.Count -ne 1) { throw 'Guest process wait seam is ambiguous' }
  # Import only these two functions, never the guest installer/profile/shutdown body.
  . ([scriptblock]::Create($function[0].Extent.Text))
}
$outputRoot = Join-Path (Split-Path -Parent $PSScriptRoot) ('apps/launcher-gpui/target/live-acceptance/process-exit-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $outputRoot | Out-Null
Run-Installer $env:ComSpec @('/d','/c','exit','0')
$rejected = $false
try { Run-Installer $env:ComSpec @('/d','/c','exit','7') }
catch {
  if ($_.Exception.Message -notlike '*installer failed: 7') { throw }
  $rejected = $true
}
if (-not $rejected) { throw 'Guest wait accepted a failed native child' }
$editorTokens = $null
$editorParseErrors = $null
$editorAst = [Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'test-gpui-windows-editor-guest.ps1'),[ref]$editorTokens,[ref]$editorParseErrors)
if ($editorParseErrors.Count -gt 0) { throw 'Guest editor could not be parsed' }
$replacement = @($editorAst.FindAll({ param($node)
  $node -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Static -and
  $node.Expression -is [Management.Automation.Language.TypeExpressionAst] -and $node.Expression.TypeName.FullName -eq 'IO.File' -and
  $node.Member.Value -eq 'Replace'
},$true))
if ($replacement.Count -ne 1 -or $replacement[0].Arguments.Count -ne 3 -or
    $replacement[0].Arguments[0].VariablePath.UserPath -ne 'temporary' -or $replacement[0].Arguments[1].VariablePath.UserPath -ne 'Path') { throw 'Guest atomic-save seam is ambiguous' }
# Execute only the actual File.Replace expression with generated workspace paths,
# never the guest identity, form, timer, registry or desktop body.
$source = Join-Path $outputRoot 'editor-source.cfg'
$destination = Join-Path $outputRoot 'editor-destination.cfg'
[IO.File]::WriteAllText($source,'new generated editor bytes')
[IO.File]::WriteAllText($destination,'old generated editor bytes')
$replace = [scriptblock]::Create('param($temporary,$Path)' + "`n" + $replacement[0].Extent.Text)
& $replace $source $destination
if ((Test-Path -LiteralPath $source) -or [IO.File]::ReadAllText($destination) -ne 'new generated editor bytes') { throw 'Guest atomic-save replacement lost the new bytes' }
@{ successfulChild='passed'; failedChild='rejected with actual code 7'; redirectedOutput=$true; guestInteropCompiled=$true; nativeUiStaticMethods='verified without invocation'; nativeAtomicEditorSave='passed'; guestSetupExecuted=$false; desktopApisInvoked=$false } | ConvertTo-Json -Compress
