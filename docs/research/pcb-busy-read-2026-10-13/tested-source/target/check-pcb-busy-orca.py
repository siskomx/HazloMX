import hashlib
import json
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[1]
evidence = root / 'docs/research/pcb-busy-read-2026-10-13'
paths = ['Cargo.lock', 'apps/desktop/src-tauri/Cargo.toml', 'apps/desktop/src-tauri/src/kicad_project.rs',
         'apps/desktop/src-tauri/src/orca.rs', 'apps/desktop/src-tauri/src/orca_run.rs',
         'apps/desktop/src-tauri/src/process.rs', 'target/check-pcb-busy-orca.cmd',
         'target/check-pcb-busy-orca.py', 'docs/research/orca-runtime-2026-10-11/Cube-mm.stl']
before = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths}
build = json.loads((evidence / 'build.json').read_text(encoding='utf-8'))
for path, identity in build['files'].items():
    assert hashlib.sha256((root / path).read_bytes()).hexdigest() == identity['sha256'], path
with (evidence / 'native-orca.log').open('w', encoding='utf-8') as log:
    result = subprocess.run(['cmd', '/c', 'target\\check-pcb-busy-orca.cmd'], cwd=root,
                            stdout=log, stderr=subprocess.STDOUT)
output = (evidence / 'native-orca.log').read_text(encoding='utf-8')
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == sha for path, sha in before.items())
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == identity['sha256'] for path, identity in build['files'].items())
passed = result.returncode == 0 and 'test orca_run::tests::actual_orca_captures_profiles_and_slices_verified_files ... ok' in output and bool(re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;', output))
(evidence / 'native-orca-status.json').write_text(json.dumps({'passed': passed,
    'exit_code': result.returncode, 'pre_post_source_hashes': before,
    'scope': 'actual OrcaSlicer captured profiles, units and verified file slicing; not hardware or native UI'}, indent=2), encoding='utf-8')
assert passed, output[-3000:]
print(output[-1500:])
