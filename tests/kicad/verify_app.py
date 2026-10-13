"""Retain actual identified KiCad adapter and shared workflow evidence."""
import hashlib
import json
import pathlib
import re
import subprocess
import uuid

root = pathlib.Path(__file__).resolve().parents[2]
output = root / 'target/kicad-probe' / ('app-' + str(uuid.uuid4()))
output.mkdir()
command = ['cargo', 'test', '-p', 'hazlomexa', '--lib', 'actual_kicad', '--', '--ignored', '--nocapture', '--test-threads=1']
result = subprocess.run(command, cwd=root, capture_output=True, timeout=300)
(output / 'stdout.log').write_bytes(result.stdout)
(output / 'stderr.log').write_bytes(result.stderr)
stdout = result.stdout.decode('utf-8')
expected = ['kicad_run::tests::actual_kicad_captured_project_checks_and_exports',
            'jobs::tests::actual_kicad_workflow_publication_and_recovery',
            'jobs::tests::actual_kicad_owned_process_cancellation',
            'kicad_run::tests::actual_kicad_process_short_deadline']
def tests_passed(code, text):
    return code == 0 and all(f'test {name} ... ' in text for name in expected) and bool(
        re.search(rf'test result: ok\. {len(expected)} passed; 0 failed; 0 ignored;', text))

# Serial --nocapture output can put a test's diagnostic before its standalone "ok" line.
fixture = '\n'.join(f'test {name} ... diagnostic\nok' for name in expected) + '\ntest result: ok. 4 passed; 0 failed; 0 ignored;'
assert tests_passed(0, fixture)
assert not tests_passed(1, fixture)
assert not tests_passed(0, fixture.replace(expected[0], 'missing'))
assert not tests_passed(0, fixture.replace('4 passed', '3 passed'))
passed = tests_passed(result.returncode, stdout)
record = dict(passed=passed, exit_code=result.returncode, command=command,
              expected_tests=expected, native_ui_verified=False, installed_workflow_verified=False,
              source_hashes={str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
                             for path in [root/'Cargo.lock', *sorted((root/'apps/desktop/src-tauri/src').glob('*.rs')),
                                          root/'apps/desktop/src/main.ts', root/'apps/desktop/src/contracts.ts',
                                          root/'apps/desktop/tests/design-flow.mjs']})
(output / 'status.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
assert passed, str(output)
print(json.dumps(dict(passed=True, evidence=str(output))))
