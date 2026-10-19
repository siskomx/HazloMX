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
  const affected=(t,id)=>{const ids=new Set([id]);for(const r of t.references)if(r.parents?.some(p=>ids.has(p)))ids.add(r.id);return ids;};
  const state=()=>db.active?{active:{...task(),history:undefined},tasks:Object.values(db.records).map(t=>({id:t.id,title:t.title}))}:null;
  const persist=()=>{snapshot.tasks=state();localStorage.setItem("owned-task-fixture",JSON.stringify(db));};
  const create=()=>{const id=crypto.randomUUID();db.records[id]={id,revision:1,title:"Nueva conversación",goal:"",provider:task()?.provider??"codex",references:[],restricted_results:false,history:JSON.stringify({version:1,turns:[]})};db.active=id;};
  persist();
  window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(command,args={})=>{
   window.calls.push({command,args:structuredClone(args)});
   if(command==="get_snapshot"&&window.failSnapshotOnce){window.failSnapshotOnce=false;throw Error("Owned snapshot reply unavailable");}
   if(command==="get_snapshot"&&window.returnStaleSnapshotOnce){window.returnStaleSnapshotOnce=false;return structuredClone(window.oldChatSnapshot);}
   if(command==="ensure_task_workspace"){db.project=["owned-project","C:/OwnedFixture/Task"];snapshot.project=db.project;create();task().provider=args.provider??"codex";persist();}
   else if(command==="task_action"){
    const r=args.request;
    if(r.action==="new")create();
    else if(r.action==="open")db.active=r.id;
    else {
     const t=task();if(r.id!==t.id || r.revision!==t.revision)throw Error("stale task");
     if(r.action==="history"){if(r.value==null)return t.history;t.history=r.value;}
     if(r.action==="message"){t.revision++;t.last_user_revision=t.revision;(t.instructions??=[]).push({id:crypto.randomUUID(),revision:t.revision,sha256:"e".repeat(64),text:r.text,shared_with:[t.provider]});t.history=r.history;if(r.correction_of){t.latest_correction={job_id:r.correction_of,revision:t.revision,text:r.text};snapshot.job.approval_id=null;}if(!t.goal){t.goal=r.text;t.title=r.text.slice(0,60);}}
     if(r.action==="share_instructions"){for(const entry of t.instructions.filter(entry=>r.statements.includes(entry.id))){entry.shared_with=entry.shared_with.filter(p=>p!==t.provider);if(r.allow)entry.shared_with.push(t.provider);}t.revision++;}
     if(r.action==="remove_instructions"){const removed=t.instructions.filter(entry=>r.statements.includes(entry.id)),texts=removed.map(entry=>entry.text);for(const entry of removed){entry.text=null;entry.shared_with=[];}const history=JSON.parse(t.history);for(const turn of history.turns)if(texts.includes(turn.content)){turn.role="assistant";turn.content="Instrucción retirada por ti.";}t.history=JSON.stringify(history);if(texts.includes(t.goal)){t.goal="";t.title="Conversación conservada";}t.revision++;t.last_user_revision=t.revision;}
     if(r.action==="provider"){t.revision++;t.provider=r.provider;}
     if(r.action==="add_reference"){t.revision++;t.references.push({id:crypto.randomUUID(),name:r.name,bytes:atob(r.data).length,sha256:"f".repeat(64),image:!!r.preview,shared_with:[],preview:r.preview,data:r.data});}
     if(r.action==="preview_reference"){const ref=t.references.find(x=>x.id===r.reference);if(!ref)throw Error("missing local reference");const text=atob(ref.data),offset=r.offset??0,next=offset+4096;return {kind:"text",content:text.slice(offset,next),next_offset:next<text.length?next:null};}
     if(r.action==="share"){const ref=t.references.find(x=>x.id===r.reference);ref.shared_with=ref.shared_with.filter(p=>p!==t.provider);if(r.allow)ref.shared_with.push(t.provider);else{const ids=affected(t,r.reference);for(const ref of t.references)if(ids.has(ref.id))ref.shared_with=ref.shared_with.filter(p=>p!==t.provider);}t.revision++;}
     if(r.action==="remove"){const ids=affected(t,r.reference),removed=t.references.filter(x=>ids.has(x.id));t.references=t.references.filter(x=>!ids.has(x.id));t.pending_removals=removed.map(({id,bytes})=>({id,bytes}));t.revision++;}
     if(r.action==="image"){const ref=t.references.find(x=>x.id===r.reference);if(!ref?.shared_with.includes(t.provider))throw Error("sharing required");return structuredClone(ref.preview);}
    }
    persist();if(window.failAfterScope===r.action){window.failAfterScope=null;throw Error("Owned storage reply lost after commit");}return structuredClone(state());
   }
   else if(command==="correct_chat"){
    window.oldChatSnapshot=structuredClone(snapshot);
    const t=task(),r=args.request;if(r.id!==t.id || r.revision!==t.revision || snapshot.chat?.id!==r.chat_id)throw Error("stale chat correction");
    if(window.failCorrectionBefore){window.failCorrectionBefore=false;snapshot.chat.cancelled=true;throw Error("Owned correction store locked");}
    const history=JSON.parse(t.history);if(snapshot.chat.text)history.turns.push({role:"assistant",content:snapshot.chat.text,interrupted:true});history.turns.push({role:"user",content:r.text});t.history=JSON.stringify(history);t.revision++;t.last_user_revision=t.revision;t.chat_correction={id:r.correction_id,chat_id:r.chat_id,revision:t.revision,text:r.text};(t.instructions??=[]).push({id:crypto.randomUUID(),revision:t.revision,sha256:"e".repeat(64),text:r.text,shared_with:[t.provider]});snapshot.chat.cancelled=true;snapshot.chat.sequence++;
    persist();if(window.failCorrectionAfter){window.failCorrectionAfter=false;throw Error("Owned correction reply lost after commit");}
    if(window.holdCorrectionReply){window.holdCorrectionReply=false;await new Promise(resolve=>{window.releaseCorrectionReply=resolve;});}
   }
   else if(command==="answer_codex_question"){
    const t=task(),question=snapshot.chat?.question;if(!question || question.id!==args.questionId || snapshot.chat.id!==args.id || t.provider!=="codex")throw Error("stale question");
    if(window.failAnswerBefore){window.failAnswerBefore=false;throw Error("Owned answer store locked");}
    const data=JSON.stringify(["hazlo_question_answers_v1",question.id,question.questions.map(q=>[q.id,q.question,args.answers[q.id]])]);
    const sha256=Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(data))),b=>b.toString(16).padStart(2,"0")).join("");
    const ref={id:crypto.randomUUID(),name:`question-${question.id}.json`,bytes:new TextEncoder().encode(data).length,sha256,image:false,shared_with:["codex"],data:btoa(unescape(encodeURIComponent(data)))};
    t.references.push(ref);t.revision++;t.last_user_revision=t.revision;
    const marker=`Usa mis respuestas guardadas para continuar esta tarea. Lee read_task_reference con reference_id ${ref.id}. Las preguntas son contexto de la IA y las respuestas son indicaciones del usuario, no mediciones ni aprobación de acciones.`;
    t.chat_correction={id:question.id,chat_id:args.id,revision:t.revision,text:marker};const instruction={id:crypto.randomUUID(),revision:t.revision,sha256:"e".repeat(64),text:marker,shared_with:["codex"]};(t.instructions??=[]).push(instruction);
    const history=JSON.parse(t.history);history.turns.push({role:"user",content:"Usar mis respuestas para continuar la tarea."},{role:"assistant",content:question.questions.map(q=>`Pregunta de la IA (contexto): ${q.question}\nTu respuesta: ${args.answers[q.id]}`).join("\n\n"),reference_ids:[ref.id],instruction_ids:t.instructions.map(i=>i.id)});t.history=JSON.stringify(history);
    snapshot.chat.question=null;snapshot.chat.cancelled=true;snapshot.chat.sequence++;persist();
    if(window.holdAnswerReply){window.holdAnswerReply=false;await new Promise(resolve=>window.releaseAnswerReply=resolve);}
    if(window.failAnswerAfter){window.failAnswerAfter=false;throw Error("Owned answer reply lost after commit");}
    return structuredClone(snapshot.chat);
   }
   else if(command==="cancel_chat"){
    snapshot.chat.cancelled=true;snapshot.chat.sequence++;if(window.failStopBefore){window.failStopBefore=false;throw Error("Owned Stop store locked");}
    const t=task();if(t.chat_stop?.chat_id!==args.id){const history=JSON.parse(t.history);if(t.chat_correction?.chat_id!==args.id && snapshot.chat.text)history.turns.push({role:"assistant",content:snapshot.chat.text,interrupted:true});t.history=JSON.stringify(history);t.revision++;t.last_user_revision=t.revision;t.chat_stop={chat_id:args.id,revision:t.revision};}
    persist();if(window.failStopAfter){window.failStopAfter=false;throw Error("Owned Stop reply lost after commit");}return structuredClone(snapshot.chat);
   }
   else if(command.startsWith("start_") && command.endsWith("_chat")){
    if(window.publicEvidenceOnNext){window.publicEvidenceOnNext=false;task().revision++;task().references.push({id:crypto.randomUUID(),name:"Documento público.json",bytes:100,sha256:"c".repeat(64),image:false,shared_with:[],data:btoa("PUBLIC_RESPONSE_SENTINEL <script>never execute</script>")});}
    if(window.nativeOnNext){task().revision++;task().references.push({id:crypto.randomUUID(),name:"Observación nativa.json",bytes:100,sha256:"e".repeat(64),image:false,shared_with:[],parents:task().references.map(r=>r.id)});window.nativeOnNext=false;}
    snapshot.chat={provider:task().provider,id:crypto.randomUUID(),project_id:db.project[0],sequence:1,busy:false,cancelled:false,text:"La tarea sigue guardada; podemos continuar.",error:null};
    if(window.holdChatOnNext){window.holdChatOnNext=false;snapshot.chat.busy=true;snapshot.chat.text="UNFINISHED_CHAT_SENTINEL";window.finishOwnedChat=()=>{snapshot.chat.busy=false;snapshot.chat.sequence++;persist();};}
    if(window.askOnNext){window.askOnNext=false;snapshot.chat.busy=true;snapshot.chat.text="";snapshot.chat.question={id:crypto.randomUUID(),questions:[{id:"intent",header:"Idea",question:"MODEL_QUESTION_SENTINEL <img src=x onerror=alert(1)>",isOther:true,isSecret:false}]};window.finishOwnedChat=()=>{snapshot.chat.busy=false;snapshot.chat.sequence++;persist();};}
    if(window.startBatchOnNext){window.startBatchOnNext=false;snapshot.job={snapshot:{schema_version:17,project_id:db.project[0],phase:"awaiting_approval",input:{schema_version:1,native_tools_version:1,job_id:crypto.randomUUID(),connection_id:crypto.randomUUID(),program:"drawing",catalog_sha256:"a".repeat(64),description:"Owned correction batch",agreement:window.nextAgreement??{kind:"software_operation",outcome:"Apply the bounded fixture changes",assumptions:[],constraints:["Preserve valid work"],losses:[],verification:"Read the final native state; physical function is unverified"},calls:[{tool:"native_edit",arguments:{value:7}},{tool:"native_edit",arguments:{value:99}}],readback:{tool:"native_state",arguments:{}},guard:{call:{tool:"native_state",arguments:{}},result_sha256:"b".repeat(64)}},audit:{source:{provider:"mcp"},files_verified:false,task:{id:task().id,revision:task().revision}}},approval_id:"owned-approval",sequence:1,busy:false,error:null};window.nextAgreement=undefined;}
    if(window.startTypedOnNext){
     const kind=window.startTypedOnNext;window.startTypedOnNext=null;
     if(window.routeTypedOnNext){window.routeTypedOnNext=false;snapshot.program_route={id:crypto.randomUUID(),program:kind==="agent"?"agent":"drawing",status:"connected",connection_id:crypto.randomUUID(),transport:"http",tools:["recover_native_result"]};}
     const inputs={
      agent:{live_agent_version:1,native_completion_version:1,before:{name:"Owned.ino",source:"void setup() {}",dirty:true},source:"void setup() { delay(1); }"},
      pcb:{live_pcb_version:1,board_name:"Owned.kicad_pcb",units:"mm",before:{components:[]},movement:{uuid:"owned-component",reference:"J1",expected_x_mm:0,expected_y_mm:0,x_mm:1,y_mm:0}},
      drawing:{live_drawing_version:1,native_working_version:1,units:"mm",before:{title:"Owned.dxf"},operation:{action:"circle",diameter:12,center:[0,0]}},
      cad:{live_cad_version:1,document_name:"Owned",before:[],operations:[{action:"edit_object",obj_name:"Box",obj_properties:{Length:"12 mm"}}]},
     };
     if(kind==="cad" && window.checkpointOnNext){window.checkpointOnNext=false;inputs.cad.working_version={verifier_sha256:"a".repeat(64)};inputs.cad.operations=[];}
     snapshot.job={snapshot:{schema_version:{agent:15,pcb:14,drawing:16,cad:13}[kind],project_id:db.project[0],phase:"awaiting_approval",input:{schema_version:1,job_id:crypto.randomUUID(),description:"Owned typed correction",agreement:{kind:"provisional_draft",outcome:"Apply the requested typed fixture change <img src=x onerror=alert(1)>",assumptions:["Fit is unmeasured"],constraints:["Preserve manual edits"],losses:["Physical function is not checked"],verification:"Inspect native state; checked reopen remains pending"},...inputs[kind]},audit:{source:{provider:"mcp"},files_verified:false,task:{id:task().id,revision:task().revision}}},approval_id:"owned-approval",sequence:1,busy:false,error:null};
    }
    if(window.startPublicOnNext){window.startPublicOnNext=false;snapshot.job={snapshot:{schema_version:18,project_id:db.project[0],phase:"awaiting_approval",input:{schema_version:1,public_document_version:1,job_id:crypto.randomUUID(),url:"https://example.org/manual?language=en",purpose:"Check the official public manual"},audit:{source:{provider:"mcp"},files_verified:false,task:{id:task().id,revision:task().revision}}},approval_id:"owned-approval",sequence:1,busy:false,error:null};}
    if(window.retireReviewOnNext){window.retireReviewOnNext=false;snapshot.job.snapshot.phase="cancelled";snapshot.job.snapshot.audit.approval_invalidated=true;snapshot.job.approval_id=null;snapshot.job.error="The task changed; prepared work remains local.";}
   }
   else if(command==="approve_operation"){
    snapshot.job.busy=true;snapshot.job.snapshot.phase="running";snapshot.job.sequence++;args.events.onmessage(structuredClone(snapshot.job));
    return await new Promise(resolve=>{window.finishOwnedBatch=()=>{snapshot.job.busy=false;snapshot.job.snapshot.phase=window.failCopyBatch?"failed":snapshot.job.snapshot.input.public_document_version||window.completeBatch?"completed":"cancelled";window.failCopyBatch=false;window.completeBatch=false;snapshot.job.sequence++;args.events.onmessage(structuredClone(snapshot.job));persist();resolve(structuredClone(snapshot.job));};});
   }
   else if(command==="open_artifact")return null;
   else if(command==="cancel_job"){snapshot.job.snapshot.phase="cancelled";snapshot.job.sequence++;return structuredClone(snapshot.job);}
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
 assert.match(await page.locator("#task-references").innerText(),/copia derivada/);
 assert.match(await page.locator("#task-references").innerText(),/también retira sus copias/);
 await retained.getByRole("button",{name:"Compartir con esta IA…",exact:true}).click();await page.locator("#approve-reference").click();await page.locator("#send:enabled").waitFor();
 const child=page.locator("#task-references > p").filter({hasText:"Observación nativa.json"});
 await child.getByRole("button",{name:"Compartir con esta IA…",exact:true}).click();await page.locator("#approve-reference").click();await page.locator("#send:enabled").waitFor();
 const revokeCallsBefore=await page.evaluate(()=>window.calls.filter(c=>c.command==="task_action"&&c.args.request.action==="share"&&!c.args.request.allow).length);
 const agentCallsBeforeRevoke=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
 await page.locator("#prompt").fill("Keep this unsent correction");
 await page.evaluate(()=>window.failAfterScope="share");
 await retained.getByRole("button",{name:"Revocar permiso",exact:true}).click();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#error").innerText(),/reply lost after commit/);
 assert.equal(await page.locator("#prompt").inputValue(),"Keep this unsent correction");
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="task_action"&&c.args.request.action==="share"&&!c.args.request.allow).length),revokeCallsBefore+1);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),agentCallsBeforeRevoke);
 assert.match(await child.innerText(),/solo local/);
 assert.equal(await page.getByRole("button",{name:"Revocar permiso",exact:true}).count(),0);
 await retained.getByRole("button",{name:"Ver localmente",exact:true}).click();await page.locator("#report[open]").waitFor();
 assert.match(await page.locator("#report-text").innerText(),/RAW_PRIVATE_SENTINEL/);
 await page.locator("#report").getByRole("button",{name:"Cerrar",exact:true}).click();
 await page.evaluate(()=>window.failSnapshotOnce=true);
 await retained.getByRole("button",{name:"Quitar referencia",exact:true}).click();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#error").innerText(),/snapshot reply unavailable/);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="task_action"&&c.args.request.action==="remove").length),1);
 assert.equal(await page.locator("#report-text").textContent(),"");assert(!await page.locator("#task-references").innerText().then(t=>t.includes("capture.bin")));
 assert.doesNotMatch(await page.locator("#task-references").innerText(),/Observación nativa.json/);
 assert.match(await page.locator("#task-references").innerText(),/pendientes de borrar/);
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#task-references").innerText(),/permisos ya están revocados/);
 await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));db.records[db.active].pending_removals=[];localStorage.setItem("owned-task-fixture",JSON.stringify(db));});
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.doesNotMatch(await page.locator("#task-references").innerText(),/pendientes de borrar/);
 await page.evaluate(()=>window.startBatchOnNext=true);
 await page.locator("#prompt").fill("Prepare a bounded native batch");await page.locator("#send").click();
 await page.locator("#approve:enabled").waitFor();await page.locator("#approve").click();
 await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();
 const chatsBeforeCorrection=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
 await page.locator("#prompt").fill("Keep the first valid change; make the next part smaller");await page.locator("#send").click();
 await page.waitForFunction(()=>JSON.parse(localStorage.getItem("owned-task-fixture")).records[JSON.parse(localStorage.getItem("owned-task-fixture")).active].latest_correction);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),chatsBeforeCorrection);
 await page.evaluate(()=>window.finishOwnedBatch());await page.locator("#new-chat:enabled").waitFor();
 await page.evaluate(()=>window.startPublicOnNext=true);
 await page.locator("#prompt").fill("Read the official public manual");await page.locator("#send").click();
 await page.getByRole("button",{name:"Permitir esta consulta pública",exact:true}).waitFor();
 assert.match(await page.locator("#change-facts").innerText(),/https:\/\/example.org\/manual\?language=en/);
 assert.match(await page.locator("#change-facts").innerText(),/permiso posterior/);
 assert(!await page.locator("#task-references").innerText().then(t=>t.includes("Documento público.json")));
 await page.locator("#approve").click();await page.locator("#cancel-running:enabled").waitFor();
 await page.evaluate(()=>{window.publicEvidenceOnNext=true;window.finishOwnedBatch();});
 await page.getByText("Documento público.json",{exact:false}).waitFor();
 assert.match(await page.locator("#task-references").innerText(),/solo local/);
 const publicRead=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).at(-1));
 assert(!JSON.stringify(publicRead).includes("PUBLIC_RESPONSE_SENTINEL"));
 assert.match(await page.locator("#conversation").innerText(),/make the next part smaller/);
 await page.reload();await page.locator("#send:enabled").waitFor();
 assert.match(await page.locator("#conversation").innerText(),/make the next part smaller/);
 assert.match(await page.locator("#task-references").innerText(),/última corrección/);
 await page.evaluate(()=>window.startBatchOnNext=true);
 await page.locator("#prompt").fill("Prepare another bounded native batch");await page.locator("#send").click();
 await page.locator("#approve:enabled").waitFor();await page.locator("#approve").click();
 await page.locator("#cancel-running:enabled").waitFor();await page.locator("#cancel-running").click();
 await page.waitForFunction(()=>window.calls.some(c=>c.command==="cancel_job"));
 await page.evaluate(()=>window.finishOwnedBatch());await page.locator("#new-chat:enabled").waitFor();
 for(const kind of ["agent","pcb","drawing","cad"]){
  await page.evaluate(kind=>window.startTypedOnNext=kind,kind);
  await page.locator("#prompt").fill("Prepare the owned live "+kind+" change");await page.locator("#send").click();
  await page.locator("#approve:enabled").waitFor();const typedFacts=await page.locator("#change-facts").innerText();for(const fact of ["Apply the requested typed fixture change <img src=x onerror=alert(1)>","Fit is unmeasured","Preserve manual edits","Physical function is not checked","checked reopen remains pending"])assert(typedFacts.includes(fact));assert.equal(await page.locator("#change-facts img").count(),0);await page.locator("#approve").click();
  await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();await page.locator("#cancel-running:enabled").waitFor();
  const callsBefore=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
  const cancelsBefore=await page.evaluate(()=>window.calls.filter(c=>c.command==="cancel_job").length);
  const text="Preserve the valid work and my edits in "+kind;
  await page.locator("#prompt").fill(text);await page.locator("#send").click();
  await page.waitForFunction(text=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].latest_correction?.text===text;},text);
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),callsBefore);
  await page.locator("#cancel-running:enabled").waitFor();await page.locator("#cancel-running").click();
  await page.waitForFunction(count=>window.calls.filter(c=>c.command==="cancel_job").length===count,cancelsBefore+1);
  await page.evaluate(()=>window.finishOwnedBatch());await page.locator("#new-chat:enabled").waitFor();
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),callsBefore);
  assert.match(await page.locator("#conversation").innerText(),new RegExp(text));
 }
 const providerCallsBeforeRetired=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
 await page.evaluate(()=>{window.startBatchOnNext=true;window.retireReviewOnNext=true;});
 await page.locator("#prompt").fill("Review the preserved proposal using the current task");await page.locator("#send").click();await page.locator("#send:enabled").waitFor();
 await page.waitForFunction(()=>document.querySelector("#history")?.textContent.includes("Propuesta conservada; necesita nueva revisión"));
 await page.locator("#results").evaluate(el=>el.open=true);
 assert.match(await page.locator("#history").innerText(),/Propuesta conservada; necesita nueva revisión/);
 assert.match(await page.locator("#job-status").textContent(),/Conservamos la propuesta sin ejecutar/);
 assert(await page.locator("#approve").isDisabled());
 await page.getByRole("button",{name:"Ver propuesta conservada",exact:true}).click();await page.locator("#report[open]").waitFor();
 assert.match(await page.locator("#report-text").innerText(),/native_edit/);assert.match(await page.locator("#report-text").innerText(),/Esta vista es local/);
 await page.locator("#report").getByRole("button",{name:"Cerrar",exact:true}).click();
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),providerCallsBeforeRetired+1);
 for(const provider of ["codex","claude","ollama_local","ollama_cloud"]){
  await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.locator("#connections").evaluate(el=>el.open=true);
  const selected=await page.locator("#provider").inputValue();if(selected!==provider){await page.locator("#provider").selectOption(provider);await page.locator("#approve-provider").click();await page.locator("#send:enabled").waitFor();}
  await page.evaluate(()=>window.holdChatOnNext=true);await page.locator("#prompt").fill("Keep the original idea for "+provider);await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();
  const started=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),text="Use inches and preserve my manual changes for "+provider;
  if(provider==="claude")await page.evaluate(()=>window.failCorrectionAfter=true);
  await page.locator("#prompt").fill(text);await page.locator("#send").click();await page.waitForFunction(text=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].chat_correction?.text===text;},text);
  await page.waitForFunction(()=>document.querySelector("#conversation").textContent.includes("borrador interrumpido"));assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),started);
  await page.evaluate(()=>window.returnStaleSnapshotOnce=true);await page.waitForFunction(()=>window.returnStaleSnapshotOnce===false);assert.match(await page.locator("#conversation").innerText(),new RegExp(text));
  await page.evaluate(()=>window.finishOwnedChat());await page.waitForFunction(expected=>window.calls.filter(c=>c.command.startsWith("start_")).length===expected,started+1);await page.locator("#new-chat:enabled").waitFor();
  const follow=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).at(-1));assert.equal(follow.args.taskUserRequest,text);assert(!follow.args.prompt.includes("UNFINISHED_CHAT_SENTINEL"));
  const history=await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return JSON.parse(db.records[db.active].history).turns;});assert.equal(history.filter(t=>t.role==="user"&&t.content===text).length,1);assert.equal(history.filter(t=>t.content==="UNFINISHED_CHAT_SENTINEL"&&t.interrupted===true).length,1);
  await page.reload();await page.locator("#send:enabled").waitFor();assert.match(await page.locator("#conversation").innerText(),/borrador interrumpido/);assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),0,"Reload silently restarted a corrected conversation");
 }
 for(const provider of ["codex","claude","ollama_local","ollama_cloud"]){
  await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.locator("#connections").evaluate(el=>el.open=true);
  if(await page.locator("#provider").inputValue()!==provider){await page.locator("#provider").selectOption(provider);await page.locator("#approve-provider").click();await page.locator("#send:enabled").waitFor();}
  await page.evaluate(()=>window.holdChatOnNext=true);await page.locator("#prompt").fill("Preserve the original work for "+provider);await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();
  const started=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
  if(provider==="claude")await page.evaluate(()=>window.failStopAfter=true);
  await page.locator("#stop:enabled").click();await page.waitForFunction(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].chat_stop;});await page.waitForFunction(()=>document.querySelector("#conversation").textContent.includes("borrador interrumpido"));
  await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),started);
  const stopped=await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active];});const turns=JSON.parse(stopped.history).turns;assert.equal(turns.filter(t=>t.role==="user").length,1);assert.equal(turns.filter(t=>t.content==="UNFINISHED_CHAT_SENTINEL"&&t.interrupted).length,1);assert.equal(stopped.last_user_revision,stopped.chat_stop.revision);
  await page.reload();await page.locator("#send:enabled").waitFor();assert.match(await page.locator("#conversation").innerText(),/borrador interrumpido/);assert.match(await page.locator("#task-references").innerText(),/pausa queda guardada/);assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),0);
  await page.locator("#prompt").fill("Continue after checking the valid work");await page.locator("#send").click();await page.locator("#new-chat:enabled").waitFor();const fresh=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")));assert.equal(fresh.length,1);assert(!fresh[0].args.prompt.includes("UNFINISHED_CHAT_SENTINEL"));
 }
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.holdChatOnNext=true;window.failStopBefore=true;});await page.locator("#prompt").fill("Keep my original goal on save failure");await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();const beforeStopFailure=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);await page.locator("#stop:enabled").click();await page.locator("#error:not([hidden])").waitFor();assert.equal(await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].chat_stop??null;}),null);await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),beforeStopFailure);
 // Stop cancels the queued continuation while retaining the durable correction.
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>window.holdChatOnNext=true);await page.locator("#prompt").fill("Keep the work");await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();
 const beforeStop=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);await page.locator("#prompt").fill("Change the next step only");await page.locator("#send").click();await page.waitForFunction(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].chat_correction;});await page.locator("#stop:enabled").waitFor();await page.locator("#stop").click();await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),beforeStop);
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.holdChatOnNext=true;window.failCorrectionBefore=true;});await page.locator("#prompt").fill("Keep the original goal");await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();const beforeFailure=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);await page.locator("#prompt").fill("My unsaved correction");await page.locator("#send").click();await page.locator("#error:not([hidden])").waitFor();assert.equal(await page.locator("#prompt").inputValue(),"My unsaved correction");await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),beforeFailure);
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.holdChatOnNext=true;window.holdCorrectionReply=true;});await page.locator("#prompt").fill("Keep the valid work");await page.locator("#send").click();await page.getByRole("button",{name:"Enviar corrección",exact:true}).waitFor();const beforePendingStop=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);await page.locator("#prompt").fill("Save this correction but stop the continuation");await page.locator("#send").click();await page.waitForFunction(()=>typeof window.releaseCorrectionReply==="function");await page.locator("#stop:enabled").click();await page.evaluate(()=>window.releaseCorrectionReply());await page.waitForFunction(()=>document.querySelector("#prompt").value==="");await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),beforePendingStop,"Stop during correction persistence restarted the AI");
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();
 const statement="Keep this exact user request <img src=x onerror=alert(1)> locally";
 await page.locator("#prompt").fill(statement);await page.locator("#send").click();await page.locator("#new-chat:enabled").waitFor();
 const review=page.getByRole("button",{name:/Revisar mensajes guardados/});await review.click();assert.equal(await page.locator("#instruction-entries pre").innerText(),statement);assert.equal(await page.locator("#instruction-entries img").count(),0);assert.equal(await page.locator("#instruction-entries input").isChecked(),false);
 await page.locator("#instruction-review").getByRole("button",{name:"Cerrar",exact:true}).click();await page.waitForFunction(()=>document.querySelector("#instruction-entries").children.length===0);
 await page.locator("#connections").evaluate(el=>el.open=true);const other=await page.locator("#provider").inputValue()==="codex"?"claude":"codex";await page.locator("#provider").selectOption(other);await page.locator("#approve-provider").click();await page.locator("#send:enabled").waitFor();
 const starts=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length);
 await review.click();assert.equal(await page.locator("#instruction-entries input").isChecked(),true);await page.evaluate(()=>window.failAfterScope="share_instructions");await page.locator("#share-instructions").click();await page.waitForFunction(()=>!document.querySelector("#instruction-review").open);
 assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="task_action"&&c.args.request.action==="share_instructions").length),1,"Lost consent reply repeated mutation");
 await review.click();assert.equal(await page.locator("#instruction-entries input").isChecked(),false);await page.locator("#instruction-entries input").check();await page.locator("#revoke-instructions").click();await page.waitForFunction(()=>!document.querySelector("#instruction-review").open);
 await review.click();assert.equal(await page.locator("#instruction-entries input").isChecked(),true);await page.locator("#remove-instructions").click();await page.waitForFunction(()=>!document.querySelector("#instruction-review").open);
 const removed=await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active];});assert.equal(removed.instructions[0].text,null);assert.equal(removed.goal,"");assert(!removed.history.includes(statement));assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),starts);
 await page.reload();await page.locator("#send:enabled").waitFor();await review.click();assert.equal(await page.locator("#instruction-entries input").isDisabled(),true);assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),0);
 await page.locator("#new-chat").evaluate(el=>el.click());await page.waitForFunction(()=>!document.querySelector("#instruction-review").open);assert.equal(await page.locator("#instruction-entries pre").count(),0,"A changed task retained text in its review dialog");
 await page.locator("#new-chat:enabled").waitFor();await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();const agreement={kind:"provisional_draft",outcome:"Create the conceptual part <img src=x onerror=alert(1)>",assumptions:["Physical fit is not measured"],constraints:["Preserve manual edits"],losses:["The copy loses its editable history"],verification:"Inspect the native values; saved-version reopen is unverified"};await page.evaluate(agreement=>{window.nextAgreement=agreement;window.startBatchOnNext=true;},agreement);await page.locator("#prompt").fill("Prepare a provisional part without assuming its measurements");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();const facts=await page.locator("#change-facts").innerText();assert.match(facts,/borrador provisional/);for(const value of [agreement.outcome,agreement.assumptions[0],agreement.constraints[0],agreement.losses[0],agreement.verification])assert(facts.includes(value));assert.match(facts,/no medición/);assert.match(facts,/No hay comprobaciones automáticas/);assert.equal(await page.locator("#change-facts img").count(),0);assert.deepEqual(JSON.parse(await page.locator("#change-details").textContent()).agreement,agreement);
 const approvals=await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length);await page.locator("#approve").click();await page.waitForFunction(expected=>window.calls.filter(c=>c.command==="approve_operation").length===expected,approvals+1);await page.evaluate(()=>window.finishOwnedBatch());await page.locator("#new-chat:enabled").waitFor();await page.locator("#results").evaluate(el=>el.open=true);await page.getByRole("button",{name:"Ver condiciones del lote",exact:true}).first().click();await page.locator("#report[open]").waitFor();const retainedAgreement=await page.locator("#report-text").innerText();assert(retainedAgreement.includes(agreement.outcome));assert.match(retainedAgreement,/no son una aprobación reutilizable/);assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),approvals+1);await page.locator("#report").getByRole("button",{name:"Cerrar",exact:true}).click();
 await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.locator("#connections").evaluate(el=>el.open=true);
 if(await page.locator("#provider").inputValue()!=="codex"){await page.locator("#provider").selectOption("codex");await page.locator("#approve-provider").click();await page.locator("#send:enabled").waitFor();}
 for(const mode of ["lost-reply","failed-save","stopped"]){
  await page.locator("#new-chat").click();await page.locator("#send:enabled").waitFor();await page.evaluate(mode=>{window.askOnNext=true;window.failAnswerAfter=mode==="lost-reply";window.failAnswerBefore=mode==="failed-save";window.holdAnswerReply=mode==="stopped";},mode);
  await page.locator("#prompt").fill("Preserve original work while clarifying intent");await page.locator("#send").click();await page.locator("#question-form:not([hidden])").waitFor();assert.match(await page.locator("#question-form").innerText(),/no aprueba cambios/);assert.equal(await page.locator("#question-fields img").count(),0);
  const input=page.locator('#question-fields input[name="intent"]');await input.fill("  USER_ANSWER_SENTINEL sí 🧰  ");const started=await page.evaluate(()=>window.calls.filter(c=>c.command==="start_codex_chat").length),submitted=await page.evaluate(()=>window.calls.filter(c=>c.command==="answer_codex_question").length);
  await page.locator("#answer-question").click();
  if(mode==="failed-save"){
   await page.locator("#error:not([hidden])").waitFor();await page.locator("#answer-question:enabled").waitFor();assert.equal(await input.inputValue(),"  USER_ANSWER_SENTINEL sí 🧰  ");assert.equal(await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active].references.length;}),0);
   await page.locator("#answer-question").click();
  }
  if(mode==="stopped"){await page.waitForFunction(()=>typeof window.releaseAnswerReply==="function");await page.locator("#stop:enabled").click();await page.evaluate(()=>window.releaseAnswerReply());}
  await page.waitForFunction(()=>document.querySelector("#question-form").hidden);const saved=await page.evaluate(()=>{const db=JSON.parse(localStorage.getItem("owned-task-fixture"));return db.records[db.active];});assert.equal(saved.references.length,1);assert.deepEqual(saved.references[0].shared_with,["codex"]);assert(!saved.instructions.some(i=>i.text.includes("USER_ANSWER_SENTINEL")||i.text.includes("MODEL_QUESTION_SENTINEL")));assert(saved.history.includes("USER_ANSWER_SENTINEL"));assert.equal(saved.goal,"Preserve original work while clarifying intent");
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="start_codex_chat").length),started,"Answers started a second turn while the old one was running");
  await page.evaluate(()=>window.finishOwnedChat());await page.locator("#new-chat:enabled").waitFor();
  assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="start_codex_chat").length),started+(mode==="stopped"?0:1));assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="answer_codex_question").length),submitted+(mode==="failed-save"?2:1),"A lost answer reply repeated the submission");
  await page.reload();await page.locator("#send:enabled").waitFor();assert.match(await page.locator("#conversation").innerText(),/USER_ANSWER_SENTINEL/);assert.match(await page.locator("#task-references").innerText(),/Respuestas guardadas de la conversación/);assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),0,"Reload resumed answers without user input");
 }
 await page.locator("#new-chat:enabled").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.startTypedOnNext="cad";window.checkpointOnNext=true;});await page.locator("#prompt").fill("Keep a checked native version without repeating the edit");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();
 assert.match(await page.locator("#change-facts").innerText(),/sin repetir ni cambiar su geometría/);const copyPlan=JSON.parse(await page.locator("#change-details").textContent());assert.equal(copyPlan.working_version.verifier_sha256,"a".repeat(64));assert.deepEqual(copyPlan.native_operations,[]);assert(copyPlan.agreement);
 const copyApprovals=await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length);await page.locator("#approve").click();await page.waitForFunction(()=>typeof window.finishOwnedBatch==="function"&&document.querySelector("#cancel-running"));await page.evaluate(()=>{window.completeBatch=true;window.finishOwnedBatch();});await page.locator("#new-chat:enabled").waitFor();await page.locator("#results").evaluate(el=>el.open=true);
 const folder=page.getByRole("button",{name:"Ver carpeta de la versión nativa",exact:true});await folder.waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="open_artifact").length),0,"Completion automatically opened a GUI document");await folder.click();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="open_artifact").at(-1).args.format),"native_working_version");assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),copyApprovals+1);
 await page.locator("#new-chat:enabled").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>window.startTypedOnNext="drawing");await page.locator("#prompt").fill("Create the circle and keep a checked drawing version");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();assert.match(await page.locator("#change-facts").innerText(),/incluye una nueva copia DXF/);assert.equal(JSON.parse(await page.locator("#change-details").textContent()).native_working_version,1);
 const drawingApprovals=await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),drawingOpens=await page.evaluate(()=>window.calls.filter(c=>c.command==="open_artifact").length);await page.locator("#approve").click();await page.waitForFunction(()=>typeof window.finishOwnedBatch==="function"&&document.querySelector("#cancel-running"));await page.evaluate(()=>{window.completeBatch=true;window.finishOwnedBatch();});await page.locator("#new-chat:enabled").waitFor();await page.locator("#results").evaluate(el=>el.open=true);
 const drawingFolder=page.getByRole("button",{name:"Ver carpeta de la versión nativa",exact:true});await drawingFolder.waitFor();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="open_artifact").length),drawingOpens);await drawingFolder.click();assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="open_artifact").at(-1).args.format),"native_working_version");assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),drawingApprovals+1);
 assert.match(await page.locator("#job-status").innerText(),/nueva copia DXF comprobada/);
 await page.locator("#new-chat:enabled").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.startTypedOnNext="drawing";window.routeTypedOnNext=true;});await page.locator("#prompt").fill("Preserve the edit if checking its copy fails");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();await page.locator("#approve").click();await page.waitForFunction(()=>typeof window.finishOwnedBatch==="function"&&document.querySelector("#cancel-running"));const startsBeforeCopyFailure=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),approvalsBeforeCopyFailure=await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length);await page.evaluate(()=>{window.failCopyBatch=true;window.finishOwnedBatch();});await page.locator("#new-chat:enabled").waitFor();await page.waitForFunction(count=>window.calls.filter(c=>c.command.startsWith("start_")).length===count,startsBeforeCopyFailure+1);const copyRecovery=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).at(-1).args.prompt);assert(copyRecovery.includes("recover_native_result")&&copyRecovery.includes("no debe repetir la edición ni capturar otra versión"));assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),approvalsBeforeCopyFailure);
 await page.locator("#new-chat:enabled").click();await page.locator("#send:enabled").waitFor();await page.evaluate(()=>{window.startTypedOnNext="agent";window.routeTypedOnNext=true;});await page.locator("#prompt").fill("Recover the original editor receipt if reporting fails");await page.locator("#send").click();await page.locator("#approve:enabled").waitFor();await page.locator("#approve").click();await page.waitForFunction(()=>typeof window.finishOwnedBatch==="function"&&document.querySelector("#cancel-running"));const startsBeforeEditorFailure=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).length),approvalsBeforeEditorFailure=await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length);await page.evaluate(()=>{window.failCopyBatch=true;window.finishOwnedBatch();});await page.locator("#new-chat:enabled").waitFor();await page.waitForFunction(count=>window.calls.filter(c=>c.command.startsWith("start_")).length===count,startsBeforeEditorFailure+1);const editorRecovery=await page.evaluate(()=>window.calls.filter(c=>c.command.startsWith("start_")).at(-1).args.prompt);assert(editorRecovery.includes("recover_native_result")&&editorRecovery.includes("nunca debe repetir la edición ni el guardado"));assert.equal(await page.evaluate(()=>window.calls.filter(c=>c.command==="approve_operation").length),approvalsBeforeEditorFailure);
 console.log("Durable task chat fixture passed: workspace/history, evidence, corrections, scoped statements, agreements, durable question answers, one-approval copy/folder output and retained editor/copy recovery. No real provider, native program or model download ran.");
} finally {await browser?.close();await server.close();}
