$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$armed=Get-Content C:\HazloProbe\output\publication-armed.json -Raw | ConvertFrom-Json
$held=Get-Content C:\HazloProbe\output\publication-held.json -Raw | ConvertFrom-Json
$helper=Get-Process -Id $armed.barrier_pid
if ($helper.Path -ne 'C:\HazloProbe\input\publication-barrier.exe' -or -not $held.pass -or $held.job_id -ne $armed.job_id) { throw 'No verified live held publication' }
[IO.File]::WriteAllText('C:\HazloProbe\release-publication-barrier.txt','release')
@{job_id=$armed.job_id;requested_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()} | ConvertTo-Json | Set-Content C:\HazloProbe\output\publication-release-request.json -Encoding UTF8
