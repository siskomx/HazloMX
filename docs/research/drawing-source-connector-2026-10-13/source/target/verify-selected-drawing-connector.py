from pathlib import Path
import hashlib,json,subprocess,uuid,shutil
base=Path.cwd()
out=base/'target/drawing-connector'/str(uuid.uuid4())
out.mkdir(parents=True)
sources=out/'sources'
sources.mkdir()
for name in ['metric.dxf','imperial.dxf']:
    shutil.copy2(base/'docs/research/cadcraft-runtime-2026-10-11'/name,sources/name)
helper=base/'target/release/hazlomexa-tools.exe'
harness=base/'target/debug/examples/installed_drawing_workflow.exe'
cli=base/'target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe'
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
identity=json.loads((base/'target/drawing-package/ed68ac5c-7474-4a9a-867c-da82d88ac130/status.json').read_text())
assert digest(helper)==identity['helper_sha256']
files=[helper,harness,cli,base/'apps/desktop/src-tauri/examples/installed_drawing_workflow.rs',Path(__file__).resolve()]
files += list((base/'apps/desktop/src-tauri/src').rglob('*.rs'))
files += list(sources.iterdir())
before={str(p.relative_to(base)):digest(p) for p in files}
(out/'inputs.json').write_text(json.dumps(before,indent=2),encoding='utf-8')
with (out/'stdout.log').open('wb') as stdout,(out/'stderr.log').open('wb') as stderr:
    r=subprocess.run([str(harness),str(helper),str(cli),str(sources),str(out/'projects')],stdout=stdout,stderr=stderr,timeout=240)
unchanged=all(digest(base/p)==h for p,h in before.items())
reply=json.loads((out/'stdout.log').read_text(encoding='utf-8')) if r.returncode==0 else None
passed=r.returncode==0 and unchanged and reply['pass'] and reply['completed_jobs']==4 and reply['recovery_verified']
for name,unit in [('metric',4),('imperial',1)]:
    if not passed:break
    calls=json.loads((out/'projects'/name/'calls.json').read_text(encoding='utf-8'))
    assert calls['source_inspection']['inspection']['insunits']==unit
    assert calls['source_proposal']['human_approval_required'] and 'approval_id' not in calls['source_proposal']
(out/'status.json').write_text(json.dumps({'passed':passed,'exit_code':r.returncode,'inputs_unchanged':unchanged,'reply':reply,'installer_sha256':identity['installer_sha256'],'installed_verified':False,'native_gui_verified':False,'signed_in_agent_verified':False},indent=2),encoding='utf-8')
print(json.dumps({'passed':passed,'evidence':str(out)}))
assert passed
