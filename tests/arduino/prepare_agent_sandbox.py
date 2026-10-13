"""Prepare an offline, owned guest for the real Arduino Agent software protocol."""
import json
from pathlib import Path
import shutil
import uuid
import sys
import os
import xml.etree.ElementTree as ET
import psutil

root = Path(__file__).resolve().parents[2]
held = '--held-reply' in sys.argv
arguments=[value for value in sys.argv[1:] if value!='--held-reply']
assert not arguments or len(arguments)==2 and arguments[0]=='--backend-tests', 'Usage: prepare_agent_sandbox.py [--backend-tests <owned-test-exe>] [--held-reply]'
package = json.loads((root/'target/arduino-agent-probe/package-result.json').read_text())
assert package['pass'] and not package['launched']
assert psutil.virtual_memory().available >= 3072 * 1048576
fixture = root/'target/arduino-agent-probe/sandbox'/str(uuid.uuid4())
(fixture/'input').mkdir(parents=True)
(fixture/'output').mkdir()
for source, name in [('agent_guest.ps1','probe.ps1'), ('agent_protocol_probe.cjs','protocol.cjs'),
                     ('agent_rotate_guest.ps1','rotate.ps1'), ('agent_adapter_guest.ps1','adapter.ps1'),
                     ('agent_workflow_guest.ps1','workflow.ps1'), ('agent_export_guest.ps1','export.ps1')]:
    shutil.copy2(root/'tests/arduino'/source, fixture/'input'/name)
if held:
    shutil.copy2(fixture/'input/probe.ps1',fixture/'input/agent-base.ps1')
    shutil.copy2(root/'tests/arduino/agent_start_held.ps1',fixture/'input/probe.ps1')
    shutil.copy2(root/'tests/arduino/agent_held_reply.cjs',fixture/'input/held-reply.cjs')
    shutil.copy2(root/'tests/arduino/agent_cancel_guest.ps1',fixture/'input/cancel.ps1')
    shutil.copy2(root/'tests/arduino/agent_export_cancel.ps1',fixture/'input/export-cancel.ps1')
if arguments:
    tests=Path(arguments[1]).resolve(strict=True)
    assert tests.is_relative_to((root/'target/debug/deps').resolve()) and tests.name.startswith('hazlomexa-') and tests.suffix=='.exe'
    shutil.copy2(tests,fixture/'input/backend-tests.exe')
    shutil.copy2(Path(os.environ['SystemRoot'])/'System32/vcruntime140.dll',fixture/'input/vcruntime140.dll')
config = ET.Element('Configuration')
for name, value in [('MemoryInMB','2048'),('Networking','Disable'),('ClipboardRedirection','Disable'),('AudioInput','Disable')]:
    ET.SubElement(config,name).text=value
folders=ET.SubElement(config,'MappedFolders')
for host, guest, readonly in [(fixture/'input','C:\\AgentProbe\\input','true'),(fixture/'output','C:\\AgentProbe\\output','false'),
                              (Path(package['executable']).parent,'C:\\AgentRO','true'),
                              (Path(shutil.which('node')).parent,'C:\\NodeRO','true'),
                              (root/'target/arduino-probe/data','C:\\ArduinoDataRO','true')]:
    folder=ET.SubElement(folders,'MappedFolder')
    for name, value in [('HostFolder',str(host)),('SandboxFolder',guest),('ReadOnly',readonly)]:
        ET.SubElement(folder,name).text=value
ET.SubElement(ET.SubElement(config,'LogonCommand'),'Command').text='powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\AgentProbe\\input\\probe.ps1'
ET.ElementTree(config).write(fixture/'probe.wsb',encoding='utf-8',xml_declaration=True)
assert ET.parse(fixture/'probe.wsb').findtext('Networking')=='Disable'
print(fixture)
