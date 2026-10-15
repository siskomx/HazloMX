from pathlib import Path
import argparse
import http.client
import http.server
import json
import subprocess
import threading
import time
import uuid
import traceback

parser=argparse.ArgumentParser(description='Real existing Ollama cloud native gate; owned fixture copies, backend approval only, private recording under target.')
parser.add_argument('--run-real-cloud',action='store_true',help='Run five real cloud conversation turns using the existing profile and account.')
parser.add_argument('--program',choices=['schematic','pcb','freecad','drawing','arduino','agent','slice'],default='schematic',help='Native saved-copy, slicing or compile-only workflow to exercise.')
args=parser.parse_args()
if not args.run_real_cloud:
    parser.error('Pass --run-real-cloud for the real-provider acceptance gate.')

workspace = Path(__file__).resolve().parents[3]
example_name={'schematic':'ollama_schematic_chat','pcb':'ollama_pcb_chat','freecad':'ollama_cad_chat','drawing':'ollama_drawing_chat','arduino':'ollama_arduino_chat','agent':'ollama_agent_chat','slice':'ollama_slice_chat'}[args.program]
example = workspace/'target/debug/examples'/f'{example_name}.exe'
if not example.is_file():
    parser.error(f'Build the {example_name} Rust example before running this gate.')
case = workspace/'target'/('ollama-'+args.program+'-chat-'+str(uuid.uuid4()))
case.mkdir()
lock = threading.Condition()
counter = 0
active = 0
successful = {}

def save_status(**values):
    values.update({'active_requests':active,'requests':counter})
    (case/'status.json').write_text(json.dumps(values),encoding='utf-8')

