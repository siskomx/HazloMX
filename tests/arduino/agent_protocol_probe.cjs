// Actual local software protocol checks; never export the bearer token or request headers.
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const assert = require('node:assert/strict');
assert.equal(process.env.USERNAME, 'WDAGUtilityAccount');
const evidence = {pass:false, hardware_mutations:false, steps:[]};
const token = fs.readFileSync(path.join(process.env.USERPROFILE,'.arduinoIDE','mcp-token'),'utf8').trim();
assert(token.length >= 32);
let session;
let id = 1;
function request(body, auth=token, sid=session, origin) {
  return new Promise((resolve,reject)=>{
    const data=JSON.stringify(body);
    const headers={'Content-Type':'application/json',Accept:'application/json, text/event-stream'};
    if(auth) headers.Authorization=`Bearer ${auth}`;
    if(sid) headers['Mcp-Session-Id']=sid;
    if(origin) headers.Origin=origin;
    const req=http.request({hostname:'127.0.0.1',port:3847,path:'/mcp',method:'POST',headers},res=>{
      let text=''; res.on('data',chunk=>{text+=chunk; if(text.length>1048576) req.destroy(new Error('Response too large'));});
      res.on('end',()=>{
        let json=null;
        if(text.trim()) {
          const message=text.startsWith('event:')||text.startsWith('data:') ? text.split('\n').filter(line=>line.startsWith('data:')).map(line=>line.slice(5).trim()).at(-1) : text;
          try {json=JSON.parse(message);} catch {return reject(new Error('Invalid response JSON'));}
        }
        resolve({status:res.statusCode,session:res.headers['mcp-session-id'],json});
      });
    });
    req.setTimeout(30000,()=>req.destroy(new Error('Request timeout')));req.on('error',reject);req.end(data);
  });
}
const rpc=(method,params)=>({jsonrpc:'2.0',id:id++,method,params});
async function call(name,args) {
  const response=await request(rpc('tools/call',{name,arguments:args}));
  assert.equal(response.status,200);
  assert(!response.json.error, JSON.stringify(response.json.error));
  return response.json.result;
}
(async()=>{
  try {
    const untouched='C:\\AgentSketchbook\\ProbeSketch\\ProbeSketch.ino';
    fs.mkdirSync(path.dirname(untouched),{recursive:true});
    const original='void setup() {}\nvoid loop() {}\n';fs.writeFileSync(untouched,original);
    const write=rpc('tools/call',{name:'execute_tool',arguments:{tool_name:'arduino_sketch',params:{action:'set_content',path:untouched,content:'changed'}}});
    assert.equal((await request(write,'',undefined)).status,401);
    assert.equal((await request(write,'wrong-token',undefined)).status,401);
    if(process.env.HAZLO_PROBE_OLD_TOKEN) {
      assert.notEqual(process.env.HAZLO_PROBE_OLD_TOKEN,token);
      assert.equal((await request(write,process.env.HAZLO_PROBE_OLD_TOKEN,undefined)).status,401);
      evidence.steps.push('rotated previous token rejected before write');
    }
    assert.equal((await request(write,token,'expired-fixture-session')).status,404);
    assert.equal((await request(write,token,undefined,'https://example.invalid')).status,403);
    assert.equal(fs.readFileSync(untouched,'utf8'),original); evidence.steps.push('auth/stale-session/origin rejected before write');
    const initialized=await request(rpc('initialize',{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'HazloMexa-probe',version:'0.1.0'}}),token,undefined);
    assert.equal(initialized.status,200); assert(initialized.session); session=initialized.session;
    evidence.initialize=initialized.json.result;
    await request({jsonrpc:'2.0',method:'notifications/initialized'});
    const list=await request(rpc('tools/list',{}));assert.equal(list.status,200);
    evidence.tools=list.json.result.tools;
    assert(evidence.tools.some(tool=>tool.name==='execute_tool'));
    evidence.sketch_catalog=await call('get_category_tools',{category:'sketch'});
    const source=await call('execute_tool',{tool_name:'arduino_sketch',params:{action:'get_content',path:untouched}});
    assert(!source.isError);evidence.source_read=source;
    const updated='void setup() {}\nvoid loop() { delay(500); }\n';
    const written=await call('execute_tool',{tool_name:'arduino_sketch',params:{action:'set_content',path:untouched,content:updated}});
    assert(!written.isError);assert.equal(fs.readFileSync(untouched,'utf8'),updated);
    evidence.steps.push('authenticated router read/write round trip');
    evidence.pass=true;
  } catch(error) {evidence.error=error.message;process.exitCode=1;}
  fs.writeFileSync('C:\\AgentProbe\\output\\protocol-result.json',JSON.stringify(evidence,null,2));
})();
