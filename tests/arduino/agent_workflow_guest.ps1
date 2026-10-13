# Verify the shared production approval/publication/recovery workflow against the real Agent.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$ready=Get-Content C:\AgentProbe\output\guest-result.json -Raw | ConvertFrom-Json
if (-not $ready.pass) { throw 'Authenticated Agent prerequisite missing' }
$test=Start-Process C:\AgentProbe\input\backend-tests.exe -ArgumentList 'jobs::tests::actual_agent_workflow_approval_publication_failure_and_recovery','--ignored','--exact','--nocapture' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput C:\AgentProbe\output\workflow-stdout.log -RedirectStandardError C:\AgentProbe\output\workflow-stderr.log
@{pass=$test.ExitCode -eq 0;exit_code=$test.ExitCode} | ConvertTo-Json | Set-Content C:\AgentProbe\output\workflow-test-result.json -Encoding UTF8
