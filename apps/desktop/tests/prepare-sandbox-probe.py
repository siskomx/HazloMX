"""Prepare an isolated Windows Sandbox installer check; launch it explicitly."""
from pathlib import Path
import shutil
import uuid
import xml.etree.ElementTree as ET
import psutil
import sys
import os

native = len(sys.argv) == 3 and sys.argv[1] == '--native-smoke'
arduino = len(sys.argv) == 3 and sys.argv[1] == '--arduino-smoke'
assert native or arduino or sys.argv[1:] in ([], ['--offline-webview']), 'Usage: prepare-sandbox-probe.py [--offline-webview | --native-smoke <FreeCADCmd.exe> | --arduino-smoke <owned-probe-root>]'
offline = '--offline-webview' in sys.argv
if native:
    freecad = Path(sys.argv[2]).resolve(strict=True)
    assert freecad.name.lower() == 'freecadcmd.exe'
    assert Path('target/debug/hazlomexa.exe').is_file(), 'Build the current debug desktop app first'
if arduino:
    import json
    arduino_root = Path(sys.argv[2]).resolve(strict=True)
    assert json.loads((arduino_root/'runtime-result.json').read_text())['pass']
    assert (arduino_root/'runtime/arduino-cli.exe').is_file()
    assert (arduino_root/'data/hazlomexa-toolchain.json').is_file()

available_mb = psutil.virtual_memory().available // 1048576
if available_mb < 3072:
    raise SystemExit(f"Sandbox probe requires at least 3072 MiB available host memory; observed {available_mb} MiB. No test configuration prepared.")

root = Path("target/sandbox-probe") / str(uuid.uuid4())
(root / "input").mkdir(parents=True)
(root / "output").mkdir()
shutil.copy2("target/release/bundle/nsis/Hazlo Mexa_0.1.0_x64-setup.exe", root / "input/setup.exe")
shutil.copy2("apps/desktop/tests/sandbox-offline-webview.ps1" if offline else "apps/desktop/tests/sandbox-probe.ps1", root / "input/probe.ps1")
if not offline:
    shutil.copy2('apps/desktop/tests/sandbox-start-manual.ps1', root/'input/start-manual.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-watch-cad.ps1', root/'input/watch-cad.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-start-missing-runtime.ps1', root/'input/start-missing-runtime.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-arm-failure.ps1', root/'input/arm-failure.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-check-failures.ps1', root/'input/check-failures.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-arm-publication.ps1', root/'input/arm-publication.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-publication-held.ps1', root/'input/publication-held.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-release-publication.ps1', root/'input/release-publication.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-check-publication.ps1', root/'input/check-publication.ps1')
if arduino:
    shutil.copy2('apps/desktop/tests/sandbox-start-arduino.ps1', root/'input/start-arduino.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-check-arduino.ps1', root/'input/check-arduino.ps1')
if native:
    for source, name in [('target/debug/hazlomexa.exe','hazlomexa-debug.exe'),
                         ('apps/desktop/tests/native-smoke.mjs','native-smoke.mjs'),
                         ('apps/desktop/src/contract-fixture.ts','contract-fixture.ts'),
                         ('apps/desktop/tests/sandbox-native-smoke.ps1','native-smoke.ps1')]:
        shutil.copy2(source, root/'input'/name)
    shutil.copy2(Path(os.environ['SystemRoot'])/'System32/vcruntime140.dll', root/'input/vcruntime140.dll')
configuration = ET.Element("Configuration")
if offline:
    ET.SubElement(configuration, "Networking").text = "Disable"
ET.SubElement(configuration, "MemoryInMB").text = "2048"
ET.SubElement(configuration, "AudioInput").text = "Disable"
ET.SubElement(configuration, "ClipboardRedirection").text = "Disable"
folders = ET.SubElement(configuration, "MappedFolders")
for name, readonly in [("input", "true"), ("output", "false")]:
    folder = ET.SubElement(folders, "MappedFolder")
    ET.SubElement(folder, "HostFolder").text = str((root / name).resolve())
    ET.SubElement(folder, "SandboxFolder").text = "C:\\HazloProbe\\" + name
    ET.SubElement(folder, "ReadOnly").text = readonly
if native:
    for host, guest in [(Path(shutil.which('node')).parent,'C:\\NodeRO'),
                        (Path('apps/desktop/node_modules/playwright-core').resolve(),'C:\\ModulesRO'),
                        (freecad.parent,'C:\\FreeCADRO')]:
        folder = ET.SubElement(folders, 'MappedFolder')
        ET.SubElement(folder, 'HostFolder').text = str(host)
        ET.SubElement(folder, 'SandboxFolder').text = guest
        ET.SubElement(folder, 'ReadOnly').text = 'true'
if arduino:
    for host, guest in [(arduino_root/'runtime', 'C:\\ArduinoRO'), (arduino_root/'data', 'C:\\ArduinoDataRO')]:
        folder = ET.SubElement(folders, 'MappedFolder')
        ET.SubElement(folder, 'HostFolder').text = str(host)
        ET.SubElement(folder, 'SandboxFolder').text = guest
        ET.SubElement(folder, 'ReadOnly').text = 'true'
ET.SubElement(ET.SubElement(configuration, "LogonCommand"), "Command").text = (
    'powershell.exe -NoProfile -Command "Get-Content -LiteralPath C:\\HazloProbe\\input\\probe.ps1 -Raw | Invoke-Expression"'
)
ET.ElementTree(configuration).write(root / "probe.wsb", encoding="utf-8", xml_declaration=True)
assert ET.parse(root / "probe.wsb").findtext("MappedFolders/MappedFolder/ReadOnly") == "true"
assert not offline or ET.parse(root / "probe.wsb").findtext("Networking") == "Disable"
print(root.resolve())
