"""Prepare the pinned, app-local Rea runtime; never register personal agents."""
import hashlib,json,os,shutil,subprocess,urllib.request,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/'apps/desktop/runtime/rea'
DEST=ROOT/'target/bundled-rea'
NODE='node-v22.23.2-win-x64'
NODE_SHA='1177b4137ba5adaa56354ae40f1080c7450e8ae09cecb47da459d1c52ac99f97'

def main():
    DEST.mkdir(parents=True,exist_ok=True)
    expected=hashlib.sha256((SOURCE/'package-lock.json').read_bytes()).hexdigest()
    receipt=DEST/'runtime-identity.json'
    if receipt.exists() and json.loads(receipt.read_text()).get('lock_sha256')==expected:
        for name,digest in json.loads(receipt.read_text())['files'].items():
            assert hashlib.sha256((DEST/name).read_bytes()).hexdigest()==digest
        print('Pinned bundled Rea runtime verified');return
    archive=ROOT/'target'/f'{NODE}.zip'
    if not archive.exists():
        with urllib.request.urlopen(f'https://nodejs.org/dist/v22.23.2/{NODE}.zip',timeout=60) as response,archive.open('wb') as output:shutil.copyfileobj(response,output)
    assert hashlib.sha256(archive.read_bytes()).hexdigest()==NODE_SHA
    with zipfile.ZipFile(archive) as source:
        for name in ['node.exe','LICENSE']:(DEST/name).write_bytes(source.read(f'{NODE}/{name}'))
    for name in ['package.json','package-lock.json']:shutil.copyfile(SOURCE/name,DEST/name)
    npm=shutil.which('npm.cmd');assert npm,'Node/npm is required only when building the installer'
    result=subprocess.run([npm,'ci','--omit=dev','--ignore-scripts','--no-audit','--no-fund'],cwd=DEST,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=300)
    (DEST/'dependency-build.log').write_bytes(result.stdout)
    assert result.returncode==0,'Rea dependency build failed; see private dependency-build.log'
    package=json.loads((DEST/'node_modules/rea-agents/package.json').read_text());assert package['version']=='6.5.0'
    files={str(p.relative_to(DEST)).replace('\\','/'):hashlib.sha256(p.read_bytes()).hexdigest() for p in DEST.rglob('*') if p.is_file() and p.name not in {'runtime-identity.json','dependency-build.log'}}
    receipt.write_text(json.dumps({'rea_version':'6.5.0','node_version':'22.23.2','node_archive_sha256':NODE_SHA,'lock_sha256':expected,'files':files},indent=2))
    print('Pinned Rea and Node runtime prepared with upstream licenses')

if __name__=='__main__':main()
