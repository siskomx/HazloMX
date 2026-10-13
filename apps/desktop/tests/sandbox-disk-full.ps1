# Create and fill only a new 64 MiB VHD inside an owned Windows Sandbox.
$ErrorActionPreference = 'Stop'
$result = @{ pass = $false; stage = 'starting'; user = $env:USERNAME }
function Save-Probe { $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath C:\HazloProbe\output\disk-probe.json -Encoding UTF8 }
Save-Probe
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Windows Sandbox user required.' }
    if ((Test-Path -LiteralPath 'T:\') -or (Test-Path -LiteralPath 'C:\HazloOwnedDisk')) { throw 'Fixture path or drive letter already exists.' }
    New-Item -ItemType Directory -Path 'C:\HazloOwnedDisk' | Out-Null
    @'
create vdisk file="C:\HazloOwnedDisk\fixture.vhd" maximum=64 type=fixed
select vdisk file="C:\HazloOwnedDisk\fixture.vhd"
attach vdisk
create partition primary
format fs=ntfs label=HMXFULL quick
assign letter=T
exit
'@ | Set-Content -LiteralPath 'C:\HazloOwnedDisk\commands.txt' -Encoding ASCII
    $result.stage = 'creating bounded VHD'; Save-Probe
    $diskpart = Start-Process -FilePath "$env:SystemRoot\System32\diskpart.exe" -ArgumentList '/s','C:\HazloOwnedDisk\commands.txt' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput 'C:\HazloProbe\output\diskpart.log' -RedirectStandardError 'C:\HazloProbe\output\diskpart-error.log'
    if (-not $diskpart.WaitForExit(60000)) { Stop-Process -Id $diskpart.Id; throw 'Owned VHD creation exceeded deadline.' }
    if ($diskpart.ExitCode -ne 0) { throw "DiskPart failed: $($diskpart.ExitCode)" }
    $volume = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='T:'"
    if ($volume.FileSystem -ne 'NTFS' -or $volume.VolumeName -ne 'HMXFULL' -or $volume.Size -gt 64MB -or $volume.Size -lt 32MB) { throw 'Unexpected fixture volume; refusing disk-full test.' }
    $result.volume = @{ device = $volume.DeviceID; filesystem = $volume.FileSystem; label = $volume.VolumeName; size = $volume.Size }
    New-Item -ItemType Directory -Path 'T:\HazloDiskProject' | Out-Null
    [IO.File]::WriteAllText('T:\HazloDiskProject\owned-fixture.txt','Hazlo owned 64 MiB NTFS fixture',[Text.Encoding]::ASCII)
    $result.stage = 'running actual filesystem test'; Save-Probe
    $test = Start-Process -FilePath 'C:\HazloProbe\input\tests.exe' -ArgumentList '--ignored','--exact','project::tests::actual_disk_exhaustion_preserves_snapshot_and_inputs','--nocapture' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput 'C:\HazloProbe\output\test.log' -RedirectStandardError 'C:\HazloProbe\output\test-error.log'
    if (-not $test.WaitForExit(60000)) { Stop-Process -Id $test.Id; throw 'Bounded disk-full test exceeded deadline.' }
    $result.test_exit = $test.ExitCode
    if ($test.ExitCode -ne 0) { throw 'Actual disk-full test failed; inspect retained test logs.' }
    $detail = Get-Content -LiteralPath C:\HazloProbe\output\disk-full-result.json -Raw | ConvertFrom-Json
    if (-not $detail.pass -or $detail.disk_full_error -notin @(112,39)) { throw 'Disk exhaustion was not proved.' }
    $result.pass = $true; $result.stage = 'finished'
} catch { $result.error = $_.Exception.Message; $result.stage = 'failed' }
Save-Probe
