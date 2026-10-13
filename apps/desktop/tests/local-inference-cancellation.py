"""Observe an owned Ollama runner after cancellation by the actual Rust client."""
import json
import subprocess
import sys
import time
import urllib.request
from pathlib import Path
import psutil

executable, runtime, output = (Path(arg).resolve() for arg in sys.argv[1:])
assert executable.is_file() and (runtime / 'models').is_dir() and not output.exists()
log_path = output.parent / (output.name + '-launcher.log')
log_path.parent.mkdir(parents=True, exist_ok=True)
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with log_path.open('w', encoding='utf-8') as log:
    process = subprocess.Popen(['cargo', 'run', '--package', 'hazlomexa', '--example', 'ollama_benchmark', '--', str(executable), str(runtime), str(output), '--cancel-only'], stdout=log, stderr=log)
    owner = psutil.Process(process.pid)
    runner = None
    cpu_samples = []
    slot_samples = []
    start = time.monotonic()
    observed_cancel = None
    try:
        while process.poll() is None and time.monotonic() - start < 180:
            if runner is None:
                for child in owner.children(recursive=True):
                    try:
                        command = child.cmdline()
                        name = child.name().lower()
                    except psutil.NoSuchProcess:
                        continue
                    if name == 'llama-server.exe' and '--model' in command:
                        model_path = Path(command[command.index('--model') + 1]).resolve()
                        assert model_path.is_relative_to(runtime / 'models')
                        runner = child
                        port = int(command[command.index('--port') + 1])
                        break
            if (output / 'cancellation-client.json').exists() and observed_cancel is None:
                observed_cancel = time.monotonic()
            if runner and observed_cancel is not None and runner.is_running():
                age = time.monotonic() - observed_cancel
                try:
                    cpu = runner.cpu_times()
                    cpu_samples.append({'seconds_after_client_result_observed': age, 'cpu_seconds': cpu.user + cpu.system, 'runner_pid': runner.pid})
                    if age < 4:
                        with opener.open(f'http://127.0.0.1:{port}/slots', timeout=1) as response:
                            slot_samples.append({'seconds_after_client_result_observed': age, 'slots': json.load(response)})
                except (OSError, psutil.NoSuchProcess):
                    pass
            time.sleep(0.2)
        assert process.poll() is not None, 'Owned probe exceeded its deadline'
        assert process.returncode == 0, log_path.read_text(encoding='utf-8')[-2000:]
        client = json.loads((output / 'cancellation-client.json').read_text(encoding='utf-8'))
        assert client['pass'] and client['delta_bytes'] > 0
        steady = [sample for sample in cpu_samples if 2 <= sample['seconds_after_client_result_observed'] <= 6]
        assert len(steady) >= 10 and steady[-1]['cpu_seconds'] - steady[0]['cpu_seconds'] <= 0.05, 'Runner remained active after cancellation'
        engine_log = (output / 'server.stderr.log').read_text(encoding='utf-8')
        assert 'stop processing:' in engine_log and 'all slots are idle' in engine_log
        idle_slots = [sample for sample in slot_samples if isinstance(sample['slots'], list) and sample['slots'] and all(slot.get('is_processing') is False for slot in sample['slots'])]
        assert idle_slots and len(slot_samples) >= 5 and all(all(slot.get('is_processing') is False for slot in sample['slots']) for sample in slot_samples[-5:]), 'Live runner slots did not stay idle after cancellation'
        result = {'pass': True, 'client': client, 'runner_pid': runner.pid, 'cpu_samples': cpu_samples, 'slot_samples': slot_samples, 'live_runner_idle_observed': bool(idle_slots), 'steady_cpu_seconds': steady[-1]['cpu_seconds'] - steady[0]['cpu_seconds'], 'engine_log_has_release_and_idle': True, 'observation_scope': 'CPU runner alive after cancellation; no server shutdown or model unload used to stop inference'}
        (output / 'cessation-result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        print(json.dumps({key: result[key] for key in ['pass', 'live_runner_idle_observed', 'steady_cpu_seconds', 'engine_log_has_release_and_idle']}, indent=2))
    finally:
        if process.poll() is None:
            for child in reversed(owner.children(recursive=True)):
                try: child.kill()
                except psutil.NoSuchProcess: pass
            process.kill()
            process.wait(timeout=10)
