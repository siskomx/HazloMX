# Export only the closed, owned Arduino fixture and verify its published source.
$ErrorActionPreference='Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
if (Get-Process hazlomexa -ErrorAction SilentlyContinue) { throw 'Close the test app first' }
$ready=Get-Content C:\HazloProbe\output\arduino-ready.json -Raw | ConvertFrom-Json
$root=Join-Path $env:USERPROFILE Desktop\HazloGuestProject
if (-not $ready.pass -or $ready.project -ne $root) { throw 'Fixture identity mismatch' }
if ((Get-Content (Join-Path $root original.txt) -Raw).Trim() -ne 'preserve original Arduino project') { throw 'Original changed' }
$runs=@(Get-ChildItem "$root\.hazlomexa\runs" -Filter '*.json' | ForEach-Object { Get-Content $_.FullName -Raw | ConvertFrom-Json })
$completed=@($runs | Where-Object phase -eq completed)
if ($completed.Count -ne 1 -or -not $completed[0].audit.files_verified) { throw 'Expected one verified compilation' }
$job=[Guid]::Parse($completed[0].input.job_id).ToString()
$artifacts="$root\.hazlomexa\artifacts\$job"
$result=Get-Content "$artifacts\result.json" -Raw | ConvertFrom-Json
if ($result.job_id -ne $job -or $result.cli_version -ne '1.5.1' -or $result.core_version -ne '1.8.8' -or $result.fqbn -ne 'arduino:avr:uno' -or $result.flash_bytes -le 0) { throw 'Compiler result mismatch' }
foreach ($name in @('Sketch.ino','firmware.hex','firmware.elf')) {
    if ((Get-Item "$artifacts\$name").Length -le 0) { throw "Missing artifact: $name" }
}
if ([IO.File]::ReadAllText("$artifacts\Sketch.ino") -cne $completed[0].input.source) { throw 'Published source changed' }
$checkpoint='C:\HazloProbe\output\arduino-project-closed'
if (Test-Path $checkpoint) { throw 'Checkpoint already exists' }
Copy-Item -LiteralPath $root -Destination $checkpoint -Recurse
$files=@(Get-ChildItem $root -File -Recurse | ForEach-Object { @{path=$_.FullName.Substring(($root+'\').Length);sha256=(Get-FileHash $_.FullName).Hash} })
@{pass=$true;job_id=$job;result=$result;app_closed=$true;original_preserved=$true;project_files=$files} | ConvertTo-Json -Depth 8 | Set-Content C:\HazloProbe\output\arduino-checkpoint.json -Encoding UTF8
