$ErrorActionPreference = 'Stop'
Get-AuthenticodeSignature "$PSScriptRoot/../../target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/*.exe" |
    ForEach-Object {
        [pscustomobject]@{
            Path = $_.Path
            Status = $_.Status.ToString()
            Subject = $_.SignerCertificate.Subject
            Thumbprint = $_.SignerCertificate.Thumbprint
        }
    } | ConvertTo-Json | Set-Content "$PSScriptRoot/../../target/cadcraft-probe/signatures.json"
