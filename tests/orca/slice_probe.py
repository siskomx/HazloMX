"""Actual slice-to-file proof with explicit, flattened stock profiles; no printer connection."""
import hashlib
import json
import os
import pathlib
import struct
import subprocess
import uuid
import zipfile
import xml.etree.ElementTree as ET

root = pathlib.Path(__file__).resolve().parents[2]
base = root / 'target/orca-probe'
runtime = base / 'runtime'
profiles = runtime / 'resources/profiles/Prusa'
run = base / ('slice-' + str(uuid.uuid4()))
run.mkdir()
captured = {}

def resolve(kind, name, seen=()):
    assert name not in seen and len(seen) < 16
    assert not any(c in name for c in '/\\:')
    path = profiles / kind / (name + '.json')
    content = path.read_bytes()
    captured[str(path.relative_to(runtime))] = hashlib.sha256(content).hexdigest()
    value = json.loads(content)
    assert value['type'] == kind and value['name'] == name
    parent = value.get('inherits', '')
    inherited = resolve(kind, parent, (*seen, name)) if parent else {}
    return {**inherited, **value, 'inherits': ''}

machine = resolve('machine', 'Prusa MK3S 0.4 nozzle')
machine['curr_bed_type'] = 'Textured PEI Plate'
process = resolve('process', '0.20mm Standard @MK3S 0.4')
material = resolve('filament', 'Prusa Generic PLA')
assert machine['name'] in process['compatible_printers']
assert float(machine['nozzle_diameter'][0]) == 0.4
assert float(process['layer_height']) == 0.2
for kind, value in [('machine', machine), ('process', process), ('filament', material)]:
    assert not value.get('post_process')
    (run / (kind + '.json')).write_text(json.dumps(value, indent=2), encoding='utf-8')

vertices = [(100, 100, 0), (110, 100, 0), (110, 110, 0), (100, 110, 0),
            (100, 100, 10), (110, 100, 10), (110, 110, 10), (100, 110, 10)]
faces = [(0, 2, 1), (0, 3, 2), (4, 5, 6), (4, 6, 7), (0, 1, 5), (0, 5, 4),
         (1, 2, 6), (1, 6, 5), (2, 3, 7), (2, 7, 6), (3, 0, 4), (3, 4, 7)]
mesh = b'Hazlo Mexa 10 mm owned proof'.ljust(80, b'\0') + struct.pack('<I', len(faces))
for face in faces:
    mesh += struct.pack('<12fH', 0, 0, 0, *(c for i in face for c in vertices[i]), 0)
model = run / 'Cube-mm.stl'
model.write_bytes(mesh)
env = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC'] if key in os.environ}
env['PATH'] = str(runtime) + os.pathsep + str(pathlib.Path(env['SystemRoot']) / 'System32')
for key in ['USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP']:
    directory = run / key.lower()
    directory.mkdir()
    env[key] = str(directory)
data = run / 'data'
data.mkdir()
output = run / 'output'
output.mkdir()
command = [str(runtime / 'orca-slicer.exe'), '--datadir', str(data), '--debug', '3',
           '--load-settings', str(run / 'machine.json') + ';' + str(run / 'process.json'),
           '--load-filaments', str(run / 'filament.json'), '--slice', '0',
           '--outputdir', str(output), '--export-3mf', 'Sliced.3mf', str(model)]
result = subprocess.run(command, cwd=run, env=env, capture_output=True, timeout=120,
                        creationflags=subprocess.CREATE_NO_WINDOW)
(run / 'stdout.log').write_bytes(result.stdout)
(run / 'stderr.log').write_bytes(result.stderr)
record = dict(command=command, exit_code=result.returncode, stock_profile_hashes=captured,
              original_sha256=hashlib.sha256(mesh).hexdigest(), printer_connected=False,
              published=False, units='mm', expected_bounds_mm=[10, 10, 10])
(run / 'calls.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
assert result.returncode == 0, str(run)
assert model.read_bytes() == mesh
with zipfile.ZipFile(output / 'Sliced.3mf') as archive:
    names = archive.namelist()
    assert '3D/3dmodel.model' in names
    gcode = [n for n in names if n.endswith('.gcode')]
    assert gcode and all(archive.read(n) for n in gcode)
    core = 'http://schemas.microsoft.com/3dmanufacturing/core/2015/02'
    top = ET.fromstring(archive.read('3D/3dmodel.model'))
    assert top.attrib['unit'] == 'millimeter'
    assert any(m.attrib.get('name') == 'OrcaSlicer' and m.text == '2.4.2' for m in top.findall(f'{{{core}}}metadata'))
    meshes = [n for n in names if n.startswith('3D/Objects/') and n.endswith('.model')]
    assert len(meshes) == 1 and len(gcode) == 1
    part = ET.fromstring(archive.read(meshes[0]))
    points = [[float(v.attrib[a]) for a in ['x', 'y', 'z']] for v in part.findall(f'.//{{{core}}}vertex')]
    bounds = [max(p[i] for p in points) - min(p[i] for p in points) for i in range(3)]
    assert bounds == [10, 10, 10]
    assert len(part.findall(f'.//{{{core}}}triangle')) == 12
    for tree in [top, part]:
        for item in tree.iter():
            if 'transform' in item.attrib:
                assert list(map(float, item.attrib['transform'].split()))[:9] == [1, 0, 0, 0, 1, 0, 0, 0, 1]
    effective = json.loads(archive.read('Metadata/project_settings.config'))
    assert effective['printer_settings_id'] == machine['name']
    assert effective['print_settings_id'] == process['name']
    assert effective['filament_settings_id'] == [material['name']]
    assert effective['curr_bed_type'] == 'Textured PEI Plate'
    assert effective['nozzle_diameter'] == ['0.4'] and effective['layer_height'] == '0.2'
    code = archive.read(gcode[0])
    assert b'generated by OrcaSlicer 2.4.2' in code
    assert b'; total layer number: 50' in code and b'; max_z_height: 10.00' in code
    active = [line.split(b';', 1)[0].strip() for line in code.splitlines()]
    commands = {line.split()[0] for line in active if line}
    assert b'G21' in commands and b'G20' not in commands
    assert hashlib.md5(code).hexdigest() == archive.read(gcode[0] + '.md5').decode().lower()
    record['verified_model_bounds_mm'] = bounds
    record['bed_type'] = effective['curr_bed_type']
    record['layers'] = 50
    record['gcode_sha256'] = hashlib.sha256(code).hexdigest()
    record['effective_settings'] = {k: effective[k] for k in ['printer_settings_id', 'print_settings_id', 'filament_settings_id', 'curr_bed_type', 'nozzle_diameter', 'layer_height', 'nozzle_temperature_initial_layer', 'textured_plate_temp_initial_layer']}
record['archive_entries'] = names
record['output_sha256'] = hashlib.sha256((output / 'Sliced.3mf').read_bytes()).hexdigest()
(run / 'result.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
print(json.dumps(dict(passed=True, evidence=str(run))))
