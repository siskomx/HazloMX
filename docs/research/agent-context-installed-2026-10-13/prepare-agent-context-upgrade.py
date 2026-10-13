from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import psutil

previous_sha='ab43ac2a00ec0d231038dca58a3b0011e6fd77378117dbc047899b2b6a598c8e'
current_sha='9314157498049b831bddb38d1dfee5e65543886b0986abba7afcdad83298970a'
old=Path('target/package-history')/previous_sha/'setup.exe'
identity=Path('docs/research/input-clarification-installed-2026-10-12/package-identity.json')
previous=json.loads(identity.read_text(encoding='utf-8-sig'))
assert previous['installer_sha256']==previous_sha
assert hashlib.sha256(old.read_bytes()).hexdigest()==previous_sha
available=psutil.virtual_memory().available//1048576
if available<3072:
    raise SystemExit(f'Guest requires 3072 MiB available; observed {available} MiB. No guest prepared or started.')
prepared=subprocess.run([sys.executable,'apps/desktop/tests/prepare-current-package.py','--online'],capture_output=True,text=True)
if prepared.returncode:
    raise SystemExit(prepared.stderr.strip() or prepared.stdout)
root=Path(prepared.stdout.strip()).resolve(strict=True)
assert root.is_relative_to(Path('target/sandbox-probe').resolve())
current=json.loads((root/'input/package-identity.json').read_text(encoding='utf-8'))
assert current['installer_sha256']==current_sha
shutil.copy2(old,root/'input/previous-setup.exe')
shutil.copy2(identity,root/'input/previous-package-identity.json')
for name in ['installed-startup.mjs','sandbox-webview-startup.ps1']:
    (root/'input'/name).unlink()
(root/'fixture-scope.json').write_text(json.dumps({
    'scope':'Current installed connector, native backend operations, upgrade/startup/uninstall preservation',
    'included':['KiCad','CADCraft','CADPilot','OrcaSlicer','Arduino CLI','Arduino Agent','Konnect schematic','Konnect PCB'],
    'omitted':['Native UI driver and engineering/picker/chat input','New-solid FreeCAD execution, retained separately'],
    'native_ui_input_stopped':True,'model_inference':False,'hardware_control':False,
    'previous_installer_sha256':previous_sha,'current_installer_sha256':current_sha,
    'available_host_mib_at_preparation':available,
},indent=2),encoding='utf-8')
Path('target/agent-context-upgrade-case.txt').write_text(str(root),encoding='utf-8')
print(root)
