import assert from "node:assert/strict";
import path from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright-core";

const root = path.resolve(import.meta.dirname, "..");
const server = await createServer({root,server:{host:"127.0.0.1",port:0}});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({executablePath:process.env.HAZLOMEXA_TEST_BROWSER ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",headless:true});
  const page = await browser.newPage();
  const errors=[]; page.on("pageerror",error=>errors.push(error.message));
  await page.addInitScript(() => {
    const project=["history-project","C:\\owned\\history-project"];
    const legacyKey=scope=>`design-chat:${JSON.stringify([...project,scope])}`;
    const scopes=["codex","claude","guided","guided:arduino","guided:agent","guided:drawing","guided:konnect","guided:pcb","guided:kicad","guided:cadpilot","guided:slice"];
    if (!localStorage.getItem("history-seeded")) {
      for(const scope of scopes) localStorage.setItem(legacyKey(scope),JSON.stringify({version:1,turns:[{role:"user",content:`OLD ${scope}: todavía no sé las medidas.`},{role:"assistant",content:"<img src=x onerror=window.historyInjection=true> ¿Para qué lo usarás?"}],answeredChat:"old-chat-must-not-resume",answeredInference:"old-inference-must-not-resume",newSession:false,approval_id:"DO-NOT-RESTORE",document:"old-document-must-not-restore"}));
      localStorage.setItem(`design-chat:${JSON.stringify([project[0],"C:\\owned\\other-folder","codex"])}`,JSON.stringify({version:1,turns:[{role:"user",content:"OTHER FOLDER PRIVATE"}]}));
      localStorage.setItem(`design-chat:${JSON.stringify(["other-project",project[1],"codex"])}`,JSON.stringify({version:1,turns:[{role:"user",content:"OTHER PROJECT PRIVATE"}]}));
      localStorage.setItem("history-seeded","true");
    }
    window.originalLegacy=Object.fromEntries(scopes.map(scope=>[legacyKey(scope),localStorage.getItem(legacyKey(scope))]));
    window.historyCalls=[];
    window.breakLegacyStorage=false;
    const originalGet=Storage.prototype.getItem;
    Storage.prototype.getItem=function(key){if(window.breakLegacyStorage && key.startsWith("design-chat:"))throw Error("Storage unavailable");return originalGet.call(this,key);};
    const snapshot={project,job:null,inference:null,recovered:[],freecad:"",freecad_available:false,codex:{version:"fixture"},codex_login:{id:"fixture-login",sequence:1,connected:true,busy:false},claude:{version:"fixture"}};
    window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(command,args)=>{
      window.historyCalls.push({command,args});
      if(command==="get_snapshot")return structuredClone(snapshot);
      if(command==="start_codex_chat" || command==="start_claude_chat" || command==="start_ollama_chat") {
        snapshot.chat={provider:command==="start_codex_chat"?"codex":command==="start_claude_chat"?"claude":args.profile==="cloud"?"ollama_cloud":"ollama_local",id:crypto.randomUUID(),project_id:snapshot.project[0],sequence:1,busy:false,cancelled:false,text:"Inspeccionemos de nuevo la selección actual.",error:null};
        return structuredClone(snapshot);
      }
      if(command==="select_project") {snapshot.project=[project[0],"C:\\owned\\copied-project"];return structuredClone(snapshot);}
      throw Error(`Unexpected history action: ${command}`);
    }};
  });
  await page.goto(server.resolvedUrls.local[0]); await page.locator("#send:enabled").waitFor({timeout:5000}).catch(async error=>{console.log(JSON.stringify({errors,ui_error:await page.locator("#error").textContent(),calls:await page.evaluate(()=>window.historyCalls)}));throw error;});
  assert.deepEqual(await page.evaluate(async()=>{
    const {legacyConversations}=await import("/src/chat-history.ts");
    const project=["parser-project","C:\\owned\\parser-project"], reads=[];
    const invalid=["{",JSON.stringify({version:2,turns:[]}),JSON.stringify({version:1,turns:[{role:"tool",content:"invalid"}]}),JSON.stringify({version:1,turns:[{role:"user",content:"x".repeat(8192)}]})];
    const rejected=invalid.every(raw=>legacyConversations(()=>raw,project).length===0);
    const empty=legacyConversations(key=>{reads.push(key);return null;},project).length===0;
    const noProject=legacyConversations(()=>{throw Error("Unexpected read");},null).length===0;
    return {rejected,empty,noProject,reads:reads.length,scoped:reads.every(key=>key.startsWith(`design-chat:${JSON.stringify(project).slice(0,-1)},`))};
  }),{rejected:true,empty:true,noProject:true,reads:11,scoped:true});
  await page.locator("#past-chats > summary").click();
  assert.equal(await page.locator("#past-chat-list > details").count(),11);
  assert(!await page.locator("#past-chat-list").textContent().then(text=>/OTHER FOLDER PRIVATE|OTHER PROJECT PRIVATE/.test(text)));
  assert.equal(await page.locator("#past-chat-list img").count(),0);
  assert.equal(await page.evaluate(()=>window.historyInjection),undefined);
  assert.equal(await page.locator("#conversation").textContent().then(text=>text.includes("OLD codex")),false,"Archive was automatically replayed");
  await page.locator("#past-chat-list > details").first().locator("summary").click();
  const before=await page.evaluate(()=>window.historyCalls.length);
  await page.getByRole("button",{name:"Continuar con Codex",exact:true}).first().click();
  assert.match(await page.locator("#conversation").innerText(),/OLD codex/);
  assert.equal(await page.evaluate(()=>window.historyCalls.length),before,"Import dispatched a backend action");
  assert.equal(await page.locator("#past-chat-list button:enabled").count(),0,"Existing conversation could be overwritten");
  assert.equal(await page.locator("#approval").isVisible(),false);
  await page.locator("#prompt").fill("Continuemos con mi intención, inspeccionando el documento antes de proponer."); await page.locator("#send").click(); await page.locator("#send:enabled").waitFor();
  const call=await page.evaluate(()=>window.historyCalls.filter(call=>call.command==="start_codex_chat").at(-1));
  const prompt=JSON.parse(call.args.prompt);
  assert.equal(call.args.newSession,true);
  assert(prompt.recovered_conversation.some(turn=>turn.content.includes("OLD codex")));
  assert(!JSON.stringify(call.args).match(/DO-NOT-RESTORE|old-chat-must-not-resume|old-inference-must-not-resume|old-document-must-not-restore/));
  assert.equal(prompt.desktop_context.current_job,null);
  assert.match(prompt.instruction,/contexto, no una aprobación ni una medición vigente/);
  await page.reload(); await page.locator("#send:enabled").waitFor();
  assert.match(await page.locator("#conversation").innerText(),/OLD codex/);
  for(const provider of ["claude","ollama_local","ollama_cloud"]) {
    await page.locator("#setup").click();await page.locator("#provider").selectOption(provider);
    assert(!await page.locator("#conversation").innerText().then(text=>text.includes("OLD codex")),"Current provider history was mixed");
    await page.locator("#past-chats > summary").click();
    if(await page.locator("#past-chat-list > details").last().getAttribute("open")===null) await page.locator("#past-chat-list > details").last().locator("summary").click();
    await page.locator("#past-chat-list > details").last().getByRole("button").click();
    assert.match(await page.locator("#conversation").innerText(),/OLD guided:slice/);
    assert.equal(await page.locator("#approval").isVisible(),false);
    await page.locator("#past-chats > summary").click();
  }
  await page.locator("#provider").selectOption("codex");
  await page.locator("#send:enabled").waitFor();
  await page.evaluate(()=>{
    const original=IDBDatabase.prototype.transaction;
    window.holdChatCommit=true;
    IDBDatabase.prototype.transaction=function(...args){
      const tx=original.apply(this,args);
      if(args[1]==="readwrite"&&window.holdChatCommit){
        window.holdChatCommit=false;
        Object.defineProperty(tx,"oncomplete",{set(handler){tx.addEventListener("complete",event=>{window.releaseChatCommit=()=>handler.call(tx,event);});}});
      }
      return tx;
    };
  });
  await page.locator("#prompt").fill("Conserva también este último mensaje confirmado.");
  await page.locator("#send").click();await page.locator("#send:enabled").waitFor();
  await page.waitForFunction(()=>typeof window.releaseChatCommit==="function");
  assert.match(await page.locator("#storage-status").innerText(),/Guardando conversación/);
  await page.evaluate(()=>window.releaseChatCommit());
  await page.waitForFunction(()=>document.getElementById("storage-status").textContent.startsWith("Conversación guardada"));
  await page.evaluate(()=>{
    const key=Object.keys(localStorage).find(key=>{
      if(!key.startsWith("hazlomexa.chat.v2:"))return false;
      const scope=JSON.parse(key.slice("hazlomexa.chat.v2:".length));
      return scope[0]==="history-project"&&scope[1].endsWith("history-project")&&scope[2]==="codex";
    });
    if(!key)throw Error("Missing scoped legacy cache");
    localStorage.setItem(key,JSON.stringify({version:1,turns:[{role:"assistant",content:"STALE-CACHE-MUST-NOT-REPLACE-COMMIT"}]}));
  });
  await page.reload();await page.locator("#send:enabled").waitFor();
  assert.match(await page.locator("#conversation").innerText(),/Conserva también este último mensaje confirmado/);
  assert(!await page.locator("#conversation").innerText().then(text=>text.includes("STALE-CACHE-MUST-NOT-REPLACE-COMMIT")));
  await page.locator("#setup").click();
  const legacyUnchanged=()=>page.evaluate(()=>Object.entries(window.originalLegacy).every(([key,value])=>localStorage.getItem(key)===value));
  assert(await legacyUnchanged());
  await page.evaluate(()=>{window.breakLegacyStorage=true;});
  await page.locator("#provider").selectOption("codex");
  assert.equal(await page.locator("#past-chats").isVisible(),false,"Read failure exposed an old archive");
  assert.equal(await page.locator("#past-chat-list").textContent(),"");
  await page.evaluate(()=>{window.breakLegacyStorage=false;});
  await page.locator("#open-project").click();await page.locator("#send:enabled").waitFor();
  assert(!await page.locator("#conversation").innerText().then(text=>text.includes("OLD")));
  assert.equal(await page.locator("#past-chats").isVisible(),false);
  assert(await legacyUnchanged());
  assert.deepEqual(errors,[]);
  console.log("PASS: eleven legacy scopes, explicit history copy across four provider choices, no automatic replay/backend action/approval, escaped messages, fresh session, reload, provider/folder isolation, storage failure, commit-only save acknowledgement, stale-cache recovery and byte-identical legacy preservation; browser IPC fixtures only");
} finally {await browser?.close();await server.close();}
