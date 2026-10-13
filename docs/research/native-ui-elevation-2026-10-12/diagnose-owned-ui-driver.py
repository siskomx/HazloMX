import base64
import subprocess
script = r'''
$ErrorActionPreference='Stop'
if($env:USERNAME -ne 'WDAGUtilityAccount'){throw 'Owned Sandbox only'}
@{policy=(Get-ExecutionPolicy);elevated=([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator));driver_live=@(Get-CimInstance Win32_Process -Filter "Name = 'powershell.exe' OR Name = 'node.exe'" | Where-Object {$_.ProcessId -ne $PID -and ($_.CommandLine -like '*HazloProbeUiDriver*' -or $_.CommandLine -like '*installed-startup.mjs*')} | Select-Object ProcessId,ParentProcessId,Name)} | ConvertTo-Json -Depth 4 | Set-Content C:\HazloProbe\output\ui-prerequisites.json -Encoding UTF8
if(Test-Path C:\HazloProbe\output\ui-driver-result.json){throw 'Refusing previous driver result'}
$shell=New-Object -ComObject Shell.Application
$shell.ShellExecute('powershell.exe','-NoProfile -WindowStyle Hidden -Command "Get-Content -LiteralPath C:\HazloProbeUiDriver.ps1 -Raw | Invoke-Expression"','C:\','open',0)
'''
encoded=base64.b64encode(script.encode('utf-16-le')).decode()
completed=subprocess.run(['wsb','exec','--id','e8cad335-79c8-4830-a06f-e48447a3a7b0','--run-as','ExistingLogin','--command','powershell.exe -NoProfile -EncodedCommand '+encoded,'--raw'],capture_output=True,text=True,timeout=30)
print(completed.stdout)
print(completed.stderr)
raise SystemExit(completed.returncode)
