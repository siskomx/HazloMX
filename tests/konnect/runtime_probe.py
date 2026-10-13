"""Bounded raw-wire discovery probe; not the application's production MCP client."""
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import copy
import threading
import uuid
from verify_schematic import assert_only_value_changed, assert_netlist

ROOT = Path(__file__).resolve().parents[2]
base = ROOT/'target/konnect-probe'
output = base/('discovery-'+str(uuid.uuid4()))
output.mkdir()
exe = base/'runtime/konnect.exe'
identity = json.loads((base/'identity.json').read_text())
assert hashlib.sha256(exe.read_bytes()).hexdigest()==identity['runtime_files']['konnect.exe']['sha256']
profile = output/'profile'
profile.mkdir()
environment = {key:os.environ[key] for key in ['SystemRoot','WINDIR','COMSPEC'] if key in os.environ}
for key in ['APPDATA','LOCALAPPDATA','USERPROFILE','TEMP','TMP','KONNECT_STATE_DIR']:
    environment[key]=str(profile)
environment['PATH']=os.environ['SystemRoot']+'/System32'
cli = ROOT/'target/kicad-probe/runtime/bin/kicad-cli.exe'
environment['KICAD_CONFIG_HOME']=str(profile/'kicad-config')
(profile/'kicad-config').mkdir()
for key,name in [('KICAD10_SYMBOL_DIR','symbols'),('KICAD10_FOOTPRINT_DIR','footprints')]:
    environment[key]=str(cli.parent.parent/'share/kicad'/name)
original_dir=output/'Original'
edited_dir=output/'Edited'
original_dir.mkdir()
edited_dir.mkdir()
originals={}
for name in ['basic_test.kicad_sch','basic_test.kicad_pcb','basic_test.kicad_pro','custom_ds.kicad_wks']:
    data=(ROOT/'target/kicad-probe/fixtures'/name).read_bytes()
    (original_dir/name).write_bytes(data)
    (edited_dir/name).write_bytes(data)
    originals[name]=data
config = output/'config.json'
config.write_text(json.dumps(dict(transport='stdio',project_dir=str(edited_dir),ipc_address='ipc://'+str(profile/'not-a-kicad-socket'),
    kicad_cli=str(cli),kicad_binary=str(cli.parent/'kicad.exe'),log_level='warn',auto_load_toolsets=False,eager_toolsets=False)),encoding='utf-8')
for args,name in [(['--help'],'help'),(['--version'],'version')]:
    result = subprocess.run([str(exe),*args],cwd=output,env=environment,stdin=subprocess.DEVNULL,capture_output=True,timeout=15)
    (output/(name+'-stdout.log')).write_bytes(result.stdout)
    (output/(name+'-stderr.log')).write_bytes(result.stderr)
    assert result.returncode==0
    if name=='version': assert result.stdout.strip()==b'konnect 0.13.0'
messages = queue.Queue(maxsize=512)
wire = []
stderr = bytearray()
process = subprocess.Popen([str(exe),'--config',str(config)],cwd=output,env=environment,
                           stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)


def read_stdout():
    try:
        total=0
        while True:
            line=process.stdout.readline(2*1024*1024+1)
            if not line: return
            total+=len(line)
            assert len(line)<=2*1024*1024 and total<=8*1024*1024
            value=json.loads(line)
            wire.append(value)
            messages.put(value,timeout=5)
    except BaseException as error:
        messages.put(error,timeout=5)


def read_stderr():
    while data:=process.stderr.read(8192):
        stderr.extend(data[:max(0,262144-len(stderr))])


readers=[threading.Thread(target=read_stdout,daemon=True),threading.Thread(target=read_stderr,daemon=True)]
for reader in readers: reader.start()
counter=0


def rpc(method,params=None):
    global counter
    counter+=1
    value=dict(jsonrpc='2.0',id=counter,method=method)
    if params is not None: value['params']=params
    process.stdin.write((json.dumps(value)+'\n').encode())
    process.stdin.flush()
    while True:
        reply=messages.get(timeout=20)
        if isinstance(reply,BaseException): raise reply
        if 'id' not in reply: continue
        assert reply['id']==counter and 'error' not in reply,reply
        return reply['result']


def tool(name,arguments):
    result=rpc('tools/call',dict(name=name,arguments=arguments))
    assert not result.get('isError',False),result
    blocks=[item['text'] for item in result['content'] if item['type']=='text']
    assert len(blocks)==1,result
    return json.loads(blocks[0])


