$ErrorActionPreference='Stop'
if($env:USERNAME -ne 'WDAGUtilityAccount'){throw 'Disposable Windows Sandbox required.'}
$expected=Get-Content C:\HazloProbe\input\package-identity.json -Raw | ConvertFrom-Json
if((Get-FileHash C:\HazloProbeApp\hazlomexa.exe).Hash.ToLowerInvariant() -ne $expected.app_sha256){throw 'Installed app identity mismatch.'}
$policyBase='HKLM:\Software\Policies\Microsoft\Edge\WebView2'
$appName='hazlomexa.exe'
$profile=Join-Path ([IO.Path]::GetTempPath()) ('Hazlo-guest-webview-'+[guid]::NewGuid())
$listener=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0)
$listener.Start(); $port=$listener.LocalEndpoint.Port; $listener.Stop()
$policies=@(
    @{path="$policyBase\AdditionalBrowserArguments";value="--remote-debugging-port=$port"},
    @{path="$policyBase\UserDataFolder";value=$profile}
)
foreach($policy in $policies){
    if((Test-Path $policy.path) -and (Get-Item $policy.path).GetValueNames().Contains($appName)){throw 'Refusing an existing app-specific WebView policy.'}
}
$receipt=@{app=$appName;port=$port;profile=$profile;exit_code=1;policy_values_removed=$false}
$created=@()
try {
    New-Item -ItemType Directory -Path $profile | Out-Null
    foreach($policy in $policies){
        if(-not (Test-Path $policy.path)){New-Item -Path $policy.path -Force | Out-Null}
        New-ItemProperty -Path $policy.path -Name $appName -PropertyType String -Value $policy.value | Out-Null
        $created+=$policy.path
    }
    $env:HAZLOMEXA_TEST_PLAYWRIGHT='file:///C:/ModulesRO/index.mjs'
    & C:\NodeRO\node.exe C:\HazloProbe\input\installed-startup.mjs C:\HazloProbeApp\hazlomexa.exe C:\HazloProbe\output\startup "--sandbox-policy-port=$port"
    $receipt.exit_code=$LASTEXITCODE
    $receipt.private_profile_used=(@(Get-ChildItem -LiteralPath $profile -Force).Count -gt 0)
} finally {
    foreach($policyPath in $created){Remove-ItemProperty -LiteralPath $policyPath -Name $appName}
    $receipt.policy_values_removed= -not (@($policies | Where-Object {(Test-Path $_.path) -and (Get-Item $_.path).GetValueNames().Contains($appName)}).Count)
    $receipt | ConvertTo-Json | Set-Content C:\HazloProbe\output\webview-policy-receipt.json -Encoding UTF8
}
if(-not $receipt.policy_values_removed -or -not $receipt.private_profile_used -or $receipt.exit_code -ne 0){throw 'Native UI test, private profile or guest policy cleanup failed.'}
