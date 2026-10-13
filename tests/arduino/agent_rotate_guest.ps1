# Rotate only the fixture's token and restart its owned tree; keep tokens out of evidence.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$old=Get-Content C:\AgentProbe\output\guest-result.json -Raw | ConvertFrom-Json
if (-not $old.pass -or -not $old.logging_redirected) { throw 'Successful redirected startup required' }
$process=Get-Process -Id $old.app_pid
if ($process.Path -ne 'C:\AgentRO\Arduino Agent.exe') { throw 'Fixture process mismatch' }
Copy-Item C:\AgentProbe\output\guest-result.json C:\AgentProbe\output\guest-before-rotation.json
Copy-Item C:\AgentProbe\output\protocol-result.json C:\AgentProbe\output\protocol-before-rotation.json
& "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Owned process tree did not stop' }
$tokenFile=Join-Path $env:USERPROFILE .arduinoIDE\mcp-token
$env:HAZLO_PROBE_OLD_TOKEN=(Get-Content $tokenFile -Raw).Trim()
$bytes=New-Object byte[] 32
$random=[Security.Cryptography.RandomNumberGenerator]::Create()
$random.GetBytes($bytes); $random.Dispose()
[IO.File]::WriteAllText($tokenFile,([BitConverter]::ToString($bytes)).Replace('-','').ToLowerInvariant())
$app=Start-Process 'C:\AgentRO\Arduino Agent.exe' -WindowStyle Hidden -RedirectStandardOutput C:\AgentProbe\output\agent-rotated-stdout.log -RedirectStandardError C:\AgentProbe\output\agent-rotated-stderr.log -PassThru
$deadline=(Get-Date).AddSeconds(60)
do {
    try {$health=Invoke-RestMethod http://127.0.0.1:3847/health -TimeoutSec 2} catch {$health=$null}
    if (-not $health) {Start-Sleep -Milliseconds 500}
} while (-not $health -and (Get-Date) -lt $deadline)
if (-not $health -or -not $health.auth) { throw 'Authenticated startup missing' }
& C:\NodeRO\node.exe C:\AgentProbe\input\protocol.cjs
Remove-Item Env:HAZLO_PROBE_OLD_TOKEN
if ($LASTEXITCODE -ne 0) { throw 'Rotated-token probe failed' }
@{pass=$true;stage='finished';app_pid=$app.Id;health=$health;logging_redirected=$true;token_rotation_verified=$true;hardware_mutations=$false} | ConvertTo-Json -Depth 5 | Set-Content C:\AgentProbe\output\guest-result.json -Encoding UTF8
