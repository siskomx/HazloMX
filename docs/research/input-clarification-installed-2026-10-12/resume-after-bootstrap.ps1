$ErrorActionPreference = 'Stop'
if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned fresh guest required.' }
if (Test-Path C:\HazloProbeApp\hazlomexa.exe) { throw 'Refusing an existing application.' }
if (Get-Process previous-setup,setup,MicrosoftEdgeWebview2Setup -ErrorAction SilentlyContinue) { throw 'A previous installer is still running.' }
if (-not (Test-Path C:\HazloProbe\output\initial-install-timeout.json)) { throw 'Initial failure receipt is missing.' }
$webview = Get-Item 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application\155.0.4283.45\msedgewebview2.exe'
$signature = Get-AuthenticodeSignature -LiteralPath $webview.FullName
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') { throw 'WebView2 signature mismatch.' }
$expected = Get-Content C:\HazloProbe\input\previous-package-identity.json -Raw | ConvertFrom-Json
$setup = 'C:\HazloProbe\input\previous-setup.exe'
if ((Get-FileHash -LiteralPath $setup).Hash.ToLowerInvariant() -ne $expected.installer_sha256) { throw 'Previous installer mismatch.' }
$process = Start-Process -FilePath $setup -ArgumentList '/S','/NS','/D=C:\HazloProbeApp' -WindowStyle Hidden -PassThru
$retainedHandle = $process.Handle
if (-not $process.WaitForExit(180000)) { Stop-Process -Id $process.Id; throw 'Baseline install timed out after prerequisite completion.' }
if ($process.ExitCode -ne 0) { throw "Baseline install failed: $($process.ExitCode)" }
foreach ($name in @('hazlomexa.exe','hazlomexa-tools.exe')) {
    $expectedHash = if ($name -eq 'hazlomexa.exe') { $expected.app_sha256 } else { $expected.helper_sha256 }
    if ((Get-FileHash -LiteralPath (Join-Path C:\HazloProbeApp $name)).Hash.ToLowerInvariant() -ne $expectedHash) { throw "Baseline binary mismatch: $name" }
}
@{pass=$true;initial_failure_retained=$true;webview_version=$webview.VersionInfo.FileVersion;webview_signature=$signature.Status.ToString();previous_installer_sha256=$expected.installer_sha256;baseline_installer_exit=$process.ExitCode;baseline_binaries_match=$true} | ConvertTo-Json | Set-Content -LiteralPath C:\HazloProbe\output\bootstrap-resume-receipt.json -Encoding UTF8
& ([scriptblock]::Create((Get-Content C:\HazloProbe\input\probe.ps1 -Raw))) -ResumeInstalled
