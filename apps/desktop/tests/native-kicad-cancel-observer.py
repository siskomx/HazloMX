import json
from pathlib import Path
import sys
import time
import uuid
import psutil

if not __debug__:
    raise RuntimeError('Run this owned-process test without Python optimization.')

workspace = Path(__file__).resolve().parents[3]
root = workspace / 'target/native-ui-2026-10-13'
project = root / 'project/.hazlomexa'
job_id = str(uuid.UUID(sys.argv[1]))
job_file = project / 'runs' / f'{job_id}.json'
job = json.loads(job_file.read_text())
assert job['phase'] == 'awaiting_approval'
app = json.loads((workspace / 'target/owned-ui-install.json').read_text())
parent = psutil.Process(app['pid'])
assert Path(parent.exe()).resolve() == Path(app['executable']).resolve()
runtime = (workspace / 'target/kicad-probe/runtime/bin/kicad-cli.exe').resolve()
receipt_path = root / 'controlled-cancel.json'
assert not receipt_path.exists()
receipt = {'job_id': job_id, 'stage': 'watching', 'started_unix': time.time()}
def save():
    receipt_path.write_text(json.dumps(receipt, indent=2))
save()
deadline = time.monotonic() + 90
while time.monotonic() < deadline:
    for child in parent.children():
        try:
            argv = child.cmdline()
            if Path(child.exe()).resolve() != runtime or argv[1:3] != ['pcb', 'drc'] or not any(job_id in a for a in argv):
                continue
            receipt.update(pid=child.pid, created=child.create_time(), argv_operation=argv[1:3])
            child.suspend()
            receipt.update(stage='paused_owned_drc', paused_unix=time.time())
            save()
            pause_deadline = time.monotonic() + 60
            try:
                while time.monotonic() < pause_deadline:
                    if not child.is_running():
                        receipt.update(stage='owned_child_exited', exited_unix=time.time())
                        save()
                        print('Controlled native DRC child exited; validate project state separately.')
                        raise SystemExit(0)
                    time.sleep(0.05)
            finally:
                if child.is_running():
                    child.resume()
                    receipt.update(stage='watchdog_resumed', resumed_unix=time.time())
                    save()
            raise SystemExit('Watchdog resumed owned child; cancellation not proven.')
        except psutil.NoSuchProcess:
            continue
    time.sleep(0.03)
receipt.update(stage='observation_expired')
save()
raise SystemExit('No matching native DRC child observed; no process changed.')
