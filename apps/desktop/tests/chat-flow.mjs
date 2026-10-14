import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import path from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright-core";

const root = path.resolve(import.meta.dirname, "..");
const server = await createServer({ root, server: { host: "127.0.0.1", port: 0 } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.HAZLOMEXA_TEST_BROWSER ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", headless: true });
  const page = await browser.newPage({ viewport: { width: 1200, height: 1000 } });
  const errors = []; page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(() => {
    let callback = 0;
    const snapshot = { project: ["project-one", "C:\\owned\\project"], job: null, inference: null, recovered: [], freecad: "owned-freecad", freecad_available: true, codex: { version: "fixture" }, codex_login: { id: "login", sequence: 1, busy: false, cancelled: false, connected: true, auth_url: null, error: null }, claude: { version: "fixture" } };
    const copy = () => structuredClone(snapshot);
    window.calls = []; window.approvals = 0;
    window.__TAURI_INTERNALS__ = { transformCallback: () => ++callback, unregisterCallback: () => {}, invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (command === "get_snapshot") return copy();
      if (command === "select_codex" || command === "select_claude" || command === "select_freecad" || command === "select_cadcraft" || command === "select_arduino" || command === "connect_agent" || command === "select_kicad" || command === "select_konnect" || command === "select_orca") return copy();
      if (command === "discover_schematic") return { id: "selected-sheet", project_id: snapshot.project[0], name: "Circuito.kicad_sch", sha256: "a".repeat(64), components: [] };
      if (command === "discover_pcb") return { id: "selected-board", project_id: window.foreignSource ? "foreign-project" : snapshot.project[0], name: "Placa.kicad_pcb", sha256: "b".repeat(64), components: [] };
      if (command === "discover_live_pcb") { if (args.connection.endpoint !== "ipc://C:/owned/kicad/api.sock" || args.connection.token !== "fixture-api-secret") throw Error("Incorrect native connection fixture"); return { id: "live-board", project_id: snapshot.project[0], name: "Placa.kicad_pcb", observation: { components_sha256: "d".repeat(64), components: [] } }; }
      if (command === "discover_drawing") return { id: "selected-drawing", project_id: snapshot.project[0], inspection: { source_name: "Plano.dxf", source_sha256: "c".repeat(64) } };
      if (command === "discover_slice") return { id: "selected-slice", project_id: window.foreignSlice ? "foreign-project" : snapshot.project[0], preview: { source_name: "Pieza.stl", source_sha256: "e".repeat(64), selection_sha256: "f".repeat(64), triangles: 12, bounds_file_coordinates: [[0,0,0],[10,10,10]], declared_bed: null, bed_profiles: [] } };
      if (command === "select_agent_sketch") { snapshot.agent_source = { project_id: snapshot.project[0], source_name: "Sketch.ino", source: "void setup() {}" }; return copy(); }
      if (command === "select_project") { snapshot.project = window.otherFolder ? ["project-one", "C:\\owned\\other-folder"] : ["project-one", "C:\\owned\\project"]; snapshot.job = null; if (snapshot.chat) snapshot.chat.text = "PREVIOUS FOLDER RESPONSE"; return copy(); }
      if (command === "start_codex_chat" || command === "start_claude_chat") {
        const provider = command === "start_codex_chat" ? "codex" : "claude";
        snapshot.chat = { provider, id: crypto.randomUUID(), project_id: snapshot.project[0], sequence: 1, busy: true, cancelled: false, text: "", error: null };
        const chat = snapshot.chat;
        const finish = () => {
          if (snapshot.chat?.id !== chat.id || chat.cancelled) return;
          chat.sequence++; chat.busy = false; chat.question = null; chat.text = "Primero revisemos el documento. <img src=x onerror=window.injection=true>";
          snapshot[provider === "codex" ? "codex_conversation_ready" : "claude_conversation_ready"] = true;
          if (window.makeProposal) {
            window.makeProposal = false;
            snapshot.job = { snapshot: { schema_version: 2, project_id: snapshot.project[0], input: { schema_version: 1, job_id: "native-job", dimensions_mm: { length: 25.4, width: 10, height: 5 } }, phase: "awaiting_approval", audit: { created_unix_ms: 1000, source: { provider: "mcp" } } }, approval_id: "private-approval", sequence: 1, busy: false, error: null };
          }
          if (window.makeNativePlan) {
            window.makeNativePlan = false;
            snapshot.job = { snapshot: { schema_version: 10, project_id: snapshot.project[0], input: { schema_version: 3, job_id: "drawing-native-job", cadcraft_version: "0.5.0", source_name: "Plano.dxf", output_format: "dxf", units: "mm", circle: null, allow_loss: false, native_tools: { intent: "Mover la pieza <img src=x>", calls: [{ command: "move", params: { handles: ["100"], delta: [25.4, 0] } }] } }, phase: "awaiting_approval", audit: { created_unix_ms: 2000, source: { provider: "mcp" } } }, approval_id: "private-approval", sequence: 1, busy: false, error: null };
            snapshot.job.snapshot.input.native_source = [{ handle: "100", type: "Circle", geometry: { type: "circle", center: { x: 0, y: 0, z: 0 }, radius: 10 } }];
          }
          args.events.onmessage(structuredClone(chat));
        };
        window.staleChatEvent = window.chatEvent;
        window.chatEvent = view => args.events.onmessage(view);
        window.finishChat = finish;
        if (window.askQuestion) {
          window.askQuestion = false; chat.sequence++;
          chat.question = { id: "question-one", questions: [{ id: "unit", question: "¿Milímetros o pulgadas?", options: [{ label: "mm", description: "Milímetros" }, { label: "in", description: "Pulgadas" }], isOther: false, isSecret: false }, { id: "private", question: "Dato privado de prueba", options: null, isOther: true, isSecret: true }] };
        } else if (!window.holdChat) setTimeout(finish, 20);
        return copy();
      }
      if (command === "answer_codex_question") { assertAnswers(); window.finishChat(); return copy().chat; function assertAnswers() { if (args.answers.unit !== "in" || args.answers.private !== "fixture-private-answer") throw Error("Incorrect fixture answers"); } }
      if (command === "cancel_chat") { snapshot.chat.sequence++; snapshot.chat.cancelled = true; snapshot.chat.busy = false; snapshot.chat.question = null; return copy().chat; }
      if (command === "approve_operation") {
        if (args.approvalId !== "private-approval" || snapshot.job.snapshot.phase !== "awaiting_approval") throw Error("Approval replay");
        window.approvals++; const running = copy().job; running.sequence++; running.busy = true; running.snapshot.phase = "running";
        snapshot.job = structuredClone(running); snapshot.job.sequence++; snapshot.job.busy = false; snapshot.job.snapshot.phase = "completed"; snapshot.job.snapshot.audit.files_verified = true; snapshot.job.approval_id = null;
        args.events.onmessage(copy().job); return running;
      }
      if (command === "cancel_job") { snapshot.job.sequence++; snapshot.job.snapshot.phase = "cancelled"; snapshot.job.approval_id = null; return copy().job; }
      if (command === "open_artifact") return "Resultado <img src=x onerror=window.reportInjection=true>";
      throw Error(`Unexpected IPC: ${command}`);
    } };
  });
  await page.goto(server.resolvedUrls.local[0]);
  await page.locator("#send:enabled").waitFor();
  assert.equal(await page.locator("#connections").getAttribute("open"), null);
  assert.equal(await page.locator("#length, #pcb-x, #konnect-value, #sketch-source, #workflow").count(), 0, "Editing forms leaked into the new production entry");
  assert.match(await page.locator("#conversation").innerText(), /No necesitas conocer/);
  await page.locator("#setup").click();
  const send = async text => { await page.locator("#prompt").fill(text); await page.locator("#send").click(); await page.locator("#send:enabled").waitFor(); };
  for (const provider of ["codex", "claude"]) {
    await page.locator("#provider").selectOption(provider);
    for (const program of ["freecad", "drawing", "arduino", "agent", "kicad", "slice"]) {
      await page.locator("#program").selectOption(program);
      await send(`Quiero cambiar mi diseño con ${program}; todavía no conozco las medidas. Usa pulgadas cuando confirmemos su tamaño.`);
      const call = await page.evaluate(() => window.calls.filter(call => call.command.startsWith("start_")).at(-1));
      assert.equal(call.command, provider === "codex" ? "start_codex_chat" : "start_claude_chat");
      const prompt = JSON.parse(call.args.prompt);
      assert.equal(prompt.desktop_context.live_document_connection, false);
      assert.equal(prompt.desktop_context.document_scope, "saved_files_and_verified_copies");
      assert.equal(prompt.desktop_context.draft_fields, undefined, "Hidden form defaults were sent to the agent");
      assert.match(prompt.instruction, /no pidas rellenar formularios/);
      assert.match(prompt.instruction, /documento abierto/);
      assert.equal(await page.evaluate(() => window.approvals), 0);
    }
  }
  assert.equal(await page.locator("#conversation img").count(), 0);
  assert.equal(await page.evaluate(() => window.injection), undefined);
  for (const provider of ["codex", "claude"]) {
    await page.locator("#provider").selectOption(provider);
    await page.locator("#program").selectOption("slice");
    await page.evaluate(() => { window.foreignSlice = true; });
    await page.getByRole("button", { name: "Seleccionar modelo y perfiles…", exact: true }).click();
    assert.match(await page.locator("#error").innerText(), /otro proyecto/);
    await page.evaluate(() => { window.foreignSlice = false; });
    await page.getByRole("button", { name: "Seleccionar modelo y perfiles…", exact: true }).click();
    await send("Quiero preparar este modelo para imprimir, pero todavía no sé su escala. Inspecciona sus perfiles antes de preguntar lo que falte.");
    const call = await page.evaluate(() => window.calls.filter(call => call.command.startsWith("start_")).at(-1));
    assert.equal(call.command, provider === "codex" ? "start_codex_chat" : "start_claude_chat");
    const context = JSON.parse(call.args.prompt).desktop_context;
    assert.deepEqual(context.slice_source, { selection: "selected-slice", selection_sha256: "f".repeat(64), units_require_confirmation: true });
    for (const invented of ["units", "bed", "bed_temperature_c", "draft_fields"]) assert.equal(context[invented], undefined);
    assert.equal(await page.evaluate(() => window.approvals), 0);
    assert.match(await page.locator("#source-status").innerText(), /sin unidad declarada/);
  }
  await page.locator("#provider").selectOption("codex");
  await page.locator("#program").selectOption("kicad");
  await page.evaluate(() => { window.foreignSource = true; });
  await page.getByRole("button", { name: "Seleccionar PCB…", exact: true }).click();
  assert.match(await page.locator("#error").innerText(), /otro proyecto/);
  await page.evaluate(() => { window.foreignSource = false; });
  await page.getByRole("button", { name: "Seleccionar PCB…", exact: true }).click();
  await send("Aleja la resistencia del conector; primero inspecciona cuál es y pregunta lo necesario.");
  const board = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt));
  assert.equal(board.desktop_context.pcb_source.selection, "selected-board");
  assert.equal(board.desktop_context.schematic_source, undefined);
  assert(!JSON.stringify(board).includes("private-approval"));
  await page.evaluate(() => { window.makeProposal = true; });
  await send("Prepara un cambio que pueda revisar.");
  await page.locator("#approval:visible").waitFor();
  assert.match(await page.locator("#change-summary").innerText(), /25.4 × 10 × 5 mm/);
  assert.equal(await page.evaluate(() => window.approvals), 0);
  await page.locator("#approve").click();
  await page.waitForFunction(() => document.querySelector("#job-status").textContent.includes("verificado"));
  assert.equal(await page.evaluate(() => window.approvals), 1);
  assert.equal(await page.locator("#approval").isVisible(), false, "A stale running reply replaced the completed event");
  await send("Ahora consulta el resultado antes de proponer otro cambio.");
  const followup = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt));
  assert.deepEqual(followup.desktop_context.current_job, { id: "native-job", phase: "completed" });
  await page.locator("#program").selectOption("slice");
  await page.getByRole("button", { name: "Seleccionar modelo y perfiles…", exact: true }).click();
  await send("Este es otro modelo; inspecciónalo antes de preparar su primer laminado.");
  const initialSlice = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt));
  assert.equal(initialSlice.desktop_context.current_job, null, "An old job was treated as the selected STL's result");
  assert.equal(initialSlice.desktop_context.slice_source.units_require_confirmation, true);
  await page.locator("#results > summary").click();
  await page.getByRole("button", { name: "Informe", exact: true }).click();
  assert.match(await page.locator("#report-text").innerText(), /<img/);
  assert.equal(await page.locator("#report img").count(), 0);
  await page.locator("#report button").click();
  await page.evaluate(() => { window.askQuestion = true; });
  await page.locator("#prompt").fill("Tengo una respuesta para la aclaración."); await page.locator("#send").click();
  await page.locator("#question-form:visible").waitFor();
  await page.locator('[name="unit"]').selectOption("in"); await page.locator('[name="private"]').fill("fixture-private-answer");
  await page.waitForTimeout(1100);
  assert.equal(await page.locator('[name="unit"]').inputValue(), "in");
  await page.locator("#answer-question").click(); await page.locator("#send:enabled").waitFor();
  assert(!await page.locator("#conversation").innerText().then(text => text.includes("fixture-private-answer")));
  assert(!await page.evaluate(() => Object.values(localStorage).join(" ").includes("fixture-private-answer")));
  const messages = await page.locator("#conversation .message").allTextContents();
  assert(messages.at(-2).includes("Respuesta privada enviada."));
  assert(messages.at(-1).includes("Primero revisemos"));
  await page.locator("#program").selectOption("kicad");
  await page.locator("#kicad-endpoint").fill("ipc://C:/owned/kicad/api.sock");
  await page.locator("#kicad-api-token").fill("fixture-api-secret");
  await page.getByRole("button", { name: "Leer PCB abierta en KiCad…", exact: true }).click();
  await page.locator("#send:enabled").waitFor();
  assert.equal(await page.locator("#kicad-api-token").inputValue(), "");
  await send("Lee la posición actual del conector sin guardar ni modificar la PCB.");
  const nativeContext = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt));
  assert.equal(nativeContext.desktop_context.document_scope, "live_pcb_read_only");
  assert.equal(nativeContext.desktop_context.live_document_connection, true);
  assert.equal(nativeContext.desktop_context.live_mutations_allowed, false);
  assert.equal(nativeContext.desktop_context.live_pcb_source.selection, "live-board");
  assert.equal(nativeContext.desktop_context.pcb_source, undefined);
  assert.equal(nativeContext.desktop_context.current_job, null);
  assert(!JSON.stringify(nativeContext).includes("fixture-api-secret"));
  assert(!await page.evaluate(() => Object.values(localStorage).join(" ").includes("fixture-api-secret")));
  assert.match(await page.locator("#document-scope").innerText(), /No permite modificar/);
  await page.getByRole("button", { name: "Seleccionar PCB…", exact: true }).click();
  await send("Ahora consulta la copia guardada que seleccioné expresamente.");
  const savedContext = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt));
  assert.equal(savedContext.desktop_context.live_document_connection, false);
  assert.equal(savedContext.desktop_context.live_pcb_source, undefined);
  assert.equal(savedContext.desktop_context.pcb_source.selection, "selected-board");
  await page.locator("#program").selectOption("drawing");
  await page.getByRole("button", { name: "Seleccionar dibujo…", exact: true }).click();
  await page.evaluate(() => { window.makeNativePlan = true; });
  await send("Mueve la pieza una pulgada usando las herramientas del programa.");
  await page.locator("#approval:visible").waitFor();
  assert.match(await page.locator("#change-summary").innerText(), /Mover la pieza <img src=x>/);
  assert.equal(await page.locator("#change-summary img").count(), 0);
  assert.match(await page.locator("#change-details").textContent(), /"command": "move"/);
  assert.match(await page.locator("#change-details").textContent(), /"current_entities"/);
  assert.equal(await page.locator("#approval input, #approval textarea").count(), 0);
  assert.equal(await page.evaluate(() => window.approvals), 1, "A native tool plan was automatically approved");
  await page.locator("#approve").click();
  await page.locator("#approval").waitFor({ state: "hidden" });
  assert.equal(await page.evaluate(() => window.approvals), 2);
  await page.evaluate(() => { window.holdChat = true; });
  await page.locator("#prompt").fill("Una consulta que voy a detener."); await page.locator("#send").click();
  await page.locator("#stop:enabled").waitFor(); await page.locator("#stop").click(); await page.locator("#send:enabled").waitFor();
  assert.match(await page.locator("#conversation").innerText(), /Conversación detenida/);
  await page.evaluate(() => { window.holdChat = false; });
  await send("Este mensaje debe conservarse entre programas.");
  await page.evaluate(() => window.staleChatEvent({ provider: "codex", id: "stale", project_id: "project-one", sequence: 999, busy: false, cancelled: false, text: "STALE SHOULD NOT APPEAR", error: null }));
  assert(!await page.locator("#conversation").innerText().then(text => text.includes("STALE SHOULD NOT APPEAR")));
  await page.locator("#program").selectOption("freecad");
  assert.match(await page.locator("#conversation").innerText(), /conservarse entre programas/);
  await page.reload(); await page.locator("#send:enabled").waitFor();
  assert.match(await page.locator("#conversation").innerText(), /conservarse entre programas/);
  await page.locator("#setup").click(); await page.evaluate(() => { window.otherFolder = true; });
  await page.locator("#open-project").click(); await page.locator("#send:enabled").waitFor();
  assert(!await page.locator("#conversation").innerText().then(text => text.includes("conservarse entre programas")), "A copied project ID mixed histories from different folders");
  assert(!await page.locator("#conversation").innerText().then(text => text.includes("PREVIOUS FOLDER RESPONSE")), "A chat view without folder identity crossed project folders");
  await send("Conversación del otro proyecto.");
  await page.locator("#new-chat").click();
  assert(!await page.locator("#conversation").innerText().then(text => text.includes("Conversación del otro proyecto")));
  const evidence = path.resolve(root, "../../target/chat-first"); await mkdir(evidence, { recursive: true });
  await page.locator("#connections > summary").click();
  await page.screenshot({ path: path.join(evidence, "chat.png"), fullPage: true });
  assert.deepEqual(errors, []);
  console.log("PASS: production chat entry, both agent routes across all six program groups, selected document context, no form defaults or automatic approvals, completed-event ordering, verified-result access, question privacy, cancellation, stale-event refusal and scoped history; IPC fixtures only");
} finally { await browser?.close(); await server.close(); }
