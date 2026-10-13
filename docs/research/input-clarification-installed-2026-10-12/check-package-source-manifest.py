import hashlib
import json
from pathlib import Path
import tempfile

source = Path('apps/desktop/tests/prepare-current-package.py').read_text(encoding='utf-8')
block = source[source.index('source_paths ='):source.rindex('print(root)')]
with tempfile.TemporaryDirectory(prefix='source-manifest-', dir='target') as temporary:
    prepared = Path(temporary).resolve()
    assert prepared.is_relative_to(Path('target').resolve())
    exec(block, {'Path': Path, 'root': prepared, 'json': json,
                 'digest': lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()})
    records = json.loads((prepared / 'release-inputs.json').read_text(encoding='utf-8'))
    required = {'Cargo.lock', 'apps/desktop/package-lock.json', 'apps/desktop/src/main.ts',
                'apps/desktop/src-tauri/src/jobs.rs', 'apps/desktop/src-tauri/src/host_tools.rs',
                'apps/desktop/tests/sandbox-current-package.ps1',
                'apps/desktop/src-tauri/examples/installed_konnect_workflow.rs'}
    assert required <= {r['path'] for r in records}
    assert len(records) == len({r['path'] for r in records})
    assert all(hashlib.sha256(Path(r['path']).read_bytes()).hexdigest() == r['sha256'] for r in records)
    print(json.dumps({'pass': True, 'source_files': len(records), 'guest_started': False}))
