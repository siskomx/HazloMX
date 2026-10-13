// Owned guest only: forward to the real Agent, then hold one confirmed sketch-write reply.
const fs=require('node:fs');
const http=require('node:http');
const path=require('node:path');
const assert=require('node:assert/strict');
assert.equal(process.env.USERNAME,'WDAGUtilityAccount');
const output='C:\\AgentProbe\\output';
let heldCount=0;
http.createServer((req,res)=>{
  let chunks=[];let size=0;
  req.on('data',chunk=>{size+=chunk.length;if(size>1048576)req.destroy();else chunks.push(chunk);});
  req.on('end',()=>{
    const body=Buffer.concat(chunks);
    let hold=null;
    try {
      const armed=JSON.parse(fs.readFileSync(path.join(output,'hold-job.json'),'utf8'));
      const rpc=JSON.parse(body.toString('utf8'));
      const args=rpc.params?.arguments;
      if(rpc.method==='tools/call'&&rpc.params.name==='execute_tool'&&args?.tool_name==='arduino_sketch'&&args.params.action==='set_content') {
        const expected=path.join(process.env.TEMP,`hazlomexa-agent-${armed.job_id}`,'Sketch','Sketch.ino');
        if(args.params.path===expected) hold={job_id:armed.job_id,path:expected,content:args.params.content};
      }
    } catch { /* Unarmed traffic continues normally. */ }
    const headers={...req.headers,host:'127.0.0.1:3848'};delete headers.connection;
    const upstream=http.request({hostname:'127.0.0.1',port:3848,path:req.url,method:req.method,headers},reply=>{
      if(!hold) {res.writeHead(reply.statusCode,reply.headers);reply.pipe(res);return;}
      let response=[];let bytes=0;
      reply.on('data',chunk=>{bytes+=chunk.length;assert(bytes<=1048576);response.push(chunk);});
      reply.on('end',()=>{
        assert.equal(reply.statusCode,200);
        assert.equal(fs.readFileSync(hold.path,'utf8'),hold.content,'Real Agent did not apply the approved write');
        heldCount++;
        const save=(name,value)=>fs.writeFileSync(path.join(output,name),JSON.stringify(value,null,2));
        save('held-reply.json',{pass:true,job_id:hold.job_id,path:hold.path,upstream_write_verified:true,write_requests:heldCount});
        res.on('close',()=>save('held-client-closed.json',{job_id:hold.job_id,unix_ms:Date.now()}));
        const timer=setInterval(()=>{
          if(!fs.existsSync(path.join(output,'release-reply.json')))return;
          clearInterval(timer);
          const data=Buffer.concat(response);const forwarded={...reply.headers};delete forwarded['transfer-encoding'];delete forwarded.connection;
          forwarded['content-length']=data.length;
          if(!res.destroyed){res.writeHead(reply.statusCode,forwarded);res.end(data);}
          save('held-released.json',{pass:true,job_id:hold.job_id,client_already_closed:res.destroyed,write_requests:heldCount});
        },20);
        setTimeout(()=>clearInterval(timer),30000).unref();
      });
    });
    upstream.on('error',()=>{if(!res.destroyed){res.writeHead(503);res.end();}});
    upstream.end(body);
  });
}).listen(3847,'127.0.0.1');
