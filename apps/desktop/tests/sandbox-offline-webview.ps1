# Run only in an owned, fresh Windows Sandbox with Networking disabled.
$ErrorActionPreference = 'Stop'
$result = @{ os = [Environment]::OSVersion.VersionString; user = $env:USERNAME; stage = 'starting'; pass = $false }
function Save-Probe { $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath C:\HazloProbe\output\result.json -Encoding UTF8 }
Save-Probe
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'This test requires the isolated Sandbox user.' }
    $result.webview_before = @(Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*','HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*','HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue | Where-Object { $_.name -like '*WebView*' } | Select-Object name,pv)
    if ($result.webview_before.Count -ne 0) { throw 'Fresh guest must not already have WebView2.' }
    if (Test-Path -LiteralPath C:\HazloProbeApp) { throw 'Test installation path already exists.' }
    $result.stage = 'installing offline'; Save-Probe
    $installer = Start-Process -FilePath C:\HazloProbe\input\setup.exe -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -PassThru
    if (-not $installer.WaitForExit(120000)) { Stop-Process -Id $installer.Id; throw 'Offline installer did not terminate within 120 seconds.' }
    $result.installer_exit = $installer.ExitCode
    $result.installed = Test-Path -LiteralPath C:\HazloProbeApp\hazlomexa.exe
    $result.registration_present = Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
    $result.pass = $result.installer_exit -ne 0 -and -not $result.installed -and -not $result.registration_present
    if (-not $result.pass) { throw 'Offline bootstrap failure masquerades as a successful installation.' }
    $result.stage = 'finished'
} catch { $result.error = $_.Exception.Message; $result.stage = 'failed' }
Save-Probe
