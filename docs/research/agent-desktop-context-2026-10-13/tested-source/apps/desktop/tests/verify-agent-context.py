"""Freeze frontend inputs and verify desktop selection context through both chat callers."""
import hashlib
import json
from pathlib import Path
import subprocess
import uuid

root = Path(__file__).resolve().parents[3]
app = root / 'apps/desktop'
output = root / 'target/agent-context' / str(uuid.uuid4())
output.mkdir(parents=True)
paths = ['apps/desktop/src/main.ts', 'apps/desktop/src/design.ts', 'apps/desktop/src/contracts.ts',
         'apps/desktop/tests/design-flow.mjs', 'apps/desktop/tests/verify-agent-context.py',
         'apps/desktop/package.json', 'apps/desktop/package-lock.json']
source_hashes = {path: hashlib.sha256((root/path).read_bytes()).hexdigest() for path in paths}
commands = [['npm.cmd', 'run', 'build'], ['node', 'tests/design-flow.mjs']]
results = []
for index, command in enumerate(commands):
    result = subprocess.run(command, cwd=app, capture_output=True, timeout=180)
    (output/f'{index}-stdout.log').write_bytes(result.stdout)
    (output/f'{index}-stderr.log').write_bytes(result.stderr)
    results.append({'command':command, 'exit_code':result.returncode})
    if result.returncode:
        break
passed = len(results) == 2 and all(item['exit_code'] == 0 for item in results)
receipt = None
if passed:
    assert b'PASS: nine workflows, desktop selection/draft context for Claude and Codex' in result.stdout
    raw = (root/'target/design-flow/agent-desktop-context.json').read_bytes()
    (output/'agent-desktop-context.json').write_bytes(raw)
    receipt = json.loads(raw)
    assert receipt['passed'] and len(receipt['contexts']) == 20
    workflows = {'freecad','arduino','agent','drawing','konnect','pcb','kicad','slice','cadpilot'}
    for provider in ['claude','codex']:
        assert {entry['context']['workflow'] for entry in receipt['contexts'] if entry['provider'] == provider} == workflows
    assert b'temporary-test-token' not in raw and b'HAZLOMEXA_TOKEN' not in raw
assert all(hashlib.sha256((root/path).read_bytes()).hexdigest() == sha for path,sha in source_hashes.items())
(output/'status.json').write_text(json.dumps({'passed':passed,'commands':results,'source_hashes':source_hashes,
    'cases':len(receipt['contexts']) if receipt else 0,'native_ui_verified':False,'signed_in_agent_verified':False},indent=2),encoding='utf-8')
assert passed, str(output)
print(json.dumps({'passed':True,'evidence':str(output)}))
