$ErrorActionPreference='Stop'
$owned='C:\HazloNonAdminOwned2'
trap { if(Test-Path $owned) {$_ | Out-String | Set-Content "$owned\failure-private.log" -Encoding UTF8}; exit 1 }
$inputRoot=Join-Path $owned 'input'
$identity=Get-Content "$inputRoot\identity.json" -Raw | ConvertFrom-Json
$principal=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
$adminSid='S-1-5-32-544'
if ($env:USERNAME -ne 'HazloAcceptance2' -or $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) -or ([Security.Principal.WindowsIdentity]::GetCurrent().Groups.Value -contains $adminSid)) { throw 'Ordinary test user required.' }
$registry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
$machine='HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
if ((Test-Path $registry) -or (Test-Path $machine)) { throw 'Refusing an existing installation.' }
$install=Join-Path $env:LOCALAPPDATA 'HazloNonAdminAcceptance\app'
if (Test-Path $install) { throw 'Fresh private installation required.' }
$result=@{pass=$false;ordinary_nonadministrator=$true;administrator_group_absent=$true;native_ui_automation=$false;signed_in_agents=$false;approval_driver='shared_backend_test';installer_sha256=$identity.installer_sha256}
function Run-Owned([string]$exe,[string[]]$arguments,[string]$name,[int]$timeout) {
    $process=Start-Process -FilePath $exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput "$owned\$name.stdout.log" -RedirectStandardError "$owned\$name.stderr.log"
    $handle=$process.Handle
    if (-not $process.WaitForExit($timeout)) { Stop-Process -Id $process.Id; throw 'Owned nonadministrator fixture timed out.' }
    Set-Content "$owned\$name-exit.json" $process.ExitCode -Encoding ASCII
    if ($process.ExitCode -ne 0) { throw "Owned nonadministrator fixture failed at $name." }
}
function Project-Hashes {
    $files=@{}
    foreach($file in Get-ChildItem "$owned\project" -Recurse -File) {$files[$file.FullName]=(Get-FileHash $file.FullName).Hash}
    return $files
}
if ((Get-FileHash "$inputRoot\setup.exe").Hash.ToLowerInvariant() -ne $identity.installer_sha256) { throw 'Package identity mismatch.' }
Run-Owned "$inputRoot\installed-tools-protocol.exe" @("$inputRoot\hazlomexa-tools.exe","$owned\project") 'prepare-project' 60000
$before=Project-Hashes
$data=Join-Path $env:LOCALAPPDATA 'mx.hazlomexa.desktop'
New-Item -ItemType Directory -Path $data -Force | Out-Null
$sentinel=Join-Path $data 'nonadmin-preservation.txt'
if(Test-Path $sentinel) {throw 'Fresh preservation marker required.'}
Set-Content $sentinel 'preserve ordinary user data' -Encoding UTF8
$sentinelHash=(Get-FileHash $sentinel).Hash
$installed=$false
$app=$null
try {
    Run-Owned "$inputRoot\setup.exe" @('/S','/NS',"/D=$install") 'install' 120000
    $installed=$true
    if (-not (Test-Path $registry) -or (Test-Path $machine) -or (Get-ItemProperty $registry).InstallLocation.Trim('"') -ne $install) { throw 'Per-user registration mismatch.' }
    $result.current_user_install=$true
    if ((Get-FileHash "$install\hazlomexa-tools.exe").Hash.ToLowerInvariant() -ne $identity.helper_sha256 -or (Get-FileHash "$install\hazlomexa.exe").Hash.ToLowerInvariant() -ne $identity.installed_app_sha256) { throw 'Installed binary identity mismatch.' }
    $result.binaries_match_package=$true
    Run-Owned "$inputRoot\setup.exe" @('/S','/NS','/UPDATE',"/D=$install") 'update' 120000
    $result.same_version_update=$true
    Run-Owned "$inputRoot\installed-tools-protocol.exe" @("$install\hazlomexa-tools.exe","$owned\protocol-project") 'installed-tools' 60000
    $result.installed_helper_protocol_pass=$true
    Run-Owned "$inputRoot\installed-kicad-workflow.exe" @("$install\hazlomexa-tools.exe",'C:\KiCadOwned\bin\kicad-cli.exe',"$owned\kicad-sources","$owned\kicad-project") 'kicad' 180000
    $native=Get-Content "$owned\kicad.stdout.log" -Raw | ConvertFrom-Json
    if (-not $native.pass -or $native.completed_jobs -ne 2 -or -not $native.recovery_verified) {throw 'Native ordinary-user KiCad jobs failed.'}
    $result.native_kicad_completed_jobs=2
    $result.native_kicad_recovery=$true
    $app=Start-Process "$install\hazlomexa.exe" -WindowStyle Hidden -PassThru
    $handle=$app.Handle
    Start-Sleep -Seconds 5
    if ($app.HasExited) { throw 'Ordinary-user app exited at startup.' }
    $result.hidden_startup_pass=$true
} finally {
    if($app -and -not $app.HasExited) {Stop-Process -Id $app.Id; $app.WaitForExit()}
    if($installed) {
        Run-Owned "$install\uninstall.exe" @('/S') 'uninstall' 120000
        $deadline=(Get-Date).AddSeconds(30)
        while(((Test-Path "$install\hazlomexa.exe") -or (Test-Path $registry)) -and (Get-Date) -lt $deadline) {Start-Sleep -Milliseconds 100}
        if((Test-Path "$install\hazlomexa.exe") -or (Test-Path $registry)) {throw 'Ordinary-user uninstall incomplete.'}
        $result.uninstalled=$true
    }
    $after=Project-Hashes
    if($before.Count -ne $after.Count -or @($before.Keys | Where-Object {$before[$_] -ne $after[$_]}).Count) {throw 'Ordinary-user lifecycle changed external project files.'}
    if((Get-FileHash $sentinel).Hash -ne $sentinelHash) {throw 'Ordinary-user app-data marker changed.'}
    $result.user_files_preserved=$true
    $result | ConvertTo-Json -Depth 6 | Set-Content "$owned\result.json" -Encoding UTF8
}
$result.pass=$true
$result | ConvertTo-Json -Depth 6 | Set-Content "$owned\result.json" -Encoding UTF8
