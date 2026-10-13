$ErrorActionPreference = 'Stop'
$result = @{ os = [Environment]::OSVersion.VersionString; user = $env:USERNAME; stage = 'starting'; pass = $false }
function Save-Probe { $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath C:\HazloProbe\output\result.json -Encoding UTF8 }
Save-Probe
try {
    $result.webview_before = @(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*','HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue | Where-Object { $_.name -like '*WebView*' } | Select-Object name,pv)
    $result.stage = 'installing'; Save-Probe
    $installer = Start-Process -FilePath C:\HazloProbe\input\setup.exe -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -PassThru
    if (-not $installer.WaitForExit(120000)) { Stop-Process -Id $installer.Id; throw 'Installer did not finish within 120 seconds.' }
    $result.installer_exit = $installer.ExitCode
    $result.installed = Test-Path -LiteralPath C:\HazloProbeApp\hazlomexa.exe
    if (-not $result.installed) { throw 'Installer did not create the application.' }
    $result.stage = 'starting installed application'; Save-Probe
    $app = Start-Process -FilePath C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 5
    $app.Refresh()
    $result.app_alive = -not $app.HasExited
    if (-not $result.app_alive) { throw 'Installed application exited during startup.' }
    Stop-Process -Id $app.Id
    $result.stage = 'uninstalling'; Save-Probe
    $uninstaller = Start-Process -FilePath C:\HazloProbeApp\uninstall.exe -ArgumentList '/S' -WindowStyle Hidden -PassThru
    if (-not $uninstaller.WaitForExit(60000)) { Stop-Process -Id $uninstaller.Id; throw 'Uninstaller did not finish within 60 seconds.' }
    $deadline = (Get-Date).AddSeconds(30)
    while ((Test-Path -LiteralPath C:\HazloProbeApp\hazlomexa.exe) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
    $result.uninstalled = -not (Test-Path -LiteralPath C:\HazloProbeApp\hazlomexa.exe)
    $result.pass = $result.app_alive -and $result.uninstalled
    $result.stage = 'finished'
} catch { $result.error = $_.Exception.Message; $result.stage = 'failed' }
Save-Probe
