# Supported ASCII-private-profile path; accented project/output paths remain tested.
$ErrorActionPreference='Stop'
$result=@{pass=$false;ascii_profiles=$true;checks=@();os=[Environment]::OSVersion.VersionString}
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned Windows Sandbox required.' }
    $env:HAZLOMEXA_FREECAD_CMD='C:\FreeCADRO\FreeCADCmd.exe'
    $env:HAZLOMEXA_TEST_ASCII_PROFILES='1'
    foreach ($test in @('freecad::tests::actual_freecad_runs_inside_owned_job_with_private_configuration','jobs::tests::actual_approved_workflow_publishes_and_recovers')) {
        $name=$test.Split('::')[-1]
        $run=Start-Process 'C:\HazloProbe\input\tests.exe' -ArgumentList '--ignored','--exact',$test -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput "C:\HazloProbe\output\$name-ascii.log" -RedirectStandardError "C:\HazloProbe\output\$name-ascii-error.log"
        $log=Get-Content -LiteralPath "C:\HazloProbe\output\$name-ascii.log" -Raw
        if ($run.ExitCode -ne 0 -or $log -notmatch 'test result: ok\. 1 passed; 0 failed') { throw "ASCII profile native check failed: $name" }
        $result.checks += $name
    }
    $result.pass=$true
} catch { $result.error=$_.Exception.Message }
finally { Remove-Item Env:\HAZLOMEXA_TEST_ASCII_PROFILES -ErrorAction SilentlyContinue }
$result | ConvertTo-Json -Depth 5 | Set-Content 'C:\HazloProbe\output\native-ascii-result.json' -Encoding UTF8
if (-not $result.pass) { exit 1 }
