import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import os from "node:os";
import net from "node:net";
import {checkInitialChat} from "./initial-chat-check.mjs";
const {chromium}=await import(process.env.HAZLOMEXA_TEST_PLAYWRIGHT ?? "playwright-core");
const [executable,output,sandboxArgument]=process.argv.slice(2);
assert(executable && output,"Pass installed release app and fresh output folder");
assert(!sandboxArgument || /^--sandbox-policy-port=\d+$/.test(sandboxArgument),"Unknown startup test argument");
const sandboxPort=sandboxArgument ? Number(sandboxArgument.split("=")[1]) : undefined;
if(sandboxPort!==undefined){assert(process.env.USERNAME==="WDAGUtilityAccount" && path.resolve(executable).toLowerCase()==="c:\\hazloprobeapp\\hazlomexa.exe","Disposable guest policy mode only");assert(Number.isInteger(sandboxPort) && sandboxPort>1024 && sandboxPort<=65535,"Invalid guest debugger port");}
await fs.mkdir(output);
const root=await fs.mkdtemp(path.join(os.tmpdir(),"Hazlo release startup-"));
const elevated=execFileSync("powershell.exe",["-NoProfile","-Command","[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)"],{windowsHide:true,encoding:"utf8",timeout:10000}).trim()==="True";
if(elevated && sandboxPort===undefined){
  const failure={error:"The UI test driver is elevated. WebView2 ignores its debugging environment; run this test from a standard-user process.",elevated_host:true,app_started:false,scope:"test prerequisite; native UI unverified"};
  await fs.writeFile(path.join(output,"failure.json"),JSON.stringify(failure,null,2));
  throw new Error(failure.error);
}
let port=sandboxPort;
if(port===undefined){const listener=net.createServer();await new Promise(resolve=>listener.listen(0,"127.0.0.1",resolve));port=listener.address().port;await new Promise(resolve=>listener.close(resolve));}
const appEnvironment={...process.env};
if(sandboxPort===undefined){appEnvironment.WEBVIEW2_USER_DATA_FOLDER=path.join(root,"webview");appEnvironment.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=`--remote-debugging-port=${port}`;}
else{delete appEnvironment.WEBVIEW2_USER_DATA_FOLDER;delete appEnvironment.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS;}
const child=spawn(path.resolve(executable),[],{cwd:path.dirname(path.resolve(executable)),windowsHide:true,env:appEnvironment,stdio:["ignore","pipe","pipe"]});
let browser,main,lastConnectionError,log="";for(const stream of [child.stdout,child.stderr])stream.on("data",bytes=>{log=(log+bytes.toString()).slice(-8192);});
try{
  const deadline=Date.now()+30000;
  while(!browser && Date.now()<deadline){assert.equal(child.exitCode,null,log);try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});}catch(error){lastConnectionError=String(error);await new Promise(resolve=>setTimeout(resolve,100));}}
  assert(browser,"Installed WebView did not start");
  const pageDeadline=Date.now()+15000;
  while(!main && Date.now()<pageDeadline){for(const page of browser.contexts()[0].pages()){if(await page.evaluate(()=>window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(()=>undefined)==="main")main=page;}if(!main)await new Promise(resolve=>setTimeout(resolve,100));}
  assert(main,"Installed main WebView missing");main.setDefaultTimeout(10000);
  await main.locator("#open-project:enabled").waitFor({state:"attached"});
  const snapshot=await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  const initial=await checkInitialChat(main,snapshot);
  await main.screenshot({path:path.join(output,"startup.png"),fullPage:true});
  await main.reload();await main.locator("#open-project:enabled").waitFor({state:"attached"});
  await checkInitialChat(main,await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot")));
  const result={pass:true,scope:"installed release native initial chat UI and read-only IPC",...initial,
    reload_preserves_initial_chat:true,webview_version:browser.version(),elevated_host:elevated,debugger_configuration:sandboxPort===undefined ? "environment" : "disposable guest per-app policy",project_pickers_operated:false,engineering_ui_workflow:false,model_inference:false};
  await fs.writeFile(path.join(output,"result.json"),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
}catch(error){await fs.writeFile(path.join(output,"failure.json"),JSON.stringify({error:String(error),log,lastConnectionError,port,root,app_pid:child.pid,exit_code:child.exitCode},null,2));throw error;}
finally{if(browser)await browser.close().catch(()=>{});if(child.exitCode===null){const ended=new Promise(resolve=>child.once("exit",resolve));child.kill();await ended;}}
