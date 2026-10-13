"""Identify and privately provision the official Konnect Windows x64 0.13.0 binary/source."""
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]/'target/konnect-probe'
ROOT.mkdir(exist_ok=True)
COMMIT = '6bbe3e4f890ba1d37c0e5d5f38ccd03d90958c9e'


def fetch(url, limit):
    request = urllib.request.Request(url,headers={'User-Agent':'HazloMexa-runtime-proof'})
    with urllib.request.urlopen(request,timeout=60) as response:
        data = response.read(limit+1)
    assert len(data)<=limit,'Download exceeds limit'
    return data


def extract(data, destination):
    assert not destination.exists(),'Refusing to overwrite an existing runtime/source folder'
    archive = zipfile.ZipFile(io.BytesIO(data))
    assert len(archive.infolist())<=20000
    seen = set()
    total = 0
    for item in archive.infolist():
        name = item.filename.rstrip('/')
        path = PurePosixPath(name)
        assert name and not path.is_absolute() and not any(part in {'.','..'} for part in path.parts)
        assert not any(c in name for c in ['\\',':','\0'])
        assert item.external_attr >> 16 & 0o170000 != 0o120000
        assert name.casefold() not in seen
        seen.add(name.casefold())
        total += item.file_size
        assert total <= 256*1024*1024
    archive.extractall(destination)


release = json.loads(fetch('https://api.github.com/repos/mixelpixx/Konnect/releases/tags/v0.13.0',2*1024*1024))
(ROOT/'release.json').write_text(json.dumps(release,indent=2),encoding='utf-8')
asset = next(item for item in release['assets'] if item['name']=='konnect-v0.13.0-x86_64-pc-windows-msvc.zip')
assert asset['size']==13354938 and asset['digest']=='sha256:9099ad41becbfdbd00263af827a9ebab10d5732a6e42e8dfcff9a1a07dc1f089'
data = fetch(asset['browser_download_url'],32*1024*1024)
assert len(data)==asset['size'] and 'sha256:'+hashlib.sha256(data).hexdigest()==asset['digest']
(ROOT/asset['name']).write_bytes(data)
extract(data,ROOT/'runtime')
source_url='https://codeload.github.com/mixelpixx/Konnect/zip/'+COMMIT
source = fetch(source_url,64*1024*1024)
(ROOT/'source.zip').write_bytes(source)
extract(source,ROOT/'source')
identity = dict(version='0.13.0',commit=COMMIT,asset_url=asset['browser_download_url'],asset_bytes=len(data),asset_sha256=hashlib.sha256(data).hexdigest(),
                source_url=source_url,source_bytes=len(source),source_sha256=hashlib.sha256(source).hexdigest(),
                runtime_files={str(path.relative_to(ROOT/'runtime')).replace('\\','/'):dict(bytes=path.stat().st_size,sha256=hashlib.sha256(path.read_bytes()).hexdigest())
                               for path in sorted((ROOT/'runtime').rglob('*')) if path.is_file()})
(ROOT/'identity.json').write_text(json.dumps(identity,indent=2),encoding='utf-8')
print(json.dumps(identity,indent=2))
