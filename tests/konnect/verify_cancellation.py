"""Retain actual Konnect SDK startup cancellation, separately from GUI/tool-call cancellation."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import uuid

root = Path(__file__).resolve().parents[2]
output = root / 'target/konnect-probe' / ('cancellation-' + str(uuid.uuid4()))
output.mkdir()
command = ['cargo', 'test', '-p', 'hazlomexa', '--lib', 'actual_konnect_owned_process_cancellation', '--', '--ignored', '--nocapture', '--test-threads=1']
completed = subprocess.run(command, cwd=root, capture_output=True, timeout=180)
(output / 'stdout.log').write_bytes(completed.stdout)
(output / 'stderr.log').write_bytes(completed.stderr)
stdout = completed.stdout.decode('utf-8')
fixtures = [Path(path.strip()) for path in re.findall(r'Verified Konnect owned cancellation: ([^\r\n]+)', stdout)]
passed = completed.returncode == 0 and len(fixtures) == 2 and 'test result: ok. 1 passed; 0 failed; 0 ignored;' in stdout
receipts = [json.loads((fixture / 'cancel-evidence.json').read_text(encoding='utf-8')) for fixture in fixtures]
assert all(receipt['passed'] and receipt['observed_owned_pids'] and not receipt['published'] and receipt['originals_preserved'] and receipt['recovered_runs'] == 1 for receipt in receipts)
paths = ['apps/desktop/src-tauri/src/jobs.rs', 'apps/desktop/src-tauri/src/process.rs', 'apps/desktop/src-tauri/src/konnect_run.rs', 'apps/desktop/src-tauri/src/konnect_pcb.rs', 'apps/desktop/src-tauri/src/project.rs', 'Cargo.lock', 'tests/konnect/verify_cancellation.py']
runtime = [root / 'target/konnect-probe/runtime/konnect.exe', root / 'target/kicad-probe/runtime/bin/kicad-cli.exe']
status = dict(passed=passed, exit_code=completed.returncode, command=command, fixtures=[str(fixture) for fixture in fixtures],
              source_hashes={path:hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths},
              runtime_hashes={str(path.relative_to(root)):hashlib.sha256(path.read_bytes()).hexdigest() for path in runtime},
              native_ui_verified=False, initialized_tool_call_cancellation_verified=False, full_deadlines_verified=False)
(output / 'status.json').write_text(json.dumps(status, indent=2), encoding='utf-8')
assert passed and {receipt['board'] for receipt in receipts} == {False, True}, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
