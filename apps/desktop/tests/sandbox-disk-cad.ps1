# Actual generation and publication on the existing bounded owned guest disk.
$ErrorActionPreference='Stop'
$result=@{pass=$false;stage='starting';user=$env:USERNAME}
function Save-Result { $result | ConvertTo-Json -Depth 5 | Set-Content C:\HazloProbe\output\cad-disk-probe.json -Encoding UTF8 }
Save-Result
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned Sandbox required' }
    & { Get-Content C:\HazloProbe\input\disk-full.ps1 -Raw | Invoke-Expression }
    $prerequisite=Get-Content C:\HazloProbe\output\disk-probe.json -Raw | ConvertFrom-Json
    if (-not $prerequisite.pass -or $prerequisite.stage -ne 'finished') { throw 'Bounded disk prerequisite failed' }
    $volume=Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='T:'"
    if ($volume.FileSystem -ne 'NTFS' -or $volume.VolumeName -ne 'HMXFULL' -or $volume.Size -gt 64MB -or $volume.Size -lt 32MB) { throw 'Unexpected test volume' }
    $env:HAZLOMEXA_FREECAD_CMD='C:\FreeCADRO\FreeCADCmd.exe'
    $result.stage='actual CAD and full-disk publication'; Save-Result
    $info=[Diagnostics.ProcessStartInfo]::new('C:\HazloProbe\input\tests.exe')
    $info.Arguments='--ignored --exact jobs::tests::actual_cad_publication_on_exhausted_disk_preserves_whole_outputs_and_recovery --nocapture'
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    $test=[Diagnostics.Process]::new(); $test.StartInfo=$info
    if (-not $test.Start()) { throw 'Actual CAD disk test did not start' }
    $result.test_pid=$test.Id; Save-Result
    $stdout=$test.StandardOutput.ReadToEndAsync(); $stderr=$test.StandardError.ReadToEndAsync()
    if (-not $test.WaitForExit(120000)) { $test.Kill(); throw 'Actual CAD disk test exceeded deadline' }
    [IO.File]::WriteAllText('C:\HazloProbe\output\cad-disk-test.log', $stdout.GetAwaiter().GetResult())
    [IO.File]::WriteAllText('C:\HazloProbe\output\cad-disk-test-error.log', $stderr.GetAwaiter().GetResult())
    $result.test_exit=$test.ExitCode
    if ($test.ExitCode -ne 0) { throw 'Actual CAD disk test failed; inspect logs' }
    $detail=Get-Content C:\HazloProbe\output\cad-disk-full-result.json -Raw | ConvertFrom-Json
    if (-not $detail.pass -or $detail.disk_full_error -notin @(112,39)) { throw 'Actual publication/disk exhaustion not proved' }
    Copy-Item -LiteralPath T:\HazloDiskPublication -Destination C:\HazloProbe\output\project-checkpoint-closed -Recurse
    $root='T:\HazloDiskPublication'
    $result.project_files=@(Get-ChildItem -LiteralPath $root -File -Recurse | ForEach-Object { @{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash} })
    $result.pass=$true; $result.stage='finished'; Save-Result
} catch { $result.pass=$false; $result.error=$_.Exception.Message; $result.stage='failed'; Save-Result; exit 1 }
