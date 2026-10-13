"""Prepare an isolated guest for disk exhaustion or real backend/CAD checks."""
import json
import os
import shutil
import subprocess
import uuid
import xml.etree.ElementTree as ET
from pathlib import Path
import psutil
import sys

native = len(sys.argv) == 3 and sys.argv[1] == '--native-cad'
disk_cad = len(sys.argv) == 3 and sys.argv[1] == '--disk-cad'
assert native or disk_cad or len(sys.argv) == 1, 'Usage: prepare-disk-full-sandbox.py [--native-cad <FreeCADCmd.exe> | --disk-cad <FreeCADCmd.exe>]'
if native or disk_cad:
    freecad = Path(sys.argv[2]).resolve(strict=True)
    assert freecad.name.lower() == 'freecadcmd.exe'
    assert (freecad.parent/'bin/python.exe').is_file()

assert psutil.virtual_memory().available >= 3072 * 1048576, 'At least 3 GiB host memory required'
build = subprocess.run(['cargo','test','--lib','--no-run','--message-format=json'],capture_output=True,text=True,encoding='utf-8')
assert build.returncode == 0, build.stderr
artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
executable = next(item['executable'] for item in artifacts if item.get('reason') == 'compiler-artifact' and item.get('profile',{}).get('test') and item.get('executable'))
root = Path('target/sandbox-probe') / str(uuid.uuid4())
(root/'input').mkdir(parents=True); (root/'output').mkdir()
shutil.copy2(executable,root/'input/tests.exe')
shutil.copy2(Path(os.environ['SystemRoot'])/'System32/vcruntime140.dll',root/'input/vcruntime140.dll')
helper = root/'input/process-probe.exe'
subprocess.run(['rustc','tests/fixtures/process_probe.rs','-o',str(helper)],check=True,capture_output=True)
shutil.copy2('apps/desktop/tests/sandbox-native-cad.ps1' if native else 'apps/desktop/tests/sandbox-disk-full.ps1',root/'input/probe.ps1')
if disk_cad:
    shutil.copy2('apps/desktop/tests/sandbox-disk-full.ps1', root/'input/disk-full.ps1')
    shutil.copy2('apps/desktop/tests/sandbox-disk-cad.ps1', root/'input/probe.ps1')
shutil.copy2('apps/desktop/tests/sandbox-backend-suite.ps1',root/'input/backend-suite.ps1')
if native:
    shutil.copy2('apps/desktop/tests/sandbox-ascii-profiles.ps1', root/'input/ascii-profiles.ps1')
    for relative in ['tests/test_freecad.py','tests/freecad/verify-artifacts.FCMacro','apps/desktop/src-tauri/resources/freecad/create-solid.FCMacro']:
        destination=root/'input/repo'/relative
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(relative,destination)
config = ET.Element('Configuration')
for tag, value in [('MemoryInMB','2048'),('Networking','Disable'),('AudioInput','Disable'),('ClipboardRedirection','Disable')]:
    ET.SubElement(config,tag).text=value
folders=ET.SubElement(config,'MappedFolders')
for name, readonly in [('input','true'),('output','false')]:
    folder=ET.SubElement(folders,'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str((root/name).resolve())
    ET.SubElement(folder,'SandboxFolder').text='C:\\HazloProbe\\'+name
    ET.SubElement(folder,'ReadOnly').text=readonly
if native or disk_cad:
    folder=ET.SubElement(folders,'MappedFolder')
    ET.SubElement(folder,'HostFolder').text=str(freecad.parent)
    ET.SubElement(folder,'SandboxFolder').text='C:\\FreeCADRO'
    ET.SubElement(folder,'ReadOnly').text='true'
ET.SubElement(ET.SubElement(config,'LogonCommand'),'Command').text='powershell.exe -NoProfile -Command "Get-Content -LiteralPath C:\\HazloProbe\\input\\probe.ps1 -Raw | Invoke-Expression"'
ET.ElementTree(config).write(root/'probe.wsb',encoding='utf-8',xml_declaration=True)
assert ET.parse(root/'probe.wsb').findtext('Networking')=='Disable'
print(root.resolve())
