"""Actual fixed CADPilot copy adapter: typed operations, rejected inputs, original preservation."""
import hashlib
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--rotated', action='store_true', help='Inspect and edit a native box whose dimensions differ from world bounds')
rotated = parser.parse_args().rotated
COMMIT = '58bfee4d45061e55c158a5fd4786ab98a427f7f6'
source = ROOT/'target/cadpilot-probe/source'/('cadpilot-'+COMMIT)/'addon/CADPilot/rpc_server'
runtime = Path(os.environ['LOCALAPPDATA'])/'HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311'
output = ROOT/'target/cadpilot-probe'/('copy-'+str(uuid.uuid4()))
output.mkdir()
resource = ROOT/'apps/desktop/src-tauri/resources/freecad'
original = (ROOT/'docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd').read_bytes()
cases = [
    ('inspect','Target','mm',{},True),
    ('edit_box','Target','in',dict(length=.5,width=1,height=.3),True),
    ('add_box','AddedBox','cm',dict(length=1,width=2,height=3,x=5,y=0,z=0),True),
    ('add_cylinder','AddedCylinder','mm',dict(radius=2,height=8,x=40,y=0,z=0),True),
    ('edit_box','Target','mm',dict(length='=external()',width=2,height=3),False),
    ('execute_code','Target','mm',{},False),
    ('edit_box','Missing','mm',dict(length=2,width=2,height=3),False),
    ('unsafe_archive','Target','mm',{},False),
    ('occupied_output','Target','mm',{},False),
]
if rotated:
    cases = cases[:2]
adapter_hashes = {name:hashlib.sha256((resource/name).read_bytes()).hexdigest() for name in ['cadpilot-copy.FCMacro','cadpilot_policy.py']}
records = []
for index, (operation,name,units,parameters,expected) in enumerate(cases):
    case = output/str(index)
    case.mkdir()
    addon = case/'addon/rpc_server'
    addon.mkdir(parents=True)
    for module in ['__init__.py','geometry_query.py','object_factory.py','property_mapper.py','tip_policy.py']:
        shutil.copyfile(source/module,addon/module)
    for file in ['cadpilot-copy.FCMacro','cadpilot_policy.py']:
        shutil.copyfile(resource/file,case/file)
    (case/'Original.FCStd').write_bytes(original)
    if operation == 'unsafe_archive':
        with zipfile.ZipFile(case/'Original.FCStd','a') as archive:
            archive.writestr('unsafe.py',b'raise RuntimeError()')
    if operation == 'occupied_output':
        (case/'Edited.FCStd').write_bytes(b'preserve occupied output')
    request = dict(schema_version=1,job_id=str(uuid.uuid4()),cadpilot_version='0.7.7',operation=operation if operation not in {'unsafe_archive','occupied_output'} else 'inspect',object=name,units=units,parameters=parameters)
    (case/'input.json').write_text(json.dumps(request),encoding='utf-8')
    source_hash = hashlib.sha256((case/'Original.FCStd').read_bytes()).hexdigest()
    profile = case/'profile'
    profile.mkdir()
    environment = {key:os.environ[key] for key in ['SystemRoot','WINDIR','COMSPEC'] if key in os.environ}
    environment['PATH'] = str(runtime/'bin')+';'+os.environ['SystemRoot']+'/System32'
    for key,sub in [('FREECAD_USER_HOME','home'),('FREECAD_USER_DATA','data'),('FREECAD_USER_TEMP','temp')]:
        path = profile/sub
        path.mkdir()
        environment[key] = str(path)
    for key in ['USERPROFILE','APPDATA','LOCALAPPDATA','TEMP','TMP']:
        environment[key] = str(profile)
    command = [str(runtime/'FreeCADCmd.exe'),'--user-cfg',str(profile/'user.cfg'),'--system-cfg',str(profile/'system.cfg'),str(case/'cadpilot-copy.FCMacro')]
    if rotated:
        generator = case / 'make-rotated.FCMacro'
        generator.write_text('''import FreeCAD as App
from pathlib import Path
assert ".".join(App.Version()[:3]) == "1.1.4"
doc = App.newDocument("RotatedFixture")
box = doc.addObject("Part::Box", "Target")
box.Length, box.Width, box.Height = 10, 20, 30
box.Placement = App.Placement(App.Vector(5, 6, 7), App.Rotation(App.Vector(0, 0, 1), 45))
doc.recompute()
doc.saveAs(str(Path.cwd() / "Rotated.FCStd"))
App.closeDocument(doc.Name)
''', encoding='utf-8')
        seed_command = [*command[:-1], str(generator)]
        seed = subprocess.run(seed_command,cwd=case,env=environment,stdin=subprocess.DEVNULL,capture_output=True,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
        (case/'seed-stdout.log').write_bytes(seed.stdout)
        (case/'seed-stderr.log').write_bytes(seed.stderr)
        assert seed.returncode == 0 and (case/'Rotated.FCStd').is_file()
        (case/'Original.FCStd').write_bytes((case/'Rotated.FCStd').read_bytes())
        source_hash = hashlib.sha256((case/'Original.FCStd').read_bytes()).hexdigest()
    completed = subprocess.run(command,cwd=case,env=environment,stdin=subprocess.DEVNULL,capture_output=True,timeout=120,creationflags=subprocess.CREATE_NO_WINDOW)
    (case/'stdout.log').write_bytes(completed.stdout)
    (case/'stderr.log').write_bytes(completed.stderr)
    actual = completed.returncode == 0 and (case/'result.json').is_file()
    assert actual == expected, str(case)
    assert hashlib.sha256((case/'Original.FCStd').read_bytes()).hexdigest() == source_hash
    if expected:
        result = json.loads((case/'result.json').read_text())
        assert result['success'] and result['job_id'] == request['job_id'] and result['operation'] == operation
        assert result['source_sha256'] == source_hash
        assert result['before']['Target']['dimensions_mm'] == dict(length=10,width=20,height=30)
        if rotated:
            bounds = result['before']['Target']['bbox']
            assert abs((bounds['xmax']-bounds['xmin'])-10) > 1
            assert abs((bounds['ymax']-bounds['ymin'])-20) > 1
        if operation == 'edit_box':
            assert result['after']['Target']['dimensions_mm'] == {key:value*25.4 for key,value in parameters.items()}
        if operation == 'inspect':
            assert result['before'] == result['after'] and not (case/'Edited.FCStd').exists()
        else:
            assert all(hashlib.sha256((case/file).read_bytes()).hexdigest()==digest for file,digest in result['sha256'].items())
            assert set(result['sha256']) == {'Edited.FCStd','Edited.step'}
            expected_names = {'Target'} if operation == 'edit_box' else {'Target',name}
            assert set(result['after']) == expected_names
    else:
        assert not (case/'Working.FCStd').exists(), 'rejected request reached document loading'
        if operation == 'occupied_output':
            assert (case/'Edited.FCStd').read_bytes() == b'preserve occupied output'
    records.append(dict(case=str(index),operation=operation,expected_success=expected,exit_code=completed.returncode,
                        source_sha256=source_hash,command=command))
assert all(hashlib.sha256((resource/name).read_bytes()).hexdigest()==sha for name,sha in adapter_hashes.items())
status = dict(passed=True,cases=records,cadpilot_commit=COMMIT,rotated_box_verified=rotated,
              adapter_hashes=adapter_hashes,
              application_workflow_verified=False,native_gui_verified=False)
(output/'status.json').write_text(json.dumps(status,indent=2),encoding='utf-8')
print(json.dumps(dict(passed=True,evidence=str(output),cases=len(records))))
