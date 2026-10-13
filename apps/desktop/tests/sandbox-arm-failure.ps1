param([Parameter(Mandatory=$true)][ValidateSet('script-output','publication')][string]$Mode)
# Fault only an owned guest run; never modify the fixed macro or existing artifacts.
$ErrorActionPreference='Stop'
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned guest required' }
    $root='C:\HazloGuestProject'
    $pending=@(Get-ChildItem -LiteralPath "$root\.hazlomexa\runs" -Filter '*.json' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json } | Where-Object phase -eq 'awaiting_approval')
    if ($pending.Count -ne 1) { throw 'Expected one pending approval' }
    $job=[Guid]::Parse($pending[0].input.job_id).ToString()
    $target="$root\.hazlomexa\artifacts\$job"
    if ($Mode -eq 'script-output') {
        $target="$root\.hazlomexa\staging\$job\solid.FCStd"
        $deadline=[DateTime]::UtcNow.AddSeconds(60)
        while (-not (Test-Path -LiteralPath "$root\.hazlomexa\staging\$job\input.json")) {
            if ([DateTime]::UtcNow -gt $deadline) { throw 'Approved run did not reach script input' }
            Start-Sleep -Milliseconds 10
        }
    }
    if (Test-Path -LiteralPath $target) { throw 'Fault target already exists' }
    New-Item -ItemType Directory -Path $target -ErrorAction Stop | Out-Null
    [IO.File]::WriteAllText("$target\fixture-original.txt", 'preserve fault sentinel')
    @{pass=$true;mode=$Mode;job_id=$job;target=$target;sentinel_sha256=(Get-FileHash -LiteralPath "$target\fixture-original.txt" -Algorithm SHA256).Hash} | ConvertTo-Json | Set-Content "C:\HazloProbe\output\fault-$Mode.json" -Encoding UTF8
} catch {
    @{pass=$false;mode=$Mode;error=$_.Exception.Message} | ConvertTo-Json | Set-Content "C:\HazloProbe\output\fault-$Mode.json" -Encoding UTF8
    exit 1
}
