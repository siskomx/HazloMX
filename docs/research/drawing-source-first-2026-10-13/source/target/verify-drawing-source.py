from pathlib import Path
import hashlib, json, subprocess, uuid
root = Path.cwd()
out = root/'target/drawing-source'/str(uuid.uuid4())
out.mkdir(parents=True)
files = list((root/'apps/desktop/src-tauri/src').rglob('*.rs'))
files += [root/p for p in ['Cargo.lock','Cargo.toml','apps/desktop/src-tauri/Cargo.toml','apps/desktop/src-tauri/build.rs','apps/desktop/src-tauri/capabilities/main.json','target/verify-drawing-source.py']]
files += [root/'docs/research/cadcraft-runtime-2026-10-11'/p for p in ['metric.dxf','imperial.dxf','foreign.dxf']]
files += [root/'docs/research/drawing-unit-guidance-2026-10-12/native-inch-drawing.dwg']
files += list((root/'target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable').rglob('*.exe'))
files += list((root/'target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable').rglob('*.dll'))
def digest(p): return hashlib.sha256(p.read_bytes()).hexdigest()
before = {str(p.relative_to(root)):digest(p) for p in files}
(out/'inputs.json').write_text(json.dumps(before,indent=2),encoding='utf-8')
tests = ['cadcraft::tests::actual_cli_discovers_units_and_bounds_without_edit_parameters','host_tools::tests::actual_selected_drawing_inspection_and_proposal_boundaries','cadcraft::tests::actual_cli_copies_converts_and_adds_circle']
results = []
for i,test in enumerate(tests):
    cmd = ['rtk','proxy','cargo','test','-p','hazlomexa','--lib',test,'--','--ignored','--exact','--nocapture']
    r = subprocess.run(cmd,capture_output=True,timeout=180)
    (out/f'{i}-stdout.log').write_bytes(r.stdout)
    (out/f'{i}-stderr.log').write_bytes(r.stderr)
    results.append({'test':test,'exit_code':r.returncode})
    if r.returncode: break
unchanged = all(digest(root/p)==h for p,h in before.items())
passed = len(results)==3 and all(r['exit_code']==0 for r in results) and unchanged
(out/'status.json').write_text(json.dumps({'passed':passed,'inputs_unchanged':unchanged,'tests':results,'native_ui_verified':False,'installed_package_verified':False},indent=2),encoding='utf-8')
print(json.dumps({'passed':passed,'evidence':str(out)}))
assert passed
