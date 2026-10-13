$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (@(Get-Process hazlomexa -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq 'C:\HazloProbeApp\hazlomexa.exe' }).Count) { throw 'Close the current fixture app before restarting.' }
$env:HAZLOMEXA_FREECAD_CMD='C:\MissingFreeCAD\FreeCADCmd.exe'
if (Test-Path -LiteralPath $env:HAZLOMEXA_FREECAD_CMD) { throw 'Missing-runtime fixture unexpectedly exists' }
$app=Start-Process C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
@{app_pid=$app.Id;runtime=$env:HAZLOMEXA_FREECAD_CMD;missing=$true} | ConvertTo-Json | Set-Content C:\HazloProbe\output\missing-runtime-ready.json -Encoding UTF8
