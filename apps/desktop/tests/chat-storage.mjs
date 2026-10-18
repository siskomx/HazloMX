import assert from "node:assert/strict";
import {mkdtemp} from "node:fs/promises";
import path from "node:path";
import {createServer} from "vite";
import {chromium} from "playwright-core";

const root=path.resolve(import.meta.dirname,"..");
const profile=await mkdtemp(path.resolve(root,"../../target/chat-storage-"));
const server=await createServer({root,server:{host:"127.0.0.1",port:0},plugins:[{
  name:"owned-storage-test-page",configureServer(server){server.middlewares.use((req,res,next)=>{
    if(req.url!=="/__storage_check") return next();
    res.setHeader("Content-Type","text/html");res.end("<!doctype html><title>Owned chat storage test</title>");
  });}
}]});
const options={executablePath:process.env.HAZLOMEXA_TEST_BROWSER??"C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",headless:true};
let context;
try {
  await server.listen();
  const url=server.resolvedUrls.local[0]+"__storage_check";
  context=await chromium.launchPersistentContext(profile,options);
  let page=await context.newPage();await page.goto(url);
  const value=turn=>JSON.stringify({version:1,turns:[{role:"assistant",content:turn}],answeredChat:"completed-chat"});
  const keys=["codex","claude","ollama_cloud"].map(provider=>`hazlomexa.chat.v2:${JSON.stringify(["same-project",provider==="claude"?"copied-folder":"original-folder",provider])}`);
  await page.evaluate(async({key,old})=>{
    window.store=await import("/src/chat-storage.ts");
    window.durability=[];
    const original=IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction=function(...args){
      const tx=original.apply(this,args);
      if(args[1]==="readwrite") {
        window.durability.push(tx.durability);
        if(window.abortNext){window.abortNext=false;queueMicrotask(()=>tx.abort());}
        if(window.unsupportedNext){window.unsupportedNext=false;Object.defineProperty(tx,"durability",{value:"relaxed"});}
        if(window.holdNext){
          window.holdNext=false;
          Object.defineProperty(tx,"oncomplete",{set(handler){tx.addEventListener("complete",event=>{window.releaseCommit=()=>handler.call(tx,event);});}});
        }
      }
      return tx;
    };
    if(await window.store.readConversation(key)!==null) throw Error("Fresh profile required");
    await window.store.writeConversation(key,old);
  },{key:keys[0],old:value("Previous acknowledged reply")});
  await page.evaluate(({key,next})=>{
    window.holdNext=true;window.writeFinished=false;
    void window.store.writeConversation(key,next).then(()=>{window.writeFinished=true;});
  },{key:keys[0],next:value("Latest acknowledged reply á 🛠️")});
  await page.waitForFunction(()=>typeof window.releaseCommit==="function");
  assert.equal(await page.evaluate(()=>window.writeFinished),false,"Save acknowledged before the transaction-complete callback");
  await page.evaluate(()=>window.releaseCommit());await page.waitForFunction(()=>window.writeFinished);
  await page.evaluate(async({key,next})=>{
    window.abortNext=true;
    try {await window.store.writeConversation(key,next);throw Error("An aborted write was acknowledged");}
    catch(error){if(error.message==="An aborted write was acknowledged") throw error;}
  },{key:keys[0],next:value("Must not replace the last committed reply")});
  assert.equal(await page.evaluate(key=>window.store.readConversation(key),keys[0]),value("Latest acknowledged reply á 🛠️"));
  await page.evaluate(async({key,next})=>{
    window.unsupportedNext=true;
    let rejected=false;
    try{await window.store.writeConversation(key,next);}catch{rejected=true;}
    if(!rejected)throw Error("Unsupported strict durability was acknowledged");
  },{key:keys[0],next:value("Must not downgrade durability")});
  assert.equal(await page.evaluate(key=>window.store.readConversation(key),keys[0]),value("Latest acknowledged reply á 🛠️"));
  await page.evaluate(async({keys,values})=>{
    await Promise.all([
      window.store.writeConversation(keys[0],values[0]),
      window.store.writeConversation(keys[0],values[1]),
      window.store.writeConversation(keys[1],values[2]),
      window.store.writeConversation(keys[2],values[3])
    ]);
    for(const invalid of ['{"version":1,"turns":[{"role":"system","content":"not a user turn"}]}',"x".repeat(512*1024+1)]){
      let rejected=false;
      try{await window.store.writeConversation(keys[0],invalid);}catch{rejected=true;}
      if(!rejected)throw Error("Invalid or oversized conversation was saved");
    }
    if(!window.durability.length||window.durability.some(value=>value!=="strict")) throw Error("A write did not request strict durability");
  },{keys,values:[value("Queued earlier reply"),value("Final acknowledged reply á 🛠️"),value("Other folder and provider"),value("Other provider")]});
  assert.equal(await page.evaluate(key=>window.store.readConversation(key),keys[0]),value("Final acknowledged reply á 🛠️"));
  assert.equal(await page.evaluate(()=>localStorage.length),0,"Recovery must not depend on the old cache");
  const browser=context.browser();assert(browser);
  const closed=new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error("Owned browser did not disconnect after the crash")),15000);
    browser.once("disconnected",()=>{clearTimeout(timer);resolve();});
  });
  const session=await browser.newBrowserCDPSession();
  void session.send("Browser.crash").catch(()=>{});
  await closed;
  assert.equal(browser.isConnected(),false,"The owned browser process did not terminate");
  context=undefined;
  context=await chromium.launchPersistentContext(profile,options);
  page=await context.newPage();await page.goto(url);
  const restored=await page.evaluate(async keys=>{
    const store=await import("/src/chat-storage.ts");
    return Promise.all(keys.map(key=>store.readConversation(key)));
  },keys);
  assert.deepEqual(restored,[value("Final acknowledged reply á 🛠️"),value("Other folder and provider"),value("Other provider")]);
  assert.equal(await page.evaluate(()=>localStorage.length),0);
  const projectFiles = new Map();
  const installNativeMock = async page => {
    await page.exposeFunction("projectHistory", args => {
      const key=JSON.stringify([args.projectId,args.projectDirectory,args.provider]);
      if(args.value!==null) {projectFiles.set(key,args.value);return null;}
      return projectFiles.get(key)??null;
    });
    await page.addInitScript(()=>{window.__TAURI_INTERNALS__={invoke:(command,args)=>{
      if(command!=="conversation_history")throw Error(command);
      return window.projectHistory(args);
    }};});
  };
  const migrationPage=await context.newPage();await installNativeMock(migrationPage);await migrationPage.goto(url);
  const migrated=await migrationPage.evaluate(async key=>(await import("/src/chat-storage.ts")).readConversation(key),keys[0]);
  assert.equal(migrated,value("Final acknowledged reply á 🛠️"));
  assert.equal(projectFiles.size,1,"Existing database history was not migrated to the selected project");
  const fresh=await context.browser().newContext();
  try {
    const next=await fresh.newPage();await installNativeMock(next);await next.goto(url.replace("127.0.0.1","localhost"));
    const recovered=await next.evaluate(async keys=>{
      const store=await import("/src/chat-storage.ts");return Promise.all(keys.map(k=>store.readConversation(k)));
    },keys);
    assert.deepEqual(recovered,[migrated,null,null],"Project history did not survive an empty browser profile and different origin");
  } finally {await fresh.close();}
  console.log("PASS: strict commit acknowledgement, aborted-write preservation, ordered writes after failure, invalid/oversized refusal, provider/folder isolation and exact Unicode recovery after crashing the owned browser process; no localStorage fallback");
} finally {await context?.close();await server.close();}
