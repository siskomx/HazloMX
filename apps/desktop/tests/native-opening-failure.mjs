import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";
import os from "node:os";
import net from "node:net";
import { createHash } from "node:crypto";
import { chromium } from "playwright-core";

const [executable, verifiedProject, freecad, format="fcstd"] = process.argv.slice(2);
assert(["fcstd","step"].includes(format),"Opening fixture supports FCStd or STEP");
const buttonName=format==="step" ? "Abrir STEP" : "Abrir FCStd";
assert(executable && verifiedProject,"Pass debug app and an existing verified FreeCAD test project");
const root=await fs.mkdtemp(path.join(os.tmpdir(),"Hazlo opening á-"));
const project=path.join(root,"project");
await fs.cp(path.resolve(verifiedProject),project,{recursive:true,errorOnExist:true,force:false});
async function hashes(folder) {
  const result={};
  async function read(current){for(const entry of await fs.readdir(current,{withFileTypes:true})){const file=path.join(current,entry.name);if(entry.isDirectory())await read(file);else if(entry.isFile()){const relative=path.relative(folder,file);if(relative!==path.join(".hazlomexa","writer.lock"))result[relative]=createHash("sha256").update(await fs.readFile(file)).digest("hex");}else throw new Error("Unexpected project entry");}}
  await read(folder);return result;
}
const before=await hashes(project);
const profile=path.join(root,"freecad-profile");await fs.mkdir(profile);
const readbackFile=path.join(root,"gui-readback.json");
if(freecad){
  const module=path.join(profile,"Mod","HazloOpeningReadback");await fs.mkdir(module,{recursive:true});
  await fs.writeFile(path.join(module,"Init.py"),`with open(${JSON.stringify(path.join(root,"observer-app-ready.txt"))}, 'w', encoding='utf-8') as marker:
    marker.write('app observer loaded')
`);
  await fs.writeFile(path.join(module,"InitGui.py"),`with open(${JSON.stringify(path.join(root,"observer-gui-ready.txt"))}, 'w', encoding='utf-8') as marker:
    marker.write('gui observer loaded')
import __main__, json
import FreeCAD as App
import FreeCADGui as Gui
from PySide import QtCore
__main__.hazloOpeningTicks = 0
def captureHazloOpening():
    import __main__, json, FreeCAD as App
    __main__.hazloOpeningTicks += 1
    doc = App.ActiveDocument
    shapes = []
    if doc:
        for obj in doc.Objects:
            if hasattr(obj, 'Shape') and not obj.Shape.isNull():
                shape = obj.Shape
                box = shape.BoundBox
                shapes.append({'name': obj.Name, 'type': obj.TypeId, 'size': [box.XLength, box.YLength, box.ZLength], 'volume': shape.Volume, 'solids': len(shape.Solids), 'valid': shape.isValid()})
    if shapes or __main__.hazloOpeningTicks >= 400:
        result = {'document': doc.Name if doc else None, 'label': doc.Label if doc else None, 'filename': doc.FileName if doc else None, 'shapes': shapes, 'user_config': App.ConfigGet('UserConfigPath'), 'user_data': App.ConfigGet('UserAppData')}
        with open(${JSON.stringify(readbackFile)}, 'w', encoding='utf-8') as output:
            json.dump(result, output, ensure_ascii=False)
        __main__.hazloOpeningTimer.stop()
__main__.hazloOpeningTimer = QtCore.QTimer(Gui.getMainWindow())
__main__.hazloOpeningTimer.timeout.connect(captureHazloOpening)
__main__.hazloOpeningTimer.start(100)
`);
}
if(freecad)assert.equal(execFileSync("powershell.exe",["-NoProfile","-Command","@(Get-Process -Name FreeCAD -ErrorAction SilentlyContinue).Count"],{windowsHide:true,encoding:"utf8"}).trim(),"0","Do not reuse a user's running FreeCAD instance");
const listener=net.createServer();await new Promise(resolve=>listener.listen(0,"127.0.0.1",resolve));
const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));
const child=spawn(path.resolve(executable),[],{windowsHide:true,env:{...process.env,
  HAZLOMEXA_SMOKE_PROJECT:project,HAZLOMEXA_FREECAD_CMD:freecad ? path.resolve(freecad) : path.join(root,"unavailable-runtime","FreeCADCmd.exe"),
  FREECAD_USER_HOME:profile,FREECAD_USER_DATA:profile,FREECAD_USER_TEMP:profile,
  WEBVIEW2_USER_DATA_FOLDER:path.join(root,"webview"),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`},stdio:["ignore","pipe","pipe"]});
let log="",browser,main;for(const stream of [child.stdout,child.stderr])stream.on("data",bytes=>{log=(log+bytes.toString()).slice(-8192);});
let observer,observed=[],openingProof,guiReadback;
try {
  const deadline=Date.now()+30000;
  while(!browser && Date.now()<deadline){assert.equal(child.exitCode,null,log);try{browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`,{timeout:1000});}catch{await new Promise(resolve=>setTimeout(resolve,100));}}
  assert(browser,"Native WebView did not start");
  const pagesDeadline=Date.now()+15000;
  while(!main && Date.now()<pagesDeadline){for(const page of browser.contexts()[0].pages()){if(await page.evaluate(()=>window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(()=>undefined)==="main")main=page;}if(!main)await new Promise(resolve=>setTimeout(resolve,100));}
  assert(main,"Main WebView missing");main.setDefaultTimeout(10000);
  await main.waitForFunction(()=>document.querySelectorAll("#history button").length>=2);
  const initial=await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(initial.freecad_available,!!freecad);
  assert(initial.recovered.some(run=>run.phase==="completed" && run.input.dimensions_mm));
  let observation="";
  if(freecad){
    observer=spawn("powershell.exe",["-NoProfile","-NonInteractive","-Command",`$ErrorActionPreference='Stop'; Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name | Out-Null; 'READY'; $deadline=(Get-Date).AddSeconds(20); while((Get-Date) -lt $deadline){$all=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name);$owned=@(${child.pid});do{$next=@($all | Where-Object {$_.ParentProcessId -in $owned -and $_.ProcessId -notin $owned} | ForEach-Object {$_.ProcessId});$owned+=$next}while($next.Count);$cad=@($all | Where-Object {$_.ProcessId -in $owned -and $_.Name -eq 'FreeCAD.exe'} | ForEach-Object {[int]$_.ProcessId});if($cad.Count){ConvertTo-Json -InputObject $cad -Compress;break};Start-Sleep -Milliseconds 20}`],{windowsHide:true,stdio:["ignore","pipe","pipe"]});
    observer.stdout.on("data",bytes=>{observation+=bytes.toString();});let observerError="";observer.stderr.on("data",bytes=>{observerError=(observerError+bytes.toString()).slice(-4096);});
    const readyDeadline=Date.now()+15000;while(!observation.includes("READY") && Date.now()<readyDeadline){assert.equal(observer.exitCode,null,observerError);await new Promise(resolve=>setTimeout(resolve,20));}assert(observation.includes("READY"),"Opening observer did not initialize");
  }
  await main.locator("#history").getByRole("button",{name:buttonName,exact:true}).first().click();
  if(freecad){
    await main.waitForFunction(()=>document.getElementById("opening-status").textContent.includes("Se inició FreeCAD"));
    const processDeadline=Date.now()+20000;while(!observed.length && Date.now()<processDeadline){const line=observation.split(/\r?\n/).find(line=>line.startsWith("["));if(line)observed=JSON.parse(line);if(!observed.length)await new Promise(resolve=>setTimeout(resolve,20));}assert(observed.length,"No owned FreeCAD GUI process observed");
    const guiPid=observed[0];const windowDeadline=Date.now()+30000;
    while(Date.now()<windowDeadline){openingProof=JSON.parse(execFileSync("powershell.exe",["-NoProfile","-NonInteractive","-Command",`[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); $gui=Get-Process -Id ${guiPid} -ErrorAction Stop; $metadata=Get-CimInstance Win32_Process -Filter 'ProcessId=${guiPid}' -Property ProcessId,CommandLine,ExecutablePath; @{pid=$gui.Id;title=$gui.MainWindowTitle;window=$gui.MainWindowHandle.ToInt64();command=$metadata.CommandLine;executable=$metadata.ExecutablePath}|ConvertTo-Json -Compress`],{windowsHide:true,encoding:"utf8"}));try{guiReadback=JSON.parse(await fs.readFile(readbackFile,"utf8"));}catch(error){if(!["ENOENT"].includes(error.code) && !(error instanceof SyntaxError))throw error;}if(openingProof.window && guiReadback?.shapes?.length)break;await new Promise(resolve=>setTimeout(resolve,100));}
    assert(openingProof.window && guiReadback?.shapes?.length,"GUI did not confirm loaded geometry");
    const completed=initial.recovered.find(run=>run.phase==="completed" && run.input.dimensions_mm);const expected=path.join(project,".hazlomexa","artifacts",completed.input.job_id,format==="step" ? "solid.step" : "solid.FCStd");
    openingProof.expected=expected;
    await fs.writeFile(path.join(root,"opening-proof.json"),JSON.stringify(openingProof,null,2));
    assert(openingProof.command.includes(expected),"GUI did not receive the exact saved artifact path");
    if(format==="fcstd")assert.equal(path.resolve(guiReadback.filename).toLowerCase(),path.resolve(expected).toLowerCase(),"Active GUI document differs from the saved native artifact");
    assert.equal(guiReadback.shapes.length,1);
    const shape=guiReadback.shapes[0];const dimensions=Object.values(completed.input.dimensions_mm);
    for(const [index,key] of ["length","width","height"].entries())assert(Math.abs(shape.size[index]-completed.input.dimensions_mm[key])<1e-6);
    assert(Math.abs(shape.volume-dimensions.reduce((a,b)=>a*b,1))<1e-6);
    assert.equal(shape.solids,1);assert.equal(shape.valid,true);
    for(const key of ["user_config","user_data"])assert.equal(path.resolve(guiReadback[key]).toLowerCase(),path.resolve(profile).toLowerCase(),"GUI settings escaped the owned profile");
    await fs.writeFile(path.join(root,"opening-proof.json"),JSON.stringify(openingProof,null,2));
    await main.screenshot({path:path.join(root,"opening-started.png"),fullPage:true});
  }else{
    await main.waitForFunction(()=>document.getElementById("opening-status").textContent.includes("No se pudo abrir en FreeCAD"));
    assert.match(await main.locator("#error").innerText(),/archivos.*guardados.*falta FreeCAD\.exe/i);
    await main.screenshot({path:path.join(root,"opening-failed.png"),fullPage:true});
  }
  assert.equal(await main.locator("#history").getByRole("button",{name:buttonName,exact:true}).count(),initial.recovered.filter(run=>run.phase==="completed" && run.input.dimensions_mm).length);
  assert.deepEqual(await hashes(project),before,"Opening failure changed engineering files");
  await main.reload();
  await main.waitForFunction(()=>document.querySelectorAll("#history button").length>=2);
  const final=await main.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.deepEqual(final.recovered,initial.recovered);
  assert.deepEqual(await hashes(project),before);
  await main.screenshot({path:path.join(root,"recovered.png"),fullPage:true});
  const result={pass:true,scope:freecad ? "native debug artifact opening in actual FreeCAD GUI" : "native debug opening failure on real saved FreeCAD output",format,project,
    verified_history_preserved:true,engineering_project_bytes_preserved:true,excluded_operational_file:".hazlomexa/writer.lock",reload_recovery:true,model_inference:false,
    actual_gui_document_opened:!!freecad,runtime_unavailable:!freecad,opening_proof:openingProof,gui_readback:guiReadback};
  await fs.writeFile(path.join(root,"result.json"),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
} catch(error){await fs.writeFile(path.join(root,"failure.json"),JSON.stringify({error:String(error),log,opening_proof:openingProof},null,2));throw error;}
finally{if(observer && observer.exitCode===null)observer.kill();if(observed.length)execFileSync("powershell.exe",["-NoProfile","-NonInteractive","-Command",`@(${observed.join(",")}) | ForEach-Object {$ownedGui=Get-CimInstance Win32_Process -Filter "ProcessId=$_" -Property Name,CommandLine; if($ownedGui.Name -eq 'FreeCAD.exe' -and $ownedGui.CommandLine.Contains('${project.replaceAll("'","''")}')){Stop-Process -Id $_ -ErrorAction Stop}}`],{windowsHide:true});if(browser)await browser.close().catch(()=>{});if(child.exitCode===null){const ended=new Promise(resolve=>child.once("exit",resolve));child.kill();await ended;}console.log(`Evidence retained at ${root}`);}
