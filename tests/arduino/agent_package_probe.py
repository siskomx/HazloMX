"""Verify/extract the pinned upstream Windows Agent package; do not launch it."""
import hashlib
import json
from pathlib import Path
import zipfile
from probe_runtime import download


if __name__ == '__main__':
    root = Path(__file__).resolve().parents[2] / 'target/arduino-agent-probe'
    release = json.loads((root.parents[1] / 'docs/research/arduino-agent-2026-10-11/identity.json').read_text())
    assert release['tag'] == 'v0.7.1'
    asset = next(a for a in release['assets'] if a['name'] == 'arduino-agent-windows.zip')
    assert asset['digest'] == 'sha256:a02ccf9fb97fa1e88b421ace4387bb5d912abe622152fcef2a9b89cae2603f58'
    download({'url': asset['browser_download_url'], 'bytes': asset['size'], 'sha256': asset['digest']}, root / asset['name'])
    runtime = root / 'runtime'
    assert not runtime.exists(), 'Refusing to replace an existing extracted package'
    with zipfile.ZipFile(root / asset['name']) as archive:
        assert archive.testzip() is None
        for member in archive.infolist():
            assert (runtime / member.filename).resolve().is_relative_to(runtime.resolve())
            assert (member.external_attr >> 16) & 0o170000 != 0o120000, 'No archive symlinks'
        archive.extractall(runtime)
    executables = list(runtime.rglob('Arduino Agent.exe'))
    assert len(executables) == 1
    bridge = executables[0].parent / 'resources/mcp-bridge/bridge/arduino-agent-bridge.js'
    assert bridge.is_file()
    result = {'pass': True, 'tag': release['tag'], 'archive_sha256': asset['digest'],
              'executable': str(executables[0]), 'executable_sha256': hashlib.sha256(executables[0].read_bytes()).hexdigest(),
              'bridge_sha256': hashlib.sha256(bridge.read_bytes()).hexdigest(), 'launched': False}
    (root / 'package-result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
