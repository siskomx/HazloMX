"""Read-only CLI discovery in an isolated OrcaSlicer portable profile."""
import hashlib
import json
import os
import pathlib
import subprocess
import uuid

root = pathlib.Path(__file__).resolve().parents[2]
base = root / 'target/orca-probe'
identity = json.loads((base / 'identity.json').read_text(encoding='utf-8'))
runtime = pathlib.Path(identity['runtime'])
assert identity['sha256'] == 'feba3009dfb9d268779cca5758a1a5bc3b7d0722bf8fa48d5c57340de975d6be'
assert hashlib.sha256((base / 'OrcaSlicer.cpp').read_bytes()).hexdigest() == '777ec1f25d37f535af30fa9a4c549ad4334ae9efae377502fa19a2cb4794b162'
for path, digest in identity['binaries'].items():
    assert hashlib.sha256((runtime / path).read_bytes()).hexdigest() == digest
run = base / ('discovery-' + str(uuid.uuid4()))
run.mkdir()
env = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC'] if key in os.environ}
env['PATH'] = str(runtime) + os.pathsep + str(pathlib.Path(env['SystemRoot']) / 'System32')
for key in ['USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP']:
    directory = run / key.lower()
    directory.mkdir()
    env[key] = str(directory)
records = []
for option in ['--help', '--version']:
    command = [str(runtime / 'orca-slicer.exe'), option]
    result = subprocess.run(command, cwd=runtime, env=env, capture_output=True, timeout=30,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    name = option.removeprefix('--')
    (run / (name + '.stdout.log')).write_bytes(result.stdout)
    (run / (name + '.stderr.log')).write_bytes(result.stderr)
    records.append(dict(command=command, exit_code=result.returncode,
                        stdout_bytes=len(result.stdout), stderr_bytes=len(result.stderr)))
(run / 'calls.json').write_text(json.dumps(records, indent=2), encoding='utf-8')
print(json.dumps(dict(evidence=str(run), calls=records)))
