"""Run identified CADPilot primitives using the already provisioned FreeCAD runtime."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import uuid

ROOT = Path(__file__).resolve().parents[2]
COMMIT = '58bfee4d45061e55c158a5fd4786ab98a427f7f6'
source = ROOT / 'target/cadpilot-probe/source' / ('cadpilot-' + COMMIT)
runtime = Path(os.environ['LOCALAPPDATA']) / 'HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311'
executable = runtime / 'FreeCADCmd.exe'
assert executable.is_file()
output = ROOT / 'target/cadpilot-probe' / ('geometry-' + str(uuid.uuid4()))
output.mkdir()
addon = output / 'addon/rpc_server'
addon.mkdir(parents=True)
modules = {}
for name in ['__init__.py', 'geometry_query.py', 'object_factory.py', 'property_mapper.py', 'tip_policy.py']:
    original = source / 'addon/CADPilot/rpc_server' / name
    content = original.read_bytes()
    modules[name] = hashlib.sha256(content).hexdigest()
    (addon / name).write_bytes(content)
(output / 'input.json').write_text(json.dumps(dict(modules=modules)), encoding='utf-8')
macro = output / 'geometry-gate.FCMacro'
shutil.copyfile(ROOT / 'tests/cadpilot/geometry-gate.FCMacro', macro)
profile = output / 'profile'
profile.mkdir()
environment = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC'] if key in os.environ}
environment['PATH'] = str(runtime / 'bin') + ';' + os.environ['SystemRoot'] + '/System32'
for key, name in [('FREECAD_USER_HOME', 'home'), ('FREECAD_USER_DATA', 'data'), ('FREECAD_USER_TEMP', 'temp')]:
    path = profile / name
    path.mkdir()
    environment[key] = str(path)
for key in ['USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP']:
    environment[key] = str(profile)
command = [str(executable), '--user-cfg', str(profile/'user.cfg'), '--system-cfg', str(profile/'system.cfg'), str(macro)]
completed = subprocess.run(command, cwd=output, env=environment, stdin=subprocess.DEVNULL, capture_output=True, timeout=120)
(output / 'stdout.log').write_bytes(completed.stdout)
(output / 'stderr.log').write_bytes(completed.stderr)
result_path = output / 'result.json'
passed = completed.returncode == 0 and result_path.is_file() and json.loads(result_path.read_text())['passed'] is True
roundtrip = None
if passed:
    check_macro = output / 'verify-roundtrip.FCMacro'
    shutil.copyfile(ROOT / 'tests/cadpilot/verify-roundtrip.FCMacro', check_macro)
    artifact_names = ['Original.FCStd', 'Unselected.FCStd', 'Edited.FCStd', 'Edited.step']
    before = {name: hashlib.sha256((output/name).read_bytes()).hexdigest() for name in artifact_names}
    check_command = [str(executable), '--user-cfg', str(profile/'check-user.cfg'), '--system-cfg', str(profile/'check-system.cfg'), str(check_macro)]
    verified = subprocess.run(check_command, cwd=output, env=environment, stdin=subprocess.DEVNULL, capture_output=True, timeout=120)
    (output/'roundtrip-stdout.log').write_bytes(verified.stdout)
    (output/'roundtrip-stderr.log').write_bytes(verified.stderr)
    checked = output/'roundtrip.json'
    roundtrip = dict(exit_code=verified.returncode, command=check_command,
                    passed=verified.returncode == 0 and checked.is_file() and json.loads(checked.read_text())['passed'] is True,
                    files_unchanged=before == {name: hashlib.sha256((output/name).read_bytes()).hexdigest() for name in artifact_names})
    passed = roundtrip['passed'] and roundtrip['files_unchanged']
record = dict(passed=passed, exit_code=completed.returncode, command=command, cadpilot_commit=COMMIT,
              roundtrip=roundtrip,
              module_hashes=modules, executable_sha256=hashlib.sha256(executable.read_bytes()).hexdigest(),
              macro_sha256=hashlib.sha256(macro.read_bytes()).hexdigest(),
              artifact_hashes={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in output.glob('*') if p.is_file()})
(output / 'status.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
assert passed, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
