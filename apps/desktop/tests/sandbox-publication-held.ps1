# Read result and attributes only; reading FCStd data would also break the fixture oplock.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$armed=Get-Content C:\HazloProbe\output\publication-armed.json -Raw | ConvertFrom-Json
$job=[Guid]::Parse($armed.job_id).ToString()
$stage="C:\HazloGuestProject\.hazlomexa\staging\$job"
$broken=Get-Content C:\HazloProbe\output\publication-barrier-broken.json -Raw | ConvertFrom-Json
if ($broken.pid -ne $armed.barrier_pid) { throw 'Unexpected barrier identity' }
$helper=Get-Process -Id $armed.barrier_pid
if ($helper.Path -ne 'C:\HazloProbe\input\publication-barrier.exe') { throw 'Barrier is not live' }
$cessation=Get-Content C:\HazloProbe\output\cad-cessation.json -Raw | ConvertFrom-Json
$granted=Get-Content C:\HazloProbe\output\publication-barrier-granted.json -Raw | ConvertFrom-Json
if (-not $cessation.pass -or $cessation.ceased_unix_ms -lt $granted.unix_ms) { throw 'Current CAD cessation is not independently observed' }
$result=Get-Content -LiteralPath "$stage\result.json" -Raw | ConvertFrom-Json
if (-not $result.success -or $result.job_id -ne $job) { throw 'Actual successful fixed macro result is absent' }
$snapshot=Get-Content -LiteralPath "C:\HazloGuestProject\.hazlomexa\runs\$job.json" -Raw | ConvertFrom-Json
if ($snapshot.phase -ne 'running' -or (Test-Path -LiteralPath "C:\HazloGuestProject\.hazlomexa\artifacts\$job")) { throw 'Publication is not held before commit' }
@{pass=$true;job_id=$job;phase=$snapshot.phase;result_success=$true;cad_ceased=$true;barrier_live=$true;published=$false;observed_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();break_unix_ms=$broken.unix_ms} | ConvertTo-Json | Set-Content C:\HazloProbe\output\publication-held.json -Encoding UTF8
