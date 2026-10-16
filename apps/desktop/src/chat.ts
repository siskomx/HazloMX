import { invoke, Channel } from "@tauri-apps/api/core";
import { agentPrompt, CHAT_HISTORY_BYTES, savedConversation, type Turn } from "./design";
import { legacyConversations } from "./chat-history";
import { proposalFacts } from "./proposal-review";
import type { ChatView, CodexLoginView, DesktopSnapshot, DiscoveredCadDocument, DiscoveredDrawing, DiscoveredLivePcb, DiscoveredPcb, DiscoveredSchematic, DiscoveredSlice, Job, Snapshot } from "./contracts";
import "./chat.css";

const programs = { planning: "Ayúdame a elegir", freecad: "FreeCAD / CADPilot", drawing: "CADCraft", arduino: "Arduino CLI", agent: "Arduino Agent", kicad: "KiCad / Konnect", slice: "OrcaSlicer" };
const phases: Record<Snapshot["phase"], string> = { awaiting_approval: "Pendiente de aprobación", running: "En curso", cancelled: "Cancelada", failed: "Falló", interrupted: "Interrumpida", completed: "Completada" };
type Program = keyof typeof programs;
type Provider = "codex" | "claude" | "ollama_local" | "ollama_cloud";
const providerNames: Record<Provider, string> = {codex:"Codex",claude:"Claude Code",ollama_local:"Ollama local · experimental",ollama_cloud:"Ollama cloud"};
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
document.querySelector("#app")!.innerHTML = `
  <header><div><h1>Hazlo Mexa</h1><span class="fine">Cuéntame qué quieres crear o cambiar.</span></div><button id="setup" class="secondary">Conexiones</button></header>
  <details id="connections" class="panel"><summary>Tu IA, programas y documentos</summary>
    <label for="provider">IA que usará las herramientas</label><select id="provider"><option value="codex">Codex instalado</option><option value="claude">Claude Code instalado</option><option value="ollama_local">Ollama local · experimental</option><option value="ollama_cloud">Ollama cloud</option></select>
    <p class="fine">Tu mensaje y los documentos seleccionados pueden enviarse al servicio de la IA y consumir tu plan. Inicia sesión personalmente cuando el cliente lo solicite.</p>
    <div class="actions"><button id="select-client" class="secondary">Elegir cliente instalado…</button><button id="connect-account">Conectar cuenta</button><button id="open-login" class="secondary" hidden>Abrir inicio de sesión</button><button id="cancel-login" class="secondary" hidden>Detener conexión</button></div><p id="login-status" class="status fine"></p>
    <label for="program">Programa con el que trabajaremos</label><select id="program"></select><div id="program-actions" class="actions"></div><p id="source-status" class="status fine"></p>
    <div id="live-pcb-setup" hidden><label for="kicad-endpoint">Dirección IPC local de las preferencias de KiCad</label><input id="kicad-endpoint" type="text" maxlength="1030" placeholder="ipc://…"><label for="kicad-api-token">Token de esta sesión, si KiCad lo requiere</label><input id="kicad-api-token" type="password" maxlength="4096" autocomplete="off"><p class="fine">Activa la API en KiCad y copia su dirección local. Esta conexión permite consultar la PCB abierta y preparar movimientos para tu aprobación. Puedes deshacerlos en KiCad; la PCB no se guarda automáticamente. El token no se guarda en el historial.</p></div>
    <p id="document-scope" class="fine"></p>
    <h2>Proyecto local</h2><p id="project-name" class="fine"></p><div class="actions"><button id="create-project" class="secondary">Crear proyecto…</button><button id="open-project" class="secondary">Abrir proyecto…</button></div>
  </details>
  <section class="panel" aria-labelledby="chat-heading"><h2 id="chat-heading">¿Qué quieres hacer?</h2><p id="connection-status" class="fine"></p><button id="choose-program" class="secondary">Elegir el programa que acordamos</button><div id="conversation" role="log" aria-label="Conversación" aria-live="polite"></div><p id="chat-progress" class="status fine" aria-live="polite"></p>
    <form id="question-form" hidden><div id="question-fields"></div><button id="answer-question">Continuar con mis respuestas</button></form>
    <form id="chat-form"><label for="prompt">Tu mensaje</label><textarea id="prompt" maxlength="4096" rows="3" placeholder="Quiero modificar este diseño, pero todavía no sé sus medidas…" required></textarea><div class="actions"><button id="send">Enviar</button><button id="stop" type="button" class="secondary" disabled>Detener</button><button id="new-chat" type="button" class="secondary">Nueva conversación</button></div></form><p id="storage-status" class="fine"></p><p id="context-status" class="fine" role="status"></p>
    <section id="approval" class="panel" hidden aria-labelledby="approval-heading"><h2 id="approval-heading">Revisa el cambio</h2><p id="change-summary"></p><ul id="change-facts"></ul><details><summary>Detalles del cambio</summary><pre id="change-details"></pre></details><div class="actions"><button id="approve">Aprobar cambio</button><button id="cancel-job" class="secondary">Cancelar</button></div></section>
    <p id="job-status" class="status" aria-live="polite"></p><button id="cancel-running" class="secondary" hidden>Detener operación</button>
  </section>
  <details id="past-chats" class="panel" hidden><summary>Conversaciones de la versión anterior</summary><p class="fine">Puedes leerlas sin enviarlas a una IA. Para continuar con sus mensajes, empieza una conversación vacía y elige la IA con la que seguirás. Solo se enviarán cuando pulses Enviar. No se restauran documentos, medidas ni aprobaciones; inspeccionaremos de nuevo la selección actual.</p><div id="past-chat-list"></div></details>
  <details id="results" class="panel"><summary>Resultados e historial verificados</summary><p id="opening-status" class="fine" aria-live="polite"></p><div id="history"></div></details>
  <p id="error" role="alert" hidden></p><dialog id="report"><h2>Resultado guardado · solo lectura</h2><pre id="report-text"></pre><form method="dialog"><button>Cerrar</button></form></dialog>`;
