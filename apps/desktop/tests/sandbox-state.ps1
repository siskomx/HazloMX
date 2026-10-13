# Inspect only the owned native-UI fixture inside Windows Sandbox.
$ErrorActionPreference = 'Stop'
$project = 'C:\HazloGuestProject'
$runs = @(Get-ChildItem -LiteralPath "$project\.hazlomexa\runs" -Filter '*.json' | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw | ConvertFrom-Json })
$artifacts = @(Get-ChildItem -LiteralPath "$project\.hazlomexa\artifacts" -Directory)
$state = @{
    manifest = Get-Content -LiteralPath "$project\hazlomx.json" -Raw | ConvertFrom-Json
    runs = $runs
    published_count = $artifacts.Count
    original_sha256 = (Get-FileHash -LiteralPath "$project\original.txt" -Algorithm SHA256).Hash
    artifact_files = @($artifacts | ForEach-Object { Get-ChildItem -LiteralPath $_.FullName -File | ForEach-Object { @{ name=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } } })
}
$stamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$state | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath "C:\HazloProbe\output\native-state-$stamp.json" -Encoding UTF8
$state | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath C:\HazloProbe\output\native-state-current.json -Encoding UTF8
if ($artifacts.Count -gt 0 -and -not (Test-Path C:\HazloProbe\output\guest-artifacts)) {
    New-Item -ItemType Directory -Path C:\HazloProbe\output\guest-artifacts | Out-Null
    $artifacts | ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination C:\HazloProbe\output\guest-artifacts -Recurse }
}
