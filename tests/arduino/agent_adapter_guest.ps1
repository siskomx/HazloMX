# Run the real production adapter fixture with inspectable output; host launch success is insufficient.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$ready=Get-Content C:\AgentProbe\output\guest-result.json -Raw | ConvertFrom-Json
if (-not $ready.pass -or $ready.stage -ne 'finished') { throw 'Authenticated Agent prerequisite missing' }
$test=Start-Process C:\AgentProbe\input\backend-tests.exe -ArgumentList 'agent::tests::actual_owned_agent_edits_only_copy_and_verifies_selected_document','--ignored','--exact','--nocapture' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput C:\AgentProbe\output\adapter-stdout.log -RedirectStandardError C:\AgentProbe\output\adapter-stderr.log
@{pass=$test.ExitCode -eq 0;exit_code=$test.ExitCode} | ConvertTo-Json | Set-Content C:\AgentProbe\output\adapter-test-result.json -Encoding UTF8
