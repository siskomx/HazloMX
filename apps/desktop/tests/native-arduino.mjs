import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import { chromium } from "playwright-core";

const [executable, cli, data] = process.argv.slice(2);
assert(executable && cli && data, "Usage: native-arduino.mjs <debug-app.exe> <arduino-cli.exe> <owned-data>");
const root = await fs.mkdtemp(path.join(os.tmpdir(), "Hazlo Arduino á native-"));
const project = path.join(root, "project");
await fs.mkdir(project);
await fs.writeFile(path.join(project, "original.txt"), "preserve original");
const listener = net.createServer();
await new Promise(resolve => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const child = spawn(path.resolve(executable), [], { windowsHide: true, stdio: ["ignore", "pipe", "pipe"],
  env: { ...process.env, HAZLOMEXA_SMOKE_PROJECT: project, HAZLOMEXA_ARDUINO_CLI: path.resolve(cli), HAZLOMEXA_ARDUINO_DATA: path.resolve(data),
    WEBVIEW2_USER_DATA_FOLDER: path.join(root, "webview"), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` } });
let browser, main, log = "";
for (const stream of [child.stdout, child.stderr]) stream.on("data", bytes => { log = (log + bytes.toString()).slice(-16384); });
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function processes() {
  const script = "@(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name | Select-Object ProcessId,ParentProcessId,Name) | ConvertTo-Json -Compress";
  return JSON.parse(execFileSync("rtk", ["proxy", "powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script], { windowsHide: true, encoding: "utf8" }));
}
function ownedCompilers(all) {
  const ids = new Set([child.pid]);
  let changed;
  do { changed = false; for (const process of all) if (ids.has(process.ParentProcessId) && !ids.has(process.ProcessId)) { ids.add(process.ProcessId); changed = true; } } while (changed);
  return all.filter(process => ids.has(process.ProcessId) && /avr-g\+\+|cc1plus|avr-gcc/i.test(process.Name)).map(process => process.ProcessId);
}
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline && !browser) {
    assert.equal(child.exitCode, null, `App exited: ${log}`);
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); } catch { await delay(100); }
  }
  assert(browser, "Native WebView2 did not start");
  let denied;
  while (Date.now() < deadline && (!main || !denied)) {
    for (const page of browser.contexts()[0].pages()) {
      const label = await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(() => null);
      if (label === "main") main = page;
      if (label === "unprivileged") denied = page;
    }
    if (!main || !denied) await delay(100);
  }
  assert(main && denied);
  main.setDefaultTimeout(15000);
  const deniedResult = await denied.evaluate(async () => {
    try { await window.__TAURI_INTERNALS__.invoke("start_compile", { fqbn: "arduino:avr:uno", source: "void setup() {} void loop() {}" }); return "ALLOWED"; }
    catch (error) { return String(error); }
  });
  assert.notEqual(deniedResult, "ALLOWED");
  const snapshot = () => main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  await main.waitForFunction(() => !document.getElementById("workflow").disabled && !document.getElementById("infer").disabled);
  await main.locator("#workflow").selectOption("arduino");
  await main.locator("#prompt").fill("Un ejemplo para parpadear el LED integrado de Arduino Uno");
  await main.locator("#infer").click();
  await main.waitForFunction(() => document.getElementById("sketch-source").value.includes("LED_BUILTIN"));
  await main.locator("#prepare-compile").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const first = await snapshot();
  assert.equal(first.inference, null);
  assert.equal((await fs.readdir(path.join(project, ".hazlomexa/staging"))).length, 0);
  const source = await main.locator("#sketch-source").inputValue() + "\n// approved edit\n";
  await main.locator("#sketch-source").fill(source);
  assert(await main.locator("#approve").isDisabled());
  await main.locator("#prepare-compile").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const edited = await snapshot();
  assert.notEqual(edited.job.approval_id, first.job.approval_id);
  const stale = await main.evaluate(async approvalId => {
    try { await window.__TAURI_INTERNALS__.invoke("approve_operation", { approvalId, events: "__CHANNEL__:90001" }); return "ALLOWED"; }
    catch (error) { return String(error); }
  }, first.job.approval_id);
  assert.notEqual(stale, "ALLOWED");
  await main.locator("#approve").focus();
  await main.keyboard.press("Enter");
  await main.waitForFunction(() => document.getElementById("publication-result").textContent.includes("disponibles"), null, { timeout: 120000 });
  const complete = await snapshot();
  const id = complete.job.snapshot.input.job_id;
  assert.equal(complete.job.snapshot.phase, "completed");
  const artifacts = path.join(project, ".hazlomexa/artifacts", id);
  const hashes = {};
  for (const name of ["firmware.hex", "firmware.elf", "Sketch.ino"]) hashes[name] = createHash("sha256").update(await fs.readFile(path.join(artifacts, name))).digest("hex");
  assert.equal(await fs.readFile(path.join(artifacts, "Sketch.ino"), "utf8"), source);
  await main.reload();
  await main.waitForFunction(() => document.querySelectorAll("#artifacts button").length === 3);
  assert.equal((await snapshot()).job.snapshot.input.job_id, id);
  await main.locator("#prepare-compile").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  const next = await snapshot();
  await main.locator("#approve").click();
  let observed = [];
  const compilerDeadline = Date.now() + 10000;
  while (Date.now() < compilerDeadline && !observed.length) {
    observed = ownedCompilers(processes());
    if (!observed.length) { const current = await snapshot(); assert(current.job.busy, "Compilation finished before cancellation observation"); await delay(10); }
  }
  assert(observed.length, "No owned compiler was observed");
  const cancelStarted = performance.now();
  await main.locator("#cancel").click();
  await main.waitForFunction(() => document.getElementById("status").textContent.includes("cancelada") && document.getElementById("cancel").disabled);
  const cancellationMs = performance.now() - cancelStarted;
  assert(cancellationMs < 5000);
  const live = new Set(processes().map(process => process.ProcessId));
  assert(observed.every(pid => !live.has(pid)), "Observed compiler survived cancellation");
  assert.equal((await snapshot()).job.snapshot.phase, "cancelled");
  await assert.rejects(fs.access(path.join(project, ".hazlomexa/artifacts", next.job.snapshot.input.job_id)));
  await main.locator("#sketch-source").fill("void setup() { missing_identifier; } void loop() {}\n");
  await main.locator("#prepare-compile").click();
  await main.waitForFunction(() => !document.getElementById("approve").disabled);
  await main.locator("#approve").click();
  await main.waitForFunction(() => !document.getElementById("error").hidden && document.getElementById("status").textContent.includes("fall"));
  assert.equal((await snapshot()).job.snapshot.phase, "failed");
  await main.reload();
  await main.waitForFunction(() => document.querySelectorAll("#history button").length === 3);
  for (const [name, hash] of Object.entries(hashes)) assert.equal(createHash("sha256").update(await fs.readFile(path.join(artifacts, name))).digest("hex"), hash);
  assert.equal(await fs.readFile(path.join(project, "original.txt"), "utf8"), "preserve original");
  await main.screenshot({ path: path.join(root, "completed-and-failed.png"), fullPage: true });
  const result = { pass: true, root, project, job_id: id, hashes, unprivileged_denial: deniedResult, cancellation_ms: cancellationMs, observed_compiler_pids: observed, models_run: 0,
    checks: ["actual native Arduino compile approval", "keyboard approval", "edit/stale approval rejection", "no staging before approval", "actual compiler cancellation and independent PID cessation", "invalid sketch failure", "reload and artifact preservation"] };
  await fs.writeFile(path.join(root, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} catch (error) {
  await fs.writeFile(path.join(root, "failure.json"), JSON.stringify({ error: String(error), log, state: main ? await main.evaluate(() => ({ error: document.getElementById("error")?.textContent, status: document.getElementById("status")?.textContent })).catch(() => null) : null }, null, 2));
  throw error;
} finally {
  await browser?.close().catch(() => {});
  if (child.exitCode === null) { const exited = new Promise(resolve => child.once("exit", resolve)); child.kill(); await exited; }
  console.log(`Evidence retained at ${root}`);
}
