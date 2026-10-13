"""Actual official KiCad CLI gate; fresh private profile, copies and fixed commands."""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import uuid
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[2]
PROBE = ROOT / 'target/kicad-probe'
RUNTIME = PROBE / 'runtime'
EXE = RUNTIME / 'bin/kicad-cli.exe'
OUT = PROBE / ('run-' + str(uuid.uuid4()))
OUT.mkdir()
SOURCE = OUT / 'source'
SOURCE.mkdir()
PROFILE = OUT / 'profile'
PROFILE.mkdir()
(PROFILE / '10.0').mkdir()
for table in ['sym-lib-table', 'fp-lib-table']:
    shutil.copy2(RUNTIME / 'share/kicad/template' / table, PROFILE / '10.0' / table)
env = {key: os.environ[key] for key in ['SystemRoot', 'WINDIR', 'COMSPEC'] if key in os.environ}
env.update({key: str(PROFILE) for key in ['TEMP', 'TMP', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'KICAD_CONFIG_HOME']})
env['KICAD10_SYMBOL_DIR'] = str(RUNTIME / 'share/kicad/symbols')
env['KICAD10_FOOTPRINT_DIR'] = str(RUNTIME / 'share/kicad/footprints')
calls = []


def run(*args, expected=0):
    result = subprocess.run([str(EXE), *map(str, args)], cwd=OUT, env=env, capture_output=True, timeout=60)
    call = dict(args=list(map(str, args)), exit_code=result.returncode,
                stdout=result.stdout.decode('utf-8', errors='replace'),
                stderr=result.stderr.decode('utf-8', errors='replace'))
    calls.append(call)
    assert result.returncode == expected, call
    return call


def findings(report, kind):
    assert report['$schema'] == f'https://schemas.kicad.org/{kind}.v1.json'
    assert report['kicad_version'] == '10.0.7'
    if kind == 'erc':
        return [value for sheet in report['sheets'] for value in sheet['violations']]
    return [value for key in ['violations', 'unconnected_items', 'schematic_parity'] for value in report[key]]


