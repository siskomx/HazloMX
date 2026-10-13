# Authenticated software-only Agent workflow in a fresh owned guest.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required.' }
$expected=Get-Content C:\HazloProbe\input\package-identity.json -Raw | ConvertFrom-Json
if ((Get-FileHash C:\HazloProbe\input\installed-agent-workflow.exe).Hash.ToLowerInvariant() -ne $expected.agent_harness_sha256 -or (Get-FileHash 'C:\AgentRO\Arduino Agent.exe').Hash.ToLowerInvariant() -ne $expected.agent_exe_sha256 -or (Get-FileHash C:\AgentRO\resources\mcp-bridge\bridge\arduino-agent-bridge.js).Hash.ToLowerInvariant() -ne $expected.agent_bridge_sha256) { throw 'Agent runtime/fixture identity mismatch.' }
$config=Join-Path $env:USERPROFILE .arduinoIDE
if ((Test-Path $config) -or (Get-NetTCPConnection -LocalPort 3847 -State Listen -ErrorAction SilentlyContinue)) { throw 'Refusing an existing Agent profile or service.' }
New-Item -ItemType Directory -Path $config,C:\AgentSketchbook,C:\HazloPackageAgent,C:\AgentPrivateLogs | Out-Null
Copy-Item -LiteralPath C:\ArduinoDataRO -Destination C:\AgentData -Recurse
@{directories=@{data='C:\AgentData';downloads='C:\AgentDownloads';user='C:\AgentSketchbook'};metrics=@{enabled=$false};updater=@{enable_notification=$false};network=@{cloud_api=@{skip_board_detection_calls=$true}}} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $config arduino-cli.yaml) -Encoding UTF8
Copy-Item -LiteralPath C:\HazloProbe\input\agent-original.ino -Destination C:\HazloPackageAgent\UserOriginal.ino
$agentApp=$null
$receipt=@{pass=$false;hardware_mutations=$false;model_inference=$false}
try {
    $agentApp=Start-Process 'C:\AgentRO\Arduino Agent.exe' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\AgentPrivateLogs\stdout.log -RedirectStandardError C:\AgentPrivateLogs\stderr.log
    $retainedHandle=$agentApp.Handle
    $deadline=(Get-Date).AddSeconds(90)
    do {
        try { $health=Invoke-RestMethod http://127.0.0.1:3847/health -TimeoutSec 2 } catch { $health=$null }
        if (-not $health) { Start-Sleep -Milliseconds 500 }
    } while (-not $health -and (Get-Date) -lt $deadline)
    if (-not $health -or -not $health.auth -or $health.version -ne '0.7.1') { throw 'Authenticated Agent 0.7.1 health missing.' }
    $receipt.authenticated_health=$true
    $test=Start-Process C:\HazloProbe\input\installed-agent-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\HazloPackageAgent\UserOriginal.ino','C:\HazloPackageAgent\project' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\agent-result.json -RedirectStandardError C:\HazloProbe\output\agent-error.log
    $testHandle=$test.Handle
    if (-not $test.WaitForExit(180000)) { Stop-Process -Id $test.Id; throw 'Installed Agent workflow timed out.' }
    if ($test.ExitCode -ne 0) { throw "Installed Agent workflow failed ($($test.ExitCode))." }
    $reply=Get-Content C:\HazloProbe\output\agent-result.json -Raw | ConvertFrom-Json
    if (-not $reply.pass -or $reply.completed_jobs -ne 2 -or $reply.denied_jobs -ne 1 -or $reply.occupied_destination_failures -ne 1 -or -not $reply.recovery_verified -or -not $reply.reconnect_invalidates_approval -or $reply.hardware_mutations) { throw 'Installed Agent result did not pass.' }
    Copy-Item -LiteralPath C:\HazloPackageAgent -Destination C:\HazloProbe\output\agent-project -Recurse
    $receipt.pass=$true
} finally {
    if ($agentApp) {
        $agentApp.Refresh()
        if (-not $agentApp.HasExited) {
            & taskkill.exe /PID $agentApp.Id /T /F | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'Could not stop the owned Agent tree.' }
        }
        if (-not $agentApp.WaitForExit(10000)) { throw 'Owned Agent root did not stop.' }
        $receipt.owned_agent_root_stopped=$true
    }
    $receipt | ConvertTo-Json | Set-Content C:\HazloProbe\output\agent-runtime-receipt.json -Encoding UTF8
}
if (Get-NetTCPConnection -LocalPort 3847 -State Listen -ErrorAction SilentlyContinue) { throw 'Agent service remained after owned tree shutdown.' }
$token=[IO.File]::ReadAllText((Join-Path $config mcp-token)).Trim()
if ($token.Length -lt 32) { throw 'Agent authentication fixture unavailable.' }
$redactions=@()
foreach ($stream in @('stdout','stderr')) {
    $raw=[IO.File]::ReadAllText((Join-Path C:\AgentPrivateLogs "$stream.log"))
    $count=($raw.Length-$raw.Replace($token,'').Length)/$token.Length
    [IO.File]::WriteAllText((Join-Path C:\HazloProbe\output "agent-$stream.log"),$raw.Replace($token,'[REDACTED_AGENT_TOKEN]'),[Text.UTF8Encoding]::new($false))
    $redactions+=@{stream=$stream;occurrences=$count}
}
foreach ($file in Get-ChildItem C:\HazloProbe\output -Recurse -File) {
    if ([Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($file.FullName)).Contains($token)) { throw 'Authentication material leaked into retained test output.' }
}
$receipt.auth_token_absent_from_export=$true
$receipt.upstream_console_redactions=$redactions
$receipt.raw_console_logs_private=$true
$receipt.service_stopped=$true
$receipt | ConvertTo-Json | Set-Content C:\HazloProbe\output\agent-runtime-receipt.json -Encoding UTF8