class Recorder(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.0'
    def log_message(self,*args): pass
    def do_GET(self): self.forward()
    def do_POST(self): self.forward()
    def forward(self):
        global counter,active
        if (self.command,self.path) not in [('GET','/api/version'),('GET','/api/tags'),('POST','/api/show'),('POST','/api/chat')]:
            self.send_error(400); return
        count = int(self.headers.get('Content-Length','0'))
        if not 0<=count<=2*1024*1024:
            self.send_error(413); return
        body = self.rfile.read(count)
        payload = json.loads(body) if body else None
        stamp = case/'run/conversation/current-turn'
        turn = int(stamp.read_text()) if stamp.exists() else 0
        with lock:
            counter+=1; number=counter; active+=1
            tools = successful.setdefault(turn,set())
            if self.path=='/api/chat':
                for message in payload.get('messages',[]):
                    if message.get('role')=='tool':
                        result=json.loads(message['content'])
                        if 'result' in result and 'error' not in result:
                            tools.add(message['tool_name'])
                (case/f'run/conversation/tools-{turn}.json').write_text(json.dumps(sorted(tools)),encoding='utf-8')
            save_status(phase='forwarding',turn=turn)
        upstream=http.client.HTTPConnection('127.0.0.1',11434,timeout=120)
        record={'turn':turn,'method':self.command,'path':self.path,'request':payload}
        chunks=[]
        started=time.monotonic()
        try:
            upstream.request(self.command,self.path,body=body if count else None,headers={'Content-Type':'application/json','Accept':'application/json','Connection':'close'})
            response=upstream.getresponse()
            record['status']=response.status
            self.send_response(response.status)
            self.send_header('Content-Type',response.getheader('Content-Type','application/json'))
            length=response.getheader('Content-Length')
            if length is not None:self.send_header('Content-Length',length)
            self.send_header('Connection','close'); self.end_headers(); self.wfile.flush()
            total=0
            while True:
                chunk=response.read1(65536)
                if not chunk:break
                total+=len(chunk)
                if total>2*1024*1024:raise ValueError('Upstream body exceeded recorder bound')
                chunks.append(chunk)
                self.wfile.write(chunk); self.wfile.flush()
            record['response']=json.loads(b''.join(chunks))
        except Exception as error:
            record['recorder_error_type']=type(error).__name__
            record['received_response_bytes']=sum(map(len,chunks))
        finally:
            upstream.close()
            record['elapsed_seconds']=round(time.monotonic()-started,3)
            (case/f'wire-{number}.json').write_text(json.dumps(record,indent=2,ensure_ascii=False),encoding='utf-8')
            with lock:
                active-=1; save_status(phase='idle',turn=turn); lock.notify_all()

server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Recorder)
threading.Thread(target=server.serve_forever,daemon=True).start()
process=None
passed=False
try:
    with (case/'stdout.log').open('wb') as output,(case/'stderr.log').open('wb') as errors:
        process=subprocess.Popen([str(example),str(server.server_port),str(case/'run')],stdout=output,stderr=errors,creationflags=subprocess.CREATE_NO_WINDOW)
        (case/'process.json').write_text(json.dumps({'pid':process.pid}),encoding='utf-8')
        code=process.wait(timeout=1800)
    with lock:
        assert lock.wait_for(lambda:active==0,timeout=130),'Recorder requests did not finish'
    summary_file=case/'run/summary.json'
    passed=code==0 and summary_file.exists()
    if passed:
        summary=json.loads(summary_file.read_text())
        assert summary['pass'] is True
        records=[json.loads(path.read_text(encoding='utf-8')) for path in case.glob('wire-*.json')]
        assert records and all(record.get('status')==200 and 'recorder_error_type' not in record for record in records),'Provider or recorder failed'
        nonces=json.loads((case/'run/conversation/approval-nonces.json').read_text())
        requests='\n'.join(json.dumps(json.loads(path.read_text(encoding='utf-8')).get('request')) for path in case.glob('wire-*.json'))
        assert not any(nonce in requests for nonce in nonces),'Approval nonce entered a model request'
        def strings(value):
            if isinstance(value,str):
                yield value
                try: parsed=json.loads(value)
                except (ValueError,TypeError): return
                yield from strings(parsed)
            elif isinstance(value,dict):
                for item in value.values(): yield from strings(item)
            elif isinstance(value,list):
                for item in value: yield from strings(item)
        home=str(Path.home()).lower()
        for path in case.glob('wire-*.json'):
            payload=json.loads(path.read_text(encoding='utf-8')).get('request')
            assert not any(home in text.lower() or home.replace('\\','/') in text.lower() for text in strings(payload)),'Builder home path entered a provider request'
        assert summary['completed_native_jobs']==2 and summary['conversation_turns']==5
        scope=json.loads((case/'run/conversation/program-scope.json').read_text())
        expected_program={'schematic':'kicad','pcb':'kicad','freecad':'freecad','drawing':'drawing','arduino':'arduino','agent':'agent','slice':'slice'}[args.program]
        assert scope['program']==expected_program
        chats=[record for record in records if record['path']=='/api/chat']
        assert chats and all([tool['function']['name'] for tool in record['request']['tools']]==scope['tools'] for record in chats),'Provider received tools outside the selected program scope'
        assert 'approve_operation' not in scope['tools']
        summary['program_scope_enforced']=True
        summary['advertised_tool_count']=len(scope['tools'])
        summary['approval_nonces_excluded']=True
        summary['builder_home_paths_excluded']=True
    else:
        records=[json.loads(path.read_text(encoding='utf-8')) for path in case.glob('wire-*.json')]
        summary={'pass':False,'exit_code':code,'turns_with_successful_tools':{str(k):sorted(v) for k,v in successful.items() if v},'chat_http_statuses':[r.get('status') for r in records if r['path']=='/api/chat'],'private_logs_retained':True}
    (case/'gate-summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    print(json.dumps(summary))
except Exception as error:
    if process is not None and process.poll() is None:
        process.kill();process.wait(timeout=10)
    (case/'launcher-error.log').write_text(traceback.format_exc(),encoding='utf-8')
    passed=False
    summary={'pass':False,'failure_type':type(error).__name__,'private_logs_retained':True}
    (case/'gate-summary.json').write_text(json.dumps(summary,indent=2),encoding='utf-8')
    print(json.dumps(summary))
finally:
    server.shutdown();server.server_close()
raise SystemExit(0 if passed else 1)
