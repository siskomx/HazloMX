$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') {throw 'Owned guest required'}
$ready=Get-Content C:\AgentProbe\output\guest-result.json -Raw | ConvertFrom-Json
if (-not $ready.pass) {throw 'Agent prerequisite missing'}
$test=Start-Process C:\AgentProbe\input\backend-tests.exe -ArgumentList 'jobs::tests::actual_agent_cancel_with_confirmed_write_reply_held','--ignored','--exact','--nocapture' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput C:\AgentProbe\output\cancel-stdout.log -RedirectStandardError C:\AgentProbe\output\cancel-stderr.log
@{pass=$test.ExitCode -eq 0;exit_code=$test.ExitCode} | ConvertTo-Json | Set-Content C:\AgentProbe\output\cancel-test-result.json -Encoding UTF8
