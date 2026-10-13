"""Real compilation checks in the owned CLI toolchain; never upload firmware."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import uuid


def valid_hex(data):
    records = data.decode('ascii').splitlines()
    assert records and records[-1] == ':00000001FF', 'Missing Intel HEX end record'
    count = 0
    for line in records:
        assert line.startswith(':')
        record = bytes.fromhex(line[1:])
        assert len(record) == record[0] + 5 and sum(record) % 256 == 0
        if record[3] == 0:
            count += record[0]
    assert count > 0, 'No firmware data'
    return count


if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2] / 'target' / 'arduino-probe'
    assert json.loads((root / 'runtime-result.json').read_text())['pass']
    run = root / 'compilations' / str(uuid.uuid4())
    run.mkdir(parents=True)
    environment = {k: v for k, v in os.environ.items() if not k.upper().startswith('ARDUINO_') and k.upper() != 'GITHUB_TOKEN'}
    temporary = run / 'temp'
    temporary.mkdir()
    environment.update(TEMP=str(temporary), TMP=str(temporary))
    sources = {
        'Blink': 'void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(500); digitalWrite(LED_BUILTIN, LOW); delay(500); }\n',
        'Invalid': 'void setup() { this_is_not_valid_cpp; }\nvoid loop() {}\n',
    }
    evidence = {'pass': False, 'cli_version': '1.5.1', 'core': 'arduino:avr@1.8.8',
                'fqbn': 'arduino:avr:uno', 'hardware_mutations': False, 'models_run': 0, 'runs': []}
    for name, source in sources.items():
        sketch = run / name
        sketch.mkdir()
        source_file = sketch / (name + '.ino')
        source_file.write_text(source)
        source_hash = hashlib.sha256(source_file.read_bytes()).hexdigest()
        output = run / (name + '-output')
        build = run / (name + '-build')
        arguments = ['rtk', 'proxy', str(root / 'runtime/arduino-cli.exe'), '--config-file', str(root / 'config.yaml'),
                     '--json', 'compile', '--fqbn', evidence['fqbn'], '--jobs', '2',
                     '--build-path', str(build), '--output-dir', str(output), str(sketch)]
        result = subprocess.run(arguments, cwd=run, stdin=subprocess.DEVNULL, capture_output=True, env=environment, timeout=120)
        (run / (name + '.stdout.json')).write_bytes(result.stdout)
        (run / (name + '.stderr')).write_bytes(result.stderr)
        assert hashlib.sha256(source_file.read_bytes()).hexdigest() == source_hash
        item = {'name': name, 'exit_code': result.returncode, 'source_sha256': source_hash, 'artifacts': []}
        evidence['runs'].append(item)
        if name == 'Blink':
            assert result.returncode == 0, result.stderr.decode(errors='replace')
            assert (output / 'Blink.ino.elf').read_bytes()[:4] == b'\x7fELF'
            item['hex_data_bytes'] = valid_hex((output / 'Blink.ino.hex').read_bytes())
            for file in output.iterdir():
                assert file.is_file() and file.stat().st_size > 0
                item['artifacts'].append({'name': file.name, 'bytes': file.stat().st_size, 'sha256': hashlib.sha256(file.read_bytes()).hexdigest()})
        else:
            assert result.returncode != 0
            assert not output.exists() or not list(output.iterdir()), 'Failed compile exported files'
    evidence['pass'] = True
    marker = {'schema_version': 1, 'owner': 'HazloMexa', 'cli_version': '1.5.1', 'core_version': '1.8.8'}
    marker_file = root / 'data/hazlomexa-toolchain.json'
    if marker_file.exists():
        assert json.loads(marker_file.read_text()) == marker
    else:
        marker_file.write_text(json.dumps(marker, indent=2))
    evidence['evidence_directory'] = str(run.relative_to(root))
    (run / 'result.json').write_text(json.dumps(evidence, indent=2))
    (root / 'compile-result.json').write_text(json.dumps(evidence, indent=2))
    print(json.dumps(evidence))