passed=False
checks={}
try:
    initialized=rpc('initialize',dict(protocolVersion='2025-11-25',capabilities={},clientInfo=dict(name='hazlomexa-konnect-probe',version='0.1.0')))
    assert initialized['serverInfo']==dict(name='konnect',version='0.13.0')
    assert initialized['protocolVersion']=='2025-06-18' and initialized['capabilities']['tools']['listChanged'] is True
    process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
    process.stdin.flush()
    initial=rpc('tools/list')
    names={tool['name'] for tool in initial['tools']}
    assert {'load_toolset','unload_toolset','get_installation_info','list_toolboxes'}<=names
    (output/'initialize.json').write_text(json.dumps(initialized,indent=2),encoding='utf-8')
    (output/'initial-tools.json').write_text(json.dumps(initial,indent=2),encoding='utf-8')
    toolboxes=rpc('tools/call',dict(name='list_toolboxes',arguments={}))
    (output/'toolboxes.json').write_text(json.dumps(toolboxes,indent=2),encoding='utf-8')
    loaded=rpc('tools/call',dict(name='load_toolset',arguments={'name':['sch_components','sch_analysis','sch_export']}))
    assert not loaded.get('isError',False),loaded
    expanded=rpc('tools/list')
    assert len(expanded['tools'])>len(initial['tools'])
    assert any(message.get('method')=='notifications/tools/list_changed' for message in wire)
    (output/'expanded-tools.json').write_text(json.dumps(expanded,indent=2),encoding='utf-8')
    schematic=str(edited_dir/'basic_test.kicad_sch')
    components_before=tool('list_schematic_components',{'schematic':schematic})
    selected_before=tool('get_schematic_component',{'schematic':schematic,'reference':'R1'})
    edited=tool('edit_schematic_component',{'schematic':schematic,'reference':'R1','value':'12.34k'})
    selected_after=tool('get_schematic_component',{'schematic':schematic,'reference':'R1'})
    components_after=tool('list_schematic_components',{'schematic':schematic})
    (output/'schematic-edit.json').write_text(json.dumps(dict(before=selected_before,edit=edited,after=selected_after,components_before=components_before,components_after=components_after),indent=2),encoding='utf-8')
    assert selected_before!=selected_after and (edited_dir/'basic_test.kicad_sch').read_bytes()!=originals['basic_test.kicad_sch']
    expected_selected=copy.deepcopy(selected_before)
    expected_selected['value']='12.34k'
    expected_selected['properties']['Value']='12.34k'
    assert selected_after==expected_selected,'Non-value component metadata changed'
    expected_components=copy.deepcopy(components_before)
    for component in expected_components['components']:
        if component['reference']=='R1':
            component['value']='12.34k'
            component['properties']['Value']='12.34k'
    assert components_after==expected_components,'Other component metadata changed'
    assert_only_value_changed(originals['basic_test.kicad_sch'],(edited_dir/'basic_test.kicad_sch').read_bytes(),'R1','12.34k')
    checks['only_approved_value_changed']=True
    # A held KiCad-editor lock must refuse the file write and preserve bytes.
    lock=edited_dir/'~basic_test.kicad_sch.lck'
    lock.write_text('{"username":"owned-fixture","hostname":"owned-fixture"}',encoding='utf-8')
    before_locked=(edited_dir/'basic_test.kicad_sch').read_bytes()
    refused=rpc('tools/call',dict(name='edit_schematic_component',arguments={'schematic':schematic,'reference':'R1','value':'99k'}))
    assert refused.get('isError') is True,refused
    refusal=json.loads(refused['content'][0]['text'])
    assert refusal['error']['kind']=='conflict',refusal
    assert (edited_dir/'basic_test.kicad_sch').read_bytes()==before_locked and lock.exists()
    checks['editor_lock_refused_without_write']=True
    (output/'lock-refusal.json').write_text(json.dumps(refused,indent=2),encoding='utf-8')
    lock.unlink()
    netlist=output/'Edited.net'
    command=[str(cli),'sch','export','netlist','--format','kicadxml','--output',str(netlist),schematic]
    native=subprocess.run(command,cwd=edited_dir,env=environment,stdin=subprocess.DEVNULL,capture_output=True,timeout=60)
    (output/'netlist-stdout.log').write_bytes(native.stdout)
    (output/'netlist-stderr.log').write_bytes(native.stderr)
    assert native.returncode==0,(native.stdout,native.stderr)
    assert_netlist(netlist,'R1','12.34k')
    checks['independent_kicad_netlist_readback']=True
    checks['netlist_command']=command
    checks['kicad_cli_sha256']=hashlib.sha256(cli.read_bytes()).hexdigest()
    for name,data in originals.items():
        assert (original_dir/name).read_bytes()==data
        if name!='basic_test.kicad_sch': assert (edited_dir/name).read_bytes()==data
    checks['originals_and_companions_preserved']=True
    checks['original_sha256']={name:hashlib.sha256(data).hexdigest() for name,data in originals.items()}
    unloaded=rpc('tools/call',dict(name='unload_toolset',arguments={'name':'sch_components'}))
    assert not unloaded.get('isError',False),unloaded
    reduced=rpc('tools/list')
    assert len(reduced['tools'])<len(expanded['tools'])
    (output/'reduced-tools.json').write_text(json.dumps(reduced,indent=2),encoding='utf-8')
    passed=True
finally:
    process.stdin.close()
    try: exit_code=process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        exit_code=process.wait(timeout=5)
        passed=False
    for reader in readers: reader.join(timeout=5)
    (output/'wire.json').write_text(json.dumps(wire,indent=2),encoding='utf-8')
    (output/'stderr.log').write_bytes(stderr)
    (output/'status.json').write_text(json.dumps(dict(passed=passed and exit_code==0,exit_code=exit_code,
        server_pid=process.pid,private_configuration=str(config),runtime_identity=identity,
        checks=checks,source_sha256={name:hashlib.sha256((Path(__file__).parent/name).read_bytes()).hexdigest()
            for name in ['runtime_probe.py','verify_schematic.py','test_verify_schematic.py']},
        raw_wire_probe=True,rust_sdk_verified=False,native_gui_verified=False),indent=2),encoding='utf-8')
assert passed and exit_code==0,str(output)
print(json.dumps(dict(passed=True,evidence=str(output),initial_tools=len(initial['tools']),expanded_tools=len(expanded['tools']),reduced_tools=len(reduced['tools']))))
