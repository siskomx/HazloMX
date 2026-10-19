import assert from "node:assert/strict";
import path from "node:path";
import {createServer} from "vite";
import {chromium} from "playwright-core";

const root=path.resolve(import.meta.dirname,"..");
const server=await createServer({root,server:{host:"127.0.0.1",port:0}});
let browser;
try {
 await server.listen();
 browser=await chromium.launch({executablePath:process.env.HAZLOMEXA_TEST_BROWSER??"C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",headless:true});
 const page=await browser.newPage();
 page.on("pageerror",e=>console.error("Owned task fixture page error:",e.message));
 await page.addInitScript(()=>{
  const saved=JSON.parse(localStorage.getItem("owned-task-fixture")??"null");
  const db=saved??{active:null,records:{},project:null};
  window.calls=[];
  let snapshot={project:db.project,task_support:true,tasks:null,job:null,inference:null,recovered:[],freecad:"",freecad_available:false,codex:{version:"0.159.2"},codex_login:{id:"owned-login",sequence:1,busy:false,connected:true},claude:{version:"2.1.302"},chat:null};
  const task=()=>db.records[db.active];
  const state=()=>db.active?{active:{...task(),history:undefined},tasks:Object.values(db.records).map(t=>({id:t.id,title:t.title}))}:null;
  const persist=()=>{snapshot.tasks=state();localStorage.setItem("owned-task-fixture",JSON.stringify(db));};
  const create=()=>{const id=crypto.randomUUID();db.records[id]={id,revision:1,title:"Nueva conversación",goal:"",provider:task()?.provider??"codex",references:[],restricted_results:false,history:JSON.stringify({version:1,turns:[]})};db.active=id;};
  persist();
  window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(command,args={})=>{
   window.calls.push({command,args:structuredClone(args)});
   if(command==="ensure_task_workspace"){db.project=["owned-project","C:/OwnedFixture/Task"];snapshot.project=db.project;create();task().provider=args.provider??"codex";persist();}
   else if(command==="task_action"){
    const r=args.request;
    if(r.action==="new")create();
    else if(r.action==="open")db.active=r.id;
    else {
     const t=task();if(r.id!==t.id || r.revision!==t.revision)throw Error("stale task");
     if(r.action==="history"){if(r.value==null)return t.history;t.history=r.value;}
     if(r.action==="message"){t.revision++;t.history=r.history;if(!t.goal){t.goal=r.text;t.title=r.text.slice(0,60);}}
     if(r.action==="provider"){t.revision++;t.provider=r.provider;}
     if(r.action==="add_reference"){t.revision++;t.references.push({id:crypto.randomUUID(),name:r.name,bytes:atob(r.data).length,sha256:"f".repeat(64),image:!!r.preview,shared_with:[],preview:r.preview,data:r.data});}
     if(r.action==="preview_reference"){const ref=t.references.find(x=>x.id===r.reference);if(!ref)throw Error("missing local reference");const text=atob(ref.data),offset=r.offset??0,next=offset+4096;return {kind:"text",content:text.slice(offset,next),next_offset:next<text.length?next:null};}
     if(r.action==="share"){const ref=t.references.find(x=>x.id===r.reference);ref.shared_with=ref.shared_with.filter(p=>p!==t.provider);if(r.allow)ref.shared_with.push(t.provider);t.revision++;}
     if(r.action==="remove"){const removed=t.references.find(x=>x.id===r.reference);t.references=t.references.filter(x=>x.id!==r.reference);t.pending_removals=[{id:removed.id,bytes:removed.bytes}];t.revision++;}
     if(r.action==="image"){const ref=t.references.find(x=>x.id===r.reference);if(!ref?.shared_with.includes(t.provider))throw Error("sharing required");return structuredClone(ref.preview);}
    }
    persist();return structuredClone(state());
   }
   else if(command.startsWith("start_") && command.endsWith("_chat")){
    if(window.nativeOnNext){task().revision++;task().references.push({id:crypto.randomUUID(),name:"Observación nativa.json",bytes:100,sha256:"e".repeat(64),image:false,shared_with:[]});window.nativeOnNext=false;}
    snapshot.chat={provider:task().provider,id:crypto.randomUUID(),project_id:db.project[0],sequence:1,busy:false,cancelled:false,text:"La tarea sigue guardada; podemos continuar.",error:null};
   }
   else if(!["get_snapshot","detect_clients"].includes(command))throw Error(command);
   persist();return structuredClone(snapshot);
  }};
 });
 await page.goto(server.resolvedUrls.local[0]);
 try {await page.locator("#send:enabled").waitFor({timeout:5000});}
 catch(e){console.error(await page.evaluate(()=>({error:document.querySelector('#error')?.textContent,status:document.querySelector('#connection-status')?.textContent,calls:window.calls})));throw e;}
 await page.locator("#prompt").fill("Create a provisional enclosure, preserve my original");await page.locator("#send").click();
 await page.locator("#send:enabled").waitFor();
 const first=await page.evaluate(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).active);
 assert.match(await page.locator("#conversation").innerText(),/preserve my original/);
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();
 const second=await page.evaluate(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).active);assert.notEqual(first,second);
 await page.locator("#task-history > summary").click();await page.locator("#saved-task").selectOption(first);await page.locator("#open-task").click();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#conversation").innerText(),/preserve my original/);
 await page.locator("#file-reference").setInputFiles({name:"capture.bin",mimeType:"application/octet-stream",buffer:Buffer.from("RAW_PRIVATE_SENTINEL <img src=x onerror=alert(1)>")});
 await page.getByRole("button",{name:"Compartir con esta IA…",exact:true}).waitFor();
 const revisionBeforePreview=await page.evaluate(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).records[JSON.parse(localStorage.getItem("owned-task-fixture")).active].revision);
 const providerCallsBeforePreview=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
 await page.getByRole("button",{name:"Ver localmente",exact:true}).click();
 await page.locator("#report[open]").waitFor();assert.match(await page.locator("#report-text").innerText(),/RAW_PRIVATE_SENTINEL/);
 assert.equal(await page.locator("#report-text img").count(),0);
 await page.locator("#report").getByRole("button",{name:"Cerrar",exact:true}).click();
 assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).records[JSON.parse(localStorage.getItem("owned-task-fixture")).active].revision),revisionBeforePreview);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),providerCallsBeforePreview);
 await page.locator("#prompt").fill("Continue with public research only");await page.locator("#send").click();await page.locator("#send:enabled").waitFor();
 const sent=await page.evaluate(()=>window.calls.filter(c=>c.command==="start_codex_chat").at(-1));assert(!JSON.stringify(sent).includes("RAW_PRIVATE_SENTINEL"));assert.equal(sent.args.image,undefined);
 await page.locator("#connections").evaluate(el=>el.open=true);
 await page.locator("#provider").selectOption("claude");
 assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).records[JSON.parse(localStorage.getItem("owned-task-fixture")).active].provider),"codex");
 await page.locator("#approve-provider").click();await page.locator("#send:enabled").waitFor();
 assert.equal(await page.locator("#report-text").textContent(),"");
 assert.match(await page.locator("#conversation").innerText(),/preserve my original/);
 assert.match(await page.locator("#task-references").innerText(),/solo local/);
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#task-references").innerText(),/capture.bin/);
 assert.match(await page.locator("#conversation").innerText(),/preserve my original/);
 await page.getByRole("button",{name:"Compartir con esta IA…",exact:true}).click();
 assert.match(await page.locator("#reference-offer-text").innerText(),/Claude Code/);
 await page.locator("#keep-reference-local").click();
 assert.match(await page.locator("#task-references").innerText(),/solo local/);
 assert(!await page.evaluate(()=>window.calls.some(c=>/pull|download|install/.test(c.command))));
 await page.evaluate(()=>window.nativeOnNext=true);
 await page.locator("#prompt").fill("Explain the non-content native status");await page.locator("#send").click();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#task-references").innerText(),/Observación nativa.json/);
 assert.equal(await page.locator("#error").isVisible(),false);
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#conversation").innerText(),/non-content native status/);
 assert.match(await page.locator("#task-references").innerText(),/Observación nativa.json/);
 const retained=page.locator("#task-references > p").filter({hasText:"capture.bin"});
 await retained.getByRole("button",{name:"Ver localmente",exact:true}).click();await page.locator("#report[open]").waitFor();
 assert.match(await page.locator("#report-text").innerText(),/RAW_PRIVATE_SENTINEL/);
 await page.locator("#report").getByRole("button",{name:"Cerrar",exact:true}).click();
 await retained.getByRole("button",{name:"Quitar referencia",exact:true}).click();await page.locator("#send:enabled").waitFor();
 assert.equal(await page.locator("#report-text").textContent(),"");assert(!await page.locator("#task-references").innerText().then(t=>t.includes("capture.bin")));
 assert.match(await page.locator("#task-references").innerText(),/pendientes de borrar/);
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#task-references").innerText(),/permisos ya están revocados/);
 await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));db.records[db.active].pending_removals=[];localStorage.setItem("owned-task-fixture",JSON.stringify(db));});
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.doesNotMatch(await page.locator("#task-references").innerText(),/pendientes de borrar/);
 console.log("Durable task chat fixture passed: automatic workspace, archived chats, reload, local references, explicit provider switch and withheld raw input. No real provider, native program or model download ran.");
} finally {await browser?.close();await server.close();}
