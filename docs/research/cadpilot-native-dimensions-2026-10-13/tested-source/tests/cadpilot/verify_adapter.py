"""Retain actual owned-process CADPilot Rust adapter evidence, separately from app acceptance."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import uuid

root = Path(__file__).resolve().parents[2]
output = root/'target/cadpilot-probe'/('adapter-'+str(uuid.uuid4()))
output.mkdir()
paths = [root/'apps/desktop/src-tauri/src/cadpilot.rs',root/'apps/desktop/src-tauri/src/freecad.rs',root/'Cargo.lock',
         root/'apps/desktop/src-tauri/src/jobs.rs',root/'apps/desktop/src-tauri/src/process.rs',root/'apps/desktop/src-tauri/src/project.rs',root/'apps/desktop/src-tauri/src/main.rs',
         root/'apps/desktop/src/main.ts',root/'apps/desktop/src/contracts.ts',root/'apps/desktop/tests/design-flow.mjs', Path(__file__),
         *sorted((root/'apps/desktop/src-tauri/resources/freecad').glob('cadpilot*'))]
source_hashes = {str(path.relative_to(root)):hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
command = ['cargo','test','-p','hazlomexa','--lib','actual_cadpilot','--','--ignored','--nocapture','--test-threads=1']
completed = subprocess.run(command,cwd=root,capture_output=True,timeout=180)
(output/'stdout.log').write_bytes(completed.stdout)
(output/'stderr.log').write_bytes(completed.stderr)
stdout = completed.stdout.decode('utf-8')
expected=['cadpilot::tests::actual_cadpilot_owned_selected_copy_adapter','jobs::tests::actual_cadpilot_workflow_publication_and_recovery','jobs::tests::actual_cadpilot_owned_process_cancellation']
passed = (completed.returncode == 0 and all(f'test {name} ... ' in stdout for name in expected)
          and bool(re.search(r'test result: ok\. 3 passed; 0 failed; 0 ignored;',stdout)))
assert all(hashlib.sha256((root/path).read_bytes()).hexdigest()==sha for path,sha in source_hashes.items())
(output/'status.json').write_text(json.dumps(dict(passed=passed,exit_code=completed.returncode,command=command,
    source_hashes=source_hashes,
    expected_tests=expected,backend_workflow_verified=passed,native_ui_verified=False,installed_workflow_verified=False),indent=2),encoding='utf-8')
assert passed, str(output)
print(json.dumps(dict(passed=True,evidence=str(output))))
