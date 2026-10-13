# Export only the closed, owned publication fixture and assert its measured outcome.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (Get-Process -Name hazlomexa -ErrorAction SilentlyContinue) { throw 'Close the test app first' }
$root='C:\HazloGuestProject'
$armed=Get-Content C:\HazloProbe\output\publication-armed.json -Raw | ConvertFrom-Json
$held=Get-Content C:\HazloProbe\output\publication-held.json -Raw | ConvertFrom-Json
$released=Get-Content C:\HazloProbe\output\publication-barrier-released.json -Raw | ConvertFrom-Json
$request=Get-Content C:\HazloProbe\output\publication-release-request.json -Raw | ConvertFrom-Json
$job=[Guid]::Parse($armed.job_id).ToString()
if (-not $held.pass -or $held.job_id -ne $job -or $request.job_id -ne $job -or $released.pid -ne $armed.barrier_pid -or $released.unix_ms -lt $request.requested_unix_ms -or $released.unix_ms -le $held.observed_unix_ms) { throw 'Barrier evidence does not agree' }
if ((Get-Item C:\HazloProbe\output\publication-barrier-error.log).Length) { throw 'Barrier helper failed' }
$snapshot=Get-Content "$root\.hazlomexa\runs\$job.json" -Raw | ConvertFrom-Json
if ($snapshot.phase -ne 'completed' -or -not $snapshot.audit.files_verified -or $snapshot.audit.updated_unix_ms -lt $released.unix_ms) { throw 'Expected verified Completed after release' }
$destination="$root\.hazlomexa\artifacts\$job"
foreach ($name in @('input.json','result.json','solid.FCStd','solid.step')) {
    if ((Get-Item -LiteralPath "$destination\$name").Length -le 0) { throw "Missing committed file: $name" }
}
$result=Get-Content "$destination\result.json" -Raw | ConvertFrom-Json
if (-not $result.success -or $result.job_id -ne $job) { throw 'Actual CAD result does not agree' }
foreach ($file in Get-ChildItem C:\HazloProbe\input\saved-project -File -Recurse) {
    $relative=$file.FullName.Substring('C:\HazloProbe\input\saved-project\'.Length)
    if ((Get-FileHash $file.FullName -Algorithm SHA256).Hash -ne (Get-FileHash "$root\$relative" -Algorithm SHA256).Hash) { throw "Prior file changed: $relative" }
}
$checkpoint='C:\HazloProbe\output\project-checkpoint-closed'
if (Test-Path -LiteralPath $checkpoint) { throw 'Checkpoint already exists' }
Copy-Item -LiteralPath $root -Destination $checkpoint -Recurse
$files=@(Get-ChildItem $root -File -Recurse | ForEach-Object { @{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash $_.FullName -Algorithm SHA256).Hash} })
@{pass=$true;job_id=$job;app_closed=$true;prior_project_preserved=$true;phase=$snapshot.phase;project_files=$files} | ConvertTo-Json -Depth 5 | Set-Content C:\HazloProbe\output\publication-checkpoint.json -Encoding UTF8
