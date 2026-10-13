# Update the owned guest and verify that its already compiled project is untouched.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (Get-Process hazlomexa -ErrorAction SilentlyContinue) { throw 'Close the test app first' }
$saved=Get-Content C:\HazloProbe\output\arduino-checkpoint.json -Raw | ConvertFrom-Json
if (-not $saved.pass) { throw 'A verified closed project is required' }
$root=Join-Path $env:USERPROFILE Desktop\HazloGuestProject
$installer=Start-Process C:\HazloProbe\input\setup-preview.exe -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -Wait -PassThru
if ($installer.ExitCode -ne 0) { throw 'Update failed' }
foreach ($file in $saved.project_files) {
    if ((Get-FileHash (Join-Path $root $file.path)).Hash -ne $file.sha256) { throw "Project changed: $($file.path)" }
}
$app=Start-Process C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
@{pass=$true;project_preserved=$true;installer_exit=$installer.ExitCode;app_pid=$app.Id;app_sha256=(Get-FileHash C:\HazloProbeApp\hazlomexa.exe).Hash} | ConvertTo-Json | Set-Content C:\HazloProbe\output\arduino-update.json -Encoding UTF8
