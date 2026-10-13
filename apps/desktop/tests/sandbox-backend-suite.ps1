$ErrorActionPreference='Stop'
$result=@{pass=$false; os=[Environment]::OSVersion.VersionString; user=$env:USERNAME}
try {
  if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned Sandbox required.' }
  $env:HAZLOMEXA_PROCESS_PROBE_EXE='C:\HazloProbe\input\process-probe.exe'
  $test=Start-Process -FilePath 'C:\HazloProbe\input\tests.exe' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput 'C:\HazloProbe\output\backend-suite.log' -RedirectStandardError 'C:\HazloProbe\output\backend-suite-error.log'
  $result.exit=$test.ExitCode
  $log=Get-Content -LiteralPath 'C:\HazloProbe\output\backend-suite.log' -Raw
  $errors=Get-Content -LiteralPath 'C:\HazloProbe\output\backend-suite-error.log' -Raw -ErrorAction SilentlyContinue
  if ($test.ExitCode -ne 0 -or $log -notmatch 'test result: ok\. 40 passed; 0 failed; 6 ignored') { throw 'Backend suite did not pass all expected tests.' }
  if (($log+$errors).Contains('hazlomexa-test-secret-do-not-persist-9bb')) { throw 'Seeded credential leaked to test logs.' }
  $result.pass=$true; $result.passed=40; $result.ignored=6; $result.seeded_credential_absent=$true
} catch { $result.error=$_.Exception.Message }
$result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath 'C:\HazloProbe\output\backend-suite-result.json' -Encoding UTF8
