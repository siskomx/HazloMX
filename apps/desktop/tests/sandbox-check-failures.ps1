# Read only the owned fixture, then export it after the app releases its handles.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (Get-Process -Name hazlomexa -ErrorAction SilentlyContinue) { throw 'Close the test app first' }
$root='C:\HazloGuestProject'
foreach ($mode in @('script-output','publication')) {
    $fault=Get-Content -LiteralPath "C:\HazloProbe\output\fault-$mode.json" -Raw | ConvertFrom-Json
    if (-not $fault.pass) { throw 'Fault was not armed' }
    $job=[Guid]::Parse($fault.job_id).ToString()
    $expected=if ($mode -eq 'publication') { "$root\.hazlomexa\artifacts\$job" } else { "$root\.hazlomexa\staging\$job\solid.FCStd" }
    if ($fault.target -ne $expected) { throw 'Unexpected fault target' }
    if ((Get-FileHash -LiteralPath "$expected\fixture-original.txt" -Algorithm SHA256).Hash -ne $fault.sentinel_sha256) { throw 'Fault sentinel changed' }
    $snapshot=Get-Content -LiteralPath "$root\.hazlomexa\runs\$job.json" -Raw | ConvertFrom-Json
    if ($snapshot.phase -ne 'failed') { throw 'Expected persisted Failed' }
    $stage="$root\.hazlomexa\staging\$job"
    $result=Get-Content -LiteralPath "$stage\result.json" -Raw | ConvertFrom-Json
    if ($mode -eq 'publication') {
        if (-not $result.success -or -not $snapshot.audit.files_verified -or $result.job_id -ne $job) { throw 'Successful verified stage not retained' }
        foreach ($name in @('solid.FCStd','solid.step')) {
            if ((Get-Item -LiteralPath "$stage\$name").Length -le 0 -or (Test-Path -LiteralPath "$expected\$name")) { throw 'Outputs missing or published into occupied destination' }
        }
    } elseif ($result.success -or $snapshot.audit.files_verified -or $result.error -ne 'Output already exists: solid.FCStd') { throw 'Expected actual fixed macro rejection' }
}
foreach ($file in Get-ChildItem -LiteralPath C:\HazloProbe\input\saved-project -File -Recurse) {
    $relative=$file.FullName.Substring('C:\HazloProbe\input\saved-project\'.Length)
    if ((Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath "$root\$relative" -Algorithm SHA256).Hash) { throw "Prior project file changed: $relative" }
}
$checkpoint='C:\HazloProbe\output\project-checkpoint-closed'
if (Test-Path -LiteralPath $checkpoint) { throw 'Checkpoint already exists' }
Copy-Item -LiteralPath $root -Destination $checkpoint -Recurse
$files=@(Get-ChildItem -LiteralPath $root -File -Recurse | ForEach-Object { @{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash} })
@{pass=$true;app_closed=$true;prior_project_preserved=$true;project_files=$files} | ConvertTo-Json -Depth 5 | Set-Content C:\HazloProbe\output\failure-checkpoint.json -Encoding UTF8
