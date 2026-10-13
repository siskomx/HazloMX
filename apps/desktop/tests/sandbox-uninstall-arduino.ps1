# Uninstall only the owned fixture; independently recheck every saved project file.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (Get-Process hazlomexa -ErrorAction SilentlyContinue) { throw 'Close the test app first' }
$saved=Get-Content C:\HazloProbe\output\arduino-checkpoint.json -Raw | ConvertFrom-Json
if (-not $saved.pass) { throw 'A verified checkpoint is required' }
$root=Join-Path $env:USERPROFILE Desktop\HazloGuestProject
$uninstaller=Start-Process C:\HazloProbeApp\uninstall.exe -ArgumentList '/S' -WindowStyle Hidden -Wait -PassThru
$deadline=(Get-Date).AddSeconds(30)
while ((Test-Path C:\HazloProbeApp\hazlomexa.exe) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
if (Test-Path C:\HazloProbeApp\hazlomexa.exe) { throw 'App was not uninstalled' }
$files=@(Get-ChildItem $root -File -Recurse)
if ($files.Count -ne $saved.project_files.Count) { throw 'Project file count changed' }
foreach ($file in $saved.project_files) {
    if ((Get-FileHash (Join-Path $root $file.path)).Hash -ne $file.sha256) { throw "Project changed: $($file.path)" }
}
@{pass=$true;uninstalled=$true;project_preserved=$true;files_verified=$files.Count;exit_code=$uninstaller.ExitCode} | ConvertTo-Json | Set-Content C:\HazloProbe\output\arduino-uninstalled.json -Encoding UTF8
