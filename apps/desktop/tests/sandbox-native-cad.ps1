# Production backend/adapter checks on guest-local NTFS; no browser or AI models.
$ErrorActionPreference='Stop'
$result=@{pass=$false;stage='starting';os=[Environment]::OSVersion.VersionString;user=$env:USERNAME;errors=@()}
function Save-Result { $result | ConvertTo-Json -Depth 5 | Set-Content 'C:\HazloProbe\output\native-cad-result.json' -Encoding UTF8 }
Save-Result
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned Windows Sandbox required.' }
    $env:HAZLOMEXA_FREECAD_CMD='C:\FreeCADRO\FreeCADCmd.exe'
    $result.stage='backend regression suite'; Save-Result
    & { Get-Content 'C:\HazloProbe\input\backend-suite.ps1' -Raw | Invoke-Expression }
    $backend=Get-Content 'C:\HazloProbe\output\backend-suite-result.json' -Raw | ConvertFrom-Json
    if (-not $backend.pass) { throw 'Backend regression failed.' }
    $result.backend=$backend
    foreach ($test in @('freecad::tests::actual_freecad_runs_inside_owned_job_with_private_configuration','jobs::tests::actual_approved_workflow_publishes_and_recovers')) {
        $name=$test.Split('::')[-1]
        $result.stage=$name; Save-Result
        $run=Start-Process 'C:\HazloProbe\input\tests.exe' -ArgumentList '--ignored','--exact',$test -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput "C:\HazloProbe\output\$name.log" -RedirectStandardError "C:\HazloProbe\output\$name-error.log"
        $log=Get-Content -LiteralPath "C:\HazloProbe\output\$name.log" -Raw
        $passed=$run.ExitCode -eq 0 -and $log -match 'test result: ok\. 1 passed; 0 failed'
        $result[$name]=$passed
        if (-not $passed) { $result.errors += "Actual native test failed: $name" }
    }
    $result.stage='native geometry and failure suite'; Save-Result
    $python=Start-Process 'C:\FreeCADRO\bin\python.exe' -ArgumentList '-B','C:\HazloProbe\input\repo\tests\test_freecad.py','-v' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput 'C:\HazloProbe\output\geometry.log' -RedirectStandardError 'C:\HazloProbe\output\geometry-error.log'
    $geometry=Get-Content 'C:\HazloProbe\output\geometry-error.log' -Raw
    if ($python.ExitCode -ne 0 -or $geometry -notmatch 'Ran 8 tests' -or $geometry -notmatch '(?m)^OK\s*$' -or $geometry -match 'skipped') { throw 'Eight real FreeCAD geometry/failure tests did not pass.' }
    $result.geometry_tests=8
    $result.pass=$result.errors.Count -eq 0; $result.stage='finished'
} catch { $result.pass=$false; $result.error=$_.Exception.Message; $result.stage='failed' }
Save-Result
if (-not $result.pass) { exit 1 }
