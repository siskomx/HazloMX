# Launch the pinned app only in the owned offline guest with a private toolchain/sketchbook.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$result=@{pass=$false;stage='preparing';hardware_mutations=$false}
function Save-Result { $result | ConvertTo-Json -Depth 6 | Set-Content C:\AgentProbe\output\guest-result.json -Encoding UTF8 }
Save-Result
try {
    $config=Join-Path $env:USERPROFILE .arduinoIDE
    if (Test-Path $config) { throw 'Refusing an existing IDE profile' }
    New-Item -ItemType Directory -Path $config | Out-Null
    New-Item -ItemType Directory -Path C:\AgentSketchbook | Out-Null
    Copy-Item C:\ArduinoDataRO C:\AgentData -Recurse
    @{directories=@{data='C:\AgentData';downloads='C:\AgentDownloads';user='C:\AgentSketchbook'};metrics=@{enabled=$false};updater=@{enable_notification=$false};network=@{cloud_api=@{skip_board_detection_calls=$true}}} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $config arduino-cli.yaml) -Encoding UTF8
    # Keep console streams open: Electron otherwise reports EPIPE after the probe shell exits.
    $app=Start-Process 'C:\AgentRO\Arduino Agent.exe' -WindowStyle Hidden -RedirectStandardOutput C:\AgentProbe\output\agent-stdout.log -RedirectStandardError C:\AgentProbe\output\agent-stderr.log -PassThru
    $result.app_pid=$app.Id; $result.logging_redirected=$true; $result.stage='waiting for MCP'; Save-Result
    $deadline=(Get-Date).AddSeconds(90)
    do {
        try { $health=Invoke-RestMethod http://127.0.0.1:3847/health -TimeoutSec 2 } catch { $health=$null }
        if (-not $health) { Start-Sleep -Milliseconds 500 }
    } while (-not $health -and (Get-Date) -lt $deadline)
    if (-not $health -or -not $health.auth -or $health.version -ne '0.7.1') { throw 'Expected authenticated Agent 0.7.1 health' }
    $result.health=$health; $result.stage='testing protocol'; Save-Result
    & C:\NodeRO\node.exe C:\AgentProbe\input\protocol.cjs
    if ($LASTEXITCODE -ne 0) { throw 'Protocol probe failed; inspect redacted result' }
    $result.pass=$true; $result.stage='finished'
} catch { $result.error=$_.Exception.Message; $result.stage='failed' }
Save-Result
