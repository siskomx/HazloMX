"""Prepare a fresh owned Sandbox package/connector lifecycle gate."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET

assert sys.argv[1:] in ([], ['--online']), 'Usage: prepare-current-package.py [--online]'
online = sys.argv[1:] == ['--online']

prepared = subprocess.run([sys.executable, 'apps/desktop/tests/prepare-sandbox-probe.py', '--offline-webview'], check=True, capture_output=True, text=True)
root = Path(prepared.stdout.strip()).resolve(strict=True)
assert root.is_relative_to(Path('target/sandbox-probe').resolve())
if online:
    tree=ET.parse(root/'probe.wsb')
    tree.getroot().find('Networking').text='Enable'
    tree.write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
    shutil.copy2('apps/desktop/tests/installed-startup.mjs',root/'input/installed-startup.mjs')
    shutil.copy2('apps/desktop/tests/sandbox-webview-startup.ps1',root/'input/sandbox-webview-startup.ps1')
    for host,guest in [(Path(shutil.which('node')).resolve().parent,'C:\\NodeRO'),(Path('apps/desktop/node_modules/playwright-core').resolve(strict=True),'C:\\ModulesRO')]:
        folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
        ET.SubElement(folder,'HostFolder').text=str(host)
        ET.SubElement(folder,'SandboxFolder').text=guest
        ET.SubElement(folder,'ReadOnly').text='true'
    tree.write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
shutil.copy2('apps/desktop/tests/sandbox-current-package.ps1', root/'input/probe.ps1')
shutil.copy2('target/debug/examples/installed_tools_protocol.exe', root/'input/installed-tools-protocol.exe')
if online:
    shutil.copy2('target/debug/examples/installed_kicad_workflow.exe',root/'input/installed-kicad-workflow.exe')
    shutil.copy2('target/debug/examples/installed_drawing_workflow.exe',root/'input/installed-drawing-workflow.exe')
    shutil.copy2('target/debug/examples/installed_cadpilot_workflow.exe',root/'input/installed-cadpilot-workflow.exe')
    shutil.copy2('target/debug/examples/installed_orca_workflow.exe',root/'input/installed-orca-workflow.exe')
    shutil.copy2('target/debug/examples/installed_arduino_workflow.exe',root/'input/installed-arduino-workflow.exe')
    shutil.copy2('target/debug/examples/installed_konnect_workflow.exe',root/'input/installed-konnect-workflow.exe')
    shutil.copy2('target/debug/examples/installed_agent_workflow.exe',root/'input/installed-agent-workflow.exe')
    shutil.copy2('apps/desktop/tests/sandbox-agent-workflow.ps1',root/'input/sandbox-agent-workflow.ps1')
    (root/'input/agent-original.ino').write_text('void setup() {} void loop() {}',encoding='utf-8')
    shutil.copytree('docs/research/konnect-runtime-2026-10-11/raw/Original',root/'input/konnect-sources')
    (root/'input/orca-sources').mkdir()
    shutil.copy2('docs/research/orca-runtime-2026-10-11/Cube-mm.stl',root/'input/orca-sources/metric.stl')
    shutil.copy2('docs/research/orca-profile-guidance-2026-10-12/native-inch/Original/Original.stl',root/'input/orca-sources/imperial.stl')
    shutil.copy2('docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd',root/'input/cadpilot-source.FCStd')
    shutil.copytree('target/cadpilot-probe/source/cadpilot-58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot',root/'input/CADPilot')
    (root/'input/drawing-sources').mkdir()
    for name in ['metric.dxf','imperial.dxf']:
        shutil.copy2(Path('docs/research/cadcraft-runtime-2026-10-11')/name,root/'input/drawing-sources'/name)
    (root/'input/kicad-sources').mkdir()
    for name in ['basic_test.kicad_sch','basic_test.kicad_pcb','basic_test.kicad_pro','custom_ds.kicad_wks']:
        shutil.copy2(Path('target/kicad-probe/fixtures')/name,root/'input/kicad-sources'/name)
    tree=ET.parse(root/'probe.wsb')
    folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str(Path('target/kicad-probe/runtime').resolve(strict=True))
    ET.SubElement(folder,'SandboxFolder').text='C:\\KiCadRO'
    ET.SubElement(folder,'ReadOnly').text='true'
    folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str(Path('target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable').resolve(strict=True))
    ET.SubElement(folder,'SandboxFolder').text='C:\\CADCraftRO'
    ET.SubElement(folder,'ReadOnly').text='true'
    folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str((Path(os.environ['LOCALAPPDATA'])/'HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311').resolve(strict=True))
    ET.SubElement(folder,'SandboxFolder').text='C:\\FreeCADRO'
    ET.SubElement(folder,'ReadOnly').text='true'
    folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str(Path('target/orca-probe/runtime').resolve(strict=True))
    ET.SubElement(folder,'SandboxFolder').text='C:\\OrcaRO'
    ET.SubElement(folder,'ReadOnly').text='true'
    tree.write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
    agent_package=json.loads(Path('target/arduino-agent-probe/package-result.json').read_text())
    assert agent_package['pass'] and agent_package['tag']=='v0.7.1' and not agent_package['launched']
    agent_folder=Path(agent_package['executable']).resolve(strict=True).parent
    folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str(agent_folder)
    ET.SubElement(folder,'SandboxFolder').text='C:\\AgentRO'
    ET.SubElement(folder,'ReadOnly').text='true'
    tree.write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
    for host,guest in [(Path('target/arduino-probe/runtime').resolve(strict=True),'C:\\ArduinoRuntimeRO'),(Path('target/arduino-probe/data').resolve(strict=True),'C:\\ArduinoDataRO'),(Path('target/konnect-probe/runtime').resolve(strict=True),'C:\\KonnectRO')]:
        folder=ET.SubElement(tree.getroot().find('MappedFolders'),'MappedFolder')
        ET.SubElement(folder,'HostFolder').text=str(host)
        ET.SubElement(folder,'SandboxFolder').text=guest
        ET.SubElement(folder,'ReadOnly').text='true'
    tree.write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
shutil.copy2(Path(os.environ['SystemRoot'])/'System32/vcruntime140.dll', root/'input/vcruntime140.dll')
def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()
app = Path('target/release/hazlomexa.exe').read_bytes()
marker = b'__TAURI_BUNDLE_TYPE_VAR_UNK'
assert app.count(marker) == 1, 'Unexpected Tauri bundle marker'
bundled_app = app.replace(marker, b'__TAURI_BUNDLE_TYPE_VAR_NSS')
identity = {
    'installer_sha256': digest(root/'input/setup.exe'),
    'app_sha256': hashlib.sha256(bundled_app).hexdigest(),
    'unbundled_app_sha256': hashlib.sha256(app).hexdigest(),
    'bundle_marker': 'NSS',
    'helper_sha256': digest('target/release/hazlomexa-tools.exe'),
    'harness_sha256': digest(root/'input/installed-tools-protocol.exe'),
    'version': '0.1.0',
    'tool_count': 15,
    'networking': 'enabled' if online else 'disabled',
}
if online:
    identity['cadcraft_cli_sha256']=digest('target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe')
    identity['drawing_harness_sha256']=digest(root/'input/installed-drawing-workflow.exe')
    identity['cadpilot_harness_sha256']=digest(root/'input/installed-cadpilot-workflow.exe')
    identity['freecad_cmd_sha256']=digest(Path(os.environ['LOCALAPPDATA'])/'HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe')
    identity['orca_cli_sha256']=digest('target/orca-probe/runtime/orca-slicer.exe')
    identity['orca_harness_sha256']=digest(root/'input/installed-orca-workflow.exe')
    identity['arduino_cli_sha256']=digest('target/arduino-probe/runtime/arduino-cli.exe')
    identity['arduino_harness_sha256']=digest(root/'input/installed-arduino-workflow.exe')
    identity['kicad_cli_sha256']=digest('target/kicad-probe/runtime/bin/kicad-cli.exe')
    identity['konnect_exe_sha256']=digest('target/konnect-probe/runtime/konnect.exe')
    identity['konnect_harness_sha256']=digest(root/'input/installed-konnect-workflow.exe')
    identity['agent_exe_sha256']=digest(agent_folder/'Arduino Agent.exe')
    identity['agent_bridge_sha256']=digest(agent_folder/'resources/mcp-bridge/bridge/arduino-agent-bridge.js')
    assert identity['agent_exe_sha256']==agent_package['executable_sha256'] and identity['agent_bridge_sha256']==agent_package['bridge_sha256']
    identity['agent_harness_sha256']=digest(root/'input/installed-agent-workflow.exe')
    identity['konnect_vcruntime_sha256']=digest(root/'input/vcruntime140.dll')
    signature=subprocess.run([shutil.which('pwsh') or 'powershell.exe','-NoProfile','-Command',
        "$ErrorActionPreference='Stop'; $sig=Get-AuthenticodeSignature -LiteralPath $env:HAZLOMEXA_TEST_VCRUNTIME; @{status=[string]$sig.Status;signer=$sig.SignerCertificate.Subject;version=(Get-Item -LiteralPath $env:HAZLOMEXA_TEST_VCRUNTIME).VersionInfo.FileVersion} | ConvertTo-Json"],
        env={**os.environ,'HAZLOMEXA_TEST_VCRUNTIME':str(root/'input/vcruntime140.dll')},capture_output=True,text=True,check=True)
    identity['konnect_vcruntime_identity']=json.loads(signature.stdout)
    assert identity['konnect_vcruntime_identity']['status']=='Valid' and 'O=Microsoft Corporation' in identity['konnect_vcruntime_identity']['signer'], 'Verified Microsoft Visual C++ prerequisite required'
(root/'input/package-identity.json').write_text(json.dumps(identity,indent=2),encoding='utf-8')
print(root)
