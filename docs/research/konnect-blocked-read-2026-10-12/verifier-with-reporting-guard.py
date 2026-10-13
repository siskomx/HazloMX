"""Verify native Konnect's full tool deadline and cancellation at a held file read."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import uuid

root = Path(__file__).resolve().parents[2]
output = root / 'target/konnect-probe' / ('blocked-read-' + str(uuid.uuid4()))
output.mkdir()
paths = ['Cargo.lock', 'apps/desktop/src-tauri/src/konnect_run.rs', 'apps/desktop/src-tauri/src/process.rs',
         'apps/desktop/src-tauri/src/kicad_project.rs', 'apps/desktop/src-tauri/src/kicad_run.rs',
         'tests/fixtures/publication_barrier.rs', 'tests/konnect/verify_blocked_read.py']
source_hashes = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths}
runtime = ['target/konnect-probe/runtime/konnect.exe', 'target/kicad-probe/runtime/bin/kicad-cli.exe']
runtime_hashes = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in runtime}
command = ['cargo', 'test', '-p', 'hazlomexa', '--lib', 'konnect_run::tests::actual_konnect_blocked_tool_',
           '--', '--ignored', '--nocapture', '--test-threads=1']
completed = subprocess.run(command, cwd=root, capture_output=True, timeout=240)
(output / 'stdout.log').write_bytes(completed.stdout)
(output / 'stderr.log').write_bytes(completed.stderr)
matches = re.findall(r'Verified actual Konnect blocked-read (cancellation|deadline): ([^\r\n]+)', completed.stdout.decode('utf-8'))
passed = completed.returncode == 0 and len(matches) == 2 and 'test result: ok. 2 passed; 0 failed; 0 ignored;' in completed.stdout.decode('utf-8')
fixtures = {}
for mode, path in matches:
    fixture = Path(path.strip()).resolve(strict=True)
    assert fixture.parent == Path(tempfile.gettempdir()).resolve() and fixture.name.startswith('hazlomexa-konnect-adapter-')
    assert mode not in fixtures
    receipt = json.loads((fixture / f'{mode}-result.json').read_text(encoding='utf-8'))
    assert receipt['pass'] and receipt['mode'] == mode and receipt['owned_pids'] and receipt['ceased_before_barrier_release'] and receipt['source_and_copy_preserved']
    assert (60000 <= receipt['elapsed_ms'] < 90000) if mode == 'deadline' else (0 <= receipt['cancellation_ms'] < 10000)
    fixtures[mode] = str(fixture)
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == sha for path, sha in source_hashes.items())
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == sha for path, sha in runtime_hashes.items())
status = dict(passed=passed, exit_code=completed.returncode, command=command, fixtures=fixtures,
              source_hashes=source_hashes, runtime_hashes=runtime_hashes,
              initialized_blocked_read_cancellation_verified=passed and 'cancellation' in fixtures,
              full_60s_tool_deadline_verified=passed and 'deadline' in fixtures,
              mutation_phase_verified=False, workflow_recovery_verified=False, native_ui_verified=False)
(output / 'status.json').write_text(json.dumps(status, indent=2), encoding='utf-8')
assert passed and set(fixtures) == {'cancellation', 'deadline'}, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