for (const [value, label] of Object.entries(programs)) $("program").append(new Option(label, value));

let state: DesktopSnapshot = { project: null, job: null, inference: null, recovered: [], freecad: "", freecad_available: false };
let program: Program = "planning", provider: Provider = "codex";
let commandBusy = false, revision = 0, newSession = true;
let turns: Turn[] = [], answeredChat: string | undefined, storageKey = "", questionId: string | undefined;
let renderedConversation = "", contextNotice = "";
let sourceContext: Record<string, unknown> = {};
let sourceNames: Partial<Record<Program, string>> = {};
let sourcePreviousJobs: Partial<Record<Program, string | null>> = {};
let renderedLegacy = "";
let resultReview: { revision: number; project: string; provider: Provider; program: Program; jobId: string } | undefined;
let inspection: {id?:string;cancelled:boolean;stopping:boolean;stop:Promise<void>} | undefined;
let inspectionNotice = "";

function error(message: unknown) { $("error").textContent = String(message); $("error").hidden = false; }
function persist() {
  if (!storageKey) return;
  const saved = JSON.stringify({ version: 1, turns, answeredChat });
  try {
    if (!savedConversation(saved, CHAT_HISTORY_BYTES)) throw Error("La conversación supera el límite de guardado.");
    localStorage.setItem(storageKey, saved); $("storage-status").textContent = "Conversación guardada localmente en este proyecto y para esta IA.";
  } catch { $("storage-status").textContent = "No se pudo guardar esta conversación. Conserva lo importante antes de cerrar."; }
}
function restore() {
  const key = state.project ? `hazlomexa.chat.v2:${JSON.stringify([...state.project, provider])}` : "";
  if (key === storageKey) return;
  persist(); storageKey = key; turns = []; answeredChat = undefined; newSession = true; contextNotice = "";
  try { const saved = savedConversation(key ? localStorage.getItem(key) : null, CHAT_HISTORY_BYTES); if (saved) { turns = saved.turns; answeredChat = saved.answeredChat; } } catch { /* Storage failure must not prevent using the chat. */ }
}
function renderLegacy() {
  try {
    const records = legacyConversations(key => localStorage.getItem(key), state.project);
    const disabled = commandBusy || !!state.chat?.busy || !!state.job?.busy || !!state.codex_login?.busy || state.job?.snapshot.phase === "awaiting_approval" || turns.length > 0;
    const signature = JSON.stringify([storageKey, provider, disabled, records]);
    if (signature === renderedLegacy) return;
    renderedLegacy = signature;
    const opened = new Set([...$("past-chat-list").querySelectorAll<HTMLDetailsElement>("details[open]")].map(entry => entry.dataset.label));
    $("past-chats").hidden = records.length === 0;
    $("past-chat-list").replaceChildren();
    const scope = storageKey;
    for (const record of records) {
      const entry = document.createElement("details"), title = document.createElement("summary");
      entry.dataset.label = record.label; entry.open = opened.has(record.label); title.textContent = record.label; entry.append(title);
      for (const turn of record.turns) { const row = document.createElement("p"); row.textContent = `${turn.role === "user" ? "Tú" : "Hazlo Mexa"}: ${turn.content}`; entry.append(row); }
      const button = document.createElement("button"); button.className = "secondary";
      button.textContent = `Continuar con ${providerNames[provider]}`; button.disabled = disabled;
      button.onclick = () => {
        if (scope !== storageKey || commandBusy || state.chat?.busy || state.job?.busy || state.codex_login?.busy || state.job?.snapshot.phase === "awaiting_approval" || turns.length) return;
        try {
          const saved = savedConversation(localStorage.getItem(record.key));
          if (!saved?.turns.length) return;
          revision++; turns = saved.turns; answeredChat = state.chat?.id; newSession = true;
          persist(); render();
        } catch { error("No se pudo leer la conversación anterior. Sus datos se conservan."); }
      };
      entry.append(button); $("past-chat-list").append(entry);
    }
  } catch { renderedLegacy = ""; $("past-chats").hidden = true; $("past-chat-list").replaceChildren(); $("storage-status").textContent = "No se pudo leer el historial anterior. Sus datos se conservan."; }
}
function apply(next: DesktopSnapshot) {
  const projectChanged = JSON.stringify(next.project) !== JSON.stringify(state.project);
  if (projectChanged) { revision++; sourceContext = {}; sourceNames = {}; sourcePreviousJobs = {}; inspectionNotice = ""; }
  if (next.chat && state.chat?.id === next.chat.id && state.chat.sequence > next.chat.sequence) next.chat = state.chat;
  if (next.job && state.job?.snapshot.input.job_id === next.job.snapshot.input.job_id && state.job.sequence > next.job.sequence) next.job = state.job;
  if (next.codex_login && state.codex_login?.id === next.codex_login.id && state.codex_login.sequence > next.codex_login.sequence) next.codex_login = state.codex_login;
  state = next; restore();
  const chat = state.chat;
  // A chat view carries a project UUID, not its folder; copied projects can share that UUID.
  if (projectChanged && chat) answeredChat = chat.id;
  if (chat && chat.provider === provider && !chat.busy && chat.id !== answeredChat && chat.project_id === state.project?.[0]) {
    if (chat.text) turns.push({ role: "assistant", content: chat.text });
    else if (chat.cancelled) turns.push({ role: "assistant", content: "Conversación detenida." });
    if (chat.error) error(chat.error);
    answeredChat = chat.id; persist();
  }
  render();
  reviewCompletedResult();
}
async function action(work: () => Promise<void>) {
  if (commandBusy) return;
  commandBusy = true; $("error").hidden = true; render();
  try { await work(); } catch (e) { if (String(e) !== "Selección cancelada.") error(e); }
  finally { commandBusy = false; render(); reviewCompletedResult(); }
}
function reviewCompletedResult() {
  const pending = resultReview;
  if (!pending || commandBusy || state.chat?.busy || state.job?.busy) return;
  const job = state.job;
  if (!job || pending.revision !== revision || pending.project !== JSON.stringify(state.project) || pending.provider !== provider || pending.program !== program || job.snapshot.project_id !== state.project?.[0] || jobProgram(job.snapshot) !== program || job.snapshot.input.job_id !== pending.jobId) { resultReview = undefined; return; }
  if (["awaiting_approval", "running"].includes(job.snapshot.phase)) return;
  resultReview = undefined;
  if (job.snapshot.phase !== "completed" || job.error) return;
  void action(() => send(`El cambio que acabo de aprobar terminó. Lee read_verified_result para job_id ${pending.jobId}; si es una operación de FreeCAD abierto, vuelve a consultar inspect_open_cad_documents con su nombre; si es un sketch abierto de Arduino Agent, consulta inspect_open_agent_documents para leer su estado actual; si es una PCB abierta, consulta inspect_selected_live_pcb con la selección actual para leer el estado nativo. Explícame brevemente qué cambió y qué comprobó el programa, con las limitaciones reales. No prepares ni ejecutes otro cambio. No afirmes haber editado el documento abierto si solo se guardó una copia.`, false));
}
function summary(run: Snapshot): string {
  const input = run.input;
  if (input.live_agent_version) return (input.save ? "Guardar sketch · " : "Sketch abierto · ") + input.description;
  if (input.live_pcb_version) return "PCB abierta · " + input.description;
  if (input.live_cad_version) return "FreeCAD abierto · " + input.description;
  if (input.konnect_version) return input.pcb_move ? `KiCad · ${input.pcb_move.reference}: (${input.pcb_move.expected_x_mm}, ${input.pcb_move.expected_y_mm}) → (${input.pcb_move.x_mm}, ${input.pcb_move.y_mm}) mm` : `KiCad · ${input.edit.reference}: ${input.edit.expected_value} → ${input.edit.value}`;
  if (input.cadpilot_version) return `FreeCAD · ${input.source_name} · ${input.request.operation === "inspect" ? "Inspeccionar y medir" : "Cambiar documento"}`;
  if (input.orca_version) return `OrcaSlicer · ${input.source_name} · ${input.settings.machine} · ${input.settings.filament}`;
  if (input.kicad_version) return `KiCad · revisar ${input.entry}`;
  if (input.cadcraft_version) return input.native_tools ? `CADCraft · ${input.source_name} · ${input.native_tools.intent}` : `CADCraft · ${input.source_name} → ${input.output_format.toUpperCase()}`;
  if (input.agent_version) return `Arduino Agent · cambios en ${input.source_name}`;
  if (input.dimensions_mm) { const d = input.dimensions_mm; return `FreeCAD · ${d.length} × ${d.width} × ${d.height} mm`; }
  return `Arduino · compilar para ${input.fqbn === "arduino:avr:uno" ? "Arduino Uno" : input.fqbn}`;
}
function changeDetails(run: Snapshot): string {
  const input = run.input;
  if (input.live_agent_version && input.save) return "Código actual que guardará Arduino Agent:\n" + input.before.source;
  if (input.live_agent_version) return "Código actual en Arduino Agent:\n" + input.before.source + "\nCódigo propuesto:\n" + input.source;
  if (input.live_pcb_version) return JSON.stringify({current_components:input.before.components,approved_move:input.movement},null,2);
  if (input.live_cad_version) return JSON.stringify({document:input.document_name,current_geometry:input.before,native_operations:input.operations},null,2);
  if (input.agent_version) return `Original:\n${input.original_source}\n\nCambio propuesto:\n${input.source}`;
  if (typeof input.source === "string") return input.source;
  if (input.konnect_version) return JSON.stringify(input.pcb_move ?? input.edit, null, 2);
  if (input.cadpilot_version) return JSON.stringify({ document: input.source_name, change: input.request, measurements: input.preview.measurements }, null, 2);
  if (input.orca_version) return JSON.stringify({ model: input.source_name, units: input.units, dimensions_mm: input.mesh.bounds_mm, settings: input.settings }, null, 2);
  if (input.kicad_version) return JSON.stringify({ document: input.entry, report_units: input.units, captured_files: input.files.map(file => file.path) }, null, 2);
  if (input.cadcraft_version) return JSON.stringify({ document: input.source_name, units: input.units, output: input.output_format, current_entities: input.native_source, native_tools: input.native_tools, circle: input.circle, accepted_loss: input.allow_loss }, null, 2);
  return JSON.stringify(input.dimensions_mm, null, 2);
}
function artifactActions(run: Snapshot): [string, string][] {
  const input = run.input;
  if (input.live_agent_version) return [[input.save ? "Informe del guardado en Arduino Agent" : "Informe del cambio en Arduino Agent","live_agent_report"]];
  if (input.live_pcb_version) return [["Informe del cambio PCB","live_pcb_report"]];
  if (input.live_cad_version) return [[input.operations[0]?.action === "save_as" ? "Informe del guardado" : input.operations[0]?.action === "export_step" ? "Informe de la exportación" : "Informe de la transacción","cadpilot_report"]];
  if (input.konnect_version) return [["Original", "konnect_original"], ["Resultado guardado", "konnect_edited"], ["Informe", "konnect_report"], ...(input.pcb_move ? [["Revisión KiCad", "kicad_report"], ["Exportaciones", "kicad_outputs"]] : input.schema_version === 2 ? [["Netlist", "konnect_netlist"]] : [])] as [string, string][];
  if (input.cadpilot_version) return [["Original", "original_cad"], ["Mediciones e informe", "cadpilot_report"], ...(input.request.operation === "inspect" ? [] : [["Abrir en FreeCAD", "fcstd"], ["STEP", "step"]])] as [string, string][];
  if (input.orca_version) return [["3MF", "sliced"], ["G-code", "gcode"], ["Informe", "slice_report"]];
  if (input.kicad_version) return [["Documento revisado", "kicad_source"], ["Exportaciones", "kicad_outputs"], ["Informe", "kicad_report"]];
  if (input.cadcraft_version) return [["Dibujo", "drawing"], ["Vista", "drawing_preview"], ["Informe", "drawing_report"]];
  if (input.agent_version) return [["Sketch original", "original_sketch"], ["Sketch verificado", "sketch"]];
  if (input.dimensions_mm) return [["Abrir en FreeCAD", "fcstd"], ["STEP", "step"], ["Informe", "solid_report"]];
  return [["Sketch", "sketch"], ["Informe de compilación", "compile_report"], ["HEX", "hex"], ["ELF", "elf"]];
}
function renderHistory() {
  $("history").replaceChildren();
  const runs = new Map(state.recovered.map(run => [run.input.job_id, run]));
  if (state.job) runs.set(state.job.snapshot.input.job_id, state.job.snapshot);
  for (const run of [...runs.values()].sort((a, b) => (b.audit?.created_unix_ms ?? 0) - (a.audit?.created_unix_ms ?? 0) || a.input.job_id.localeCompare(b.input.job_id))) {
    const row = document.createElement("article"), title = document.createElement("p"); title.textContent = `${summary(run)} · ${phases[run.phase]}`; row.append(title);
    if (run.phase === "completed") {
      const actions = document.createElement("div"); actions.className = "actions";
      for (const [label, format] of artifactActions(run)) {
        const button = document.createElement("button"); button.className = "secondary"; button.textContent = label; button.disabled = commandBusy;
        button.onclick = () => void action(async () => {
          const content = await invoke<string | null>("open_artifact", { jobId: run.input.job_id, format });
          if (content !== null) { $("report-text").textContent = content; $<HTMLDialogElement>("report").showModal(); }
          $("opening-status").textContent = content !== null ? "Informe guardado abierto en modo de solo lectura." : "Se solicitó abrir el archivo verificado en Windows.";
        }); actions.append(button);
      } row.append(actions);
    } $("history").append(row);
  }
}
function renderQuestion() {
  const chat = state.chat?.provider === provider ? state.chat : null;
  const question = chat?.busy && !chat.cancelled ? chat.question : null;
  $("question-form").hidden = !question;
  $<HTMLButtonElement>("answer-question").disabled = commandBusy;
  if (!question) { questionId = undefined; $("question-fields").replaceChildren(); return; }
  if (question.id === questionId) return;
  questionId = question.id; $("question-fields").replaceChildren();
  for (const item of question.questions) {
    const label = document.createElement("label"); label.textContent = item.question;
    if (item.options?.length && !item.isOther && !item.isSecret) {
      const select = document.createElement("select"); select.name = item.id; select.required = true; select.add(new Option("Elige una respuesta", ""));
      for (const option of item.options) select.add(new Option(`${option.label} · ${option.description}`, option.label)); label.append(select);
    } else { const input = document.createElement("input"); input.name = item.id; input.required = true; input.type = item.isSecret ? "password" : "text"; input.autocomplete = "off"; label.append(input); }
    $("question-fields").append(label);
  }
}
function render() {
  const busy = commandBusy || !!state.chat?.busy || !!state.job?.busy || !!state.codex_login?.busy;
  $("context-status").textContent = contextNotice;
  const ollama = provider.startsWith("ollama_");
  const clientReady = ollama || (provider === "claude" ? !!state.claude : !!state.codex && !!state.codex_login?.connected);
  $("project-name").textContent = state.project ? "Proyecto local seleccionado. Sus archivos e historial se conservan aquí." : "Elige dónde guardar el proyecto y su historial.";
  const native = program === "kicad" && !!sourceContext.live_pcb_source;
  $("choose-program").hidden = program !== "planning";
  $("live-pcb-setup").hidden = program !== "kicad";
  $("document-scope").textContent = program === "planning" ? "Describe tu idea. Elegiremos el programa y las medidas necesarias antes de conectar un documento o preparar cambios." : native ? "PCB abierta conectada: el chat consulta sus posiciones actuales y prepara movimientos para tu aprobación. Verás los cambios en KiCad y puedes deshacerlos allí. No se guarda la PCB ni se vuelven a trazar las pistas automáticamente. Para editar una copia guardada, selecciónala expresamente." : program==="freecad" ? "Para trabajar en el documento abierto, abre FreeCAD y activa CADPilot. El chat puede leerlo y preparar cambios que verás allí después de aprobarlos. Conservas Deshacer para las ediciones. Para guardar o exportar una pieza a STEP, FreeCAD abrirá su propio diálogo y elegirás el destino allí. Exportar conserva el modelo abierto sin guardarlo y requiere un archivo nuevo. También puedes elegir expresamente una copia guardada." : program === "agent" ? "Para trabajar en el sketch abierto, instala el complemento desde Conexiones y abre Arduino Agent. Si estaba abierto, guarda tu trabajo y reinícialo para activar el complemento. El chat consulta el código actual, incluso sin guardar, y prepara cambios para tu aprobación. Puedes deshacer las ediciones en el IDE. Para guardar el código actual, pídelo en el chat y aprueba el guardado por separado; usa el archivo actual e incluye tus propios cambios sin guardar. También puedes elegir expresamente una copia guardada." : "Estas capacidades trabajan con archivos guardados y copias verificadas. Guarda los cambios abiertos antes de seleccionar un documento para editar su copia. La edición de documentos vivos sigue pendiente.";
  $("connection-status").textContent = !state.project ? "Abre o crea tu proyecto en Conexiones para empezar." : !clientReady ? "Conecta tu IA en Conexiones para conversar con las herramientas del programa." : `${providerNames[provider]} · ${programs[program]} · ${program === "planning" ? "vamos a elegir el programa" : native ? "PCB abierta en KiCad" : program === "freecad" ? "FreeCAD abierto o copia seleccionada" : program === "agent" ? "sketch abierto o copia seleccionada" : "archivos guardados"}`;
  $("source-status").textContent = program === "planning" ? "La IA puede explicar las opciones disponibles. Elige el programa que acuerden para habilitar sus herramientas." : sourceNames[program] ?? (program === "agent" ? "Puedes describir el cambio en tu sketch abierto. Para editar una copia, selecciónala expresamente." : program === "slice" ? "Selecciona el STL y los perfiles guardados. Confirmaremos su escala y las opciones de impresión en la conversación." : "Selecciona un documento si quieres trabajar sobre uno existente.");
  const messages = turns.length ? turns : [{ role: "assistant", content: "Cuéntame qué quieres crear o cambiar y para qué lo necesitas. Revisaremos el documento con las herramientas del programa y te preguntaré lo que falte. No necesitas conocer sus nombres internos ni todas sus medidas para empezar." }];
  if (JSON.stringify(messages) !== renderedConversation) {
    renderedConversation = JSON.stringify(messages);
    const log = $("conversation"), follow = log.scrollHeight - log.scrollTop - log.clientHeight < 48;
    log.replaceChildren();
    for (const message of messages) { const row = document.createElement("p"); row.className = `message ${message.role}`; row.textContent = `${message.role === "user" ? "Tú" : "Hazlo Mexa"}: ${message.content}`; log.append(row); }
    if (follow) log.scrollTop = log.scrollHeight;
  }
  $("chat-progress").textContent = inspection ? inspection.cancelled || inspection.stopping ? "Deteniendo inspección…" : inspection.id ? "Inspeccionando el documento con sus herramientas…" : "Selecciona las fuentes o cancela la ventana de selección." : state.chat?.provider === provider && state.chat.busy ? state.chat.cancelled ? "Deteniendo conversación…" : state.chat.text || "Consultando a tu IA…" : inspectionNotice;
  for (const id of ["send", "new-chat"]) $<HTMLButtonElement>(id).disabled = busy || !state.project || !clientReady;
  $<HTMLTextAreaElement>("prompt").disabled = busy;
  $<HTMLButtonElement>("stop").disabled = inspection ? !inspection.id || inspection.stopping || inspection.cancelled : commandBusy || !state.chat?.busy || !!state.chat.cancelled;
  $("stop").textContent = inspection ? "Detener inspección" : "Detener";
  for (const id of ["provider", "program"]) $<HTMLSelectElement>(id).disabled = busy || state.job?.snapshot.phase === "awaiting_approval";
  for (const id of ["select-client", "create-project", "open-project"]) $<HTMLButtonElement>(id).disabled = busy;
  $("select-client").hidden = ollama;
  $<HTMLButtonElement>("connect-account").hidden = provider !== "codex";
  $<HTMLButtonElement>("connect-account").disabled = busy || !state.codex;
  $("open-login").hidden = !state.codex_login?.busy || !state.codex_login.auth_url || !!state.codex_login.cancelled;
  $("cancel-login").hidden = !state.codex_login?.busy || !!state.codex_login.cancelled;
  $("login-status").textContent = ollama ? `Usa Ollama 0.35.1 en este equipo y el perfil ${provider === "ollama_local" ? "qwen3:4b" : "kimi-k2.7-code:cloud"} ya instalado y conectado. Comprobamos su identidad y soporte de herramientas antes de enviar. No se descargan modelos. ${provider === "ollama_cloud" ? "Este perfil envía el contexto al servicio cloud de Ollama." : "La calidad de este perfil local sigue siendo experimental."}` : provider === "claude" ? state.claude ? `Claude Code ${state.claude.version}. Usa su inicio de sesión existente.` : "Elige el cliente Claude Code instalado." : state.codex_login?.error ?? (state.codex_login?.connected ? "Cuenta Codex conectada." : state.codex_login?.busy ? "Completa personalmente la conexión con Codex." : "Elige Codex y conecta tu cuenta.");
  const active = state.job?.snapshot.phase === "awaiting_approval";
  $("approval").hidden = !active;
  if (state.job) {
    $("change-summary").textContent = summary(state.job.snapshot); $("change-details").textContent = changeDetails(state.job.snapshot);
    $("change-facts").replaceChildren(...(active ? proposalFacts(state.job.snapshot) : []).map(text => { const item = document.createElement("li"); item.textContent = text; return item; }));
  }
  $<HTMLButtonElement>("approve").disabled = busy || !active || !state.job?.approval_id;
  $("approve").textContent = `Aprobar y pedir revisión a ${providerNames[provider]}`;
  $<HTMLButtonElement>("cancel-job").disabled = commandBusy || !active;
  $("cancel-running").hidden = !state.job?.busy; $<HTMLButtonElement>("cancel-running").disabled = commandBusy;
  const job = state.job;
  $("job-status").textContent = !job ? "" : job.error ?? (job.snapshot.phase === "completed" ? job.snapshot.input.live_agent_version ? job.snapshot.input.save ? "Arduino Agent confirmó el guardado del código aprobado y su estado sin cambios pendientes." : "Cambio leído en el sketch abierto de Arduino Agent. El guardado lo gestiona el IDE." : job.snapshot.input.live_pcb_version ? "Cambio leído en la PCB abierta de KiCad. El archivo todavía no se guardó." : job.snapshot.input.live_cad_version ? job.snapshot.input.operations[0]?.action === "save_as" ? "FreeCAD confirmó el guardado. El informe conserva el nombre y la huella del archivo en ese momento." : job.snapshot.input.operations[0]?.action === "export_step" ? "STEP exportado y verificado. El documento abierto sigue sin cambios; exportar no guarda el modelo nativo." : "Cambio leído en el documento abierto de FreeCAD. Puedes deshacerlo allí; todavía no se guardó ni exportó el modelo." : "Resultado guardado y verificado por Hazlo. Puedes revisarlo en Resultados." : job.snapshot.phase === "awaiting_approval" ? "Cambio pendiente de tu aprobación." : job.busy ? "El programa está realizando la operación…" : `Operación: ${phases[job.snapshot.phase]}.`);
  renderConnections(busy || !!active); renderQuestion(); renderHistory(); renderLegacy();
}
function connectionButton(label: string, disabled: boolean, work: () => Promise<void>) {
  const button = document.createElement("button"); button.textContent = label; button.className = "secondary"; button.disabled = disabled || !state.project; button.onclick = () => void action(work); $("program-actions").append(button);
}
function selectedNewSource(program: Program) {
  sourcePreviousJobs[program] = state.job?.snapshot.input.job_id ?? null;
}
async function inspectSource<T>(command: string, args: Record<string,unknown> = {}): Promise<T> {
  const pending = {id:undefined as string|undefined,cancelled:false,stopping:false,stop:Promise.resolve()};
  inspection = pending; inspectionNotice = "";
  const events = new Channel<string>();
  events.onmessage = id => { if (inspection === pending) { pending.id = id; render(); } };
  render();
  try {
    const selected = await invoke<T>(command,{...args,events});
    await pending.stop;
    if (pending.cancelled) throw "Selección cancelada.";
    inspectionNotice = "Documento inspeccionado. Cuéntame qué quieres cambiar.";
    return selected;
  } catch (e) {
    await pending.stop;
    if (pending.cancelled) { inspectionNotice = "Inspección detenida. Se conserva la selección anterior."; throw "Selección cancelada."; }
    throw e;
  } finally { if (inspection === pending) inspection = undefined; render(); }
}
function renderConnections(disabled: boolean) {
  $("program-actions").replaceChildren();
  if (program === "planning") return;
  const connect = (label: string, command: string) => connectionButton(label, disabled, async () => apply(await invoke<DesktopSnapshot>(command)));
  if (program === "freecad") {
    connect("Elegir FreeCAD…", "select_freecad");
    connectionButton("Seleccionar documento guardado e inspeccionar…", disabled, async () => {
      const selected = await inspectSource<DiscoveredCadDocument>("discover_cad_document");
      if (selected.project_id !== state.project?.[0]) throw Error("El documento pertenece a otro proyecto.");
      sourceContext.cad_document_source = { selection: selected.id, source_sha256: selected.inspection.source_sha256 };
      sourceNames.freecad = selected.inspection.source_name; selectedNewSource("freecad");
    });
  } else if (program === "drawing") {
    connect("Elegir CADCraft…", "select_cadcraft");
    connectionButton("Seleccionar dibujo…", disabled, async () => { const selected = await inspectSource<DiscoveredDrawing>("discover_drawing"); if (selected.project_id !== state.project?.[0]) throw Error("El dibujo pertenece a otro proyecto."); sourceContext.drawing_source = { selection: selected.id, sha256: selected.inspection.source_sha256 }; sourceNames.drawing = selected.inspection.source_name; selectedNewSource("drawing"); });
  } else if (program === "arduino") connect("Elegir Arduino CLI y su entorno…", "select_arduino");
  else if (program === "agent") {
    connect("Conectar Arduino Agent", "connect_agent");
    connectionButton("Instalar complemento para el sketch abierto", disabled, async () => {
      inspectionNotice = await invoke<string>("install_agent_companion");
    });
    connectionButton("Seleccionar copia guardada del sketch…", disabled, async () => { apply(await invoke<DesktopSnapshot>("select_agent_sketch")); sourceNames.agent = state.agent_source?.source_name; sourceContext.sketch_source = { name: state.agent_source?.source_name }; selectedNewSource("agent"); });
  } else if (program === "kicad") {
    connect("Elegir KiCad…", "select_kicad"); connect("Elegir Konnect…", "select_konnect");
    connectionButton("Seleccionar esquema…", disabled, async () => { const selected = await invoke<DiscoveredSchematic>("discover_schematic"); if (selected.project_id !== state.project?.[0]) throw Error("El esquema pertenece a otro proyecto."); sourceContext.schematic_source = { selection: selected.id, sha256: selected.sha256 }; delete sourceContext.pcb_source; delete sourceContext.live_pcb_source; sourceNames.kicad = selected.name; selectedNewSource("kicad"); });
    connectionButton("Seleccionar PCB…", disabled, async () => { const selected = await invoke<DiscoveredPcb>("discover_pcb"); if (selected.project_id !== state.project?.[0]) throw Error("La PCB pertenece a otro proyecto."); sourceContext.pcb_source = { selection: selected.id, sha256: selected.sha256 }; delete sourceContext.schematic_source; delete sourceContext.live_pcb_source; sourceNames.kicad = selected.name; selectedNewSource("kicad"); });
    connectionButton("Conectar PCB abierta en KiCad…", disabled, async () => {
      const connection = { endpoint: $<HTMLInputElement>("kicad-endpoint").value.trim(), token: $<HTMLInputElement>("kicad-api-token").value };
      $<HTMLInputElement>("kicad-api-token").value = "";
      const selected = await inspectSource<DiscoveredLivePcb>("discover_live_pcb", { connection });
      if (selected.project_id !== state.project?.[0]) throw Error("La PCB pertenece a otro proyecto.");
      sourceContext.live_pcb_source = { selection: selected.id, document_sha256: selected.observation.document_sha256, instance_sha256: selected.observation.instance_sha256, state_sha256: selected.observation.state_sha256, mutations_allowed: true, human_approval_required: true };
      delete sourceContext.pcb_source; delete sourceContext.schematic_source; sourceNames.kicad = `${selected.name} · lectura nativa`;
    });
  } else {
    connect("Elegir OrcaSlicer…", "select_orca");
    connectionButton("Seleccionar modelo y perfiles…", disabled, async () => {
      const selected=await inspectSource<DiscoveredSlice>("discover_slice");
      if(selected.project_id!==state.project?.[0]) throw Error("El modelo pertenece a otro proyecto.");
      sourceContext.slice_source={selection:selected.id,selection_sha256:selected.preview.selection_sha256,units_require_confirmation:true};
      selectedNewSource("slice");
      sourceNames.slice=`${selected.preview.source_name} · STL sin unidad declarada`;
    });
  }
}
function jobProgram(run: Snapshot): Program | undefined {
  const input = run.input;
  if (input.konnect_version || input.kicad_version) return "kicad";
  if (input.live_agent_version) return "agent";
  if (input.live_pcb_version) return "kicad";
  if (input.live_cad_version || input.cadpilot_version || input.dimensions_mm) return "freecad";
  if (input.cadcraft_version) return "drawing";
  if (input.orca_version) return "slice";
  if (input.agent_version) return "agent";
  if (input.fqbn) return "arduino";
}
function context() {
  if (program === "planning") return { program: "planning", current_job: null, available_programs: Object.fromEntries(Object.entries(programs).filter(([key]) => key !== "planning")), program_selection_required: true, document_scope: "choose_program", live_document_connection: false, live_mutations_allowed: false };
  let job = state.job?.snapshot.project_id === state.project?.[0] ? state.job?.snapshot : undefined;
  if (job && (jobProgram(job) !== program || job.input.job_id === sourcePreviousJobs[program])) job = undefined;
  const native = program === "kicad" && !!sourceContext.live_pcb_source;
  const selected = program === "kicad" ? native ? { live_pcb_source: sourceContext.live_pcb_source } : sourceContext.pcb_source ? { pcb_source: sourceContext.pcb_source } : { schematic_source: sourceContext.schematic_source } : program === "drawing" ? { drawing_source: sourceContext.drawing_source } : program === "agent" ? { sketch_source: sourceContext.sketch_source } : program === "slice" ? {slice_source:sourceContext.slice_source} : program === "freecad" ? {cad_document_source:sourceContext.cad_document_source} : {};
  return { project_id: state.project?.[0], program: programs[program], ...selected, current_job: job && (!native || !!job.input.live_pcb_version) ? { id: job.input.job_id, phase: job.phase } : null, source_is_untrusted_data: true, document_scope: native ? "open_native_pcb" : program === "agent" ? "open_agent_and_explicit_saved_copies" : program === "freecad" ? "open_freecad_and_explicit_saved_copies" : "saved_files_and_verified_copies", live_document_connection: program === "freecad" || program === "agent" ? null : native, live_mutations_allowed: native || program === "freecad" || program === "agent", native_connection_check: native ? "inspect_selected_live_pcb" : program === "freecad" ? "inspect_open_cad_documents" : program === "agent" ? "inspect_open_agent_documents" : null };
}
async function send(content: string, userInitiated = true) {
  inspectionNotice = "";
  const token = ++revision;
  const ready = provider === "codex" ? state.codex_conversation_ready : provider === "claude" ? state.claude_conversation_ready : false;
  const startNew = newSession || !ready;
  const prompt = agentPrompt(content, startNew ? turns : [], context());
  contextNotice = JSON.parse(prompt).omitted_conversation_turns ? "Se envió solo parte del historial a la IA. Los mensajes completos siguen visibles; revisaremos el documento y confirmaremos cualquier dato que falte." : "";
  const events = new Channel<ChatView>();
  events.onmessage = chat => {
    if (token !== revision || chat.project_id !== state.project?.[0] || chat.provider !== provider) return;
    apply({ ...state, chat });
    if (!chat.busy) void invoke<DesktopSnapshot>("get_snapshot").then(next => { if (token === revision) apply(next); }).catch(error);
  };
  if (userInitiated) { turns.push({ role: "user", content }); persist(); $<HTMLTextAreaElement>("prompt").value = ""; }
  render();
  const next = await invoke<DesktopSnapshot>(provider === "codex" ? "start_codex_chat" : provider === "claude" ? "start_claude_chat" : "start_ollama_chat", { prompt, program, newSession: startNew, events, ...(provider.startsWith("ollama_") ? {profile:provider === "ollama_local" ? "local_experimental" : "cloud"} : {}) });
  if (token === revision) { newSession = false; apply(next); }
}
$("choose-program").onclick = () => { $<HTMLDetailsElement>("connections").open = true; $("program").focus(); };
$("setup").onclick = () => { $<HTMLDetailsElement>("connections").open = true; $("provider").focus(); };
$("provider").onchange = () => { persist(); revision++; provider = $<HTMLSelectElement>("provider").value as Provider; storageKey = ""; turns = []; answeredChat = undefined; restore(); render(); };
$("program").onchange = () => { program = $<HTMLSelectElement>("program").value as Program; $("opening-status").textContent = ""; render(); };
$("select-client").onclick = () => void action(async () => { revision++; apply(await invoke<DesktopSnapshot>(provider === "codex" ? "select_codex" : "select_claude")); });
$("connect-account").onclick = () => void action(async () => {
  const token = ++revision, events = new Channel<CodexLoginView>();
  events.onmessage = login => { if (token !== revision || (state.codex_login?.id === login.id && state.codex_login.sequence > login.sequence)) return; state.codex_login = login; render(); };
  const next = await invoke<DesktopSnapshot>("connect_codex", { events });
  if (token === revision) { if (next.codex_login?.id === state.codex_login?.id && next.codex_login && state.codex_login && next.codex_login.sequence < state.codex_login.sequence) next.codex_login = state.codex_login; apply(next); }
});
$("open-login").onclick = () => void action(async () => { if (state.codex_login) await invoke("open_codex_login", { id: state.codex_login.id }); });
$("cancel-login").onclick = () => void action(async () => { if (state.codex_login) { const login = await invoke<CodexLoginView>("cancel_codex_login", { id: state.codex_login.id }); if (state.codex_login.id === login.id && state.codex_login.sequence <= login.sequence) state.codex_login = login; } });
for (const [id, create] of [["create-project", true], ["open-project", false]] as const) $(id).onclick = () => void action(async () => { revision++; apply(await invoke<DesktopSnapshot>("select_project", { create })); $("opening-status").textContent = ""; });
$("chat-form").onsubmit = event => { event.preventDefault(); const content = $<HTMLTextAreaElement>("prompt").value.trim(); if (content) void action(() => send(content)); };
$("new-chat").onclick = () => { if (state.chat?.busy || commandBusy) return; revision++; turns = []; answeredChat = state.chat?.id; newSession = true; contextNotice = ""; persist(); render(); };
$("stop").onclick = () => {
  const pending = inspection;
  if (pending) {
    if (!pending.id || pending.cancelled || pending.stopping) return;
    pending.stopping = true;
    pending.stop = invoke<boolean>("cancel_source_inspection",{id:pending.id}).then(accepted=>{pending.cancelled=accepted;}).catch(error).finally(()=>{pending.stopping=false;render();});
    render(); return;
  }
  void action(async () => { if (state.chat) apply({ ...state, chat: await invoke<ChatView>("cancel_chat", { id: state.chat.id }) }); });
};
$("question-form").onsubmit = event => {
  event.preventDefault(); const chat = state.chat, question = chat?.question;
  if (!chat?.busy || chat.cancelled || !question || commandBusy) return;
  const answers = Object.fromEntries(new FormData($<HTMLFormElement>("question-form")).entries());
  void action(async () => {
    const token = revision;
    turns.push({ role: "user", content: question.questions.map(item => `${item.question}\n${item.isSecret ? "Respuesta privada enviada." : answers[item.id]}`).join("\n\n") }); persist();
    const reply = await invoke<ChatView>("answer_codex_question", { id: chat.id, questionId: question.id, answers });
    if (token !== revision) return;
    apply({ ...state, chat: reply });
  });
};
$("approve").onclick = () => void action(async () => {
  if (state.job?.snapshot.phase !== "awaiting_approval" || !state.job.approval_id) return;
  const token = revision, jobId = state.job.snapshot.input.job_id, events = new Channel<Job>();
  resultReview = { revision: token, project: JSON.stringify(state.project), provider, program, jobId };
  events.onmessage = job => { if (token === revision && state.job?.snapshot.input.job_id === jobId) apply({ ...state, job }); };
  let job: Job;
  try { job = await invoke<Job>("approve_operation", { approvalId: state.job.approval_id, events }); }
  catch (e) { resultReview = undefined; throw e; }
  if (token === revision && state.job?.snapshot.input.job_id === jobId) apply({ ...state, job });
});
for (const id of ["cancel-job", "cancel-running"]) $(id).onclick = () => void action(async () => { resultReview = undefined; if (state.job) apply({ ...state, job: await invoke<Job>("cancel_job", { jobId: state.job.snapshot.input.job_id }) }); });
void action(async () => apply(await invoke<DesktopSnapshot>("get_snapshot")));
setInterval(() => {
  const token = revision;
  if (!commandBusy && (state.chat?.busy || state.job?.busy || state.codex_login?.busy)) void invoke<DesktopSnapshot>("get_snapshot").then(next => { if (token === revision) apply(next); }).catch(error);
}, 1000);
