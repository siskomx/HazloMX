$ErrorActionPreference='Stop'
if($env:USERNAME -ne 'WDAGUtilityAccount'){throw 'Owned guest required'}
$output='C:\HazloProbe\output'
$result=Get-Content "$output\package-result.json" -Raw|ConvertFrom-Json
$expected=Get-Content C:\HazloProbe\input\package-identity.json -Raw|ConvertFrom-Json
if($result.stage -ne 'failed' -or -not $result.native_kicad_connector_workflow -or -not $result.native_drawing_connector_workflow){throw 'Unexpected partial gate'}
foreach($name in @('hazlomexa.exe','hazlomexa-tools.exe')){
  $expectedHash=if($name -eq 'hazlomexa.exe'){$expected.app_sha256}else{$expected.helper_sha256}
  if((Get-FileHash (Join-Path C:\HazloProbeApp $name)).Hash.ToLowerInvariant() -ne $expectedHash){throw 'Installed binary mismatch'}
}
$data=Join-Path $env:LOCALAPPDATA 'mx.hazlomexa.desktop\preserve-package-test.txt'
$paths=@('C:\HazloPackageProject\user-source.txt','C:\HazloPackageProject\hazlomx.json',$data)
$paths+=@(Get-ChildItem C:\HazloPackageEngineering,C:\HazloPackageDrawing -Recurse -File|ForEach-Object FullName)
$before=@($paths|ForEach-Object {@{path=$_;hash=(Get-FileHash -LiteralPath $_).Hash}})
$app=Start-Process C:\HazloProbeApp\hazlomexa.exe -WindowStyle Hidden -PassThru
$appHandle=$app.Handle
Start-Sleep -Seconds 5
$app.Refresh();if($app.HasExited){throw 'Basic startup failed'}
Stop-Process -Id $app.Id;$app.WaitForExit(15000)|Out-Null
$uninstaller=Start-Process C:\HazloProbeApp\uninstall.exe -ArgumentList '/S' -WindowStyle Hidden -PassThru
$uninstallerHandle=$uninstaller.Handle
if(-not $uninstaller.WaitForExit(60000)){throw 'Uninstall timeout'}
if($uninstaller.ExitCode -ne 0){throw 'Uninstall failed'}
$registry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hazlo Mexa'
$deadline=(Get-Date).AddSeconds(30)
while(((Test-Path C:\HazloProbeApp\hazlomexa.exe) -or (Test-Path $registry)) -and (Get-Date) -lt $deadline){Start-Sleep -Milliseconds 100}
if((Test-Path C:\HazloProbeApp\hazlomexa.exe) -or (Test-Path C:\HazloProbeApp\hazlomexa-tools.exe) -or (Test-Path $registry)){throw 'Uninstall incomplete'}
foreach($file in $before){if((Get-FileHash -LiteralPath $file.path).Hash -ne $file.hash){throw 'Engineering/user file changed'}}
$result|Add-Member app_alive $true -Force
$result|Add-Member uninstalled $true -Force
$result|Add-Member user_files_preserved $true -Force
$result|Add-Member lifecycle_pass $true -Force
$result|Add-Member native_release_initial_ui $false -Force
$result|Add-Member completion_driver 'separate owned lifecycle continuation after terminal UI-test failure' -Force
$result.stage='finished with unverified native UI';$result.pass=$false
$result|ConvertTo-Json -Depth 8|Set-Content "$output\package-result.json" -Encoding UTF8
$before|ConvertTo-Json|Set-Content "$output\uninstall-preserved-files.json" -Encoding UTF8
