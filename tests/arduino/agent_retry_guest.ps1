# Retry only the known fixture process with stable log handles; preserve the initial result.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$old=Get-Content C:\AgentProbe\output\guest-result.json -Raw | ConvertFrom-Json
if ($old.stage -ne 'finished') { throw 'Initial probe is still running' }
$process=Get-Process -Id $old.app_pid
if ($process.Path -ne 'C:\AgentRO\Arduino Agent.exe') { throw 'Fixture process identity mismatch' }
Copy-Item C:\AgentProbe\output\guest-result.json C:\AgentProbe\output\guest-initial-result.json
Copy-Item C:\AgentProbe\output\protocol-result.json C:\AgentProbe\output\protocol-initial-result.json
& "$env:SystemRoot\System32\taskkill.exe" /PID $process.Id /T /F | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Owned process tree did not stop' }
$app=Start-Process 'C:\AgentRO\Arduino Agent.exe' -WindowStyle Hidden -RedirectStandardOutput C:\AgentProbe\output\agent-stdout.log -RedirectStandardError C:\AgentProbe\output\agent-stderr.log -PassThru
$deadline=(Get-Date).AddSeconds(60)
do {
    try {$health=Invoke-RestMethod http://127.0.0.1:3847/health -TimeoutSec 2} catch {$health=$null}
    if (-not $health) {Start-Sleep -Milliseconds 500}
} while (-not $health -and (Get-Date) -lt $deadline)
if (-not $health -or -not $health.auth -or $health.version -ne '0.7.1') { throw 'Expected authenticated Agent health' }
& C:\NodeRO\node.exe C:\AgentProbe\input\protocol.cjs
if ($LASTEXITCODE -ne 0) { throw 'Protocol retry failed' }
@{pass=$true;stage='finished';app_pid=$app.Id;health=$health;logging_redirected=$true;hardware_mutations=$false} | ConvertTo-Json -Depth 5 | Set-Content C:\AgentProbe\output\guest-result.json -Encoding UTF8