try:
    assert run('version')['stdout'].strip() == '10.0.7'
    for args in [('sch', 'erc'), ('pcb', 'drc'), ('sch', 'export', 'pdf'),
                 ('sch', 'export', 'netlist'), ('pcb', 'export', 'gerbers'), ('pcb', 'export', 'drill')]:
        run(*args, '--help')
    for file in (PROBE / 'fixtures').iterdir():
        shutil.copy2(file, SOURCE / file.name)
    originals = {file.name: hashlib.sha256(file.read_bytes()).hexdigest() for file in SOURCE.iterdir()}
    results = []
    for mode, kind, extension in [('sch', 'erc', 'kicad_sch'), ('pcb', 'drc', 'kicad_pcb')]:
        reports = []
        for units in ['mm', 'in', 'mils']:
            report_path = OUT / f'{kind}-{units}.json'
            run(mode, kind, '--format', 'json', '--units', units, '--severity-all', '--exit-code-violations',
                '--output', report_path, SOURCE / f'basic_test.{extension}', expected=5)
            report = json.loads(report_path.read_bytes())
            assert report['coordinate_units'] == units
            violations = findings(report, kind)
            assert violations
            assert all(value['severity'] in ['error', 'warning', 'exclusion'] for value in violations)
            reports.append((report, violations))
        a, b, c = reports
        assert [(v['type'], v['severity']) for v in a[1]] == [(v['type'], v['severity']) for v in b[1]]
        position = next(value['items'][0]['pos'] for value in a[1] if value['items'])
        position_in = next(value['items'][0]['pos'] for value in b[1] if value['items'])
        assert abs(position['x'] / 25.4 - position_in['x']) < 1e-5
        assert [(v['type'], v['severity']) for v in a[1]] == [(v['type'], v['severity']) for v in c[1]]
        position_mils = next(value['items'][0]['pos'] for value in c[1] if value['items'])
        assert abs(position['x'] / 0.0254 - position_mils['x']) < 1e-3
        results.append(dict(kind=kind, exit_code=5, findings=len(a[1]), ignored_checks=a[0].get('ignored_checks'),
                            severity_counts={severity: sum(v['severity'] == severity for v in a[1]) for severity in ['error', 'warning', 'exclusion']}))
    # A deliberate short between two named nets tests real geometry findings, not missing libraries.
    shorted = SOURCE / 'shorted.kicad_pcb'
    text = (SOURCE / 'basic_test.kicad_pcb').read_text().rstrip()
    additions = '\n'.join(f'(segment (start {x1} {y1}) (end {x2} {y2}) (width 0.3) (layer "F.Cu") (net {net}) (tstamp 06865587-2b98-4d18-8da3-06075429130{net}))'
                          for x1, y1, x2, y2, net in [(200, 140, 205, 145, 1), (200, 145, 205, 140, 2)])
    shorted.write_text(text[:-1] + additions + '\n)', encoding='utf-8')
    shutil.copy2(SOURCE / 'basic_test.kicad_pro', SOURCE / 'shorted.kicad_pro')
    short_report = OUT / 'shorted.json'
    run('pcb', 'drc', '--format', 'json', '--severity-all', '--exit-code-violations', '--output', short_report, shorted, expected=5)
    shorts = findings(json.loads(short_report.read_bytes()), 'drc')
    baseline_drc = findings(json.loads((OUT / 'drc-mm.json').read_bytes()), 'drc')
    assert not any(value['severity'] == 'error' for value in baseline_drc)
    assert any(value['type'] in ['shorting_items', 'tracks_crossing'] and value['severity'] == 'error' for value in shorts)
    for file in (ROOT / 'tests/kicad/fixtures').iterdir():
        shutil.copy2(file, SOURCE / file.name)
    for mode, kind, extension in [('sch', 'erc', 'kicad_sch'), ('pcb', 'drc', 'kicad_pcb')]:
        clean = OUT / f'clean-{kind}.json'
        run(mode, kind, '--format', 'json', '--severity-all', '--exit-code-violations', '--output', clean,
            SOURCE / f'clean.{extension}')
        assert findings(json.loads(clean.read_bytes()), kind) == []
    pdf = OUT / 'schematic.pdf'
    run('sch', 'export', 'pdf', '--output', pdf, SOURCE / 'basic_test.kicad_sch')
    assert pdf.read_bytes().startswith(b'%PDF-')
    netlist = OUT / 'netlist.xml'
    run('sch', 'export', 'netlist', '--format', 'kicadxml', '--output', netlist, SOURCE / 'basic_test.kicad_sch')
    assert ET.parse(netlist).getroot().tag == 'export'
    gerbers = OUT / 'gerbers'
    run('pcb', 'export', 'gerbers', '--no-protel-ext', '--layers', 'F.Cu,B.Cu,Edge.Cuts', '--output', str(gerbers) + os.sep, SOURCE / 'basic_test.kicad_pcb')
    generated = list(gerbers.glob('*.gbr'))
    assert len(generated) == 3
    assert all(file.read_bytes().rstrip().endswith(b'M02*') for file in generated)
    drill = OUT / 'drill'
    run('pcb', 'export', 'drill', '--format', 'excellon', '--excellon-units', 'mm', '--output', str(drill) + os.sep, SOURCE / 'basic_test.kicad_pcb')
    holes = list(drill.glob('*.drl'))
    assert holes and all(file.read_bytes().startswith(b'M48') for file in holes)
    bad = SOURCE / 'bad.kicad_pcb'
    bad.write_text('invalid board', encoding='ascii')
    rejected = OUT / 'bad-report.json'
    run('pcb', 'drc', '--format', 'json', '--output', rejected, bad, expected=3)
    assert not rejected.exists()
    assert all(hashlib.sha256((SOURCE / name).read_bytes()).hexdigest() == digest for name, digest in originals.items())
    record = dict(passed=True, kicad_version='10.0.7', cli_sha256=hashlib.sha256(EXE.read_bytes()).hexdigest(),
                  findings=results, clean_checks_exit_zero=True, actual_short_error_verified=True, native_ui_verified=False,
                  files={str(file.relative_to(OUT)): hashlib.sha256(file.read_bytes()).hexdigest()
                         for file in OUT.rglob('*') if file.is_file()})
    (OUT / 'result.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
    print(json.dumps(dict(passed=True, evidence=str(OUT))))
finally:
    (OUT / 'calls.json').write_text(json.dumps(calls, indent=2, ensure_ascii=True), encoding='utf-8')
