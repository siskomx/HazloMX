# Prepare desktop-visible, owned runtime/project choices for actual release picker checks.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
$prerequisite=Get-Content C:\HazloProbe\output\result.json -Raw | ConvertFrom-Json
if (-not $prerequisite.pass -or $prerequisite.stage -ne 'finished') { throw 'Wait for prerequisite install/uninstall to finish' }
if (Get-Process hazlomexa -ErrorAction SilentlyContinue) { throw 'Fixture app already running' }
$desktop=Join-Path $env:USERPROFILE Desktop
$project=Join-Path $desktop HazloGuestProject
$data=Join-Path $desktop ArduinoData
$cli=Join-Path $desktop arduino-cli.exe
foreach ($path in @($project,$data,$cli)) { if (Test-Path -LiteralPath $path) { throw "Refusing existing fixture target: $path" } }
New-Item -ItemType Directory -Path $project | Out-Null
'preserve original Arduino project' | Set-Content (Join-Path $project original.txt) -Encoding UTF8
Copy-Item C:\ArduinoRO\arduino-cli.exe -Destination $cli
Copy-Item C:\ArduinoDataRO -Destination $data -Recurse
if ((Get-FileHash $cli -Algorithm SHA256).Hash -ne (Get-FileHash C:\ArduinoRO\arduino-cli.exe -Algorithm SHA256).Hash) { throw 'CLI copy identity mismatch' }
if (-not (Test-Path (Join-Path $data hazlomexa-toolchain.json))) { throw 'Prepared toolchain marker missing' }
$installer=Start-Process C:\HazloProbe\input\setup.exe -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -Wait -PassThru
if ($installer.ExitCode -ne 0) { throw "Install failed: $($installer.ExitCode)" }
# Runtime choices deliberately remain unset so the real native picker controls are tested.
Remove-Item Env:HAZLOMEXA_ARDUINO_CLI,Env:HAZLOMEXA_ARDUINO_DATA -ErrorAction SilentlyContinue
$app=Start-Process C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
@{pass=$true;app_pid=$app.Id;project=$project;data=$data;cli=$cli;installer_exit=$installer.ExitCode;runtime_seeded=$false} | ConvertTo-Json | Set-Content C:\HazloProbe\output\arduino-ready.json -Encoding UTF8
