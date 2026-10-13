"""Check staged text for the current Windows profile path and credential patterns."""
import os
import json
from pathlib import Path, PureWindowsPath
import re
import subprocess

profile = os.environ.get('USERPROFILE')
patterns = [rb'gh[pousr]_[A-Za-z0-9]{30,}', rb'sk-[A-Za-z0-9]{24,}',
            rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
            rb'eyJ[A-Za-z0-9_-]{15,}[.]eyJ[A-Za-z0-9_-]{15,}[.][A-Za-z0-9_-]{15,}']
if profile:
    parts = str(PureWindowsPath(profile)).replace('\\', '/').split('/')
    rule = re.compile(rb'[\\/]+'.join(re.escape(p.encode()) for p in parts), re.I)
    for form in [profile, profile.replace('\\', '/'), json.dumps(profile)]:
        assert rule.search(form.encode()), 'Windows path guard failed its self-check.'
    patterns.append(rule.pattern)
email = subprocess.run(['git', 'config', '--get', 'user.email'], capture_output=True).stdout.strip()
if email and not email.endswith(b'@users.noreply.github.com'):
    patterns.append(re.escape(email))
private = re.compile(b'|'.join(patterns), re.I)
names = subprocess.check_output(['git', 'ls-files', '-z']).split(b'\0')
requests = [b':' + name for name in names if name]
result = subprocess.run(['git', 'cat-file', '--batch'], input=b'\n'.join(requests) + b'\n',
                        capture_output=True, check=True).stdout
offset = 0
failures = 0
details = []
for request in requests:
    end = result.index(b'\n', offset)
    header = result[offset:end].split()
    if header[-1] == b'missing':
        raise SystemExit('Missing staged file; publication check refused.')
    size = int(header[-1])
    data = result[end + 1:end + 1 + size]
    if data.startswith((b'\xff\xfe', b'\xfe\xff')):
        data = data.decode('utf-16').encode()
    if b'\0' not in data and private.search(data):
        failures += 1
        details.append({'file': request[1:].decode(),
                        'rules': [i for i, pattern in enumerate(patterns) if re.search(pattern, data, re.I)]})
    offset = end + 2 + size
print(f'Publication privacy check: {failures} matching staged text files.')
if failures:
    Path('target').mkdir(exist_ok=True)
    Path('target/private-publication-findings.json').write_text(json.dumps(details, indent=2))
raise SystemExit(1 if failures else 0)
