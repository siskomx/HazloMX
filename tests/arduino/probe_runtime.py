"""Verify the official portable CLI and its isolated configuration; no board access."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request
import zipfile


def download(asset, destination):
    expected = asset['sha256'].removeprefix('sha256:')
    if len(expected) != 64:
        raise ValueError('Release must provide a SHA256 digest')
    if not destination.exists():
        request = urllib.request.Request(asset['url'], headers={'User-Agent': 'HazloMexa-runtime-proof'})
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read(asset['bytes'] + 1)
        if len(data) != asset['bytes'] or hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('Downloaded release identity mismatch')
        destination.write_bytes(data)
    if hashlib.sha256(destination.read_bytes()).hexdigest() != expected:
        raise ValueError('Stored release identity mismatch')


if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2] / 'target' / 'arduino-probe'
    root.mkdir(parents=True, exist_ok=True)
    release_file = Path(__file__).resolve().parents[2] / 'docs/research/arduino-runtime-2026-10-11/release.json'
    release = json.loads(release_file.read_text())
    if release['tag'] != 'v1.5.1':
        raise ValueError('This proof is pinned to Arduino CLI 1.5.1')
    archive_asset = next(a for a in release['assets'] if a['name'] == 'arduino-cli_1.5.1_Windows_64bit.zip')
    checksum_asset = next(a for a in release['assets'] if a['name'] == '1.5.1-checksums.txt')
    download(archive_asset, root / archive_asset['name'])
    download(checksum_asset, root / checksum_asset['name'])
    checksums = dict(line.split()[::-1] for line in (root / checksum_asset['name']).read_text().splitlines())
    assert checksums[archive_asset['name']] == archive_asset['sha256'].removeprefix('sha256:')
    runtime = root / 'runtime'
    runtime.mkdir(exist_ok=True)
    with zipfile.ZipFile(root / archive_asset['name']) as archive:
        executable_bytes = archive.read('arduino-cli.exe')
    executable = runtime / 'arduino-cli.exe'
    if executable.exists():
        assert executable.read_bytes() == executable_bytes
    else:
        executable.write_bytes(executable_bytes)
    config = {'directories': {key: (root / key).as_posix() for key in ('data', 'downloads', 'user')},
              'build_cache': {'path': (root / 'cache').as_posix()},
              'metrics': {'enabled': False}, 'updater': {'enable_notification': False},
              'network': {'cloud_api': {'skip_board_detection_calls': True}}}
    config_file = root / 'config.yaml'
    config_file.write_text(json.dumps(config, indent=2))
    environment = {k: v for k, v in os.environ.items() if not k.upper().startswith('ARDUINO_') and k.upper() != 'GITHUB_TOKEN'}
    outputs = {}
    for name, arguments in [('version', ['version', '--json']), ('config', ['config', 'dump', '--json']), ('compile-help', ['compile', '--help'])]:
        result = subprocess.run(['rtk', 'proxy', str(executable), '--config-file', str(config_file), *arguments],
                                stdin=subprocess.DEVNULL, capture_output=True, env=environment, timeout=30)
        (root / (name + '.stdout')).write_bytes(result.stdout)
        (root / (name + '.stderr')).write_bytes(result.stderr)
        assert result.returncode == 0, (name, result.returncode, result.stderr.decode(errors='replace'))
        outputs[name] = result.stdout.decode('utf-8-sig')
    assert json.loads(outputs['version'])['VersionString'] == '1.5.1'
    actual_config = json.loads(outputs['config'])['config']
    for key, path in config['directories'].items():
        assert Path(actual_config['directories'][key]).resolve() == Path(path).resolve()
    assert Path(actual_config['build_cache']['path']).resolve() == (root / 'cache').resolve()
    assert '--fqbn' in outputs['compile-help'] and '--output-dir' in outputs['compile-help']
    evidence = {'pass': True, 'version': '1.5.1', 'archive_sha256': archive_asset['sha256'],
                'exe_sha256': hashlib.sha256(executable_bytes).hexdigest(), 'isolated_paths_verified': True,
                'compiled_sketch': False, 'hardware_operations': False, 'models_run': 0}
    (root / 'runtime-result.json').write_text(json.dumps(evidence, indent=2))
    print(json.dumps(evidence))
