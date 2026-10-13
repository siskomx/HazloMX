# Verify and export the closed cancelled fixture; preserve its unaccepted working copy separately.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') {throw 'Owned guest required'}
$test=Get-Content C:\AgentProbe\output\cancel-test-result.json -Raw | ConvertFrom-Json
$result=Get-Content C:\AgentProbe\output\cancel-result.json -Raw | ConvertFrom-Json
$held=Get-Content C:\AgentProbe\output\held-reply.json -Raw | ConvertFrom-Json
if (-not $test.pass -or -not $result.pass -or $result.job_id -ne $held.job_id) {throw 'Passing cancellation required'}
$root=(Resolve-Path -LiteralPath $result.root).ProviderPath
$temp=[IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')+'\'
if (-not $root.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -or -not ([IO.Path]::GetFileName($root)).StartsWith('hazlomexa-workflow-')) {throw 'Unexpected fixture root'}
$expected=Join-Path $temp "hazlomexa-agent-$($result.job_id)\Sketch\Sketch.ino"
if ($held.path -ne $expected) {throw 'Unexpected working copy'}
$snapshot=Get-Content "$root\.hazlomexa\runs\$($result.job_id).json" -Raw | ConvertFrom-Json
if ($snapshot.phase -ne 'cancelled' -or $snapshot.audit.files_verified) {throw 'Expected unverified Cancelled snapshot'}
if ([IO.File]::ReadAllText($expected) -cne $snapshot.input.source) {throw 'Actual Agent copy does not match requested write'}
if ([IO.File]::ReadAllText("$root\UserOriginal.ino") -cne $snapshot.input.original_source) {throw 'Original changed'}
$checkpoint='C:\AgentProbe\output\cancel-project-closed'
if (Test-Path $checkpoint) {throw 'Checkpoint exists'}
Copy-Item -LiteralPath $root -Destination $checkpoint -Recurse
Copy-Item -LiteralPath $expected -Destination C:\AgentProbe\output\unaccepted-copy.ino
$files=@(Get-ChildItem $root -File -Recurse | ForEach-Object {@{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash $_.FullName).Hash}})
$health=Invoke-RestMethod http://127.0.0.1:3847/health -TimeoutSec 2
if ($health.version -ne '0.7.1' -or -not $health.auth) {throw 'Separate Agent was not preserved'}
@{pass=$true;project_files=$files;unaccepted_copy_sha256=(Get-FileHash $expected).Hash;agent_remains_running=$true} | ConvertTo-Json -Depth 5 | Set-Content C:\AgentProbe\output\cancel-hashes.json -Encoding UTF8
