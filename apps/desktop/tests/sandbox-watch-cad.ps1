$ErrorActionPreference='Stop'
$ready=Get-Content 'C:\HazloProbe\output\manual-ready.json' -Raw | ConvertFrom-Json
$app=Get-Process -Id $ready.app_pid
if ($app.Path -ne 'C:\HazloProbeApp\hazlomexa.exe') { throw 'Unexpected owner' }
$seen=@();$observedAt=$null;$deadline=(Get-Date).AddSeconds(60)
while ((Get-Date) -lt $deadline) {
    $all=@(Get-CimInstance Win32_Process)
    $owned=@($app.Id)
    do { $next=@($all | Where-Object { $_.ParentProcessId -in $owned -and $_.ProcessId -notin $owned } | ForEach-Object { $_.ProcessId });$owned+=$next } while ($next.Count)
    $cad=@($all | Where-Object { $_.ProcessId -in $owned -and $_.Name -like '*freecad*' } | Select-Object ProcessId,ParentProcessId,Name)
    if ($cad.Count) {
        if (-not $observedAt) { $observedAt=Get-Date; @{observed=$cad;app_pid=$app.Id;observed_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()} | ConvertTo-Json -Depth 5 | Set-Content 'C:\HazloProbe\output\cad-observed.json' -Encoding UTF8 }
        $seen+=@($cad | ForEach-Object { $_.ProcessId });$seen=@($seen | Select-Object -Unique)
    }
    if ($seen.Count -and -not $cad.Count) {
        $remaining=@($seen | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
        if (-not $remaining.Count) { @{pass=$true;observed_ids=$seen;remaining=@();ceased_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds();observation='Owned CAD processes ceased; persisted phase must be checked separately.'} | ConvertTo-Json -Depth 5 | Set-Content 'C:\HazloProbe\output\cad-cessation.json' -Encoding UTF8; exit 0 }
    }
    Start-Sleep -Milliseconds 50
}
@{pass=$false;observed_ids=$seen;error='No verified cessation within observer deadline'} | ConvertTo-Json | Set-Content 'C:\HazloProbe\output\cad-cessation.json' -Encoding UTF8
