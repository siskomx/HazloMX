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
    # The harness hosts the current catalog through either installed transport helper.
    if (-not $reply.pass -or $reply.tool_count -ne $expected.tool_count -or -not $reply.no_engineering_jobs) { throw 'Installed tool surface did not pass.' }
}
Save-Probe
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Fresh owned Windows Sandbox required.' }
    if (-not $ResumeInstalled -and (Test-Path -LiteralPath C:\HazloProbeApp)) { throw 'Refusing an existing application directory.' }
    $registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
    $machine = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
    if ((Test-Path $machine) -or (-not $ResumeInstalled -and (Test-Path $registry))) { throw 'Refusing an existing installation.' }
    $expected = Get-Content -LiteralPath C:\HazloProbe\input\package-identity.json -Raw | ConvertFrom-Json
    $initialSetup = 'C:\HazloProbe\input\setup.exe'
    $initialExpected = $expected
    if (Test-Path -LiteralPath C:\HazloProbe\input\previous-setup.exe) {
        $initialSetup = 'C:\HazloProbe\input\previous-setup.exe'
        $initialExpected = Get-Content -LiteralPath C:\HazloProbe\input\previous-package-identity.json -Raw | ConvertFrom-Json
        if ((Get-FileHash -LiteralPath $initialSetup).Hash.ToLowerInvariant() -ne $initialExpected.installer_sha256) { throw 'Previous installer identity mismatch.' }
        if ($initialExpected.installer_sha256 -eq $expected.installer_sha256) { throw 'Previous installer must differ from the update.' }
        $result.previous_installer_sha256 = $initialExpected.installer_sha256
    }
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
    if (-not $ResumeInstalled) { Run-Owned $initialSetup @('/S','/NS','/D=C:\HazloProbeApp') 120 }
    $result.resumed_after_identity_check = [bool]$ResumeInstalled
    if (-not (Test-Path $registry) -or (Test-Path $machine)) { throw 'Installation scope is not current user.' }
    $location = (Get-ItemProperty -LiteralPath $registry).InstallLocation.Trim('"')
    if ($location -ne 'C:\HazloProbeApp') { throw 'Unexpected installation location.' }
    foreach ($name in @('hazlomexa.exe','hazlomexa-tools.exe')) {
        $hash = (Get-FileHash -LiteralPath (Join-Path 'C:\HazloProbeApp' $name)).Hash.ToLowerInvariant()
        $expectedHash = if ($name -eq 'hazlomexa.exe') { $initialExpected.app_sha256 } else { $initialExpected.helper_sha256 }
        if ($hash -ne $expectedHash) { throw "Installed binary identity mismatch: $name" }
    }
    $result.current_user_install=$true; $result.binaries_match_package_build=$true
    $result.stage='installed connector'; Save-Probe
    Probe-Tools 'installed-tools'
    $projectBefore = (Get-FileHash C:\HazloPackageProject\hazlomx.json).Hash
    $result.stage='updating'; Save-Probe
    Run-Owned C:\HazloProbe\input\setup.exe @('/S','/NS','/UPDATE','/D=C:\HazloProbeApp') 120
    foreach ($name in @('hazlomexa.exe','hazlomexa-tools.exe')) {
        $expectedHash = if ($name -eq 'hazlomexa.exe') { $expected.app_sha256 } else { $expected.helper_sha256 }
        if ((Get-FileHash -LiteralPath (Join-Path 'C:\HazloProbeApp' $name)).Hash.ToLowerInvariant() -ne $expectedHash) { throw "Updated binary identity mismatch: $name" }
    }
    if ((Get-FileHash C:\HazloPackageProject\hazlomx.json).Hash -ne $projectBefore) { throw 'Update changed the project.' }
    Probe-Tools 'updated-tools'
    $result.same_version_update=$true
    $result.previous_build_update=($initialSetup -ne 'C:\HazloProbe\input\setup.exe')
    if (Test-Path C:\HazloProbe\input\installed-agent-workflow.exe) {
        $result.stage='native Arduino Agent workflow through installed connector'; Save-Probe
        Get-Content -LiteralPath C:\HazloProbe\input\sandbox-agent-workflow.ps1 -Raw | Invoke-Expression
        $agentBefore=@(Get-ChildItem C:\HazloPackageAgent -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_agent_connector_workflow=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-arduino-workflow.exe) {
        $result.stage='native Arduino compile workflow through installed connector'; Save-Probe
        if ((Get-FileHash C:\HazloProbe\input\installed-arduino-workflow.exe).Hash.ToLowerInvariant() -ne $expected.arduino_harness_sha256) { throw 'Arduino harness identity mismatch.' }
        Copy-Item -LiteralPath C:\ArduinoRuntimeRO -Destination C:\ArduinoRuntimeOwned -Recurse
        Copy-Item -LiteralPath C:\ArduinoDataRO -Destination C:\ArduinoDataOwned -Recurse
        if ((Get-FileHash C:\ArduinoRuntimeOwned\arduino-cli.exe).Hash.ToLowerInvariant() -ne $expected.arduino_cli_sha256) { throw 'Arduino runtime identity mismatch.' }
        $arduino=Start-Process -FilePath C:\HazloProbe\input\installed-arduino-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\ArduinoRuntimeOwned\arduino-cli.exe','C:\ArduinoDataOwned','C:\HazloPackageArduino' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\arduino-result.json -RedirectStandardError C:\HazloProbe\output\arduino-error.log
        $retainedHandle=$arduino.Handle
        if (-not $arduino.WaitForExit(180000)) { Stop-Process -Id $arduino.Id; throw 'Native Arduino test timed out.' }
        if ($arduino.ExitCode -ne 0) { throw "Native Arduino test failed ($($arduino.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\arduino-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 2 -or $reply.failed_jobs -ne 1 -or $reply.denied_jobs -ne 1 -or -not $reply.recovery_verified -or $reply.hardware_mutations) { throw 'Native Arduino result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageArduino -Destination C:\HazloProbe\output\arduino-project -Recurse
        $arduinoBefore=@(Get-ChildItem C:\HazloPackageArduino -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_arduino_connector_workflow=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-kicad-workflow.exe) {
        $result.stage='native KiCad workflow through installed connector'; Save-Probe
        Copy-Item -LiteralPath C:\KiCadRO -Destination C:\KiCadOwned -Recurse
        New-Item -ItemType Directory -Path C:\HazloPackageEngineering | Out-Null
        Copy-Item -LiteralPath C:\HazloProbe\input\kicad-sources -Destination C:\HazloPackageEngineering\sources -Recurse
        $engineering=Start-Process -FilePath C:\HazloProbe\input\installed-kicad-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\KiCadOwned\bin\kicad-cli.exe','C:\HazloPackageEngineering\sources','C:\HazloPackageEngineering\project' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\engineering-result.json -RedirectStandardError C:\HazloProbe\output\engineering-error.log
        $retainedHandle=$engineering.Handle
        if (-not $engineering.WaitForExit(120000)) { Stop-Process -Id $engineering.Id; throw 'Native engineering test timed out.' }
        if ($engineering.ExitCode -ne 0) { throw "Native engineering test failed ($($engineering.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\engineering-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 2 -or -not $reply.recovery_verified) { throw 'Native engineering result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageEngineering\project -Destination C:\HazloProbe\output\engineering-project -Recurse
        $engineeringBefore=@(Get-ChildItem C:\HazloPackageEngineering\project -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_kicad_connector_workflow=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-konnect-workflow.exe) {
        $result.stage='native Konnect schematic and PCB workflows through installed connector'; Save-Probe
        if ((Get-FileHash C:\HazloProbe\input\installed-konnect-workflow.exe).Hash.ToLowerInvariant() -ne $expected.konnect_harness_sha256) { throw 'Konnect harness identity mismatch.' }
        if (-not (Test-Path C:\KiCadOwned)) { Copy-Item -LiteralPath C:\KiCadRO -Destination C:\KiCadOwned -Recurse }
        Copy-Item -LiteralPath C:\KonnectRO -Destination C:\KonnectOwned -Recurse
        if ((Get-FileHash C:\HazloProbe\input\vcruntime140.dll).Hash.ToLowerInvariant() -ne $expected.konnect_vcruntime_sha256) { throw 'Konnect Visual C++ prerequisite identity mismatch.' }
        if ((Get-FileHash C:\KiCadOwned\bin\kicad-cli.exe).Hash.ToLowerInvariant() -ne $expected.kicad_cli_sha256 -or (Get-FileHash C:\KonnectOwned\konnect.exe).Hash.ToLowerInvariant() -ne $expected.konnect_exe_sha256) { throw 'Konnect/KiCad runtime identity mismatch.' }
        New-Item -ItemType Directory -Path C:\HazloPackageKonnect | Out-Null
        Copy-Item -LiteralPath C:\HazloProbe\input\konnect-sources -Destination C:\HazloPackageKonnect\sources -Recurse
        $konnectSources=@(Get-ChildItem C:\HazloPackageKonnect\sources -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        if (Test-Path C:\Windows\System32\vcruntime140.dll) { throw 'Fresh guest already has the Visual C++ prerequisite; missing-runtime regression cannot be proved here.' }
        $missing=Start-Process -FilePath C:\HazloProbe\input\installed-konnect-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\KonnectOwned\konnect.exe','C:\KiCadOwned\bin\kicad-cli.exe','C:\HazloPackageKonnect\sources','C:\HazloPackageKonnect\missing-runtime' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\konnect-missing-runtime.stdout -RedirectStandardError C:\HazloProbe\output\konnect-missing-runtime.stderr
        $retainedHandle=$missing.Handle
        if (-not $missing.WaitForExit(60000)) { Stop-Process -Id $missing.Id; throw 'Missing-runtime regression timed out.' }
        if ($missing.ExitCode -ne 101 -or -not (Get-Content C:\HazloProbe\output\konnect-missing-runtime.stderr -Raw).Contains('Microsoft Visual C++')) { throw 'Missing runtime did not produce the actionable prerequisite error.' }
        $missingRuns=@(Get-ChildItem C:\HazloPackageKonnect\missing-runtime\schematic\.hazlomexa\runs -File -Filter '*.json')
        if ($missingRuns.Count -ne 1) { throw 'Missing runtime created unexpected engineering runs.' }
        $missingRun=Get-Content -LiteralPath $missingRuns[0].FullName -Raw | ConvertFrom-Json
        if ($missingRun.phase -ne 'failed' -or $missingRun.audit.files_verified -or (Test-Path (Join-Path C:\HazloPackageKonnect\missing-runtime\schematic\.hazlomexa\artifacts $missingRun.input.job_id))) { throw 'Missing runtime published or verified an engineering result.' }
        foreach ($file in $konnectSources) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Missing-runtime refusal changed a source.' } }
        $result.native_konnect_missing_runtime_refusal=$true; Save-Probe
        Copy-Item -LiteralPath C:\HazloProbe\input\vcruntime140.dll -Destination C:\KonnectOwned\vcruntime140.dll
        $konnect=Start-Process -FilePath C:\HazloProbe\input\installed-konnect-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\KonnectOwned\konnect.exe','C:\KiCadOwned\bin\kicad-cli.exe','C:\HazloPackageKonnect\sources','C:\HazloPackageKonnect\projects' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\konnect-result.json -RedirectStandardError C:\HazloProbe\output\konnect-error.log
        $retainedHandle=$konnect.Handle
        if (-not $konnect.WaitForExit(240000)) { Stop-Process -Id $konnect.Id; throw 'Native Konnect test timed out.' }
        if ($konnect.ExitCode -ne 0) { throw "Native Konnect test failed ($($konnect.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\konnect-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 4 -or $reply.schematic_jobs -ne 2 -or $reply.pcb_jobs -ne 2 -or -not $reply.recovery_verified -or $reply.hardware_mutations) { throw 'Native Konnect result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageKonnect -Destination C:\HazloProbe\output\konnect-projects -Recurse
        $konnectBefore=@(Get-ChildItem C:\HazloPackageKonnect -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_konnect_connector_workflows=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-drawing-workflow.exe) {
        $result.stage='native CADCraft workflow through installed connector'; Save-Probe
        Copy-Item -LiteralPath C:\CADCraftRO -Destination C:\CADCraftOwned -Recurse
        if ((Get-FileHash C:\CADCraftOwned\cadcraft-cli.exe).Hash.ToLowerInvariant() -ne $expected.cadcraft_cli_sha256) { throw 'CADCraft runtime identity mismatch.' }
        New-Item -ItemType Directory -Path C:\HazloPackageDrawing | Out-Null
        Copy-Item -LiteralPath C:\HazloProbe\input\drawing-sources -Destination C:\HazloPackageDrawing\sources -Recurse
        $drawing=Start-Process -FilePath C:\HazloProbe\input\installed-drawing-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\CADCraftOwned\cadcraft-cli.exe','C:\HazloPackageDrawing\sources','C:\HazloPackageDrawing\projects' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\drawing-result.json -RedirectStandardError C:\HazloProbe\output\drawing-error.log
        $retainedHandle=$drawing.Handle
        if (-not $drawing.WaitForExit(180000)) { Stop-Process -Id $drawing.Id; throw 'Native drawing test timed out.' }
        if ($drawing.ExitCode -ne 0) { throw "Native drawing test failed ($($drawing.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\drawing-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 4 -or -not $reply.recovery_verified) { throw 'Native drawing result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageDrawing\projects -Destination C:\HazloProbe\output\drawing-projects -Recurse
        $drawingBefore=@(Get-ChildItem C:\HazloPackageDrawing -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_drawing_connector_workflow=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-cadpilot-workflow.exe) {
        $result.stage='native CADPilot workflow through installed connector'; Save-Probe
        if ((Get-FileHash C:\HazloProbe\input\installed-cadpilot-workflow.exe).Hash.ToLowerInvariant() -ne $expected.cadpilot_harness_sha256) { throw 'CADPilot harness identity mismatch.' }
        Copy-Item -LiteralPath C:\FreeCADRO -Destination C:\FreeCADOwned -Recurse
        if ((Get-FileHash C:\FreeCADOwned\FreeCADCmd.exe).Hash.ToLowerInvariant() -ne $expected.freecad_cmd_sha256) { throw 'FreeCAD runtime identity mismatch.' }
        New-Item -ItemType Directory -Path C:\HazloPackageCadpilot | Out-Null
        Copy-Item -LiteralPath C:\HazloProbe\input\cadpilot-source.FCStd -Destination C:\HazloPackageCadpilot\Selected.FCStd
        Copy-Item -LiteralPath C:\HazloProbe\input\CADPilot -Destination C:\HazloPackageCadpilot\CADPilot -Recurse
        $cadpilot=Start-Process -FilePath C:\HazloProbe\input\installed-cadpilot-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\FreeCADOwned\FreeCADCmd.exe','C:\HazloPackageCadpilot\CADPilot','C:\HazloPackageCadpilot\Selected.FCStd','C:\HazloPackageCadpilot\project' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\cadpilot-result.json -RedirectStandardError C:\HazloProbe\output\cadpilot-error.log
        $retainedHandle=$cadpilot.Handle
        if (-not $cadpilot.WaitForExit(180000)) { Stop-Process -Id $cadpilot.Id; throw 'Native CADPilot test timed out.' }
        if ($cadpilot.ExitCode -ne 0) { throw "Native CADPilot test failed ($($cadpilot.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\cadpilot-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 3 -or -not $reply.recovery_verified) { throw 'Native CADPilot result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageCadpilot\project -Destination C:\HazloProbe\output\cadpilot-project -Recurse
        $cadpilotBefore=@(Get-ChildItem C:\HazloPackageCadpilot -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_cadpilot_connector_workflow=$true
    }
    if (Test-Path C:\HazloProbe\input\installed-orca-workflow.exe) {
        $result.stage='native Orca workflow through installed connector'; Save-Probe
        if ((Get-FileHash C:\HazloProbe\input\installed-orca-workflow.exe).Hash.ToLowerInvariant() -ne $expected.orca_harness_sha256) { throw 'Orca harness identity mismatch.' }
        Copy-Item -LiteralPath C:\OrcaRO -Destination C:\OrcaOwned -Recurse
        if ((Get-FileHash C:\OrcaOwned\orca-slicer.exe).Hash.ToLowerInvariant() -ne $expected.orca_cli_sha256) { throw 'Orca runtime identity mismatch.' }
        New-Item -ItemType Directory -Path C:\HazloPackageOrca | Out-Null
        Copy-Item -LiteralPath C:\HazloProbe\input\orca-sources -Destination C:\HazloPackageOrca\sources -Recurse
        $orca=Start-Process -FilePath C:\HazloProbe\input\installed-orca-workflow.exe -ArgumentList 'C:\HazloProbeApp\hazlomexa-tools.exe','C:\OrcaOwned\orca-slicer.exe','C:\OrcaOwned\resources\profiles\Prusa','C:\HazloPackageOrca\sources','C:\HazloPackageOrca\projects' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\orca-result.json -RedirectStandardError C:\HazloProbe\output\orca-error.log
        $retainedHandle=$orca.Handle
        if (-not $orca.WaitForExit(240000)) { Stop-Process -Id $orca.Id; throw 'Native Orca test timed out.' }
        if ($orca.ExitCode -ne 0) { throw "Native Orca test failed ($($orca.ExitCode))." }
        $reply=Get-Content C:\HazloProbe\output\orca-result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or $reply.completed_jobs -ne 4 -or -not $reply.recovery_verified -or $reply.printer_control) { throw 'Native Orca result did not pass.' }
        Copy-Item -LiteralPath C:\HazloPackageOrca\projects -Destination C:\HazloProbe\output\orca-projects -Recurse
        $orcaBefore=@(Get-ChildItem C:\HazloPackageOrca -Recurse -File | ForEach-Object { @{path=$_.FullName;hash=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        $result.native_orca_connector_workflow=$true
    }
    $result.stage='starting application'; Save-Probe
    if (Test-Path -LiteralPath C:\HazloProbe\input\installed-startup.mjs) {
      try {
        $result.native_ui_driver_elevated=([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))
        $env:HAZLOMEXA_TEST_PLAYWRIGHT='file:///C:/ModulesRO/index.mjs'
        $startup=Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile','-Command','"Get-Content -LiteralPath C:\HazloProbe\input\sandbox-webview-startup.ps1 -Raw | Invoke-Expression"' -WindowStyle Hidden -PassThru -RedirectStandardOutput C:\HazloProbe\output\startup-log.json -RedirectStandardError C:\HazloProbe\output\startup-error.log
        $retainedHandle=$startup.Handle
        if (-not $startup.WaitForExit(60000)) { Stop-Process -Id $startup.Id; throw 'Installed native startup test timed out.' }
        if ($startup.ExitCode -ne 0) { throw "Installed native startup test failed ($($startup.ExitCode)); elevated driver: $($result.native_ui_driver_elevated)." }
        $reply=Get-Content C:\HazloProbe\output\startup\result.json -Raw | ConvertFrom-Json
        if (-not $reply.pass -or -not $reply.no_engineering_forms -or -not $reply.no_implicit_proposal -or -not $reply.send_disabled_without_project -or $reply.program_groups -ne 7 -or $reply.agent_choices -ne 4 -or -not $reply.reload_preserves_initial_chat) { throw 'Installed initial chat UI did not pass.' }
        $result.native_release_initial_ui=$true
      } catch {
        $result.native_release_initial_ui=$false
        $result.native_release_ui_error=$_.Exception.Message
        Save-Probe
      }
    }
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
    foreach ($file in $engineeringBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed engineering project files.' } }
    foreach ($file in $drawingBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed drawing source/project files.' } }
    foreach ($file in $cadpilotBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed CADPilot source/project files.' } }
    foreach ($file in $orcaBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed Orca source/project files.' } }
    foreach ($file in $arduinoBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed Arduino project files.' } }
    foreach ($file in $konnectBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed Konnect source/project files.' } }
    foreach ($file in $agentBefore) { if ((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash) { throw 'Uninstall changed Agent source/project files.' } }
    $result.uninstalled=$true; $result.user_files_preserved=$true; $result.lifecycle_pass=$true
    $result.pass= -not (Test-Path -LiteralPath C:\HazloProbe\input\installed-startup.mjs) -or $result.native_release_initial_ui
    $result.stage=if($result.pass){'finished'}else{'finished with unverified native UI'}
} catch { $result.error=$_.Exception.Message; $result.stage='failed' }
Save-Probe
