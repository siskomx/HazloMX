# Test the debug instrumentation of the same desktop commands in an owned clean guest.
$ErrorActionPreference='Stop'
@{pass=$false;stage='starting native test'} | ConvertTo-Json | Set-Content 'C:\HazloProbe\output\native-suite-result.json' -Encoding UTF8
try {
    if ($env:USERNAME -ne 'WDAGUtilityAccount') { throw 'Owned Windows Sandbox required.' }
    if (-not (Test-Path 'C:\HazloProbe\output\result.json')) { throw 'Run the installer prerequisite probe first.' }
    $prerequisite=Get-Content 'C:\HazloProbe\output\result.json' -Raw | ConvertFrom-Json
    if (-not $prerequisite.pass) { throw 'WebView2/installer prerequisite did not pass.' }
    $env:HAZLOMEXA_TEST_PLAYWRIGHT='file:///C:/ModulesRO/index.mjs'
    $env:HAZLOMEXA_TEST_CONTRACT_FIXTURE='C:\HazloProbe\input\contract-fixture.ts'
    $node=Start-Process 'C:\NodeRO\node.exe' -ArgumentList 'C:\HazloProbe\input\native-smoke.mjs','C:\HazloProbe\input\hazlomexa-debug.exe','C:\FreeCADRO\FreeCADCmd.exe','--missing-model' -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput 'C:\HazloProbe\output\native-smoke.log' -RedirectStandardError 'C:\HazloProbe\output\native-smoke-error.log'
    $line=Get-Content 'C:\HazloProbe\output\native-smoke.log' -Encoding UTF8 | Where-Object { $_ -like 'Evidence retained at *' } | Select-Object -Last 1
    if (-not $line) { throw 'Native test did not identify its evidence folder.' }
    $root=[IO.Path]::GetFullPath($line.Substring('Evidence retained at '.Length))
    $temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $root.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($root) -notlike '*native-*') { throw 'Unexpected evidence path.' }
    # Project and WebView2 data must live on guest NTFS, not a redirected mapped folder.
    $dest=Join-Path 'C:\HazloProbe\output' ('native-evidence-'+[Guid]::NewGuid())
    New-Item -ItemType Directory -Path $dest | Out-Null
    foreach ($name in @('project','result.json','failure.json','completed.png')) {
        $source=Join-Path $root $name
        if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination $dest -Recurse }
    }
    if ($node.ExitCode -ne 0) { throw "Native test exited $($node.ExitCode); evidence exported to $dest." }
    $result=Get-Content -LiteralPath (Join-Path $dest 'result.json') -Raw | ConvertFrom-Json
    if (-not $result.pass) { throw 'Native smoke did not pass.' }
    @{pass=$true;scope='Debug instrumented native webviews with real CAD, not installed release UI';result=$result;evidence=$dest} | ConvertTo-Json -Depth 10 | Set-Content 'C:\HazloProbe\output\native-suite-result.json' -Encoding UTF8
} catch {
    @{pass=$false;error=$_.Exception.Message} | ConvertTo-Json | Set-Content 'C:\HazloProbe\output\native-suite-result.json' -Encoding UTF8
    exit 1
}
