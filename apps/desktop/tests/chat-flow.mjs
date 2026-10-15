import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright-core";
import {checkInitialChat} from "./initial-chat-check.mjs";

const root = path.resolve(import.meta.dirname, "..");
const mainSource = await readFile(path.join(root, "src-tauri/src/main.rs"), "utf8");
const buildSource = await readFile(path.join(root, "src-tauri/build.rs"), "utf8");
const capability = JSON.parse(await readFile(path.join(root, "src-tauri/capabilities/main.json"), "utf8"));
const handlers = mainSource.match(/generate_handler!\[([^\]]+)\]/)[1].split(",").map(name => name.trim().split("::").at(-1)).sort();
const declared = [...buildSource.match(/\.commands\(&\[([^\]]+)\]/)[1].matchAll(/"([a-z_]+)"/g)].map(match => match[1]).sort();
assert.deepEqual(declared, handlers, "Desktop permission declarations must cover the actual command handler");
assert.deepEqual([...capability.permissions].sort(), declared.map(command => `allow-${command.replaceAll("_", "-")}`).sort(), "Bundled chat permissions must match the declared app commands");
assert.deepEqual(capability.webviews, ["main"]);
assert.equal(capability.remote, undefined, "App commands must not be granted to remote pages");
const server = await createServer({ root, server: { host: "127.0.0.1", port: 0 } });
let browser;
try {
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.HAZLOMEXA_TEST_BROWSER ?? "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", headless: true });
  const initialPage = await browser.newPage();
  await initialPage.addInitScript(() => {
    window.initialCalls = [];
    const snapshot = {project:null,job:null,inference:null,recovered:[],freecad:"",freecad_available:false};
    window.__TAURI_INTERNALS__ = {transformCallback:()=>1,unregisterCallback:()=>{},invoke:async command=>{
      window.initialCalls.push(command);
      if(command!=="get_snapshot") throw Error("Initial chat dispatched an unexpected command");
      return structuredClone(snapshot);
    }};
  });
  for (let reload = 0; reload < 2; reload++) {
    if (reload) await initialPage.reload(); else await initialPage.goto(server.resolvedUrls.local[0]);
    await initialPage.locator("#open-project:enabled").waitFor({state:"attached"});
    await checkInitialChat(initialPage,await initialPage.evaluate(()=>window.__TAURI_INTERNALS__.invoke("get_snapshot")));
    assert((await initialPage.evaluate(()=>window.initialCalls)).every(command=>command==="get_snapshot"));
  }
  await initialPage.close();
  const page = await browser.newPage({ viewport: { width: 1200, height: 1000 } });
  const errors = []; page.on("pageerror", error => errors.push(error.message));
  await page.addInitScript(() => {
    let callback = 0;
    const snapshot = { project: ["project-one", "C:\\owned\\project"], job: null, inference: null, recovered: [], freecad: "owned-freecad", freecad_available: true, codex: { version: "fixture" }, codex_login: { id: "login", sequence: 1, busy: false, cancelled: false, connected: true, auth_url: null, error: null }, claude: { version: "fixture" } };
    const copy = () => structuredClone(snapshot);
    window.setCompletedJob = input => {
      snapshot.job = { snapshot: { schema_version: input.live_cad_version ? 13 : 12, project_id: snapshot.project[0], input, phase: "completed", audit: { created_unix_ms: 3000, files_verified: true, source: { provider: "mcp" } } }, approval_id: null, sequence: 1, busy: false, error: null };
    };
    window.setPendingJob = input => { window.setCompletedJob(input); snapshot.job.snapshot.phase = "awaiting_approval"; snapshot.job.approval_id = "private-approval"; };
    window.calls = []; window.approvals = 0;
    window.__TAURI_INTERNALS__ = { transformCallback: () => ++callback, unregisterCallback: () => {}, invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (command === "get_snapshot") return copy();
      if (["discover_cad_document","discover_drawing","discover_slice","discover_live_pcb"].includes(command)) {
        if(window.rejectInspectionPicker) throw "Selección cancelada.";
        window.previousInspectionEvent = window.inspectionEvent;
        window.inspectionEvent = id => args.events.onmessage(id);
        window.inspectionId = crypto.randomUUID(); args.events.onmessage(window.inspectionId);
        if(window.holdInspection) await new Promise(resolve=>{window.finishInspection=resolve;});
      }
      if(command === "cancel_source_inspection") {
        if(args.id!==window.inspectionId) throw Error("Stale inspection cancellation");
        window.finishInspection();
        await new Promise(resolve=>setTimeout(resolve,20));
        return window.cancelInspectionAccepted ?? true;
      }
      if (command === "select_codex" || command === "select_claude" || command === "select_freecad" || command === "select_cadcraft" || command === "select_arduino" || command === "connect_agent" || command === "select_kicad" || command === "select_konnect" || command === "select_orca") return copy();
      if (command === "discover_schematic") return { id: "selected-sheet", project_id: snapshot.project[0], name: "Circuito.kicad_sch", sha256: "a".repeat(64), components: [] };
      if (command === "discover_pcb") return { id: "selected-board", project_id: window.foreignSource ? "foreign-project" : snapshot.project[0], name: "Placa.kicad_pcb", sha256: "b".repeat(64), components: [] };
      if (command === "discover_live_pcb") { if (args.connection.endpoint !== "ipc://C:/owned/kicad/api.sock" || args.connection.token !== "fixture-api-secret") throw Error("Incorrect native connection fixture"); return { id: window.holdInspection ? "late-live-board" : "live-board", project_id: snapshot.project[0], name: "Placa.kicad_pcb", observation: { document_sha256: "2".repeat(64), components_sha256: "d".repeat(64), components: [] } }; }
      if (command === "discover_drawing") return { id: window.holdInspection ? "late-selected-drawing" : "selected-drawing", project_id: snapshot.project[0], inspection: { source_name: "Plano.dxf", source_sha256: "c".repeat(64) } };
      if (command === "discover_cad_document") return { id: window.holdInspection ? "late-selected-cad-document" : "selected-cad-document", project_id: window.foreignCad ? "foreign-project" : snapshot.project[0], inspection: { source_name: "Pieza.FCStd", source_sha256: "1".repeat(64), preview: {catalog:{Target:"Part::Box"},measurements:{Target:{dimensions_mm:{length:10,width:20,height:30}}}} } };
      if (command === "discover_slice") return { id: window.holdInspection ? "late-selected-slice" : "selected-slice", project_id: window.foreignSlice ? "foreign-project" : snapshot.project[0], preview: { source_name: "Pieza.stl", source_sha256: "e".repeat(64), selection_sha256: "f".repeat(64), triangles: 12, bounds_file_coordinates: [[0,0,0],[10,10,10]], declared_bed: null, bed_profiles: [] } };
      if (command === "select_agent_sketch") { snapshot.agent_source = { project_id: snapshot.project[0], source_name: "Sketch.ino", source: "void setup() {}" }; return copy(); }
      if (command === "select_project") { snapshot.project = window.otherFolder ? ["project-one", "C:\\owned\\other-folder"] : ["project-one", "C:\\owned\\project"]; snapshot.job = null; if (snapshot.chat) snapshot.chat.text = "PREVIOUS FOLDER RESPONSE"; return copy(); }
      if (command === "start_codex_chat" || command === "start_claude_chat" || command === "start_ollama_chat") {
        const provider = command === "start_codex_chat" ? "codex" : command === "start_claude_chat" ? "claude" : args.profile === "cloud" ? "ollama_cloud" : "ollama_local";
        snapshot.chat = { provider, id: crypto.randomUUID(), project_id: snapshot.project[0], sequence: 1, busy: true, cancelled: false, text: "", error: null };
        const chat = snapshot.chat;
        const finish = () => {
          if (snapshot.chat?.id !== chat.id || chat.cancelled) return;
          chat.sequence++; chat.busy = false; chat.question = null; chat.text = window.replyText ?? "Primero revisemos el documento. <img src=x onerror=window.injection=true>";
          if (provider === "codex" || provider === "claude") snapshot[provider === "codex" ? "codex_conversation_ready" : "claude_conversation_ready"] = true;
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
        if (window.rejectApproval) throw Error("Fixture approval failed");
        window.approvals++; const running = copy().job; running.sequence++; running.busy = true; running.snapshot.phase = "running";
        snapshot.job = structuredClone(running);
        window.finishJob = () => { snapshot.job.sequence++; snapshot.job.busy = false; snapshot.job.snapshot.phase = window.jobOutcome ?? "completed"; snapshot.job.snapshot.audit.files_verified = snapshot.job.snapshot.phase === "completed"; snapshot.job.approval_id = null; args.events.onmessage(copy().job); };
        if (!window.delayJob) window.finishJob();
        return running;
      }
      if (command === "cancel_job") { snapshot.job.sequence++; snapshot.job.busy = false; snapshot.job.snapshot.phase = "cancelled"; snapshot.job.approval_id = null; return copy().job; }
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
  for (const provider of ["codex", "claude", "ollama_local", "ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    if (provider.startsWith("ollama_")) {
      assert.equal(await page.locator("#select-client").isVisible(), false);
      assert.equal(await page.locator("#connect-account").isVisible(), false);
      assert.match(await page.locator("#login-status").innerText(), /No se descargan modelos/);
    }
    for (const program of ["freecad", "drawing", "arduino", "agent", "kicad", "slice"]) {
      await page.locator("#program").selectOption(program);
      await send(`Quiero cambiar mi diseño con ${program}; todavía no conozco las medidas. Usa pulgadas cuando confirmemos su tamaño.`);
      const call = await page.evaluate(() => window.calls.filter(call => call.command.startsWith("start_")).at(-1));
      assert.equal(call.command, provider === "codex" ? "start_codex_chat" : provider === "claude" ? "start_claude_chat" : "start_ollama_chat");
      assert.equal(call.args.program,program,"Selected program must bind the backend tool connection");
      if (provider.startsWith("ollama_")) assert.equal(call.args.profile, provider === "ollama_local" ? "local_experimental" : "cloud");
      const prompt = JSON.parse(call.args.prompt);
      assert.equal(prompt.desktop_context.live_document_connection, program === "freecad" ? null : false);
      assert.equal(prompt.desktop_context.live_mutations_allowed, program === "freecad");
      if (program === "freecad") assert.equal(prompt.desktop_context.native_connection_check, "inspect_open_cad_documents");
      assert.equal(prompt.desktop_context.document_scope, program === "freecad" ? "open_freecad_and_explicit_saved_copies" : "saved_files_and_verified_copies");
      assert.equal(prompt.desktop_context.draft_fields, undefined, "Hidden form defaults were sent to the agent");
      assert.match(prompt.instruction, /no pidas rellenar formularios/);
      assert.match(prompt.instruction, /documento abierto/);
      assert.equal(await page.evaluate(() => window.approvals), 0);
    }
  }
  assert.equal(await page.locator("#conversation img").count(), 0);
  assert.equal(await page.evaluate(() => window.injection), undefined);
  for (const provider of ["ollama_local", "ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    await page.locator("#new-chat").click();
    await send("Quiero recrear una pieza; todavía no sé sus dimensiones.");
    await send("La usaré como soporte. Pregunta qué información necesitas.");
    const call = await page.evaluate(() => window.calls.filter(call => call.command === "start_ollama_chat").at(-1));
    const context = JSON.parse(call.args.prompt).recovered_conversation;
    assert.equal(call.args.newSession, true, "Ollama must refresh visible conversation and document context on each turn");
    assert(context.some(turn => turn.role === "user" && turn.content.includes("recrear una pieza")), "Ollama lost the earlier user intent");
    assert(context.some(turn => turn.role === "assistant" && turn.content.includes("revisemos el documento")), "Ollama lost the clarification context");
    assert(!await page.evaluate(() => Object.values(localStorage).join(" ").includes("private-approval")));
    await page.evaluate(() => { window.holdChat = true; });
    await page.locator("#prompt").fill("Voy a detener esta consulta de Ollama."); await page.locator("#send").click();
    await page.locator("#stop:enabled").waitFor(); await page.locator("#stop").click(); await page.locator("#send:enabled").waitFor();
    assert.match(await page.locator("#conversation").innerText(), /Conversación detenida/);
    await page.evaluate(() => { window.holdChat = false; });
  }
  for (const provider of ["codex", "claude", "ollama_local", "ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    await page.locator("#program").selectOption("slice");
    await page.evaluate(() => { window.foreignSlice = true; });
    await page.getByRole("button", { name: "Seleccionar modelo y perfiles…", exact: true }).click();
    assert.match(await page.locator("#error").innerText(), /otro proyecto/);
    await page.evaluate(() => { window.foreignSlice = false; });
    await page.getByRole("button", { name: "Seleccionar modelo y perfiles…", exact: true }).click();
    await send("Quiero preparar este modelo para imprimir, pero todavía no sé su escala. Inspecciona sus perfiles antes de preguntar lo que falte.");
    const call = await page.evaluate(() => window.calls.filter(call => call.command.startsWith("start_")).at(-1));
    assert.equal(call.command, provider === "codex" ? "start_codex_chat" : provider === "claude" ? "start_claude_chat" : "start_ollama_chat");
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
  await page.locator("#program").selectOption("freecad");
  await page.getByRole("button",{name:"Seleccionar documento guardado e inspeccionar…",exact:true}).click();
  assert.equal(await page.locator("#approval").isVisible(),false,"Selecting FreeCAD created an inspection proposal");
  await send("Todavía no conozco sus medidas; revisa lo que hay y pregunta para qué lo necesito.");
  const cadSource = await page.evaluate(()=>JSON.parse(window.calls.filter(call=>call.command==="start_codex_chat").at(-1).args.prompt).desktop_context);
  assert.deepEqual(cadSource.cad_document_source,{selection:"selected-cad-document",source_sha256:"1".repeat(64)});
  assert.equal(cadSource.current_job,null);
  assert.equal(cadSource.live_document_connection,null);
  assert.equal(cadSource.live_mutations_allowed,true);
  assert.equal(await page.evaluate(()=>window.calls.filter(call=>call.command==="prepare_cadpilot").length),0);
  const sourcePick = await page.evaluate(()=>window.calls.filter(call=>call.command==="discover_cad_document").at(-1));
  assert.deepEqual(Object.keys(sourcePick.args),["events"],"Source selection invented user measurements or units");
  await page.evaluate(()=>{window.foreignCad=true;});
  await page.getByRole("button",{name:"Seleccionar documento guardado e inspeccionar…",exact:true}).click();
  await page.locator("#error:visible").waitFor();
  assert.match(await page.locator("#error").innerText(),/otro proyecto/);
  assert.equal(await page.locator("#approval").isVisible(),false);
  await page.evaluate(()=>{window.foreignCad=false;});
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
  for (const program of ["drawing", "arduino", "agent", "kicad", "slice"]) {
    await page.locator("#program").selectOption(program);
    await send("Inspecciona el documento de este programa antes de modificarlo.");
    const context = await page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat").at(-1).args.prompt).desktop_context);
    assert.equal(context.current_job, null, `A FreeCAD result crossed into ${program}`);
  }
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
  assert.equal(nativeContext.desktop_context.live_pcb_source.document_sha256, "2".repeat(64));
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
  assert.match(await page.locator("#change-facts").innerText(), /Unidades del dibujo: milímetros/);
  assert.match(await page.locator("#change-facts").innerText(), /1\. Mover elementos/);
  assert.equal(await page.locator("#change-facts img").count(), 0);
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
  const cases = [
    ["freecad", { dimensions_mm: { length: 10, width: 10, height: 10 } }, "Seleccionar documento guardado e inspeccionar…"],
    ["freecad", { cadpilot_version: "fixture", source_name: "Part.FCStd", request: { operation: "inspect", object: "Target", units: "mm", parameters: {} }, preview: { measurements: {} } }, "Seleccionar documento guardado e inspeccionar…"],
    ["drawing", { cadcraft_version: "fixture", source_name: "Plano.dxf", units: "in", output_format: "dxf" }, "Seleccionar dibujo…"],
    ["arduino", { fqbn: "arduino:avr:uno", source: "void setup() {}" }],
    ["agent", { agent_version: "fixture", source_name: "Sketch.ino", original_source: "void setup() {}", source: "void setup() {}" }, "Seleccionar sketch…"],
    ["kicad", { kicad_version: "10.0.7", entry: "Board.kicad_pcb", units: "mm", files: [] }, "Seleccionar PCB…"],
    ["kicad", { konnect_version: "fixture", source: {entry:"Circuit.kicad_sch"}, edit: { reference: "R1", expected_value: "1k", value: "2k" } }, "Seleccionar esquema…"],
    ["kicad", { konnect_version: "fixture", source: {entry:"Board.kicad_pcb"}, pcb_move: { reference: "R1", expected_x_mm: 10, expected_y_mm: 10, x_mm: 20, y_mm: 10 } }, "Seleccionar PCB…"],
    ["slice", { orca_version: "fixture", source_name: "Part.stl", units: "mm", mesh: { bounds_mm: [[0,0,0],[10,10,10]] }, settings: { machine: "Printer", filament: "PLA", process:"0.20 mm", bed:"High Temp Plate", nozzle_mm:0.4, layer_mm:0.2, nozzle_c:210, bed_c:60 } }, "Seleccionar modelo y perfiles…"],
  ];
  for (const provider of ["codex", "claude"]) {
    await page.locator("#provider").selectOption(provider);
    for (const [index, [program, fields, picker]] of cases.entries()) {
      await page.locator("#new-chat").click();
      const job = { schema_version: 1, job_id: `${provider}-${index}`, ...fields };
      await page.evaluate(input => window.setCompletedJob(input), job);
      await page.locator("#select-client").click(); await page.locator("#send:enabled").waitFor();
      await page.locator("#program").selectOption(program);
      await send("Lee el resultado verificado de este programa.");
      const lastContext = () => page.evaluate(() => JSON.parse(window.calls.filter(call => call.command === "start_codex_chat" || call.command === "start_claude_chat").at(-1).args.prompt).desktop_context);
      assert.deepEqual((await lastContext()).current_job, { id: job.job_id, phase: "completed" });
      if(program==="arduino") {
        if(await page.locator("#results").getAttribute("open")===null) await page.locator("#results > summary").click();
        const before = await page.evaluate(()=>window.calls.filter(call=>call.command.startsWith("start_")).length);
        await page.getByRole("button",{name:"Informe de compilación",exact:true}).click();
        await page.locator("#report:visible").waitFor();
        assert.match(await page.locator("#report-text").innerText(),/Resultado <img/);
        assert.equal(await page.locator("#report img").count(),0);
        const call = await page.evaluate(()=>window.calls.filter(call=>call.command==="open_artifact").at(-1));
        assert.deepEqual(call.args,{jobId:job.job_id,format:"compile_report"});
        assert.equal(await page.evaluate(()=>window.calls.filter(call=>call.command.startsWith("start_")).length),before);
        await page.locator("#report button").click();
      }
      await page.locator("#program").selectOption(program === "freecad" ? "drawing" : "freecad");
      await send("Consulta este otro programa.");
      assert.equal((await lastContext()).current_job, null, `${provider}: unrelated ${program} job leaked`);
      await page.locator("#program").selectOption(program);
      if (picker) {
        await page.getByRole("button", { name: picker, exact: true }).click();
        await send("Este es otro documento; empieza inspeccionándolo.");
        assert.equal((await lastContext()).current_job, null, `${provider}: replaced ${program} source retained old result`);
        await page.evaluate(input => window.setCompletedJob(input), { ...job, job_id: `${job.job_id}-new` });
        await page.locator("#select-client").click(); await page.locator("#send:enabled").waitFor();
        await send("Consulta el nuevo resultado verificado.");
        assert.deepEqual((await lastContext()).current_job, { id: `${job.job_id}-new`, phase: "completed" });
      }
    }
  }
  const chatCalls = () => page.evaluate(() => window.calls.filter(call => ["start_codex_chat","start_claude_chat","start_ollama_chat"].includes(call.command)));
  for(const [program,picker,field,selection] of [
    ["freecad","Seleccionar documento guardado e inspeccionar…","cad_document_source","selected-cad-document"],
    ["drawing","Seleccionar dibujo…","drawing_source","selected-drawing"],
    ["slice","Seleccionar modelo y perfiles…","slice_source","selected-slice"],
    ["kicad","Leer PCB abierta en KiCad…","pcb_source","selected-board"],
  ]) {
    await page.locator("#program").selectOption(program);
    if(program==="kicad") {
      await page.getByRole("button",{name:"Seleccionar PCB…",exact:true}).click();
      await page.locator("#kicad-endpoint").fill("ipc://C:/owned/kicad/api.sock");
      await page.locator("#kicad-api-token").fill("fixture-api-secret");
    } else await page.getByRole("button",{name:picker,exact:true}).click();
    await page.locator("#send:enabled").waitFor();
    const statusBefore = await page.locator("#source-status").innerText();
    const before = (await chatCalls()).length;
    const cancels = await page.evaluate(()=>window.calls.filter(call=>call.command==="cancel_source_inspection").length);
    const approvals = await page.evaluate(()=>window.approvals);
    await page.evaluate(()=>{window.holdInspection=true;window.cancelInspectionAccepted=true;});
    await page.getByRole("button",{name:picker,exact:true}).click(); await page.locator("#stop:enabled").waitFor();
    assert.match(await page.locator("#stop").innerText(),/Detener inspección/);
    assert.equal(await page.locator("#provider").isEnabled(),false);
    await page.evaluate(()=>window.previousInspectionEvent?.("stale-inspection-id"));
    await page.locator("#stop").click(); await page.locator("#send:enabled").waitFor();
    assert.equal((await chatCalls()).length,before,"Inspecting/cancelling dispatched a model turn");
    assert.equal(await page.evaluate(()=>window.approvals),approvals);
    assert.equal(await page.evaluate(()=>window.calls.filter(call=>call.command==="cancel_source_inspection").length),cancels+1);
    assert.equal(await page.locator("#source-status").innerText(),statusBefore);
    assert.match(await page.locator("#chat-progress").innerText(),/Se conserva la selección anterior/);
    assert.equal(await page.locator("#error").isVisible(),false);
    await page.evaluate(()=>{window.holdInspection=false;});
    await send("Consulta la selección anterior; la nueva inspección fue cancelada.");
    const context = JSON.parse((await chatCalls()).at(-1).args.prompt).desktop_context;
    assert.equal(context[field].selection,selection);
    assert(!JSON.stringify(context).includes("late-"));
    assert(!JSON.stringify(context).includes("fixture-api-secret"));
    if(program==="kicad") assert.equal(context.live_pcb_source,undefined);
  }
  await page.locator("#program").selectOption("drawing");
  await page.evaluate(()=>{window.holdInspection=true;window.cancelInspectionAccepted=false;});
  await page.getByRole("button",{name:"Seleccionar dibujo…",exact:true}).click(); await page.locator("#stop:enabled").waitFor();
  await page.locator("#stop").click(); await page.locator("#send:enabled").waitFor();
  assert.match(await page.locator("#chat-progress").innerText(),/Documento inspeccionado/);
  await page.evaluate(()=>{window.holdInspection=false;});
  await send("Consulta el dibujo cuya inspección terminó antes de la solicitud de detener.");
  assert.equal(JSON.parse((await chatCalls()).at(-1).args.prompt).desktop_context.drawing_source.selection,"late-selected-drawing");
  const beforePickerCancel = await page.evaluate(()=>window.calls.filter(call=>call.command==="cancel_source_inspection").length);
  await page.evaluate(()=>{window.rejectInspectionPicker=true;});
  await page.getByRole("button",{name:"Seleccionar dibujo…",exact:true}).click(); await page.locator("#send:enabled").waitFor();
  await page.evaluate(()=>{window.rejectInspectionPicker=false;});
  assert.equal(await page.locator("#error").isVisible(),false);
  assert.equal(await page.locator("#stop").isEnabled(),false);
  assert.equal(await page.evaluate(()=>window.calls.filter(call=>call.command==="cancel_source_inspection").length),beforePickerCancel);
  for (const provider of ["codex","claude","ollama_local","ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    for (const [index,[program,fields]] of cases.entries()) {
      await page.locator("#new-chat").click(); await page.locator("#program").selectOption(program);
      const jobId = `review-${provider}-${index}`;
      await page.evaluate(input => window.setPendingJob(input),{schema_version:1,job_id:jobId,...fields});
      await page.locator("#program-actions button").first().click();
      await page.locator("#approve:enabled").waitFor();
      assert.match(await page.locator("#change-facts").innerText(),/El resultado se guarda en el proyecto/);
      assert(!/undefined|NaN/.test(await page.locator("#change-facts").innerText()));
      if(program==="slice") assert.match(await page.locator("#change-facts").innerText(),/Cama seleccionada: High Temp Plate/);
      assert.match(await page.locator("#approve").innerText(),/pedir revisión/);
      const before = (await chatCalls()).length;
      await page.locator("#prompt").fill("Mi siguiente pregunta todavía es un borrador.");
      await page.locator("#approve").click(); await page.locator("#send:enabled").waitFor();
      assert.equal(await page.locator("#prompt").inputValue(),"Mi siguiente pregunta todavía es un borrador.");
      assert(!await page.locator("#conversation").innerText().then(text=>text.includes("read_verified_result") || text.includes(jobId)),"Internal result review appeared as a user message");
      const calls = await chatCalls(); assert.equal(calls.length,before+1,"Approval must request exactly one result review");
      const prompt = JSON.parse(calls.at(-1).args.prompt);
      assert.equal(calls.at(-1).args.program,program,"Automatic result review changed its tool scope");
      assert.deepEqual(prompt.desktop_context.current_job,{id:jobId,phase:"completed"});
      assert.match(prompt.user_request,/Lee read_verified_result/); assert(prompt.user_request.includes(jobId));
      assert.match(prompt.user_request,/No prepares ni ejecutes otro cambio/);
      assert(!JSON.stringify(prompt).includes("private-approval"));
      const approvals = await page.evaluate(()=>window.approvals);
      await page.evaluate(()=>window.finishJob());
      assert.equal((await chatCalls()).length,before+1,"Duplicate completion replayed the review");
      assert.equal(await page.evaluate(()=>window.approvals),approvals,"Review automatically approved another operation");
    }
  }
  await page.locator("#program").selectOption("freecad");
  for (const outcome of ["completed","failed","cancelled","rejected","cancel-running"]) {
    await page.locator("#new-chat").click();
    await page.evaluate(outcome=>{window.delayJob=true;window.rejectApproval=outcome==="rejected";window.jobOutcome=outcome;window.setPendingJob({schema_version:1,job_id:`late-${outcome}`,dimensions_mm:{length:10,width:10,height:10}});},outcome);
    await page.locator("#program-actions button").first().click(); await page.locator("#approve:enabled").waitFor();
    const before = (await chatCalls()).length;
    await page.locator("#approve").click();
    if(outcome==="rejected") { await page.locator("#error:visible").waitFor(); await page.locator("#cancel-job").click(); }
    else {
      await page.locator("#cancel-running:enabled").waitFor();
      assert.equal((await chatCalls()).length,before,"Review started before the native operation finished");
      if(outcome==="cancel-running") await page.locator("#cancel-running").click();
      else await page.evaluate(()=>window.finishJob());
    }
    await page.locator("#send:enabled").waitFor();
    assert.equal((await chatCalls()).length,before+(outcome==="completed"?1:0),`Incorrect review for ${outcome}`);
  }
  await page.evaluate(()=>{window.delayJob=false;window.rejectApproval=false;delete window.jobOutcome;});
  await page.locator("#new-chat").click();
  await page.evaluate(()=>{window.holdChat=true;window.setPendingJob({schema_version:1,job_id:"cancel-result-review",dimensions_mm:{length:10,width:10,height:10}});});
  await page.locator("#program-actions button").first().click(); await page.locator("#approve:enabled").waitFor();
  const beforeReviewCancel = (await chatCalls()).length;
  await page.locator("#approve").click(); await page.locator("#stop:enabled").waitFor();
  await page.locator("#stop").click(); await page.locator("#send:enabled").waitFor();
  await page.evaluate(()=>window.finishJob());
  assert.equal((await chatCalls()).length,beforeReviewCancel+1,"Cancelled result review was retried");
  assert.match(await page.locator("#conversation").innerText(),/Conversación detenida/);
  await page.evaluate(()=>{window.holdChat=false;});
  const beforeRecovered = (await chatCalls()).length;
  await page.locator("#program-actions button").first().click(); await page.locator("#send:enabled").waitFor();
  assert.equal((await chatCalls()).length,beforeRecovered,"An existing completed result started a new review");
  await page.locator("#provider").selectOption("ollama_cloud");
  await page.locator("#new-chat").click();
  await page.evaluate(()=>{window.replyText="Respuesta extensa conservada: "+"La copia usa milímetros; lee el resultado antes de continuar. ".repeat(70);});
  await send("Primera intención que debe seguir guardada.");
  await send("Segunda intención, con datos confirmados del documento.");
  const longBefore=await page.locator("#conversation .message").allTextContents();
  await page.evaluate(()=>window.setPendingJob({schema_version:1,job_id:"long-history-review",dimensions_mm:{length:25.4,width:10,height:5}}));
  await page.locator("#program-actions button").first().click(); await page.locator("#approve:enabled").waitFor();
  const beforeLongReview=(await chatCalls()).length;
  await page.locator("#approve").click(); await page.locator("#send:enabled").waitFor();
  const longCalls=await chatCalls(); assert.equal(longCalls.length,beforeLongReview+1);
  const longPrompt=JSON.parse(longCalls.at(-1).args.prompt);
  assert(Buffer.byteLength(longCalls.at(-1).args.prompt)<=8192);
  assert(longPrompt.omitted_conversation_turns>0);
  assert.deepEqual(longPrompt.desktop_context.current_job,{id:"long-history-review",phase:"completed"});
  assert.match(longPrompt.user_request,/Lee read_verified_result/);
  assert(!JSON.stringify(longPrompt).includes("private-approval"));
  assert.match(await page.locator("#context-status").innerText(),/parte del historial/);
  assert.deepEqual(longPrompt.recovered_conversation.filter(t=>t.role==="user").map(t=>`Tú: ${t.content}`),longBefore.filter(t=>t.startsWith("Tú:")),"Long replies displaced confirmed user requests");
  assert.equal(await page.locator("#error").isVisible(),false);
  const longAfter=await page.locator("#conversation .message").allTextContents();
  assert.deepEqual(longAfter.slice(0,longBefore.length),longBefore,"Request trimming erased visible history");
  assert.match(await page.locator("#storage-status").innerText(),/Conversación guardada localmente/);
  const longProject=await page.evaluate(async()=>(await window.__TAURI_INTERNALS__.invoke("get_snapshot")).project);
  await page.reload(); await page.locator("#setup").click(); await page.locator("#provider").selectOption("ollama_cloud");
  assert(!await page.locator("#conversation").innerText().then(text=>text.includes("Primera intención que debe seguir guardada")),"Other-folder history leaked into the default project");
  await page.evaluate(project=>{window.otherFolder=project[1]==="C:\\owned\\other-folder";},longProject);
  await page.locator("#open-project").click();
  await page.locator("#send:enabled").waitFor();
  assert.deepEqual(await page.locator("#conversation .message").allTextContents(),longAfter,"History beyond 8 KiB did not recover intact");
  assert.equal((await chatCalls()).length,0,"Reload replayed a model request");
  await page.locator("#new-chat").click(); assert.equal(await page.locator("#context-status").innerText(),"");
  for(const provider of ["codex","claude","ollama_local","ollama_cloud"]){
    await page.locator("#provider").selectOption(provider);
    await page.locator("#program").selectOption("freecad");
    await page.locator("#new-chat").click();
    await page.evaluate(()=>window.setPendingJob({schema_version:1,live_cad_version:1,job_id:crypto.randomUUID(),document_name:"Enclosure",document_token:"11111111-1111-4111-8111-111111111111",state_sha256:"a".repeat(64),description:"Haz la carcasa más larga",before:[{name:"InternalBox",label:"Carcasa",type:"Part::Box",properties:{Length:"25.4 mm"}}],operations:[{action:"edit_object",obj_name:"InternalBox",obj_properties:{Length:30.4}}]}));
    await page.locator("#program-actions button").first().click();
    await page.locator("#approve:enabled").waitFor();
    const facts=await page.locator("#change-facts").innerText();
    assert.match(facts,/Carcasa[\s\S]*25.4 mm → 30.4 mm/);
    assert.match(facts,/No se guarda ni exporta el modelo/);
    assert(!facts.includes("InternalBox")&&!facts.includes("archivos originales se conservan"));
    assert.equal(await page.locator("#change-details").isVisible(),false);
    const before=(await chatCalls()).length;
    await page.locator("#approve").click();await page.locator("#send:enabled").waitFor();
    const calls=await chatCalls();assert.equal(calls.length,before+1);
    assert.equal(calls.at(-1).args.program,"freecad");
    assert.match(JSON.parse(calls.at(-1).args.prompt).user_request,/inspect_open_cad_documents/);
    assert.match(await page.locator("#job-status").innerText(),/documento abierto de FreeCAD[\s\S]*no se guardó ni exportó/);
    if(!await page.locator("#history").isVisible())await page.locator("#results > summary").click();
    const history=await page.locator("#history").innerText();
    assert.match(history,/Informe de la transacción/);assert(!history.includes("STEP")&&!history.includes("HEX"));
  }
  for (const provider of ["codex","claude","ollama_local","ollama_cloud"]) {
    await page.locator("#provider").selectOption(provider);
    await page.locator("#new-chat").click();
    await page.evaluate(()=>window.setPendingJob({schema_version:1,live_cad_version:1,job_id:crypto.randomUUID(),document_name:"Enclosure",document_token:"11111111-1111-4111-8111-111111111111",state_sha256:"a".repeat(64),description:"Guarda la carcasa terminada",before:[],operations:[{action:"save_as"}]}));
    await page.locator("#program-actions button").first().click();
    await page.locator("#approve:enabled").waitFor();
    const facts=await page.locator("#change-facts").innerText();
    assert.match(facts,/FreeCAD abrirá Guardar como/);
    assert.match(facts,/Guardar no es una edición de geometría/);
    assert(!facts.includes("No se guarda ni exporta"));
    const before=(await chatCalls()).length;
    await page.locator("#approve").click();await page.locator("#send:enabled").waitFor();
    assert.equal((await chatCalls()).length,before+1);
    assert.match(await page.locator("#job-status").innerText(),/FreeCAD confirmó el guardado/);
    if (!await page.locator("#history").isVisible()) await page.locator("#results > summary").click();
    assert.match(await page.locator("#history").innerText(),/Informe del guardado/);
  }
  const evidence = path.resolve(root, "../../target/chat-first"); await mkdir(evidence, { recursive: true });
  await page.locator("#connections > summary").click();
  await page.screenshot({ path: path.join(evidence, "chat.png"), fullPage: true });
  assert.deepEqual(errors, []);
  console.log("PASS: production chat entry; all four AI choices and six program groups; 36 saved-copy, four open-FreeCAD edit and four native Save As approval/review handoffs; native facts, scope and report-only actions; delayed/duplicate completion, failure, rejection and cancellation; selected context, no automatic approvals, verified-result access, question privacy, stale-event refusal and scoped history; IPC fixtures only");
} finally { await browser?.close(); await server.close(); }
