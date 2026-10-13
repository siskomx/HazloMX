# Keep the real Agent and the controlled reply barrier inside this owned guest.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') {throw 'Owned guest required'}
$env:ARDUINO_MCP_PORT='3848'
$proxy=Start-Process C:\NodeRO\node.exe -ArgumentList C:\AgentProbe\input\held-reply.cjs -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\AgentProbe\output\proxy-stdout.log -RedirectStandardError C:\AgentProbe\output\proxy-stderr.log
@{proxy_pid=$proxy.Id;upstream_port=3848;client_port=3847} | ConvertTo-Json | Set-Content C:\AgentProbe\output\held-proxy.json -Encoding UTF8
& C:\AgentProbe\input\agent-base.ps1
