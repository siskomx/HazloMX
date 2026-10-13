"""Identify, download and privately extract the official OrcaSlicer x64 portable release."""
import hashlib
import json
import pathlib
import shutil
import stat
import urllib.request
import zipfile

root = pathlib.Path(__file__).resolve().parents[2]
out = root / 'target/orca-probe'
out.mkdir(exist_ok=True)
tag = 'v2.4.2'
name = 'OrcaSlicer_Windows_V2.4.2_x64_portable.zip'
expected = 'feba3009dfb9d268779cca5758a1a5bc3b7d0722bf8fa48d5c57340de975d6be'
request = urllib.request.Request(f'https://api.github.com/repos/OrcaSlicer/OrcaSlicer/releases/tags/{tag}', headers={'User-Agent': 'HazloMexa-runtime-proof'})
with urllib.request.urlopen(request, timeout=60) as response:
    metadata = json.loads(response.read(8 * 1024 * 1024))
assert metadata['tag_name'] == tag
asset = next(a for a in metadata['assets'] if a['name'] == name)
assert asset['digest'] == 'sha256:' + expected and asset['size'] == 171367668
assert asset['browser_download_url'] == f'https://github.com/OrcaSlicer/OrcaSlicer/releases/download/{tag}/{name}'
(out / 'release.json').write_text(json.dumps(metadata, indent=2), encoding='utf-8')
package = out / name
if not package.exists():
    partial = package.with_suffix('.partial')
    with urllib.request.urlopen(asset['browser_download_url'], timeout=120) as response, partial.open('xb') as stream:
        copied = 0
        while block := response.read(1024 * 1024):
            copied += len(block)
            assert copied <= asset['size']
            stream.write(block)
    assert copied == asset['size']
    assert hashlib.sha256(partial.read_bytes()).hexdigest() == expected
    partial.rename(package)
assert package.stat().st_size == asset['size']
assert hashlib.sha256(package.read_bytes()).hexdigest() == expected
runtime = out / 'runtime'
runtime.mkdir()
with zipfile.ZipFile(package) as archive:
    assert sum(e.file_size for e in archive.infolist()) < 2 * 1024 * 1024 * 1024
    for entry in archive.infolist():
        path = pathlib.PurePosixPath(entry.filename)
        assert not path.is_absolute() and '..' not in path.parts and ':' not in entry.filename and '\\' not in entry.filename
        assert not stat.S_ISLNK(entry.external_attr >> 16)
        destination = runtime.joinpath(*path.parts)
        if entry.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
        else:
            destination.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(entry) as source, destination.open('xb') as target:
                shutil.copyfileobj(source, target, 1024 * 1024)
record = dict(tag=tag, asset=name, bytes=package.stat().st_size, sha256=expected,
              download_url=asset['browser_download_url'], runtime=str(runtime),
              binaries={str(p.relative_to(runtime)): hashlib.sha256(p.read_bytes()).hexdigest()
                        for p in sorted(runtime.rglob('*')) if p.suffix.lower() in ['.exe', '.dll', '.com']})
(out / 'identity.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
print(json.dumps(dict(passed=True, identity=str(out / 'identity.json'), runtime=str(runtime))))
