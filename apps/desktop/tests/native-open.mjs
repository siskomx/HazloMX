import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { promises as fs } from "node:fs";
import { createHash } from "node:crypto";
import os from "node:os";
import path from "node:path";
import net from "node:net";
import { chromium } from "playwright-core";

const [executable, freecad, project, format = "fcstd"] = process.argv.slice(2);
assert(executable && freecad && project && ["fcstd", "step"].includes(format));
const root = await fs.mkdtemp(path.join(os.tmpdir(), "Hazlo-open-"));
const listener = net.createServer();
await new Promise(resolve => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise(resolve => listener.close(resolve));
const child = spawn(path.resolve(executable), [], { windowsHide: true,
  env: { ...process.env, HAZLOMEXA_SMOKE_PROJECT: path.resolve(project), HAZLOMEXA_FREECAD_CMD: path.resolve(freecad),
    WEBVIEW2_USER_DATA_FOLDER: path.join(root, "webview"), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, stdio: "ignore" });
let browser;
try {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 1000 }); break; } catch {}
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert(browser);
  let main;
  const pagesDeadline = Date.now() + 15000;
  while (!main && Date.now() < pagesDeadline) {
    for (const page of browser.contexts()[0].pages()) {
      if (await page.evaluate(() => window.__TAURI_INTERNALS__?.metadata?.currentWebview?.label).catch(() => null) === "main") main = page;
    }
    if (!main) await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert(main);
  const before = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  const job = before.recovered.find(job => job.phase === "completed"); assert(job);
  const directory = path.join(project, ".hazlomexa", "artifacts", job.input.job_id);
  const hashes = async () => Object.fromEntries(await Promise.all(["solid.FCStd", "solid.step", "result.json"].map(async name => [name, createHash("sha256").update(await fs.readFile(path.join(directory, name))).digest("hex")])));
  const originals = await hashes();
  const outcome = await main.evaluate(async ({ jobId, format }) => {
    try { await window.__TAURI_INTERNALS__.invoke("open_artifact", { jobId, format }); return { requested: true }; }
    catch (error) { return { requested: false, error: String(error) }; }
  }, { jobId: job.input.job_id, format });
  const after = await main.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_snapshot"));
  assert.equal(after.recovered.find(item => item.input.job_id === job.input.job_id).phase, "completed");
  assert.deepEqual(await hashes(), originals);
  const result = { ...outcome, format, directory, project, checks: ["opening preserves completed phase", "opening preserves artifact bytes"], note: "A successful request still requires separate GUI observation to prove the document loaded" };
  await fs.writeFile(path.join(root, "result.json"), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result, null, 2));
} finally {
  if (browser) await browser.close().catch(() => {});
  if (child.exitCode === null) child.kill();
  console.log(`Evidence retained at ${root}`);
}
