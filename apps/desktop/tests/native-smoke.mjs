import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import http from "node:http";
const { chromium } = await import(process.env.HAZLOMEXA_TEST_PLAYWRIGHT ?? "playwright-core");

const [executable, freecad, mode, ollama] = process.argv.slice(2);
const cloud = mode === "--cloud";
const missingModel = mode === "--missing-model";
assert(!cloud || ollama, "Cloud smoke requires the installed ollama.exe path");
assert(executable && freecad, "Usage: node tests/native-smoke.mjs <debug-app.exe> <FreeCADCmd.exe>");
const root = await fs.mkdtemp(path.join(os.tmpdir(), "Hazlo Mexa á native-"));
const project = path.join(root, "project");
await fs.mkdir(project);
const listener = net.createServer();
await new Promise(resolve => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const child = spawn(path.resolve(executable), [], {
  windowsHide: true,
  env: { ...process.env, HAZLOMEXA_SMOKE_PROJECT: project, HAZLOMEXA_FREECAD_CMD: path.resolve(freecad),
    WEBVIEW2_USER_DATA_FOLDER: path.join(root, "webview"), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: ["ignore", "pipe", "pipe"],
});
function windowsProcessIds(script) {
  return JSON.parse(execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], { windowsHide: true, encoding: "utf8" }).trim());
}
let log = "";
child.stdout.on("data", bytes => { log = (log + bytes.toString()).slice(-16384); });
child.stderr.on("data", bytes => { log = (log + bytes.toString()).slice(-16384); });
let browser;
let lastPage;
let server;
let fixture;
const fixtureRequests = [];
let serverLog = "";
const inferenceChecks = [];
let inferenceCancellationMs;
let cadCancellationMs;
let observedCadProcesses = [];
let cadObserver;
let publicationRace;
try {
  if (missingModel) {
    fixture = http.createServer((request, response) => {
      fixtureRequests.push(request.url);
      response.setHeader("Content-Type", "application/json");
      if (request.url === "/api/version") response.end(JSON.stringify({ version: "0.35.1" }));
      else if (request.url === "/api/tags") response.end(JSON.stringify({ models: [] }));
      else { response.statusCode = 404; response.end(JSON.stringify({ error: "unexpected request" })); }
    });
    await new Promise((resolve, reject) => { fixture.once("error", reject); fixture.listen(11434, "127.0.0.1", resolve); });
  }
  if (cloud) {
    // This test owns only this server. It must not replace a user's existing service.
    try { await fetch("http://127.0.0.1:11434/api/version", { signal: AbortSignal.timeout(1500) }); throw new Error("Ollama port already in use; stop this test before taking ownership."); }
    catch (error) { if (String(error).includes("already in use")) throw error; }
    server = spawn(path.resolve(ollama), ["serve"], { windowsHide: true, cwd: root,
      env: { ...process.env, OLLAMA_HOST: "127.0.0.1:11434", OLLAMA_NO_CLOUD: "0", OLLAMA_NOPRUNE: "1", OLLAMA_REMOTES: "ollama.com", OLLAMA_MODELS: path.join(os.homedir(), ".ollama", "models") }, stdio: ["ignore", "pipe", "pipe"] });
    for (const pipe of [server.stdout, server.stderr]) pipe.on("data", bytes => { serverLog = (serverLog + bytes.toString()).slice(-16384); });
    const serverDeadline = Date.now() + 30000;
    let ready = false;
    while (Date.now() < serverDeadline) {
      if (server.exitCode !== null) throw new Error(`Owned Ollama exited: ${serverLog}`);
      try { const result = await fetch("http://127.0.0.1:11434/api/version", { signal: AbortSignal.timeout(1000) }); if (result.ok) { ready = true; break; } } catch {}
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    assert(ready, "Owned Ollama server did not start");
  }
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Native app exited ${child.exitCode}: ${log}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; }
    catch { await new Promise(resolve => setTimeout(resolve, 250)); }
  }
  assert(browser, "WebView2 CDP did not start");
  const context = browser.contexts()[0];
  let main, denied;
  const pagesDeadline = Date.now() + 15000;
  while (Date.now() < pagesDeadline && (!main || !denied)) {
    for (const page of context.pages()) {
      const label = await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(() => undefined);
      if (label === "main") main = page;
      if (label === "unprivileged") denied = page;
    }
    if (!main || !denied) await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert(main && denied, "Expected privileged and unprivileged native webviews");
  lastPage = main;
  await main.bringToFront();
  await main.locator("#manual-dimensions > summary").click();
  main.setDefaultTimeout(10000);
  await main.waitForFunction(() => document.getElementById("project-path")?.textContent?.includes("native-"), null, { timeout: 30000 });
  const fixtureText = await fs.readFile(process.env.HAZLOMEXA_TEST_CONTRACT_FIXTURE ?? new URL("../src/contract-fixture.ts", import.meta.url), "utf8");
  const dimensionLimits = JSON.parse(fixtureText.split("// JSON-BEGIN")[1].split("// JSON-END")[0]).dimension_limits;
  for (const id of ["length", "width", "height"]) {
    assert.equal(Number(await main.locator(`#${id}`).getAttribute("min")), dimensionLimits.minimum);
    assert.equal(Number(await main.locator(`#${id}`).getAttribute("max")), dimensionLimits.maximum);
    assert.equal(await main.locator(`#${id}`).inputValue(), "");
  }
  assert.equal(await main.locator("#prepare").isDisabled(), true);
  await main.locator("#unit").selectOption("in");
  for (const id of ["length", "width", "height"]) assert.equal(await main.locator(`#${id}`).inputValue(), "");
  await main.locator("#length").fill("1");
  await main.locator("#unit").selectOption("cm");
  assert.equal(await main.locator("#length").inputValue(), "2.54");
  for (const id of ["width", "height"]) assert.equal(await main.locator(`#${id}`).inputValue(), "");
  assert.equal(await main.locator("#prepare").isDisabled(), true);
  assert.equal(await main.locator(".preview svg").isVisible(), false);
  const unknown = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(unknown.job, null);
  await main.screenshot({path:path.join(root,"unknown-dimensions.png"),fullPage:true});
  inferenceChecks.push("native unknown dimensions create no job", "partial unit conversion preserves unknown fields");
  await main.locator("#unit").selectOption("mm");
  for (const [id, value] of [["length", "10"], ["width", "20"], ["height", "30"]]) await main.locator(`#${id}`).fill(value);
  const deniedResult = await denied.evaluate(async () => {
    try { await window.__TAURI_INTERNALS__.invoke("get_snapshot"); return "ALLOWED"; }
    catch (error) { return String(error); }
  });
  assert.notEqual(deniedResult, "ALLOWED", "Unprivileged webview bypassed app permissions");
  async function keyboardTo(id) {
    for (let step = 0; step < 40; step++) {
      if (await main.evaluate(id => document.activeElement?.id === id, id)) {
        const focus = await main.locator(`#${id}`).evaluate(element => ({ visible: element.matches(":focus-visible"), outline: getComputedStyle(element).outlineWidth }));
        assert(focus.visible && parseFloat(focus.outline) >= 2, `No visible keyboard focus on ${id}: ${JSON.stringify(focus)}`);
        return;
      }
      await main.keyboard.press("Tab");
    }
    throw new Error(`Keyboard navigation did not reach ${id}`);
  }
  for (const id of ["length", "width", "height", "prepare", "model-profile", "prompt", "infer"]) await keyboardTo(id);
  async function waitInference() {
    const deadline = Date.now() + 125000;
    while (Date.now() < deadline) {
      const current = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
      if (current.inference && !current.inference.busy) return current;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    throw new Error("Inference did not finish");
  }
  if (missingModel) {
    await main.locator("#prompt").fill("Un bloque de largo 10 mm, ancho 20 mm y alto 30 mm");
    await main.locator("#infer").click();
    const rejected = await waitInference();
    assert(rejected.inference.error, "Missing model must report an error");
    assert.equal(rejected.job, null);
    assert(!fixtureRequests.includes("/api/generate"), "Prompt sent despite missing model");
    await main.waitForFunction(() => !document.getElementById("error").hidden);
    assert.deepEqual(fixtureRequests, ["/api/version", "/api/tags"], "Missing model attempted inference/download/fallback");
    inferenceChecks.push("missing model reports error without sending prompt or creating job");
  }
  if (cloud) {
    await main.locator("#model-profile").selectOption("cloud");
    await main.locator("#prompt").fill("Un bloque de largo 10 mm y ancho 20 mm");
    await main.locator("#infer").click();
    let s = await waitInference();
    assert(s.inference.message, JSON.stringify(s.inference)); assert.equal(s.job, null);
    await main.locator("#prompt").fill("Un cubo de 20 mm por lado");
    await main.locator("#infer").click();
    const cancelStart = performance.now();
    await main.locator("#cancel-inference").click();
    s = await waitInference();
    inferenceCancellationMs = performance.now() - cancelStart;
    assert(inferenceCancellationMs < 1000, `Inference cancellation took ${inferenceCancellationMs}ms`);
    assert(s.inference.cancelled); assert.equal(s.job, null);
    await main.locator("#prompt").fill("Un bloque de largo 10 mm, ancho 20 mm y alto 30 mm");
    await main.locator("#infer").click();
    await main.reload();
    await main.waitForFunction(() => !document.getElementById("approve").disabled);
    s = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
    assert.equal(s.inference.model, "kimi-k2.7-code:cloud"); assert(s.inference.text.length > 0);
    assert.equal(await main.locator("#model-profile").inputValue(), "cloud");
    assert.deepEqual(s.job.snapshot.input.dimensions_mm, { length: 10, width: 20, height: 30 });
    assert.equal(await main.locator("#height").inputValue(), "30");
    inferenceChecks.push("cloud clarification creates no job", "cloud cancellation creates no job", "cloud reload restores streamed proposal", "cloud proposal requires approval");
  } else {
    await main.locator("#unit").selectOption("in");
    for (const [id, mm] of [["length", 10], ["width", 20], ["height", 30]]) await main.locator(`#${id}`).fill(String(mm / 25.4));
    await main.locator("#prepare").click();
    await main.locator("#unit").selectOption("mm");
  }
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const before = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(before.job.snapshot.phase, "awaiting_approval");
  assert.deepEqual(before.job.snapshot.input.dimensions_mm, { length: 10, width: 20, height: 30 });
  if (missingModel) {
    assert.equal(before.inference, null, "Manual proposal retained failed inference state");
    assert.equal(await main.locator("#error").isVisible(), false, "Stale provider error survived successful manual proposal");
    inferenceChecks.push("manual fallback clears failed inference and error");
  }
  assert.equal(before.job.snapshot.schema_version, 2);
  assert.equal(before.job.snapshot.audit.sequence, before.job.sequence);
  assert.equal(before.job.snapshot.audit.source.provider, cloud ? "ollama" : "manual");
  if (cloud) {
    assert.equal(before.job.snapshot.audit.source.cloud, true);
    assert.equal(before.job.snapshot.audit.source.digest, before.inference.digest);
  }
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa", "staging"))).length, 0);
  await main.locator("#length").fill("12.5");
  assert.equal(await main.locator("#approve").isDisabled(), true);
  await main.locator("#prepare").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const edited = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.notEqual(edited.job.approval_id, before.job.approval_id);
  assert.equal(edited.job.snapshot.audit.user_edited, true);
  assert.equal(edited.job.snapshot.audit.sequence, edited.job.sequence);
  const stale = await main.evaluate(async approvalId => {
    try { await window.__TAURI_INTERNALS__.invoke("approve_operation", { approvalId, events: "__CHANNEL__:90001" }); return "ALLOWED"; }
    catch (error) { return String(error); }
  }, before.job.approval_id);
  assert.match(stale, /aprobación.*(venció|utilizada)/i);
  // Keyboard activation is the same explicit approval path used by a person.
  await keyboardTo("approve");
  await main.keyboard.press("Enter");
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Generando y verificando con FreeCAD…");
  await main.reload();
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Archivos verificados y publicados.", null, { timeout: 90000 });
  const complete = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(complete.job.snapshot.phase, "completed");
  assert.equal(complete.job.snapshot.input.dimensions_mm.length, 12.5);
  const runFile = path.join(project, ".hazlomexa", "runs", `${complete.job.snapshot.input.job_id}.json`);
  const persisted = JSON.parse(await fs.readFile(runFile, "utf8"));
  assert.deepEqual(persisted, complete.job.snapshot);
  assert.equal(persisted.audit.sequence, complete.job.sequence);
  assert.deepEqual(persisted.audit.source, before.job.snapshot.audit.source);
  assert.equal(persisted.audit.created_unix_ms, before.job.snapshot.audit.created_unix_ms);
  assert(persisted.audit.updated_unix_ms >= persisted.audit.created_unix_ms);
  assert.equal(persisted.audit.destination, `.hazlomexa/artifacts/${complete.job.snapshot.input.job_id}`);
  const artifact = path.join(project, ".hazlomexa", "artifacts", complete.job.snapshot.input.job_id);
  for (const name of ["solid.FCStd", "solid.step", "result.json"]) assert((await fs.stat(path.join(artifact, name))).size > 0);
  await main.reload();
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Archivos verificados y publicados.");
  const afterReload = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(afterReload.job.snapshot.input.job_id, complete.job.snapshot.input.job_id);
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa", "artifacts"))).length, 1);
  // Explicit denial by keyboard must not start another CAD process or publish another run.
  await keyboardTo("prepare");
  await main.keyboard.press("Enter");
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  await main.waitForFunction(() => document.querySelectorAll("#history button").length === 2);
  await keyboardTo("cancel");
  await main.keyboard.press("Enter");
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Operación cancelada.");
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa", "artifacts"))).length, 1);
  // Observe actual descendants of this test's app; never enumerate unrelated process arguments.
  await keyboardTo("prepare"); await main.keyboard.press("Enter");
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  // Warm the observer before approval so PowerShell startup cannot miss a short native process.
  let observerText="";
  cadObserver=spawn("powershell.exe",["-NoProfile","-NonInteractive","-Command",`$ErrorActionPreference='Stop'; Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name | Out-Null; 'READY'; $deadline=(Get-Date).AddSeconds(15); while((Get-Date) -lt $deadline){$all=@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name); $owned=@(${child.pid}); do{$next=@($all | Where-Object {$_.ParentProcessId -in $owned -and $_.ProcessId -notin $owned} | ForEach-Object {$_.ProcessId}); $owned+=$next}while($next.Count); $cad=@($all | Where-Object {$_.ProcessId -in $owned -and $_.Name -like '*freecad*'} | ForEach-Object {[int]$_.ProcessId}); if($cad.Count){ConvertTo-Json -InputObject $cad -Compress; break}; Start-Sleep -Milliseconds 10}`],{windowsHide:true,stdio:["ignore","pipe","pipe"]});
  cadObserver.stdout.on("data",bytes=>{observerText+=bytes.toString();});
  let observerErrors="";cadObserver.stderr.on("data",bytes=>{observerErrors=(observerErrors+bytes.toString()).slice(-4096);});
  const observerDeadline=Date.now()+15000;
  while(!observerText.includes("READY") && Date.now()<observerDeadline){assert.equal(cadObserver.exitCode,null,observerErrors);await new Promise(resolve=>setTimeout(resolve,10));}
  assert(observerText.includes("READY"),"Native process observer did not initialize");
  await keyboardTo("approve"); await main.keyboard.press("Enter");
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Generando y verificando con FreeCAD…");
  await main.locator("#cancel").focus();
  const cadStart = Date.now();
  while (!observedCadProcesses.length && Date.now() - cadStart < 3000) {
    const observed=observerText.split(/\r?\n/).find(line=>line.startsWith("["));
    if(observed)observedCadProcesses=JSON.parse(observed);
    if (!observedCadProcesses.length) await new Promise(resolve => setTimeout(resolve, 10));
  }
  assert(observedCadProcesses.length, "No actual owned FreeCAD process was observed before cancellation");
  const cancelCadStart = performance.now();
  await main.keyboard.press("Enter");
  await main.waitForFunction(() => document.getElementById("status")?.textContent === "Operación cancelada.");
  await main.waitForFunction(() => !document.getElementById("prepare").disabled, null, { timeout: 5000 });
  cadCancellationMs = performance.now() - cancelCadStart;
  assert(cadCancellationMs < 5000);
  const cancelled = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(cancelled.job.snapshot.phase, "cancelled"); assert.equal(cancelled.job.busy, false);
  assert.equal(cancelled.job.sequence, cancelled.job.snapshot.audit.sequence);
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa", "artifacts"))).length, 1);
  const alive = windowsProcessIds(`ConvertTo-Json -InputObject @(${observedCadProcesses.join(",")} | Where-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }) -Compress`);
  assert.deepEqual(alive, [], "Cancelled owned FreeCAD processes remain alive");
  await main.reload();
  await main.waitForFunction(() => document.querySelectorAll("#history button").length === 2);
  if (await main.locator("#manual-dimensions").getAttribute("open") === null) await main.locator("#manual-dimensions > summary").click();
  await main.locator("#prepare").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const raceProposal = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  const raceJob = raceProposal.job.snapshot.input.job_id;
  const raceStage = path.join(project, ".hazlomexa", "staging", raceJob);
  await main.locator("#approve").click();
  const resultDeadline = Date.now() + 60000;
  let raceResult;
  while (Date.now() < resultDeadline) {
    try { raceResult = JSON.parse(await fs.readFile(path.join(raceStage, "result.json"), "utf8")); break; }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    const live = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
    if (!live.job.busy) {
      assert.equal(live.job.snapshot.phase, "completed", live.job.error);
      raceResult = JSON.parse(await fs.readFile(path.join(project, ".hazlomexa", "artifacts", raceJob, "result.json"), "utf8"));
      break;
    }
    await new Promise(resolve => setTimeout(resolve, 5));
  }
  assert(raceResult?.success && raceResult.job_id === raceJob, "Actual CAD did not produce a successful terminal result");
  const cancelReply = await main.evaluate(jobId => window.__TAURI_INTERNALS__.invoke("cancel_job", { jobId }), raceJob);
  await main.waitForFunction(() => !document.getElementById("prepare").disabled);
  const raceFinal = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert(["cancelled", "completed"].includes(raceFinal.job.snapshot.phase));
  assert.equal(raceFinal.job.busy, false);
  assert.equal(raceFinal.job.snapshot.phase, cancelReply.snapshot.phase);
  assert.equal(raceFinal.job.sequence, raceFinal.job.snapshot.audit.sequence);
  const racePublished = path.join(project, ".hazlomexa", "artifacts", raceJob);
  const published = await fs.stat(racePublished).then(() => true, error => { if (error.code === "ENOENT") return false; throw error; });
  assert.equal(published, raceFinal.job.snapshot.phase === "completed");
  if (published) for (const name of ["solid.FCStd", "solid.step"]) assert((await fs.stat(path.join(racePublished, name))).size > 0);
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa", "artifacts"))).length, published ? 2 : 1);
  publicationRace = { job_id: raceJob, trigger: "actual fixed macro successful result observed", cancel_reply: cancelReply.snapshot.phase, final_phase: raceFinal.job.snapshot.phase, published, exact_commit_timing_observed: false };
  await main.reload();
  await main.waitForFunction(count => document.querySelectorAll("#history button").length === count, published ? 4 : 2);
  const screenshot = path.join(root, "completed.png");
  await main.screenshot({ path: screenshot, fullPage: true });
  const result = { pass: true, project, artifact, screenshot, unprivileged_denial: deniedResult, job_id: complete.job.snapshot.input.job_id,
    inference_cancellation_ms: inferenceCancellationMs, cad_cancellation_ms: cadCancellationMs, observed_cad_pids: observedCadProcesses, publication_race: publicationRace,
    checks: [...inferenceChecks, "native ACL denial", "no staging before approval", "edit invalidates approval", "stale approval rejected", "keyboard navigation with visible focus", "keyboard approval and denial", "reload during CAD", "real CAD publication", "persisted source, timestamps, edits and sequence match native snapshot", "reload preserves one job", "keyboard cancellation terminates observed real CAD processes without another published run", "native cancellation after successful actual macro result preserves one authoritative publication outcome", "previous completed result remains openable after new jobs and reload"] };
  await fs.writeFile(path.join(root, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} catch (error) {
  const ui = lastPage ? await lastPage.evaluate(() => ({ project: document.getElementById("project-path")?.textContent, error: document.getElementById("error")?.textContent, status: document.getElementById("status")?.textContent })).catch(() => null) : null;
  await fs.writeFile(path.join(root, "failure.json"), JSON.stringify({ error: String(error), ui, log }, null, 2));
  console.log(JSON.stringify({ ui }, null, 2));
  throw error;
} finally {
  if(cadObserver && cadObserver.exitCode===null)cadObserver.kill();
  if (browser) await browser.close().catch(() => {});
  if (child.exitCode === null) {
    const exited = new Promise(resolve => child.once("exit", resolve));
    child.kill();
    await exited;
  }
  if (server && server.exitCode === null) server.kill();
  if (fixture) await new Promise(resolve => fixture.close(resolve));
  console.log(`Evidence retained at ${root}`);
}
