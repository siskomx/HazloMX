# Export only the closed project named by the passing owned workflow fixture.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$test=Get-Content C:\AgentProbe\output\workflow-test-result.json -Raw | ConvertFrom-Json
$result=Get-Content C:\AgentProbe\output\workflow-result.json -Raw | ConvertFrom-Json
if (-not $test.pass -or -not $result.pass) { throw 'Passing real workflow required' }
$root=(Resolve-Path -LiteralPath $result.root).ProviderPath
$temp=[IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')+'\'
if (-not $root.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($root)).StartsWith('hazlomexa-workflow-')) { throw 'Unexpected fixture root' }
$checkpoint='C:\AgentProbe\output\agent-project-closed'
if (Test-Path $checkpoint) { throw 'Checkpoint exists' }
Copy-Item -LiteralPath $root -Destination $checkpoint -Recurse
$files=@(Get-ChildItem $root -File -Recurse | ForEach-Object { @{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash $_.FullName).Hash} })
@{pass=$true;project_files=$files} | ConvertTo-Json -Depth 5 | Set-Content C:\AgentProbe\output\project-hashes.json -Encoding UTF8
