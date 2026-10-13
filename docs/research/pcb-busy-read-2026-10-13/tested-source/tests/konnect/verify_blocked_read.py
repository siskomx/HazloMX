"""Verify native Konnect interruption, optionally through the approved workflow."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import uuid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--workflow', action='store_true', help='Include approval, publication refusal and project recovery')
parser.add_argument('--pcb', action='store_true', help='Verify the approved PCB workflow')
parser.add_argument('--cancel-only', action='store_true', help='Run only the cancellation case')
args = parser.parse_args()
workflow = args.workflow or args.pcb
assert not (args.pcb and args.cancel_only), '--pcb verifies held-host-read refusal, not SDK cancellation'
expected_modes = {'refusal'} if args.pcb else {'cancellation'} if args.cancel_only else {'cancellation', 'deadline'}
root = Path(__file__).resolve().parents[2]
output = root / 'target/konnect-probe' / (('blocked-pcb-workflow-' if args.pcb else 'blocked-workflow-' if workflow else 'blocked-read-') + str(uuid.uuid4()))
output.mkdir()
paths = ['Cargo.lock', 'apps/desktop/src-tauri/src/konnect_run.rs', 'apps/desktop/src-tauri/src/process.rs',
         'apps/desktop/src-tauri/src/kicad_project.rs', 'apps/desktop/src-tauri/src/kicad_run.rs',
         'tests/fixtures/publication_barrier.rs', 'tests/konnect/verify_blocked_read.py']
if workflow:
    paths += ['apps/desktop/src-tauri/src/jobs.rs', 'apps/desktop/src-tauri/src/project.rs']
if args.pcb:
    paths += ['apps/desktop/src-tauri/src/konnect_pcb.rs', 'apps/desktop/src-tauri/src/konnect.rs',
              'docs/research/konnect-pcb-2026-10-12/Original/basic_test.kicad_pcb',
              'docs/research/konnect-pcb-2026-10-12/catalogs.json']
paths += ['apps/desktop/src-tauri/Cargo.toml']
source_hashes = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in paths}
runtime = ['target/konnect-probe/runtime/konnect.exe', 'target/kicad-probe/runtime/bin/kicad-cli.exe']
if workflow:
    runtime += [f'target/kicad-probe/fixtures/{name}' for name in
                ['basic_test.kicad_sch', 'basic_test.kicad_pcb', 'basic_test.kicad_pro', 'custom_ds.kicad_wks']]
runtime_hashes = {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in runtime}
test_filter = 'jobs::tests::actual_konnect_pcb_held_workflow_' if args.pcb else 'jobs::tests::actual_konnect_blocked_workflow_' if workflow else 'konnect_run::tests::actual_konnect_blocked_tool_'
if args.cancel_only:
    test_filter += 'cancellation'
command = ['cargo', 'test', '-p', 'hazlomexa', '--lib', test_filter,
           '--', '--ignored', '--nocapture', '--test-threads=1']
completed = subprocess.run(command, cwd=root, capture_output=True, timeout=240)
(output / 'stdout.log').write_bytes(completed.stdout)
(output / 'stderr.log').write_bytes(completed.stderr)
pattern = r'Verified Konnect blocked workflow (cancellation|deadline|refusal): ([^\r\n]+)' if workflow else r'Verified actual Konnect blocked-read (cancellation|deadline): ([^\r\n]+)'
matches = re.findall(pattern, completed.stdout.decode('utf-8'))
passed = completed.returncode == 0 and len(matches) == len(expected_modes) and f'test result: ok. {len(expected_modes)} passed; 0 failed; 0 ignored;' in completed.stdout.decode('utf-8')
fixtures = {}
for mode, path in matches:
    fixture = Path(path.strip()).resolve(strict=True)
    prefix = 'hazlomexa-workflow-' if workflow else 'hazlomexa-konnect-adapter-'
    assert fixture.parent == Path(tempfile.gettempdir()).resolve() and fixture.name.startswith(prefix)
    assert mode not in fixtures
    name = f'blocked-workflow-{mode}.json' if workflow else f'{mode}-result.json'
    receipt = json.loads((fixture / name).read_text(encoding='utf-8'))
    assert receipt['passed' if workflow else 'pass'] and receipt['mode'] == mode
    if args.pcb:
        assert receipt['no_owned_server_remaining_before_release']
    else:
        assert receipt['owned_pids'] and receipt['ceased_before_barrier_release']
    assert receipt['source_and_working_copy_preserved' if workflow else 'source_and_copy_preserved']
    if workflow:
        assert receipt['integration'] == ('pcb' if args.pcb else 'schematic')
        assert receipt['approval_consumed'] and receipt['published'] is False and receipt['recovered_runs'] == 1
        assert receipt['phase'] == ('cancelled' if mode == 'cancellation' else 'failed')
        if mode == 'cancellation':
            assert 0 <= receipt['cancel_return_ms'] < 1000
    if mode == 'refusal':
        assert 0 <= receipt['held_response_ms'] < 10000
    else:
        assert (60000 <= receipt['elapsed_ms'] < 90000) if mode == 'deadline' else (0 <= receipt['cancellation_ms'] < 10000)
    fixtures[mode] = str(fixture)
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == sha for path, sha in source_hashes.items())
assert all(hashlib.sha256((root / path).read_bytes()).hexdigest() == sha for path, sha in runtime_hashes.items())
status = dict(passed=passed, exit_code=completed.returncode, command=command, fixtures=fixtures,
              integration='pcb' if args.pcb else 'schematic',
              held_host_read_refusal_verified=passed and args.pcb and 'refusal' in fixtures,
              source_hashes=source_hashes, runtime_hashes=runtime_hashes,
              initialized_blocked_read_cancellation_verified=passed and not workflow and 'cancellation' in fixtures,
              held_workflow_access_cancellation_verified=passed and workflow and 'cancellation' in fixtures,
              full_60s_tool_deadline_verified=passed and 'deadline' in fixtures,
              mutation_phase_verified=False, workflow_recovery_verified=passed and workflow and set(fixtures) == expected_modes, native_ui_verified=False)
(output / 'status.json').write_text(json.dumps(status, indent=2), encoding='utf-8')
assert passed and set(fixtures) == expected_modes, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
