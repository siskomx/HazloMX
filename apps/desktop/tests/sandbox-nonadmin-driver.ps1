$ErrorActionPreference='Stop'
trap { if(Test-Path C:\HazloProbe\output) {$_ | Out-String | Set-Content C:\HazloProbe\output\nonadmin-driver-private.log -Encoding UTF8}; exit 1 }
if($env:USERNAME -ne 'WDAGUtilityAccount' -or -not (Test-Path 'C:\HazloProbe\output\package-result.json')) {throw 'Owned logged-in guest required.'}
$completed=Get-Content C:\HazloProbe\output\package-result.json -Raw | ConvertFrom-Json
if(-not $completed.pass -or -not $completed.uninstalled -or -not $completed.user_files_preserved) {throw 'Passing completed guest lifecycle required.'}
if(Get-LocalUser -Name HazloAcceptance2 -ErrorAction SilentlyContinue) {throw 'Refusing an existing test account.'}
$owned='C:\HazloNonAdminOwned2'
if(Test-Path $owned) {throw 'Fresh ordinary-user fixture directory required.'}
New-Item -ItemType Directory $owned | Out-Null
$bytes=New-Object byte[] 32
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$password=ConvertTo-SecureString ('Ha!'+[Convert]::ToBase64String($bytes)) -AsPlainText -Force
$user=New-LocalUser -Name HazloAcceptance2 -Password $password -AccountNeverExpires -PasswordNeverExpires
Add-LocalGroupMember -SID 'S-1-5-32-545' -Member $user
$acl=Get-Acl $owned
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($user.SID,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))
Set-Acl -LiteralPath $owned -AclObject $acl
Copy-Item C:\HazloNonAdminInput "$owned\input" -Recurse
Copy-Item C:\HazloProbe\input\kicad-sources "$owned\kicad-sources" -Recurse
$credentials=[Management.Automation.PSCredential]::new("$env:COMPUTERNAME\HazloAcceptance2",$password)
$process=Start-Process powershell.exe -Credential $credentials -LoadUserProfile -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File',"$owned\input\user-test.ps1" -WorkingDirectory $owned -PassThru
$handle=$process.Handle
if(-not $process.WaitForExit(600000)) {Stop-Process -Id $process.Id; throw 'Ordinary-user fixture timeout.'}
if($process.ExitCode -ne 0) {throw 'Ordinary-user fixture failed; guest-private diagnostics retained.'}
$result=Get-Content "$owned\result.json" -Raw | ConvertFrom-Json
if(-not $result.pass -or -not $result.ordinary_nonadministrator -or -not $result.administrator_group_absent -or -not $result.uninstalled) {throw 'Ordinary-user receipt did not pass.'}
$result | ConvertTo-Json -Depth 6 | Set-Content C:\HazloProbe\output\nonadmin-result.json -Encoding UTF8
