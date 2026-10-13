"""Retain actual identified-runtime adapter and shared workflow evidence."""
import hashlib
import json
import pathlib
import subprocess
import uuid

root = pathlib.Path(__file__).resolve().parents[2]
output = root / 'target/cadcraft-probe' / ('app-' + str(uuid.uuid4()))
output.mkdir()
command = ['cargo', 'test', '-p', 'hazlomexa', '--lib', 'cadcraft', '--', '--ignored', '--nocapture', '--test-threads=1']
result = subprocess.run(command, cwd=root, capture_output=True, timeout=240)
(output / 'stdout.log').write_bytes(result.stdout)
(output / 'stderr.log').write_bytes(result.stderr)
stdout = result.stdout.decode('utf-8')
expected = ['cadcraft::tests::actual_cli_copies_converts_and_adds_circle',
            'jobs::tests::actual_cadcraft_workflow_publication_and_recovery',
            'jobs::tests::actual_cadcraft_owned_process_cancellation']
passed = (result.returncode == 0 and all(f'test {name} ...' in stdout for name in expected)
          and f'test result: ok. {len(expected)} passed; 0 failed; 0 ignored;' in stdout)
record = dict(passed=passed, exit_code=result.returncode, command=command,
              expected_tests=expected, native_ui_verified=False, installed_workflow_verified=False,
              source_hashes={str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
                             for path in [root/'Cargo.lock', *sorted((root/'apps/desktop/src-tauri/src').glob('*.rs')),
                                          root/'apps/desktop/src/main.ts', root/'apps/desktop/src/contracts.ts']})
(output / 'status.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
assert passed, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
