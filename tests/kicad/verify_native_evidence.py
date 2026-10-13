"""Check retained native Sky observations against actual source/output/restart bytes."""
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1]).resolve()
project = root / 'project'
snapshots = [json.loads(p.read_text(encoding='utf-8')) for p in (project / '.hazlomexa/runs').glob('*.json')]
assert len(snapshots) == 1 and snapshots[0]['phase'] == 'completed'
snapshot = snapshots[0]
assert snapshot['input']['units'] == 'mils'
assert len(snapshot['input']['files']) == 4
outputs = project / '.hazlomexa/artifacts' / snapshot['input']['job_id']
result = json.loads((outputs / 'result.json').read_text(encoding='utf-8'))
assert result['summary']['errors'] == 2 and result['summary']['warnings'] == 1
assert result['summary']['units'] == 'mils' and len(result['summary']['ignored_checks']) == 4
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
for source in snapshot['input']['files']:
    assert digest(root / 'source' / source['path']) == source['sha256']
    assert digest(outputs / 'Original' / source['path']) == source['sha256']
for output in result['files']:
    assert digest(outputs / output['path']) == output['sha256']
assert (outputs / 'Exports/Schematic.pdf').read_bytes().startswith(b'%PDF-')
assert (outputs / 'Exports/Netlist.xml').read_bytes().startswith(b'<?xml')
assert not list((project / '.hazlomexa/staging').iterdir())
before = json.loads((root / 'before-restart-hashes.json').read_text(encoding='utf-8'))
after = {str(p.relative_to(project)): digest(p) for p in project.rglob('*') if p.is_file() and p.name != 'writer.lock'}
assert before == after
for name in ['findings-ui.txt', 'recovered-findings-ui.txt']:
    text = (root / name).read_text(encoding='utf-8')
    for phrase in ['Hallazgos ERC', 'informe mils', '2 errores', '1 advertencias', '4 comprobaciones desactivadas']:
        assert phrase in text, (name, phrase)
assert 'Ver informe ERC/DRC' in (root / 'restarted-ui.txt').read_text(encoding='utf-8')
assert 'Exports' in (root / 'exports-ui.txt').read_text(encoding='utf-8')
record = dict(passed=True, fixture=str(root), job_id=snapshot['input']['job_id'],
              summary=result['summary'], output_hashes={f['path']: f['sha256'] for f in result['files']},
              before_restart_hashes=before, after_restart_hashes=after,
              project_seeded_by_debug_environment=True,
              native_source_folder_and_file_pickers=True,
              native_runtime_picker=True, native_schematic_workflow=True,
              native_pcb_workflow_verified=False, installed_workflow_verified=False,
              in_flight_cancellation_verified=False, timeout_verified=False,
              observation_method='Sky Windows UI; filesystem assertions replay retained observations')
(root / 'verified-evidence.json').write_text(json.dumps(record, indent=2), encoding='utf-8')
print(json.dumps(dict(passed=True, evidence=str(root / 'verified-evidence.json'))))
