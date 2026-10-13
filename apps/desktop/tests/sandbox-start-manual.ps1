$ErrorActionPreference='Stop'
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
    $prerequisite=Get-Content 'C:\HazloProbe\output\result.json' -Raw | ConvertFrom-Json
    if (-not $prerequisite.pass -or $prerequisite.stage -ne 'finished') { throw 'Wait for the automatic prerequisite probe to finish before installing the manual fixture.' }
    if (@(Get-Process hazlomexa -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq 'C:\HazloProbeApp\hazlomexa.exe' }).Count) { throw 'The manual test app is already running.' }
    $before=@(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*','HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue | Where-Object { $_.name -like '*WebView*' } | Select-Object name,pv)
    $installer=Start-Process 'C:\HazloProbe\input\setup.exe' -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -Wait -PassThru
    if ($installer.ExitCode -ne 0) { throw "Installer failed: $($installer.ExitCode)" }
    New-Item -ItemType Directory -Path C:\HazloGuestProject -Force | Out-Null
    if (-not (Test-Path 'C:\HazloGuestProject\original.txt')) { 'preserve original file' | Set-Content C:\HazloGuestProject\original.txt -Encoding UTF8 }
    $env:HAZLOMEXA_FREECAD_CMD='C:\FreeCADRO\FreeCADCmd.exe'
    $app=Start-Process C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
    @{pass=$true;app_pid=$app.Id;webview_before=$before;installer_exit=$installer.ExitCode;project='C:\HazloGuestProject'} | ConvertTo-Json -Depth 5 | Set-Content C:\HazloProbe\output\manual-ready.json -Encoding UTF8
} catch { @{pass=$false;error=$_.Exception.Message} | ConvertTo-Json | Set-Content C:\HazloProbe\output\manual-ready.json -Encoding UTF8 }
