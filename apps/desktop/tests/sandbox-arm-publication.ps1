$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$ready=Get-Content C:\HazloProbe\output\manual-ready.json -Raw | ConvertFrom-Json
$app=Get-Process -Id $ready.app_pid
if ($app.Path -ne 'C:\HazloProbeApp\hazlomexa.exe') { throw 'Unexpected app owner' }
$pending=@(Get-ChildItem C:\HazloGuestProject\.hazlomexa\runs -Filter '*.json' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json } | Where-Object phase -eq 'awaiting_approval')
if ($pending.Count -ne 1) { throw 'Expected one pending approval' }
$job=[Guid]::Parse($pending[0].input.job_id).ToString()
$stage="C:\HazloGuestProject\.hazlomexa\staging\$job"
if (Test-Path -LiteralPath $stage) { throw 'Stage already exists before approval' }
foreach ($name in @('publication-barrier-granted.json','publication-barrier-broken.json','publication-barrier-released.json','publication-held.json','cad-observed.json','cad-cessation.json')) {
    Remove-Item -LiteralPath ('C:\HazloProbe\output\'+$name) -ErrorAction SilentlyContinue
}
[IO.File]::WriteAllText('C:\HazloProbe\release-publication-barrier.txt','hold')
$barrier=Start-Process C:\HazloProbe\input\publication-barrier.exe -ArgumentList '--stage',$stage -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\publication-barrier.log -RedirectStandardError C:\HazloProbe\output\publication-barrier-error.log
$observer=Start-Process powershell.exe -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','C:\HazloProbe\input\watch-cad.ps1' -WindowStyle Hidden -PassThru
@{app_pid=$app.Id;barrier_pid=$barrier.Id;observer_pid=$observer.Id;job_id=$job;stage=$stage} | ConvertTo-Json | Set-Content C:\HazloProbe\output\publication-armed.json -Encoding UTF8
