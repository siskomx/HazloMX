"""Actual pinned Windows CLI proof; writes only a fresh owned target directory."""
import hashlib
import json
import pathlib
import subprocess
import uuid
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[2]
EXE = ROOT / 'target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe'
OUT = ROOT / 'target/cadcraft-probe' / ('run-' + str(uuid.uuid4()))
OUT.mkdir()
calls = []


def run(*args, ok=True):
    result = subprocess.run([str(EXE), *map(str, args)], capture_output=True, timeout=30)
    item = dict(args=list(map(str, args)), exit_code=result.returncode,
                stdout=result.stdout.decode('utf-8'), stderr=result.stderr.decode('utf-8'))
    calls.append(item)
    assert (result.returncode == 0) == ok, item
    return item


def inspect(path):
    return json.loads(run('info', path)['stdout'])


def tags(path):
    lines = path.read_text(encoding='ascii').splitlines()
    assert len(lines) % 2 == 0
    return [(int(lines[i].strip()), lines[i + 1].strip()) for i in range(0, len(lines), 2)]


def header(path, name):
    values = tags(path)
    index = values.index((9, name))
    return values[index + 1][1]


try:
    assert run('--version')['stdout'].strip() == 'cadcraft-cli 0.5.0'
    catalog = json.loads(run('commands')['stdout'])
    ids = [entry['id'] for entry in catalog]
    assert len(ids) == len(set(ids))
    assert {'drawing.inspect', 'units', 'circle', 'document.bytes', 'saveas'} <= set(ids)
    units = []
    for metric, expected in [(False, '1'), (True, '4')]:
        source = OUT / ('metric.dxf' if metric else 'imperial.dxf')
        args = ['run'] + (['--metric'] if metric else [])
        run(*args, '--cmd', 'circle {"center":[0,0],"radius":10}', '--save', source)
        assert header(source, '$INSUNITS') == expected
        before = source.read_bytes()
        original = inspect(source)
        assert original['counts'] == {'Circle': 1}
        assert original['extents'] == [[-10.0, -10.0], [10.0, 10.0]]
        copy = source.with_name(source.stem + '-copy.dxf')
        run('convert', source, copy)
        after = inspect(copy)
        assert after['counts'] == original['counts']
        assert after['extents'] == original['extents']
        assert header(copy, '$INSUNITS') == expected
        assert source.read_bytes() == before
        dwg = source.with_suffix('.dwg')
        run('convert', source, dwg)
        assert dwg.read_bytes().startswith(b'AC10')
        dwg_info = inspect(dwg)
        assert dwg_info['counts'] == original['counts']
        assert dwg_info['extents'] == original['extents']
        reopened = source.with_name(source.stem + '-dwg-reopened.dxf')
        run('convert', dwg, reopened)
        assert header(reopened, '$INSUNITS') == expected
        svg = source.with_suffix('.svg')
        run('convert', source, svg, '--view', 'extents')
        assert ET.parse(svg).getroot().tag == '{http://www.w3.org/2000/svg}svg'
        png = source.with_suffix('.png')
        run('convert', source, png, '--view', 'extents')
        data = png.read_bytes()
        assert data[:8] == b'\x89PNG\r\n\x1a\n'
        assert int.from_bytes(data[16:20], 'big') == 2400
        assert int.from_bytes(data[20:24], 'big') == 1600
        units.append(dict(metric=metric, insunits=expected, counts=after['counts'], extents=after['extents']))

    # A supported entity plus an unsupported imported DXF entity: success is not losslessness.
    source = OUT / 'metric.dxf'
    values = tags(source)
    index = values.index((2, 'ENTITIES')) + 1
    values[index:index] = [(0, '3DSOLID'), (5, 'FF00'), (100, 'AcDbEntity'),
                           (8, '0'), (100, 'AcDbModelerGeometry'), (70, '1')]
    foreign = OUT / 'foreign.dxf'
    foreign.write_text(''.join(f'{code}\n{value}\n' for code, value in values), encoding='ascii')
    before = foreign.read_bytes()
    input_info = inspect(foreign)
    lost = OUT / 'foreign-copy.dxf'
    conversion = run('convert', foreign, lost)
    output_info = inspect(lost)
    assert 'Not saved' in conversion['stderr'] and '1 3DSOLID' in conversion['stderr']
    assert input_info['entityCount'] == output_info['entityCount'] + 1
    assert foreign.read_bytes() == before

    bad = OUT / 'bad.dxf'
    bad.write_text('invalid drawing', encoding='ascii')
    rejected = OUT / 'bad-output.dxf'
    run('convert', bad, rejected, ok=False)
    assert not rejected.exists()
    run('run', '--cmd', 'circle {"center":[0,0],"radius":10}', '--unknown', '--save', rejected, ok=False)
    assert not rejected.exists()
    run('convert', source, OUT / 'bad-window.png', '--window', '0,0,NaN,1', ok=False)
    assert not (OUT / 'bad-window.png').exists()

    # The upstream CLI overwrites an existing output: app staging must enforce nonreplacement.
    occupied = OUT / 'occupied.dxf'
    occupied.write_text('owned sentinel', encoding='ascii')
    run('convert', source, occupied)
    assert occupied.read_bytes() != b'owned sentinel'
    result = dict(passed=True, version='0.5.0', cli_sha256=hashlib.sha256(EXE.read_bytes()).hexdigest(),
                  units=units, conversion_reports_loss_with_exit_zero=True,
                  upstream_overwrites_output=True, native_ui_verified=False,
                  hashes={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in OUT.iterdir() if p.is_file()})
    (OUT / 'result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps({'passed': True, 'evidence': str(OUT)}))
finally:
    (OUT / 'calls.json').write_text(json.dumps(calls, indent=2, ensure_ascii=True), encoding='utf-8')
