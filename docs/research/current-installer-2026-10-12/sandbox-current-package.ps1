param([switch]$ResumeInstalled)
$ErrorActionPreference = 'Stop'
$result = @{pass=$false;stage='starting';os=[Environment]::OSVersion.VersionString;user=$env:USERNAME}
function Save-Probe { $result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath C:\HazloProbe\output\package-result.json -Encoding UTF8 }
function Run-Owned($file, $arguments, $seconds) {
    $process = Start-Process -FilePath $file -ArgumentList $arguments -WindowStyle Hidden -PassThru
    $retainedHandle = $process.Handle
    if (-not $process.WaitForExit($seconds*1000)) { Stop-Process -Id $process.Id; throw "Owned process timed out: $file" }
    if ($process.ExitCode -ne 0) { throw "Owned process failed ($($process.ExitCode)): $file" }
}
function Probe-Tools($name) {
    $process = Start-Process -FilePath C:\HazloProbe\input\installed-tools-protocol.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\HazloPackageProject' -WindowStyle Hidden -PassThru -RedirectStandardOutput "C:\HazloProbe\output\$name.json" -RedirectStandardError "C:\HazloProbe\output\$name-error.log"
    $retainedHandle = $process.Handle
    if (-not $process.WaitForExit(45000)) { Stop-Process -Id $process.Id; throw 'Connector protocol timed out.' }
    $result.connector_exit=$process.ExitCode
    if ($process.ExitCode -ne 0) { throw 'Installed connector protocol failed.' }
    $reply = Get-Content -LiteralPath "C:\HazloProbe\output\$name.json" -Raw | ConvertFrom-Json
    if (-not $reply.pass -or $reply.tool_count -ne 13 -or -not $reply.no_engineering_jobs) { throw 'Installed tool surface did not pass.' }
}
Save-Probe
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Fresh owned Windows Sandbox required.' }
    if (-not $ResumeInstalled -and (Test-Path -LiteralPath C:\HazloProbeApp)) { throw 'Refusing an existing application directory.' }
    $registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
    $machine = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
    if ((Test-Path $machine) -or (-not $ResumeInstalled -and (Test-Path $registry))) { throw 'Refusing an existing installation.' }
    $expected = Get-Content -LiteralPath C:\HazloProbe\input\package-identity.json -Raw | ConvertFrom-Json
    $result.installer_sha256 = (Get-FileHash C:\HazloProbe\input\setup.exe).Hash.ToLowerInvariant()
    if ($result.installer_sha256 -ne $expected.installer_sha256) { throw 'Installer identity mismatch.' }
    New-Item -ItemType Directory -Path C:\HazloPackageProject -Force | Out-Null
    $sentinel = 'C:\HazloPackageProject\user-source.txt'
    Set-Content -LiteralPath $sentinel -Value 'Preserve this user-owned source across installation.' -Encoding UTF8
    $sourceHash = (Get-FileHash -LiteralPath $sentinel).Hash
    $data = Join-Path $env:LOCALAPPDATA 'mx.hazlomexa.desktop'
    New-Item -ItemType Directory -Path $data -Force | Out-Null
    $dataSentinel = Join-Path $data 'preserve-package-test.txt'
    Set-Content -LiteralPath $dataSentinel -Value 'Preserve application user data.' -Encoding UTF8
    $dataHash = (Get-FileHash -LiteralPath $dataSentinel).Hash
    $result.stage='installing'; Save-Probe
    if (-not $ResumeInstalled) { Run-Owned C:\HazloProbe\input\setup.exe @('/S','/NS','/D=C:\HazloProbeApp') 120 }
    $result.resumed_after_identity_check = [bool]$ResumeInstalled
    if (-not (Test-Path $registry) -or (Test-Path $machine)) { throw 'Installation scope is not current user.' }
    $location = (Get-ItemProperty -LiteralPath $registry).InstallLocation.Trim('"')
    if ($location -ne 'C:\HazloProbeApp') { throw 'Unexpected installation location.' }
    foreach ($name in @('hazlomexa.exe','hazlomexa-tools.exe')) {
        $hash = (Get-FileHash -LiteralPath (Join-Path 'C:\HazloProbeApp' $name)).Hash.ToLowerInvariant()
        $expectedHash = if ($name -eq 'hazlomexa.exe') { $expected.app_sha256 } else { $expected.helper_sha256 }
        if ($hash -ne $expectedHash) { throw "Installed binary identity mismatch: $name" }
    }
    $result.current_user_install=$true; $result.binaries_match_package_build=$true
    $result.stage='installed connector'; Save-Probe
    Probe-Tools 'installed-tools'
    $projectBefore = (Get-FileHash C:\HazloPackageProject\hazlomx.json).Hash
    $result.stage='updating'; Save-Probe
    Run-Owned C:\HazloProbe\input\setup.exe @('/S','/NS','/UPDATE','/D=C:\HazloProbeApp') 120
    if ((Get-FileHash C:\HazloPackageProject\hazlomx.json).Hash -ne $projectBefore) { throw 'Update changed the project.' }
    Probe-Tools 'updated-tools'
    $result.same_version_update=$true
    $result.stage='starting application'; Save-Probe
    $app = Start-Process -FilePath C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 5
    $app.Refresh()
    if ($app.HasExited) { throw 'Installed application exited at startup.' }
    $result.app_alive=$true
    Stop-Process -Id $app.Id
    $app.WaitForExit(15000) | Out-Null
    $result.stage='uninstalling'; Save-Probe
    Run-Owned C:\HazloProbeApp\uninstall.exe @('/S') 60
    $deadline = (Get-Date).AddSeconds(30)
    while (((Test-Path C:\HazloProbeApp\hazlomexa.exe) -or (Test-Path $registry)) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if ((Test-Path C:\HazloProbeApp\hazlomexa.exe) -or (Test-Path C:\HazloProbeApp\hazlomexa-tools.exe) -or (Test-Path $registry)) { throw 'Uninstall left application files or registration.' }
    if ((Get-FileHash -LiteralPath $sentinel).Hash -ne $sourceHash -or (Get-FileHash -LiteralPath $dataSentinel).Hash -ne $dataHash -or (Get-FileHash C:\HazloPackageProject\hazlomx.json).Hash -ne $projectBefore) { throw 'Update/uninstall changed user files.' }
    $result.uninstalled=$true; $result.user_files_preserved=$true; $result.pass=$true; $result.stage='finished'
} catch { $result.error=$_.Exception.Message; $result.stage='failed' }
Save-Probe
