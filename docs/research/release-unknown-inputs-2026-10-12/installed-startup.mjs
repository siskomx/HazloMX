import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import os from "node:os";
import net from "node:net";
const {chromium}=await import(process.env.HAZLOMEXA_TEST_PLAYWRIGHT ?? "playwright-core");
const [executable,output]=process.argv.slice(2);
assert(executable && output,"Pass installed release app and fresh output folder");
await fs.mkdir(output);
const root=await fs.mkdtemp(path.join(os.tmpdir(),"Hazlo release startup-"));
const listener=net.createServer();await new Promise(resolve=>listener.listen(0,"127.0.0.1",resolve));
const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));
const child=spawn(path.resolve(executable),[],{cwd:path.dirname(path.resolve(executable)),windowsHide:false,env:{...process.env,
  WEBVIEW2_USER_DATA_FOLDER:path.join(root,"webview"),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`},stdio:["ignore","pipe","pipe"]});
let browser,main,lastConnectionError,log="";for(const stream of [child.stdout,child.stderr])stream.on("data",bytes=>{log=(log+bytes.toString()).slice(-8192);});
try{
  const deadline=Date.now()+30000;
  while(!browser && Date.now()<deadline){assert.equal(child.exitCode,null,log);try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});}catch(error){lastConnectionError=String(error);await new Promise(resolve=>setTimeout(resolve,100));}}
  assert(browser,"Installed WebView did not start");
  const pageDeadline=Date.now()+15000;
  while(!main && Date.now()<pageDeadline){for(const page of browser.contexts()[0].pages()){if(await page.evaluate(()=>window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(()=>undefined)==="main")main=page;}if(!main)await new Promise(resolve=>setTimeout(resolve,100));}
  assert(main,"Installed main WebView missing");main.setDefaultTimeout(10000);
  await main.waitForFunction(()=>document.getElementById("conversation")?.textContent.includes("construir o recrear"));
  const snapshot=await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(snapshot.project,null);assert.equal(snapshot.job,null);
  for(const id of ["length","width","height"])assert.equal(await main.locator(`#${id}`).inputValue(),"");
  assert.equal(await main.locator("#prepare").isDisabled(),true);
  assert.equal(await main.locator("#approve").isDisabled(),true);
  assert.equal(await main.locator(".preview svg").isVisible(),false);
  assert.equal(await main.locator("#workflow option").count(),9);
  assert.equal(await main.locator("#create-project").isDisabled(),false);
  assert.equal(await main.locator("#open-project").isDisabled(),false);
  await main.screenshot({path:path.join(output,"startup.png"),fullPage:true});
  await main.reload();await main.waitForFunction(()=>document.getElementById("conversation")?.textContent.includes("construir o recrear"));
  for(const id of ["length","width","height"])assert.equal(await main.locator(`#${id}`).inputValue(),"");
  const result={pass:true,scope:"installed release native initial UI and read-only IPC",empty_dimensions:true,no_implicit_proposal:true,
    workflows:9,reload_preserves_unknown_inputs:true,project_pickers_operated:false,engineering_ui_workflow:false,model_inference:false};
  await fs.writeFile(path.join(output,"result.json"),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
}catch(error){await fs.writeFile(path.join(output,"failure.json"),JSON.stringify({error:String(error),log,lastConnectionError,port,root,app_pid:child.pid,exit_code:child.exitCode},null,2));throw error;}
finally{if(browser)await browser.close().catch(()=>{});if(child.exitCode===null){const ended=new Promise(resolve=>child.once("exit",resolve));child.kill();await ended;}}
