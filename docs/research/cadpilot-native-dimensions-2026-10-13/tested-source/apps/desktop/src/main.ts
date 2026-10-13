import { invoke, Channel } from "@tauri-apps/api/core";
import "./style.css";

import type { Dimensions, Phase, Snapshot, Job, Inference, DesktopSnapshot, DrawingInput, KicadInput, CheckSummary, SliceInput, CadpilotInput, CadpilotRequest, DiscoveredSchematic, BridgeConfig, ChatView, CodexLoginView, KonnectInput, KonnectPcbInput, DiscoveredPcb } from "./contracts";
import { units, convertDimensions, dimensionText, conversationPrompt, savedConversation, agentPrompt, solidProjection, guidedBlink, guidedSchematic, schematicKind, type Unit, type Turn } from "./design";

document.querySelector<HTMLDivElement>("#app")!.innerHTML = `
  <header><span class="brand-mark" aria-hidden="true">⬡</span><div><h1>Hazlo Mexa</h1><p>De una idea a un archivo que puedes editar.</p></div><span class="badge">CAD y Arduino · v0.1</span></header>
  <section class="project-bar" aria-labelledby="project-heading"><div><h2 id="project-heading">Tu proyecto</h2><p id="project-path">Selecciona una carpeta local para guardar tus archivos.</p></div><div class="actions"><button id="create-project">Crear en carpeta…</button><button id="open-project" class="secondary">Abrir proyecto…</button></div></section>
  <details id="agent-tools-panel" class="panel"><summary>Usar las herramientas de Hazlo con Claude Code o Codex</summary><p>Permite que el agente consulte las opciones disponibles, prepare una pieza o código Arduino, y lea el sketch, esquema o PCB que elegiste. Apruebas las propuestas aquí. El agente puede enviar el contexto elegido a su servicio de IA.</p><div class="actions"><button id="enable-agent-tools" type="button">Permitir conexión al proyecto</button><button id="disable-agent-tools" class="secondary" type="button" disabled>Revocar conexión</button></div><p id="agent-tools-status" role="status">Conexión desactivada.</p><details id="agent-tools-config" hidden><summary>Configuración de la conexión temporal</summary><p>Configura este servidor MCP en tu agente. La conexión termina al cerrar Hazlo, revocarla o cambiar de proyecto. Ofrece capacidades, propuesta de pieza rectangular, compilación Arduino Uno, lectura y edición del sketch seleccionado, inspección y valor de componentes, lectura y movimiento de componentes PCB, cambios CADPilot sobre documentos medidos, conversión y círculos CADCraft sobre dibujos verificados, nuevo laminado OrcaSlicer con modelo y perfiles verificados, revisión y exportación KiCad de resultados guardados, resultados verificados y estado. Claude Code también está disponible en el selector de conversación.</p><p>Claude Code · configuración MCP</p><pre id="claude-tools-config"></pre><p>Codex · configuración TOML</p><pre id="codex-tools-config"></pre></details></details>
  <div class="workspace">
    <section class="panel" aria-labelledby="proposal-heading"><span class="eyebrow">01 / CONVERSA</span><h2 id="proposal-heading">¿Qué quieres crear?</h2><p id="workflow-note" class="muted">Cuéntame tu idea o describe la pieza que quieres recrear. Podemos reunir las medidas paso a paso.</p>
      <label for="workflow">¿Qué vas a trabajar?</label><select id="workflow"><option value="freecad">Pieza rectangular · FreeCAD</option><option value="arduino">Sketch y compilación · Arduino</option><option value="agent">Editar copia del sketch · Arduino Agent</option><option value="drawing">Dibujo y conversión · CADCraft</option><option value="konnect">Editar esquema · Konnect</option><option value="pcb">Mover componente · PCB</option><option value="kicad">Revisión y exportación · KiCad</option><option value="cadpilot">Documento existente · CADPilot</option><option value="slice">Laminado a archivos · OrcaSlicer</option></select>
      <label for="assistant-provider">¿Con quién quieres conversar?</label><select id="assistant-provider"><option value="guided">Guía actual / Ollama</option><option value="claude">Claude Code instalado</option><option value="codex">Codex instalado</option></select><div id="claude-controls" hidden><button id="select-claude" type="button" class="secondary">Conectar Claude Code instalado…</button><p id="claude-runtime" class="fine"></p><p class="fine">Usa tu inicio de sesión del cliente Claude Code. Al enviar, tu mensaje y el contexto seleccionado pueden ir a su servicio de IA y consumir tu plan. Hazlo conserva la aprobación de las propuestas.</p></div><div id="codex-controls" hidden><button id="select-codex" type="button" class="secondary">Elegir Codex instalado…</button><p id="codex-runtime" class="fine"></p><div class="actions"><button id="connect-codex" type="button">Conectar mi cuenta</button><button id="open-codex-login" type="button" class="secondary" disabled>Abrir inicio de sesión</button><button id="cancel-codex-login" type="button" class="secondary" disabled>Detener conexión</button></div><p id="codex-login-status" role="status" aria-live="polite"></p><p class="fine">Conecta tu cuenta en el navegador; Codex gestiona el inicio de sesión para Hazlo. Al enviar, tu mensaje y el contexto seleccionado pueden ir a su servicio de IA y consumir tu plan. Las propuestas requieren tu aprobación aquí.</p></div><p id="claude-chat-progress" role="status" aria-live="polite"></p>
      <form id="chat-question-form" hidden><div id="chat-question-fields"></div><button id="answer-chat-question" type="submit">Continuar con mis respuestas</button><p class="fine">Tu respuesta aporta información. Revisarás cualquier cambio antes de aprobarlo.</p></form>
      <form id="inference-form"><p id="conversation-storage" class="fine" role="status"></p><div id="conversation" role="log" aria-label="Conversación de diseño" aria-live="polite"></div><div id="model-controls"><label for="model-profile">Modelo en Ollama</label><select id="model-profile"><option value="local_experimental">Qwen3 4B local · experimental</option><option value="cloud">Kimi K2.7 Code · cloud</option></select><p id="model-note" class="fine"></p></div><label for="prompt">Tu idea o respuesta</label><textarea id="prompt" rows="3" maxlength="4096" placeholder="Quiero recrear una pieza rectangular, pero todavía no sé todas sus medidas" required></textarea><div class="actions"><button id="infer" type="submit">Enviar</button><button id="cancel-inference" type="button" class="secondary" disabled>Detener</button><button id="new-conversation" type="button" class="secondary">Nueva conversación</button></div><p id="inference-status" role="status" aria-live="polite"></p><details id="model-response"><summary>Respuesta técnica del modelo</summary><pre id="inference-text"></pre></details><p id="conversation-note" class="fine">Requiere Ollama abierto y el modelo instalado. Esta versión genera sólidos rectangulares; no interpreta fotos ni archivos de referencia.</p></form>
      <form id="cadpilot-form" hidden>
        <p id="cadpilot-path" class="fine"></p><button id="select-pilot-freecad" type="button" class="secondary">Seleccionar FreeCAD 1.1.4…</button>
        <p id="cadpilot-base" class="fine"></p><button id="cadpilot-new-source" type="button" class="secondary" hidden>Elegir otro documento</button>
        <label for="cadpilot-operation">¿Qué quieres hacer con el documento?</label><select id="cadpilot-operation"><option value="inspect">Medir e inspeccionar primero</option><option value="edit_box">Cambiar dimensiones de una caja</option><option value="add_box">Añadir una caja</option><option value="add_cylinder">Añadir un cilindro</option></select>
        <label id="cadpilot-object-choice" for="cadpilot-component" hidden>¿Qué caja quieres cambiar?<select id="cadpilot-component"><option value="">Elige una caja medida</option></select></label>
        <label id="cadpilot-object-label" for="cadpilot-object">Nombre del objeto nuevo o identificador técnico<input id="cadpilot-object" placeholder="Para editar sin conocer el nombre, mide primero y elige Continuar desde este resultado" pattern="[A-Za-z_][A-Za-z0-9_]{0,63}" maxlength="64" required></label>
        <label for="cadpilot-unit">Unidad de los parámetros nuevos</label><select id="cadpilot-unit"><option value="mm">Milímetros</option><option value="cm">Centímetros</option><option value="m">Metros</option><option value="in">Pulgadas</option><option value="ft">Pies</option></select>
        <fieldset id="cadpilot-size"><legend>Medidas que tendrá la copia</legend><label>Largo<input id="cadpilot-length" type="number" step="any" required></label><label>Ancho<input id="cadpilot-width" type="number" step="any" required></label><label>Alto<input id="cadpilot-height" type="number" step="any" required></label><label id="cadpilot-diameter-label">Diámetro · ancho completo del cilindro<input id="cadpilot-diameter" type="number" min="0" step="any" required></label></fieldset>
        <fieldset id="cadpilot-position"><legend>Posición del objeto nuevo desde el origen del documento</legend><label>Horizontal (X)<input id="cadpilot-x" type="number" step="any" required></label><label>Profundidad (Y)<input id="cadpilot-y" type="number" step="any" required></label><label>Elevación (Z)<input id="cadpilot-z" type="number" step="any" required></label><p class="fine">Para una caja, indica la esquina de inicio; para un cilindro, el centro de su base circular. Si no conoces el origen o las medidas, inspecciona primero y confirma dónde debe quedar la pieza.</p></fieldset>
        <p class="fine">El documento nativo conserva sus medidas en mm. La inspección reúne el catálogo y medidas antes de aprobar; el original se conserva. Por ahora se aceptan hasta 32 primitivas nativas independientes, con visibilidad validada limitada, sin expresiones ni documentos vinculados.</p>
        <button id="prepare-cadpilot" type="submit">Seleccionar documento e inspeccionar propuesta…</button>
      </form>
      <form id="drawing-form" hidden>
        <p id="cadcraft-path" class="fine"></p><button id="select-cadcraft" type="button" class="secondary">Seleccionar CADCraft CLI 0.5.0…</button>
        <label for="drawing-format">Archivo de salida</label><select id="drawing-format"><option value="dxf">DXF editable</option><option value="dwg">DWG editable</option><option value="png">PNG · vista del modelo</option></select>
        <label for="drawing-unit">Unidades del dibujo original</label><select id="drawing-unit"><option value="mm">Milímetros · métrico</option><option value="in">Pulgadas · inglés</option></select><p class="fine">Se conserva la escala original. Si el archivo no declara unidades, confirma cómo deben interpretarse sus coordenadas.</p>
        <label><input id="drawing-add-circle" type="checkbox">Añadir un círculo a la copia</label>
        <fieldset id="drawing-circle"><legend>¿Dónde y de qué tamaño quieres el círculo?</legend><label for="drawing-circle-unit">Unidad de tus medidas<select id="drawing-circle-unit"><option value="mm">Milímetros</option><option value="in">Pulgadas</option></select></label><label for="drawing-x">Posición horizontal del centro (X)<input id="drawing-x" type="number" step="any"></label><label for="drawing-y">Posición vertical del centro (Y)<input id="drawing-y" type="number" step="any"></label><p class="fine">Las posiciones se miden desde el origen del dibujo. Si no sabes dónde está, pide inspeccionar el dibujo antes de elegirlas.</p><label for="drawing-diameter">Diámetro · ancho completo del círculo<input id="drawing-diameter" type="number" min="0" step="any"></label><p class="fine">Estas medidas se convierten a las unidades del dibujo original; cambiar su unidad no cambia el tamaño del círculo.</p></fieldset>
        <label><input id="drawing-loss" type="checkbox">Acepto que CADCraft omita objetos incompatibles de la copia o de la imagen</label><p class="fine">Sin esta aceptación, una advertencia de pérdida impide publicar. El original importado siempre se conserva.</p>
        <button id="prepare-drawing" type="submit">Seleccionar original y preparar propuesta…</button>
      </form>
      <form id="konnect-form" hidden>
        <p id="konnect-path" class="fine"></p><button id="select-konnect" type="button" class="secondary">Seleccionar Konnect 0.13.0…</button><button id="select-konnect-kicad" type="button" class="secondary">Seleccionar KiCad CLI 10.0.7…</button>
        <button id="discover-schematic" type="button">Abrir esquema y ver qué contiene…</button><p id="schematic-discovery" class="fine" role="status"></p>
        <label for="schematic-component">¿Qué componente quieres cambiar?</label><select id="schematic-component" disabled><option value="">Primero abre un esquema</option></select>
        <details><summary>Identificador técnico del componente</summary><label for="konnect-reference">Referencia<input id="konnect-reference" placeholder="Se completa al elegir un componente" pattern="[A-Za-z0-9_]{1,64}" maxlength="64" required></label><p class="fine">R1 y R2 son identificadores del dibujo, no medidas. Puedes elegir el componente de la lista sin memorizarlos.</p></details>
        <label for="konnect-value">¿Qué valor quieres que tenga?<input id="konnect-value" placeholder="Escribe el valor que necesitas" maxlength="1024" required></label>
        <p class="fine">El valor actual se obtiene del esquema seleccionado. Revisa el cambio antes de aprobar. El original se conserva; se publica una copia editada.</p>
        <button id="prepare-konnect" type="submit">Revisar el cambio…</button>
      </form>
      <form id="pcb-form" hidden>
        <p id="pcb-runtime" class="fine"></p><button id="select-pcb-konnect" type="button" class="secondary">Seleccionar Konnect 0.13.0…</button><button id="select-pcb-kicad" type="button" class="secondary">Seleccionar KiCad CLI 10.0.7…</button>
        <button id="discover-pcb" type="button">Abrir PCB y ver sus componentes…</button><p id="pcb-discovery" class="fine" role="status"></p>
        <label for="pcb-component">¿Qué componente quieres mover?</label><select id="pcb-component" required disabled><option value="">Primero abre una PCB</option></select>
        <label for="pcb-unit">Unidades de la posición y del informe</label><select id="pcb-unit"><option value="mm">Milímetros · métrico</option><option value="in">Pulgadas · inglés</option><option value="mils">Milésimas de pulgada · mils</option></select>
        <p id="pcb-current" role="status"></p><fieldset><legend>Posición nueva desde el origen de la PCB</legend><label for="pcb-x">Horizontal · X<input id="pcb-x" type="number" step="any" required></label><label for="pcb-y">Vertical · Y<input id="pcb-y" type="number" step="any" required></label></fieldset>
        <p class="fine">Leemos la PCB guardada; guarda primero los cambios abiertos en KiCad. Si aún no sabes dónde colocarlo, describe lo que necesitas en el chat. La propuesta requiere una posición confirmada. El original se conserva; mover el componente puede desconectar pistas. KiCad revisará y guardará una copia antes de publicar.</p>
        <button id="prepare-pcb" type="submit">Revisar el movimiento…</button>
      </form>
      <form id="kicad-form" hidden>
        <p id="kicad-path" class="fine"></p><button id="select-kicad" type="button" class="secondary">Seleccionar KiCad CLI 10.0.7…</button>
        <label for="kicad-unit">Unidades de las coordenadas del informe</label><select id="kicad-unit"><option value="mm">Milímetros · métrico</option><option value="in">Pulgadas · inglés</option><option value="mils">Milésimas de pulgada · mils</option></select>
        <p class="fine">Selecciona la carpeta que contiene el proyecto y sus dependencias locales, después la hoja principal o PCB. Se conserva el original. PDF y netlist para esquemas; Gerber y taladros en milímetros para PCB.</p>
        <button id="prepare-kicad" type="submit">Seleccionar proyecto y preparar revisión…</button>
      </form>
      <form id="slice-form" hidden>
        <p id="orca-path" class="fine"></p><button id="select-orca" type="button" class="secondary">Seleccionar OrcaSlicer 2.4.2…</button>
        <label for="slice-unit">Unidades del STL original</label><select id="slice-unit"><optgroup label="Métrico"><option value="mm">Milímetros</option><option value="cm">Centímetros</option><option value="m">Metros</option></optgroup><optgroup label="Inglés / imperial"><option value="in">Pulgadas</option><option value="ft">Pies</option></optgroup></select>
        <label for="slice-bed">Superficie de la cama</label><select id="slice-bed"><option value="textured">Textured PEI Plate</option><option value="cool">Cool Plate</option><option value="smooth">High Temp Plate</option></select>
        <label for="slice-bed-temperature">Temperatura inicial de cama · °C</label><input id="slice-bed-temperature" type="number" min="0" max="200" step="any" placeholder="Confirma para tu cama y material" required>
        <p class="fine">Seleccionaremos el STL, la carpeta de perfiles con machine/process/filament y un JSON de cada tipo. Se capturan sus herencias. Confirma la máquina, boquilla, material y temperatura antes de aprobar.</p>
        <button id="prepare-slice" type="submit">Seleccionar modelo y perfiles…</button>
      </form>
      <form id="arduino-form" hidden>
        <div id="agent-setup" hidden><p id="agent-path" class="fine"></p><button id="connect-agent" type="button" class="secondary">Conectar Arduino Agent 0.7.1 local</button><button id="select-agent-sketch" type="button" class="secondary">Seleccionar sketch original…</button><p id="agent-source-name"></p><details><summary>Original importado · solo lectura</summary><pre id="agent-original"></pre></details></div>
        <label for="arduino-board">Placa y núcleo</label><select id="arduino-board"><option value="arduino:avr:uno">Arduino Uno · AVR 1.8.8</option></select><label for="sketch-source">Código del sketch</label><textarea id="sketch-source" rows="10" maxlength="65536" spellcheck="false" placeholder="void setup() { }&#10;void loop() { }"></textarea><p id="sketch-note" class="fine">Hasta 64 KiB. Se guarda Sketch.ino y se verifica la compilación. No se carga firmware a una placa.</p><button id="prepare-compile" type="submit">Preparar compilación</button><details id="arduino-runtime"><summary>Arduino CLI 1.5.1</summary><p id="arduino-path"></p><button id="select-arduino" type="button" class="secondary">Seleccionar CLI y toolchain privado…</button></details>
      </form>
      <details id="manual-dimensions"><summary>Ya sé las medidas / ajustar propuesta</summary><form id="dimensions-form"><label for="unit">Unidad de las medidas</label><select id="unit"><optgroup label="Sistema métrico"><option value="mm">Milímetros (mm)</option><option value="cm">Centímetros (cm)</option><option value="m">Metros (m)</option></optgroup><optgroup label="Sistema inglés / imperial"><option value="in">Pulgadas (in)</option><option value="ft">Pies (ft)</option></optgroup></select><fieldset id="dimensions"><legend id="dimension-legend">Medidas en mm</legend><label>Largo<input id="length" type="number" min="1" max="200" step="any" required /></label><label>Ancho<input id="width" type="number" min="1" max="200" step="any" required /></label><label>Alto<input id="height" type="number" min="1" max="200" step="any" required /></label></fieldset><p id="dimension-range" class="fine">Cada medida debe equivaler a 1–200 mm.</p><button id="prepare" type="submit">Preparar propuesta</button></form></details>
      <details id="runtime"><summary>FreeCAD 1.1.4</summary><p id="runtime-path"></p><button id="select-freecad" class="secondary">Seleccionar FreeCADCmd.exe…</button></details>
    </section>
    <section class="panel preview" aria-labelledby="preview-heading"><span class="eyebrow">02 / REVISA</span><h2 id="preview-heading">Vista de la propuesta</h2><p id="review-note" class="muted">Confirma estas medidas antes de generar archivos.</p>
      <svg viewBox="0 0 420 310" role="img" aria-labelledby="solid-title"><title id="solid-title">Medidas de la pieza por confirmar</title><path class="grid" d="M30 240H390 M50 265H370 M75 290H345 M105 210L65 300 M175 210L160 300 M245 210L260 300 M315 210L355 300"/><path class="top" d="M110 115 220 55 330 115 220 175Z"/><path class="left" d="M110 115 220 175 220 265 110 205Z"/><path class="right" d="M220 175 330 115 330 205 220 265Z"/><text id="length-label" x="125" y="235">Largo por confirmar</text><text id="width-label" x="245" y="250">Ancho por confirmar</text><text id="height-label" x="340" y="175">Alto por confirmar</text></svg>
      <div id="operation-preview" class="operation">Todavía no hay una operación pendiente.</div><pre id="code-preview" hidden></pre>
      <div class="actions"><button id="approve" disabled>Aprobar y generar</button><button id="cancel" class="secondary" disabled>Cancelar</button></div>
      <p id="approval-note" class="fine">La aprobación genera un archivo FCStd editable y su copia STEP. No se reemplazan resultados anteriores.</p>
    </section>
  </div>
  <section class="panel results" aria-labelledby="results-heading"><span class="eyebrow">03 / ABRE</span><h2 id="results-heading">Resultados</h2><p id="status" role="status" aria-live="polite">Listo para seleccionar un proyecto.</p><dl class="workflow-results"><div><dt>Generación</dt><dd id="generation-result"></dd></div><div><dt>Verificación</dt><dd id="validation-result"></dd></div><div><dt>Publicación</dt><dd id="publication-result"></dd></div></dl><p id="opening-status" role="status" aria-live="polite" hidden></p><div id="artifacts"></div><div id="history"></div></section>
  <p id="error" class="error" role="alert" hidden></p>
  <dialog id="artifact-preview" aria-labelledby="artifact-preview-heading"><h2 id="artifact-preview-heading">Sketch verificado · solo lectura</h2><pre id="artifact-source"></pre><form method="dialog"><button>Cerrar</button></form></dialog>
`;

const element = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
let state: DesktopSnapshot = { project: null, job: null, inference: null, recovered: [], freecad: "", freecad_available: false };
let commandBusy = false;
let dirty = false;
let projectRevision = 0;
let channel: Channel<Job> | undefined;
let inferenceChannel: Channel<Inference> | undefined;
let inferenceRevision = 0;
let selectedUnit: Unit = "mm";
let turns: Turn[] = [];
let answeredInference: string | undefined;
let schematic: DiscoveredSchematic | undefined;
let pcb:DiscoveredPcb|undefined;
let pcbUnit="mm";
const pcbFactors:Record<string,number>={mm:1,in:25.4,mils:0.0254};
let drawingCircleUnit:Unit="mm";
let bridgeConfig: BridgeConfig | undefined;
let assistantProvider="guided";
let chatChannel:Channel<ChatView>|undefined;
let codexLoginChannel:Channel<CodexLoginView>|undefined;
let codexLoginRevision=0;
let answeredChat:string|undefined;
let newChatSession=false;
let conversationProject: string | undefined;
let renderedConversation = "";
let selectedWorkflow: "freecad" | "arduino" | "agent" | "drawing" | "kicad" | "slice" | "konnect" | "cadpilot" | "pcb" = "freecad";
let pilotBase: {project_id:string;job_id:string;sha256:string;report:{catalog:Record<string,string>;after:CadpilotInput["preview"]["measurements"];operation:string;object:string;parameters_mm:Record<string,number>}} | undefined;
let pilotFormUnit:Unit="mm";

function resetPilotSelection() {
  pilotBase=undefined;
  element<HTMLSelectElement>("cadpilot-operation").value="inspect";
  element<HTMLSelectElement>("cadpilot-component").replaceChildren(new Option("Elige una caja medida",""));
  for(const key of ["object","length","width","height","diameter","x","y","z"])element<HTMLInputElement>(`cadpilot-${key}`).value="";
}

function pilotObjectChanged() {
  const name=element<HTMLSelectElement>("cadpilot-component").value;
  element<HTMLInputElement>("cadpilot-object").value=name;
  // Native dimensions or earlier explicit edit parameters; rotated bounds are not box dimensions.
  for (const key of ["length","width","height"]) {
    const value=pilotBase?.report.after[name]?.dimensions_mm?.[key as keyof Dimensions] ?? (pilotBase?.report.operation==="edit_box" && pilotBase.report.object===name ? pilotBase.report.parameters_mm[key] : undefined);
    element<HTMLInputElement>(`cadpilot-${key}`).value=value===undefined ? "" : String(value/units[element<HTMLSelectElement>("cadpilot-unit").value as Unit].factor);
  }
}

function renderConversation() {
  const messages: Turn[] = turns.length ? turns : [{ role: "assistant", content: selectedWorkflow === "agent" ? "¿Qué sketch quieres recrear o modificar y qué debe cambiar? Conecta Arduino Agent, selecciona el original y revisa el código de la copia antes de aprobar. El original se conserva."
    : selectedWorkflow === "arduino"
    ? "¿Qué quieres que haga tu Arduino y qué placa tienes? Esta integración verifica Arduino Uno. Puedo preparar un ejemplo para parpadear el LED integrado, o puedes pegar tu propio sketch."
    : "¿Qué quieres construir o recrear, y para qué lo usarás? Si no sabes las medidas, empezamos por describir la forma. Puedo generar un sólido rectangular en esta versión." }];
  if (!turns.length && selectedWorkflow === "drawing") messages[0].content = "¿Quieres convertir el dibujo o añadir un círculo? Elige DXF, DWG o PNG y las unidades originales, milímetros o pulgadas. Luego seleccionaremos el original y revisaremos la propuesta antes de aprobar.";
  if (!turns.length && selectedWorkflow === "pcb")messages[0].content="¿Qué quieres cambiar en tu placa? Abre la PCB y veremos sus componentes y posiciones. Puedes elegir por valor y ubicación sin conocer sus identificadores. Después confirmaremos dónde moverlo antes de aprobar.";
  if (!turns.length && selectedWorkflow === "konnect") messages[0].content="¿Qué quieres cambiar en tu circuito? Abre el esquema y veremos qué componentes contiene y sus valores actuales. No necesitas saber sus identificadores. Elegiremos el componente y revisaremos el cambio antes de aprobar.";
  if (!turns.length && selectedWorkflow === "kicad") messages[0].content = "¿Quieres revisar un esquema o una PCB, y qué archivos necesitas? Reúne la hoja principal, su proyecto y las dependencias locales en una carpeta. Elige las unidades del informe; revisaremos los archivos capturados antes de aprobar.";
  if (!turns.length && selectedWorkflow === "slice") messages[0].content = "¿Qué modelo quieres preparar y para qué máquina, boquilla y material? Confirma las unidades del STL, la superficie de la cama y su temperatura inicial. Reuniremos los perfiles y revisaremos las condiciones antes de crear 3MF y G-code.";
  if (!turns.length && selectedWorkflow === "cadpilot") messages[0].content = pilotBase ? "Ya leí el resultado guardado y verificado. Elige la caja por sus medidas; puedes cambiar una medida y conservar las demás según sus dimensiones nativas medidas. Si un informe anterior no las incluye, inspecciona de nuevo el documento. Revisaremos el cambio antes de crear otra copia." : "¿Quieres medir un documento, modificar una caja o añadir geometría? Si aún no sabes sus medidas o nombres internos, empieza por Medir e inspeccionar. Seleccionaremos una copia del FCStd y reuniremos su catálogo antes de aprobar; el original se conserva.";
  const serialized = JSON.stringify(messages);
  if (serialized === renderedConversation) return;
  renderedConversation = serialized;
  element("conversation").replaceChildren();
  for (const turn of messages) {
    const row = document.createElement("p"); row.className = `chat-message ${turn.role}`;
    const speaker = document.createElement("strong"); speaker.textContent = turn.role === "user" ? "Tú: " : "Hazlo Mexa: ";
    row.append(speaker, document.createTextNode(turn.content)); element("conversation").append(row);
  }
  element("conversation").scrollTop = element("conversation").scrollHeight;
}

function syncConversation() {
  const scope = state.project ? JSON.stringify([state.project[0],state.project[1],assistantProvider!=="guided" ? assistantProvider : selectedWorkflow==="freecad" ? "guided" : `guided:${selectedWorkflow}`]) : undefined;
  if (conversationProject !== scope) {
    conversationProject = scope; turns = []; answeredInference = undefined; answeredChat = undefined; newChatSession=false;
    if (conversationProject) {
      try {
        const saved = savedConversation(localStorage.getItem(`design-chat:${conversationProject}`));
        if (saved) { turns=saved.turns; answeredInference=saved.answeredInference; answeredChat=saved.answeredChat; newChatSession=saved.newSession ?? false; }
      } catch { /* Local storage may be unavailable; the conversation still works in memory. */ }
    }
  }
  const inference = state.inference;
  const chat=state.chat;
  if(assistantProvider!=="guided" && chat?.provider===assistantProvider && chat.project_id===state.project?.[0] && !chat.busy && answeredChat!==chat.id){
    answeredChat=chat.id;turns.push({role:"assistant",content:chat.cancelled ? "Solicitud detenida. Puedes aclarar tu idea y continuar." : chat.error ?? chat.text});
  }

  if (assistantProvider==="guided" && selectedWorkflow === "freecad" && inference && !inference.busy && answeredInference !== inference.id) {
    answeredInference = inference.id;
    const response = inference.cancelled ? "Solicitud detenida. Puedes corregir tu idea o empezar de nuevo."
      : inference.error ? "No pude interpretar la solicitud. Puedes volver a intentar o ingresar las medidas manualmente."
      : inference.message ?? (state.job?.snapshot.input.dimensions_mm ? `Tengo una propuesta de ${dimensionText(state.job.snapshot.input.dimensions_mm, selectedUnit)}. Revisa o ajusta las medidas y aprueba para generar los archivos.` : "No hay una propuesta disponible.");
    turns.push({ role: "assistant", content: response });
  }
  if (conversationProject) {
    try {
      const saved=JSON.stringify({version:1,turns,answeredInference,answeredChat,newSession:newChatSession});
      if (!savedConversation(saved)) throw new Error("history limit");
      localStorage.setItem(`design-chat:${conversationProject}`,saved);
      element("conversation-storage").textContent="La conversación se guarda en este equipo por proyecto y asistente. Nueva conversación borra esta historia; las propuestas siguen necesitando aprobación.";
    } catch { element("conversation-storage").textContent="No pude guardar toda la conversación. Sigue disponible durante esta sesión; empieza una nueva con un resumen si alcanzaste el límite."; }
  } else { element("conversation-storage").textContent=""; }

  renderConversation();
}

function setDimensionFields(d: Dimensions) {
  const display = convertDimensions(d, "mm", selectedUnit);
  for (const id of ["length", "width", "height"] as const) element<HTMLInputElement>(id).value = String(display[id]);
}
function renderUnits() {
  element("dimension-legend").textContent = `Medidas en ${selectedUnit}`;
  const missing=(["length","width","height"] as const).filter(id=>!element<HTMLInputElement>(id).value.trim()).map(id=>({length:"largo",width:"ancho",height:"alto"})[id]);
  element("dimension-range").textContent = `Cada medida debe equivaler a 1–200 mm. ${selectedUnit === "mm" ? "" : `1 ${selectedUnit} = ${units[selectedUnit].factor} mm.`} ${missing.length ? `Falta confirmar: ${missing.join(", ")}. Podemos reunirlas en el chat.` : !dimensionsConfirmed() ? "Revisa las medidas: deben estar dentro de ese intervalo." : "Medidas listas para preparar una propuesta."}`;
  for (const id of ["length", "width", "height"]) {
    element<HTMLInputElement>(id).min = String(convertDimensions({ length: 1, width: 1, height: 1 }, "mm", selectedUnit).length);
    element<HTMLInputElement>(id).max = String(convertDimensions({ length: 200, width: 200, height: 200 }, "mm", selectedUnit).length);
  }
}
const phaseText: Record<Phase, string> = { awaiting_approval: "Propuesta lista. Revisa las medidas y aprueba para continuar.", running: "Generando y verificando con FreeCAD…", completed: "Archivos verificados y publicados.", cancelled: "Operación cancelada.", failed: "La generación falló. Revisa el detalle y prepara otra propuesta.", interrupted: "La operación se interrumpió. Prepara y aprueba una propuesta nueva." };
function runStatus(snapshot: Snapshot): string {
  if (snapshot.input.konnect_version) {
    if (snapshot.phase === "running") return snapshot.input.pcb_move ? "Konnect está moviendo el componente y KiCad está revisando la PCB…" : "Konnect está editando y verificando la copia del esquema…";
    if (snapshot.phase === "awaiting_approval") return snapshot.input.pcb_move ? "Revisa la posición actual → nueva; mover el componente puede dejar pistas desconectadas." : "Revisa el componente y el valor actual → nuevo antes de aprobar.";
    if (snapshot.phase === "failed" && snapshot.audit?.files_verified) return "Copia verificada. Falló la publicación; se conservan los archivos de trabajo.";
    return phaseText[snapshot.phase];
  }
  if (snapshot.phase === "failed" && snapshot.audit?.files_verified) return "Archivos generados y verificados. Falló la publicación; los archivos de trabajo se conservan.";
  if (snapshot.input.cadpilot_version) {
    if (snapshot.phase === "awaiting_approval") return "Revisa el catálogo medido, objeto, unidades y parámetros de la copia antes de aprobar.";
    if (snapshot.phase === "running") return "CADPilot está midiendo, editando y verificando la copia privada…";
    if (snapshot.phase === "completed") return "Mediciones y archivos verificados y publicados. El original se conserva.";
    return phaseText[snapshot.phase];
  }
  if (snapshot.input.orca_version) {
    if (snapshot.phase === "awaiting_approval") return "Revisa unidades, modelo, perfiles, cama y temperaturas antes de aprobar el laminado.";
    if (snapshot.phase === "running") return "OrcaSlicer está laminando y verificando archivos en una copia privada…";
    if (snapshot.phase === "completed") return "3MF y G-code verificados y publicados. Revisa el informe y la calibración de tu máquina antes de usar esos archivos.";
    if (snapshot.phase === "failed") return "El laminado o su verificación falló. El original se conserva; revisa el detalle.";
    return phaseText[snapshot.phase];
  }
  if (snapshot.input.kicad_version) {
    if (snapshot.phase === "awaiting_approval") return "Revisa los archivos capturados y las unidades antes de aprobar ERC/DRC y exportación.";
    if (snapshot.phase === "running") return "KiCad está revisando y exportando una copia del proyecto…";
    if (snapshot.phase === "completed") return "Revisión y exportación publicadas. Consulta los hallazgos: completar la operación no implica que el diseño esté libre de errores.";
    if (snapshot.phase === "failed") return "La revisión o exportación falló. El original se conserva; revisa el detalle.";
    return phaseText[snapshot.phase];
  }
  if (snapshot.input.cadcraft_version) {
    if (snapshot.phase === "awaiting_approval") return "Revisa el original importado, las unidades y la operación de la copia antes de aprobar.";
    if (snapshot.phase === "running") return "CADCraft está creando y verificando la copia del dibujo…";
    if (snapshot.phase === "failed") return "La operación de dibujo falló. El original se conserva; revisa el detalle.";
    return phaseText[snapshot.phase];
  }
  if (!snapshot.input.dimensions_mm) {
    if (snapshot.input.agent_version) {
      if (snapshot.phase === "awaiting_approval") return "Revisa el original y el código de la copia antes de aprobar la edición.";
      if (snapshot.phase === "running") return "Editando y verificando una copia con Arduino Agent…";
      if (snapshot.phase === "failed") return "Agent no confirmó la edición. El original sigue intacto; revisa el detalle y la copia conservada.";
    }
    if (snapshot.phase === "awaiting_approval") return "Propuesta lista. Revisa el código y la placa y aprueba la compilación.";
    if (snapshot.phase === "running") return "Compilando y verificando el sketch…";
    if (snapshot.phase === "failed") return "La compilación falló. Revisa el detalle y prepara otra propuesta.";
  }
  return phaseText[snapshot.phase];
}

function dimensions(): Dimensions {
  return convertDimensions(Object.fromEntries(["length", "width", "height"].map(id => [id, Number(element<HTMLInputElement>(id).value)])) as Dimensions, selectedUnit, "mm");
}
function dimensionsConfirmed(): boolean {
  return ["length","width","height"].every(id=>element<HTMLInputElement>(id).value.trim()) && Object.values(dimensions()).every(value=>Number.isFinite(value) && value>=1 && value<=200);
}

function preview() {
  const projection = solidProjection(dimensions());
  const visible = !!state.job || element<HTMLDetailsElement>("manual-dimensions").open;
  document.querySelector<SVGElement>(".preview svg")!.style.display = selectedWorkflow === "freecad" && visible && projection ? "" : "none";
  if(!projection){element("solid-title").textContent="Medidas de la pieza por confirmar";return;}
  if (projection) {
    for (const face of ["top", "left", "right"] as const) document.querySelector(`.preview .${face}`)!.setAttribute("d", projection[face]);
    for (const id of ["length", "width", "height"] as const) {
      const location = projection[`${id}Label`];
      element(`${id}-label`).setAttribute("x", String(location[0]));
      element(`${id}-label`).setAttribute("y", String(location[1]));
      element(`${id}-label`).setAttribute("text-anchor", id === "height" ? "start" : "middle");
    }
  }
  const d = convertDimensions(dimensions(), "mm", selectedUnit);
  element("length-label").textContent = `Largo: ${Number(d.length.toPrecision(10))} ${selectedUnit}`;
  element("width-label").textContent = `Ancho: ${Number(d.width.toPrecision(10))} ${selectedUnit}`;
  element("height-label").textContent = `${Number(d.height.toPrecision(10))} ${selectedUnit}`;
  element("solid-title").textContent = `Sólido rectangular de ${d.length} por ${d.width} por ${d.height} ${selectedUnit}. Ilustración esquemática.`;
}

function drawingDescription(input: DrawingInput): string {
  const summary = input.original_summary;
  return `${input.source_name} · ${summary.entity_count} entidades · ${input.units === "mm" ? "milímetros" : "pulgadas"} · salida ${input.output_format.toUpperCase()}${input.circle ? ` · círculo con centro (${input.circle.center.join(", ")}), diámetro ${input.circle.radius * 2}` : " · conversión de copia"}${input.allow_loss ? " · pérdida aceptada" : " · sin aceptar pérdida"}`;
}
function kicadDescription(input: KicadInput): string {
  return `${input.entry} · ${input.files.length} archivos capturados · ${input.entry.endsWith(".kicad_sch") ? "ERC + PDF + netlist" : "DRC + Gerber + taladros"} · informe ${input.units} · KiCad ${input.kicad_version}`;
}
function sliceDescription(input:SliceInput):string {
  const d=input.mesh.bounds_mm[0].map((v,i)=>Number((input.mesh.bounds_mm[1][i]-v).toPrecision(7)));
  const s=input.settings;
  return `${input.source_name} · original ${input.units} → ${d.join(" × ")} mm · ${input.mesh.triangles} triángulos · ${s.machine} · ${s.process} · ${s.filament} · ${s.bed} · boquilla ${s.nozzle_mm} mm · capa ${s.layer_mm} mm · primera capa ${s.nozzle_c} °C / cama ${s.bed_c} °C`;
}
const pilotOperations:Record<CadpilotRequest["operation"],string>={inspect:"Medir documento",edit_box:"Editar caja",add_box:"Añadir caja",add_cylinder:"Añadir cilindro"};
function pilotDescription(input:CadpilotInput):string {
  const r=input.request;
  const labels:Record<string,string>={length:"largo",width:"ancho",height:"alto",radius:"diámetro",x:"horizontal (X)",y:"profundidad (Y)",z:"elevación (Z)"};
  const parameters=Object.entries(r.parameters).map(([key,value])=>{const measurement=key==="radius" ? value*2 : value;return `${labels[key] ?? key}: ${measurement} ${r.units} → ${Number((measurement*units[r.units].factor).toPrecision(8))} mm`;}).join(" · ");
  const count=Object.keys(input.preview.catalog).length;
  return `${input.source_name} · ${pilotOperations[r.operation]}${r.operation==="inspect" ? "" : ` · ${r.object}`} · ${count} ${count===1 ? "objeto medido" : "objetos medidos"}${parameters ? ` · ${parameters}` : ""}`;
}
function konnectDescription(input:KonnectInput|KonnectPcbInput):string {
  if("pcb_move" in input && input.pcb_move){const move=input.pcb_move;return `${input.source.entry} · ${move.reference}: (${move.expected_x_mm}, ${move.expected_y_mm}) → (${move.x_mm}, ${move.y_mm}) mm`;}
  const edit=input.edit!;return `${input.source.entry} · ${edit.reference}: ${edit.expected_value} → ${edit.value}`;
}
function artifactButtons(job: Snapshot, parent: HTMLElement) {
  const row = document.createElement("div"); row.className = "artifact-row";
  const label = document.createElement("span"); const d = job.input.dimensions_mm;
  const description = job.input.konnect_version ? konnectDescription(job.input) : job.input.cadpilot_version ? pilotDescription(job.input) : job.input.orca_version ? sliceDescription(job.input) : job.input.kicad_version ? kicadDescription(job.input) : job.input.cadcraft_version ? drawingDescription(job.input) : d ? `${dimensionText(d, selectedUnit)}${selectedUnit === "mm" ? "" : ` (${dimensionText(d, "mm")})`}` : job.input.agent_version ? `Copia · ${job.input.source_name} · Agent ${job.input.agent_version}` : `Sketch · ${job.input.fqbn}`;
  label.textContent = `${description}${historyText(job)}`; row.append(label);
  let formats = job.input.orca_version ? [["original_mesh","Mostrar STL original"],["sliced","Mostrar 3MF laminado"],["gcode","Mostrar G-code"],["slice_report","Ver informe de laminado"]] : job.input.kicad_version ? [["kicad_source", "Mostrar copia revisada"], ["kicad_outputs", "Mostrar exportaciones"], ["kicad_report", "Ver informe ERC/DRC"]] : job.input.cadcraft_version ? [["original_drawing", "Mostrar original"], ["drawing", "Mostrar resultado"], ["drawing_preview", "Mostrar PNG"], ["drawing_report", "Ver informe de conversión"]] : d ? [["fcstd", "Abrir FCStd"], ["step", "Abrir STEP"]] : job.input.agent_version ? [["original_sketch", "Ver original importado"], ["sketch", "Ver copia verificada"]] : [["sketch", "Ver sketch verificado"], ["hex", "Mostrar HEX"], ["elf", "Mostrar ELF"]];
  if (job.input.cadpilot_version) {
    formats=[["original_cad","Mostrar FCStd original"],["cadpilot_report","Ver mediciones y operación"]];
    if(job.input.request.operation!=="inspect") formats.push(["fcstd","Abrir FCStd editado"],["step","Abrir STEP editado"]);
  }
  if (job.input.konnect_version) {
    formats=[["konnect_original","Mostrar esquema original"],["konnect_edited","Mostrar copia editada"]];
    if(job.input.pcb_move)formats=[["konnect_original","Mostrar PCB original"],["konnect_edited","Mostrar PCB guardada y revisada"],["kicad_report","Ver errores y advertencias DRC"],["kicad_outputs","Mostrar exportaciones"]];
    if (job.input.schema_version===2) formats.push(["konnect_netlist","Mostrar netlist KiCad"]);
    formats.push(["konnect_report","Ver informe de edición"]);
  }
  for (const [format, text] of formats) {
    const button = document.createElement("button"); button.className = "secondary"; button.textContent = text;
    button.addEventListener("click", () => void action(async () => {
      const status = element("opening-status"); status.hidden = false; status.textContent = `Abriendo ${format.toUpperCase()} (${description})…`;
      try {
        const source = await invoke<string | null>("open_artifact", { jobId: job.input.job_id, format });
        if ((format === "sketch" || format === "original_sketch" || format.endsWith("_report")) && typeof source === "string") {
          element("artifact-preview-heading").textContent = format === "original_sketch" ? "Original importado · solo lectura" : "Sketch verificado · solo lectura";
          if (format === "drawing_report") element("artifact-preview-heading").textContent = "Informe de CADCraft · solo lectura";
          if (format === "kicad_report") element("artifact-preview-heading").textContent = "Informe ERC/DRC · solo lectura";
          if (format === "slice_report") element("artifact-preview-heading").textContent = "Informe de laminado · solo lectura";
          if (format === "cadpilot_report") element("artifact-preview-heading").textContent = "Mediciones y operación CADPilot · solo lectura";
          if (format === "konnect_report") element("artifact-preview-heading").textContent="Informe de edición Konnect · solo lectura";
          element("artifact-source").textContent = source;
          element<HTMLDialogElement>("artifact-preview").showModal();
          status.textContent = format.endsWith("_report") ? "Informe abierto en modo de solo lectura." : "Sketch verificado abierto en modo de solo lectura.";
        } else {
          status.textContent = d ? `Se inició FreeCAD para abrir ${format.toUpperCase()} (${description}).` : `Se solicitó abrir ${format.toUpperCase()} (${description}).`;
        }
      } catch (error) {
        status.textContent = d ? "No se pudo abrir en FreeCAD. Los archivos generados siguen disponibles." : "No se pudo abrir el archivo. Los resultados siguen guardados.";
        throw error;
      }
    }));
    row.append(button);
  }
  if (job.input.kicad_version || job.input.pcb_move) {
    const button = document.createElement("button"); button.className = "secondary"; button.textContent = "Ver hallazgos";
    button.addEventListener("click", () => void action(async () => {
      const summary = await invoke<CheckSummary>("read_kicad_summary", {jobId: job.input.job_id});
      element("artifact-preview-heading").textContent = `Hallazgos ${summary.check.toUpperCase()} · informe ${summary.units}`;
      element("artifact-source").textContent = `${summary.errors} errores\n${summary.warnings} advertencias\n${summary.excluded} hallazgos excluidos\n${summary.ignored_checks.length} comprobaciones desactivadas${summary.ignored_checks.length ? `\n${summary.ignored_checks.join("\n")}` : ""}\n\nRevisa el informe completo antes de usar las exportaciones.`;
      element<HTMLDialogElement>("artifact-preview").showModal();
    }));
    row.append(button);
  }
  if (job.input.cadpilot_version && job.phase==="completed") {
    const button=document.createElement("button");button.className="secondary";button.textContent="Continuar desde este resultado";
    button.disabled=commandBusy || !!state.job?.busy || state.job?.snapshot.phase==="awaiting_approval";
    button.addEventListener("click",()=>void action(async()=>{
      const revision=projectRevision;
      const result=await invoke<{project_id:string;job_id:string;sha256:string;format:string;content:string}>("read_cadpilot_result",{jobId:job.input.job_id});
      if(revision!==projectRevision || result.project_id!==state.project?.[0])throw new Error("El proyecto cambió. Selecciona de nuevo el resultado.");
      if(result.format!=="cadpilot_report")throw new Error("Este resultado no es un documento CADPilot.");
      resetPilotSelection();
      pilotBase={...result,report:JSON.parse(result.content)};
      const choices=element<HTMLSelectElement>("cadpilot-component");choices.replaceChildren(new Option("Elige una caja medida", ""));
      for(const [name,type] of Object.entries(pilotBase.report.catalog))if(type==="Part::Box"){
        const measured=pilotBase.report.after[name], b=measured.bbox, d=measured.dimensions_mm;
        const values=d ? [d.length,d.width,d.height] : [b.xmax-b.xmin,b.ymax-b.ymin,b.zmax-b.zmin];
        choices.add(new Option(`Caja · ${values.map(v=>Number(v.toPrecision(7))).join(" × ")} mm${d ? "" : " de extensión"} · ${name}`,name));
      }
      selectedWorkflow="cadpilot";element<HTMLSelectElement>("workflow").value=selectedWorkflow;
      element<HTMLSelectElement>("cadpilot-operation").value="edit_box";
      element<HTMLSelectElement>("cadpilot-unit").value="mm";
      pilotFormUnit="mm";
      pilotObjectChanged();render();element("cadpilot-form").scrollIntoView({block:"center"});
    }));row.append(button);
  }
  parent.append(row);
}

function historyText(job: Snapshot): string {
  const audit = job.audit;
  if (!audit) return " · Historial de una versión anterior";
  const source = audit.source.provider === "mcp" ? "Propuesta por herramienta · MCP" : audit.source.provider === "manual" ? (job.input.dimensions_mm ? "Medidas manuales" : job.input.konnect_version || job.input.cadpilot_version || job.input.orca_version || job.input.kicad_version || job.input.cadcraft_version ? "Archivos seleccionados" : "Código manual") : `${audit.source.model}${audit.source.cloud ? " · Cloud" : " · Local"}`;
  return ` · ${source}${audit.user_edited ? (job.input.dimensions_mm ? " · Medidas editadas" : " · Código editado") : ""} · ${new Date(audit.created_unix_ms).toLocaleString("es-MX")}`;
}

let shownQuestion:string|undefined;
function renderChatQuestion() {
  const pending=assistantProvider==="codex" && state.chat?.busy && !state.chat.cancelled ? state.chat.question : undefined;
  const form=element<HTMLFormElement>("chat-question-form");form.hidden=!pending;
  element<HTMLButtonElement>("answer-chat-question").disabled=commandBusy || !pending;
  if(shownQuestion===pending?.id)return;
  shownQuestion=pending?.id;
  const fields=element("chat-question-fields");fields.replaceChildren();
  for(const [index,question] of (pending?.questions ?? []).entries()) {
    const label=document.createElement("label");label.htmlFor=`chat-answer-${index}`;label.textContent=question.question;
    const input=question.options && !question.isOther ? document.createElement("select") : document.createElement("input");
    input.id=label.htmlFor;input.name=question.id;input.required=true;
    if(input instanceof HTMLSelectElement){
      input.add(new Option("Elige una respuesta", ""));
      for(const option of question.options ?? [])input.add(new Option(option.label,option.label));
    }else{
      input.maxLength=2048;
      if(question.options){
        const suggestions=document.createElement("datalist");suggestions.id=`chat-options-${index}`;
        for(const option of question.options)suggestions.append(new Option(option.label,option.label));
        input.setAttribute("list",suggestions.id);fields.append(suggestions);
      }
    }
    fields.append(label,input);
    if(question.options){const help=document.createElement("p");help.className="fine";help.textContent=question.options.map(option=>`${option.label}: ${option.description}`).join(" · ");fields.append(help);}
  }
}
element<HTMLFormElement>("chat-question-form").addEventListener("submit",event=>{
  event.preventDefault();const chat=state.chat;const question=chat?.question;
  if(!chat || !question || !chat.busy || chat.cancelled || commandBusy)return;
  const answers=Object.fromEntries(new FormData(element<HTMLFormElement>("chat-question-form")).entries());
  void action(async()=>{
    const view=await invoke<ChatView>("answer_codex_question",{id:chat.id,questionId:question.id,answers});
    if(state.chat?.id===view.id)turns=[...turns,{role:"user",content:question.questions.map(question=>`${question.question}\n${answers[question.id]}`).join("\n\n")}];
    if(state.chat?.id===view.id && state.chat.sequence<=view.sequence)state.chat=view;
  });
});

function render() {
  renderChatQuestion();
  const claudeChat=assistantProvider==="claude";
  const codexChat=assistantProvider==="codex";
  const agentChat=claudeChat || codexChat;
  const login=state.codex_login;
  const loginBusy=!!login?.busy;
  element("claude-controls").hidden=!claudeChat;
  element("codex-controls").hidden=!codexChat;
  element("codex-runtime").textContent=state.codex ? `Codex ${state.codex.version} · ${state.codex.executable}` : "Elige el cliente instalado antes de conectar tu cuenta.";
  element<HTMLButtonElement>("select-codex").disabled=commandBusy || !!state.chat?.busy || loginBusy || !!state.inference?.busy;
  element<HTMLButtonElement>("connect-codex").disabled=commandBusy || !state.codex || !!state.chat?.busy || loginBusy || !!state.inference?.busy;
  element<HTMLButtonElement>("open-codex-login").disabled=commandBusy || !loginBusy || !login?.auth_url || !!login.cancelled;
  element<HTMLButtonElement>("cancel-codex-login").disabled=commandBusy || !loginBusy || !!login?.cancelled;
  element("codex-login-status").textContent=login?.error ?? (loginBusy ? login?.cancelled ? "Deteniendo conexión…" : login?.auth_url ? "Abre el inicio de sesión y termina la conexión en el navegador." : "Comprobando la conexión con Codex…" : login?.connected ? "Cuenta conectada. Ya puedes conversar con Codex." : login?.cancelled ? "Conexión detenida. Puedes volver a conectar." : "Conecta tu cuenta para conversar.");
  element<HTMLButtonElement>("select-claude").disabled=commandBusy || !!state.chat?.busy || loginBusy;
  element<HTMLSelectElement>("assistant-provider").disabled=commandBusy || !!state.chat?.busy || !!state.inference?.busy || loginBusy;
  element("claude-runtime").textContent=state.claude ? `Claude Code ${state.claude.version} · ${state.claude.executable}` : "Selecciona el cliente instalado; su inicio de sesión se gestiona en Claude Code.";
  element("claude-chat-progress").textContent=agentChat && state.chat?.busy ? `${state.chat.cancelled ? "Deteniendo…" : "Conversando…"}${state.chat.text ? ` ${state.chat.text}` : ""}` : "";
  element<HTMLButtonElement>("enable-agent-tools").disabled=commandBusy || !state.project;
  element<HTMLButtonElement>("enable-agent-tools").textContent=state.agent_tools_enabled ? "Ver configuración de la conexión" : "Permitir conexión al proyecto";
  element<HTMLButtonElement>("disable-agent-tools").disabled=commandBusy || !state.agent_tools_enabled;
  element("agent-tools-status").textContent=state.agent_tools_enabled ? "Herramientas disponibles para agentes. Los cambios siguen pendientes de tu aprobación aquí." : "Conexión desactivada.";
  const job = state.job;
  const active = job?.snapshot.phase === "awaiting_approval";
  const busy = commandBusy || !!job?.busy || !!state.inference?.busy || !!state.chat?.busy || loginBusy;
  const inference = state.inference;
  const arduino = selectedWorkflow === "arduino";
  const agent = selectedWorkflow === "agent";
  const drawing = selectedWorkflow === "drawing";
  const kicad = selectedWorkflow === "kicad";
  const konnect = selectedWorkflow === "konnect";
  const pcbWorkflow=selectedWorkflow==="pcb";
  const slice = selectedWorkflow === "slice";
  const cadpilot = selectedWorkflow === "cadpilot";
  element("pcb-form").hidden=!pcbWorkflow;
  element("pcb-runtime").textContent=`${state.konnect ?? "Selecciona Konnect 0.13.0"} · ${state.kicad ?? "Selecciona KiCad CLI 10.0.7"}`;
  for(const id of ["select-pcb-konnect","select-pcb-kicad"])element<HTMLButtonElement>(id).disabled=busy || !!active;
  element<HTMLButtonElement>("discover-pcb").disabled=busy || !!active || !state.project;
  element<HTMLSelectElement>("pcb-component").disabled=busy || !!active || !pcb || pcb.project_id!==state.project?.[0];
  for(const id of ["pcb-unit","pcb-x","pcb-y"])element<HTMLInputElement>(id).disabled=busy || !!active;
  element<HTMLButtonElement>("prepare-pcb").disabled=busy || !!active || !pcb || !state.konnect || !state.kicad || !element<HTMLSelectElement>("pcb-component").value;
  renderPcbPosition();
  element("konnect-form").hidden=!konnect;
  element("konnect-path").textContent=`${state.konnect ?? "Selecciona Konnect 0.13.0"} · ${state.kicad ?? "Selecciona KiCad CLI 10.0.7"}`;
  for (const id of ["select-konnect","select-konnect-kicad"]) element<HTMLButtonElement>(id).disabled=busy || !!active;
  for (const id of ["konnect-reference","konnect-value"]) element<HTMLInputElement>(id).disabled=busy || !!active;
  element<HTMLButtonElement>("discover-schematic").disabled=busy || !!active || !state.project;
  element<HTMLSelectElement>("schematic-component").disabled=busy || !!active || !schematic || schematic.project_id !== state.project?.[0];
  element<HTMLButtonElement>("prepare-konnect").disabled=busy || !!active || !state.project || !state.konnect || !state.kicad;
  const software = arduino || agent;
  element("workflow-note").textContent = agent ? "Describe el cambio que buscas. Selecciona el sketch original y prepara el código de su copia." : arduino ? "Describe qué debe hacer tu placa. Reúne el código y revisa la compilación antes de aprobarla." : "Cuéntame tu idea o describe la pieza que quieres recrear. Podemos reunir las medidas paso a paso.";
  element("review-note").textContent = agent ? "Compara el original importado con el código que tendrá la copia." : arduino ? "Revisa la placa y todo el código antes de compilar." : "Confirma estas medidas antes de generar archivos.";
  element("approve").textContent = agent ? "Aprobar edición de copia" : arduino ? "Aprobar y compilar" : "Aprobar y generar";
  element<HTMLTextAreaElement>("prompt").placeholder = agent ? "Quiero modificar este sketch y conservar el original" : arduino ? "Quiero un ejemplo para parpadear el LED integrado de un Arduino Uno" : "Quiero recrear una pieza rectangular, pero todavía no sé todas sus medidas";
  element<HTMLSelectElement>("workflow").disabled = busy || !!active;
  element("arduino-form").hidden = !software;
  element("manual-dimensions").hidden = software || drawing || kicad || slice || cadpilot;
  element("runtime").hidden = software || drawing || kicad || slice || cadpilot;
  element("model-controls").hidden = software || drawing || kicad || slice || cadpilot;
  element("model-response").hidden = software || drawing || kicad || slice || cadpilot;
  element("slice-form").hidden = !slice;
  element("cadpilot-form").hidden = !cadpilot;
  element("cadpilot-path").textContent=state.freecad_available ? state.freecad : "Selecciona FreeCADCmd.exe de FreeCAD 1.1.4.";
  element<HTMLButtonElement>("select-pilot-freecad").disabled=busy || !!active;
  element<HTMLButtonElement>("prepare-cadpilot").disabled=busy || !!active || !state.project || !state.freecad_available;
  const pilotOperation=element<HTMLSelectElement>("cadpilot-operation").value;
  const inspecting=pilotOperation==="inspect", adding=pilotOperation.startsWith("add_"), cylinder=pilotOperation==="add_cylinder";
  const choosing=!!pilotBase && pilotOperation==="edit_box";
  element("cadpilot-base").textContent=pilotBase ? "Cambiarás una copia de este resultado verificado. Sus archivos se volverán a comprobar antes de preparar la propuesta." : "";
  element("cadpilot-new-source").hidden=!pilotBase;
  element<HTMLButtonElement>("cadpilot-new-source").disabled=busy || !!active;
  element("cadpilot-object-choice").hidden=!choosing;
  element<HTMLSelectElement>("cadpilot-component").disabled=busy || !!active || !choosing;
  element<HTMLSelectElement>("cadpilot-component").required=choosing;
  element("cadpilot-object-label").hidden=choosing || inspecting;
  element("prepare-cadpilot").textContent=pilotBase ? "Preparar cambio sobre este resultado" : "Seleccionar documento e inspeccionar propuesta…";
  for (const id of ["cadpilot-operation","cadpilot-unit"]) element<HTMLSelectElement>(id).disabled=busy || !!active;
  element<HTMLInputElement>("cadpilot-object").disabled=busy || !!active || inspecting || choosing;
  element<HTMLFieldSetElement>("cadpilot-size").disabled=busy || !!active || inspecting;
  element("cadpilot-size").hidden=inspecting;
  for (const id of ["cadpilot-length","cadpilot-width"]) {element<HTMLInputElement>(id).disabled=busy || !!active || inspecting || cylinder;element<HTMLInputElement>(id).parentElement!.hidden=cylinder;}
  element<HTMLInputElement>("cadpilot-diameter").disabled=busy || !!active || !cylinder;
  element("cadpilot-diameter-label").hidden=!cylinder;
  element<HTMLFieldSetElement>("cadpilot-position").disabled=busy || !!active || !adding;
  element("cadpilot-position").hidden=!adding;
  element("orca-path").textContent = state.orca ?? "Selecciona orca-slicer.exe del paquete oficial Windows x64 2.4.2.";
  element<HTMLButtonElement>("select-orca").disabled = busy || !!active;
  element<HTMLButtonElement>("prepare-slice").disabled = busy || !!active || !state.project || !state.orca;
  for (const id of ["slice-unit","slice-bed","slice-bed-temperature"]) element<HTMLInputElement>(id).disabled = busy || !!active;
  element("kicad-form").hidden = !kicad;
  element("kicad-path").textContent = state.kicad ?? "Selecciona kicad-cli.exe del paquete oficial Windows x64 10.0.7.";
  element<HTMLButtonElement>("select-kicad").disabled = busy || !!active;
  element<HTMLButtonElement>("prepare-kicad").disabled = busy || !!active || !state.project || !state.kicad;
  element<HTMLSelectElement>("kicad-unit").disabled = busy || !!active;
  element("drawing-form").hidden = !drawing;
  element("cadcraft-path").textContent = state.cadcraft ?? "Selecciona el CLI oficial CADCraft Windows x64 0.5.0.";
  element<HTMLButtonElement>("select-cadcraft").disabled = busy || !!active;
  element<HTMLButtonElement>("prepare-drawing").disabled = busy || !!active || !state.project || !state.cadcraft;
  element<HTMLFieldSetElement>("drawing-circle").disabled = busy || !!active || !element<HTMLInputElement>("drawing-add-circle").checked;
  for (const id of ["drawing-format", "drawing-unit", "drawing-add-circle", "drawing-loss"]) element<HTMLInputElement>(id).disabled = busy || !!active;
  element("agent-setup").hidden = !agent;
  element("arduino-runtime").hidden = agent;
  element("arduino-board").hidden = agent;
  document.querySelector<HTMLElement>('label[for="arduino-board"]')!.hidden = agent;
  element<HTMLButtonElement>("connect-agent").disabled = busy;
  element<HTMLButtonElement>("select-agent-sketch").disabled = busy || !!active || !state.project;
  element("agent-path").textContent = state.agent ? `Arduino Agent ${state.agent.version} conectado · ${state.agent.protocol_version}` : "Abre Arduino Agent 0.7.1 y conecta su servidor local autenticado.";
  element("agent-source-name").textContent = state.agent_source?.source_name ?? "Selecciona un original. Agent solo editará una copia temporal nueva.";
  element("agent-original").textContent = job?.snapshot.input.original_source ?? state.agent_source?.source ?? "";
  element("sketch-note").textContent = agent ? "Hasta 64 KiB. Se conservan el original importado y la copia editada. Esta operación no compila ni carga firmware." : "Hasta 64 KiB. Se guarda Sketch.ino y se verifica la compilación. No se carga firmware a una placa.";
  element<HTMLTextAreaElement>("sketch-source").disabled = busy || !state.project;
  element<HTMLSelectElement>("arduino-board").disabled = busy || !state.project;
  element<HTMLButtonElement>("prepare-compile").disabled = busy || !state.project || (agent && !state.agent_source) || (!!active && !dirty);
  element<HTMLButtonElement>("prepare-compile").textContent = active ? "Actualizar propuesta" : agent ? "Preparar edición de copia" : "Preparar compilación";
  element<HTMLButtonElement>("select-arduino").disabled = busy;
  element("arduino-path").textContent = state.arduino ? `${state.arduino.executable} · ${state.arduino.data}` : "Selecciona Arduino CLI 1.5.1 y una carpeta data privada preparada para Hazlo Mexa con AVR 1.8.8.";
  element("conversation-note").textContent = agent ? "La conversación reúne tu intención. Ajusta el código de la copia y aprueba sus cambios; Agent no modifica el original ni ejecuta herramientas de hardware." : arduino ? "Confirmamos Arduino Uno, el LED integrado y su intervalo antes de escribir el ejemplo. Para otras funciones, usa Claude Code o Codex o pega y revisa tu código. Esta compilación no usa un modelo ni carga firmware." : "Requiere Ollama abierto y el modelo instalado. Esta versión genera sólidos rectangulares; no interpreta fotos ni archivos de referencia.";
  syncConversation();
  renderUnits();
  element<HTMLButtonElement>("new-conversation").disabled = busy || !!active;
  element<HTMLSelectElement>("unit").disabled = busy || !state.project;
  element<HTMLButtonElement>("infer").disabled = busy || !!active || !state.project;
  element<HTMLButtonElement>("cancel-inference").disabled = software || drawing || kicad || slice || cadpilot || commandBusy || !inference?.busy || !!inference.cancelled;
  element<HTMLTextAreaElement>("prompt").disabled = busy;
  element<HTMLSelectElement>("model-profile").disabled = busy;
  element("model-note").textContent = element<HTMLSelectElement>("model-profile").value === "cloud"
    ? "Esta opción envía toda esta conversación a Ollama Cloud. Requiere acceso a tu cuenta. El modelo remoto puede cambiar."
    : "Procesa en tu equipo. Este modelo todavía falla en algunas solicitudes ambiguas; revisa las medidas o usa la entrada manual.";
  element("inference-status").textContent = inference?.busy ? (inference.cancelled ? "Cancelando solicitud…" : "Interpretando tu solicitud…") : inference?.message ?? (inference?.error ? "No se preparó una propuesta. Corrige la solicitud o usa las medidas manuales." : inference ? "Solicitud terminada. Revisa la propuesta antes de aprobar." : "");
  element("inference-text").textContent = inference?.text ?? "";
  element("project-path").textContent = state.project?.[1] ?? "Selecciona una carpeta local para guardar tus archivos.";
  element("runtime-path").textContent = state.freecad_available ? state.freecad : "FreeCADCmd.exe no está disponible. Selecciona el archivo del paquete FreeCAD 1.1.4.";
  element<HTMLFieldSetElement>("dimensions").disabled = busy || !state.project;
  element<HTMLButtonElement>("prepare").disabled = busy || !state.project || !dimensionsConfirmed() || (!!active && !dirty);
  element<HTMLButtonElement>("prepare").textContent = active ? "Actualizar propuesta" : "Preparar propuesta";
  element<HTMLButtonElement>("approve").disabled = commandBusy || !!job?.busy || !!state.inference?.busy || loginBusy || !active || dirty || !job?.approval_id || (job.snapshot.input.konnect_version ? (!state.konnect || !state.kicad) : job.snapshot.input.cadpilot_version ? !state.freecad_available : job.snapshot.input.orca_version ? !state.orca : job.snapshot.input.kicad_version ? !state.kicad : job.snapshot.input.cadcraft_version ? !state.cadcraft : job.snapshot.input.dimensions_mm ? !state.freecad_available : job.snapshot.input.agent_version ? !state.agent : !state.arduino);
  element<HTMLButtonElement>("cancel").disabled = commandBusy || !(active || job?.busy);
  for (const id of ["create-project", "open-project", "select-freecad"]) element<HTMLButtonElement>(id).disabled = busy;
  const d = job?.snapshot.input.dimensions_mm;
  element("operation-preview").textContent = d ? `${dimensionText(d, selectedUnit)}${selectedUnit === "mm" ? "" : ` (${dimensionText(d, "mm")})`} · FreeCAD 1.1.4 · FCStd + STEP${dirty ? " · Cambios sin aplicar" : ""}` : job?.snapshot.input.agent_version ? `${job.snapshot.input.source_name} · Arduino Agent ${job.snapshot.input.agent_version} · Solo copia${dirty ? " · Cambios sin aplicar" : ""}` : job ? `${job.snapshot.input.fqbn} · Arduino CLI 1.5.1 · AVR 1.8.8 · Sketch + HEX + ELF${dirty ? " · Cambios sin aplicar" : ""}` : "Todavía no hay una operación pendiente.";
  element("code-preview").hidden = !job || !!d;
  element("code-preview").textContent = typeof job?.snapshot.input.source === "string" ? job.snapshot.input.source : "";
  if (drawing) {
    element("workflow-note").textContent = "¿Qué dibujo quieres recrear, convertir o modificar? Elige formato y unidades; el original se conserva.";
    element<HTMLTextAreaElement>("prompt").placeholder = "Quiero convertir este dibujo o añadir un círculo; todavía necesito confirmar sus unidades";
    element("review-note").textContent = "Revisa las entidades importadas, unidades y cambios de la copia. Para cambiar la propuesta, cancélala y prepara otra.";
    element("approve").textContent = "Aprobar operación de dibujo";
    element("conversation-note").textContent = "La conversación reúne tu intención. La operación admite DXF/DWG, conversión y adición de un círculo. Usa los campos para confirmar los parámetros.";
    element("code-preview").hidden = true;
    element("operation-preview").textContent = job?.snapshot.input.cadcraft_version ? drawingDescription(job.snapshot.input) : "Selecciona el original para inspeccionarlo y preparar una propuesta.";
  }
  element("approval-note").textContent = agent ? "La aprobación escribe y abre una copia temporal en Agent. Solo se publican el original importado y el código verificado. Cancelar detiene la espera; Agent puede terminar de modificar la copia temporal." : arduino ? "La aprobación compila exactamente el código mostrado y guarda Sketch.ino, HEX y ELF. No carga firmware ni reemplaza resultados anteriores." : "La aprobación genera un archivo FCStd editable y su copia STEP. No se reemplazan resultados anteriores.";
  if (drawing) element("approval-note").textContent = "La aprobación trabaja en una copia privada y publica el original importado, resultado, PNG e informe. CADCraft puede omitir propiedades u objetos incompatibles; revisa el informe antes de usar la copia.";
  if (kicad) {
    const input = job?.snapshot.input.kicad_version ? job.snapshot.input : undefined;
    element("workflow-note").textContent = "Describe el circuito y qué necesitas revisar. Seleccionaremos sus archivos y confirmaremos la operación antes de trabajar en una copia.";
    element<HTMLTextAreaElement>("prompt").placeholder = "Quiero revisar este esquema o PCB y obtener sus archivos de intercambio";
    element("conversation-note").textContent = "La conversación reúne tu intención. Este flujo realiza ERC/DRC y exporta archivos; confirma la carpeta, hoja principal o PCB y unidades con los campos.";
    element("review-note").textContent = "Revisa la lista de archivos capturados. Para cambiar el archivo o las unidades, cancela y prepara otra propuesta.";
    element("approve").textContent = "Aprobar revisión y exportación";
    element("operation-preview").textContent = input ? kicadDescription(input) : "Selecciona un proyecto para capturar sus archivos y preparar ERC/DRC.";
    element("code-preview").hidden = !input;
    element("code-preview").textContent = input?.files.map(file => `${file.path} · ${file.bytes} bytes`).join("\n") ?? "";
    element("approval-note").textContent = "KiCad revisa una copia privada y rellena las zonas de esa PCB. Se publican original, copia revisada, informe y exportaciones sin reemplazar resultados. Los errores de diseño se conservan en el informe y requieren revisión.";
  }
  if (slice) {
    const input=job?.snapshot.input.orca_version ? job.snapshot.input : undefined;
    element("workflow-note").textContent="Describe el modelo y las condiciones de tu máquina. Reúne el STL y perfiles de máquina, proceso y material.";
    element<HTMLTextAreaElement>("prompt").placeholder="Quiero preparar este modelo; necesito confirmar sus unidades, perfiles, cama y temperaturas";
    element("conversation-note").textContent="La conversación reúne tu intención. Confirma los campos y selecciona los perfiles JSON; este flujo convierte STL a milímetros y lamina a archivos.";
    element("review-note").textContent="Revisa el tamaño convertido, perfiles, boquilla, capa, cama y temperaturas. Para cambiar parámetros, cancela y prepara otra propuesta.";
    element("approve").textContent="Aprobar laminado a archivos";
    element("operation-preview").textContent=input ? sliceDescription(input) : "Confirma unidades y cama, después selecciona el modelo y sus perfiles.";
    element("code-preview").hidden=!input;
    element("code-preview").textContent=input?.files.map(f=>`${f.path} · ${f.bytes} bytes`).join("\n") ?? "";
    element("approval-note").textContent="Se conservan STL y perfiles originales. OrcaSlicer puede acomodar el modelo en la cama; se verifican dimensiones, perfiles, unidades y G-code antes de publicar. Revisa la calibración de tu máquina antes de usar el resultado.";
  }
  if (cadpilot) {
    const input=job?.snapshot.input.cadpilot_version ? job.snapshot.input : undefined;
    element("workflow-note").textContent="Describe el documento que quieres medir, modificar o ampliar. Podemos reunir primero su catálogo y medidas.";
    element<HTMLTextAreaElement>("prompt").placeholder="Quiero recrear o modificar este documento, pero aún no conozco sus medidas";
    element("conversation-note").textContent="La inspección mide una copia del documento nativo; no necesitas conocer sus dimensiones para empezar. Confirma después los parámetros y unidades de la operación.";
    element("review-note").textContent="Compara las medidas actuales con los parámetros de la copia. Para cambiar la propuesta, cancélala y prepara otra.";
    element("approve").textContent=input?.request.operation==="inspect" ? "Aprobar informe de medición" : "Aprobar operación sobre copia";
    element("operation-preview").textContent=input ? pilotDescription(input) : "Empieza por medir el documento, o confirma el objeto y parámetros que quieres modificar.";
    element("code-preview").hidden=!input;
    element("code-preview").textContent=input ? Object.entries(input.preview.measurements).map(([name,report])=>{
      const bbox=report.bbox;
      const dimensions=[bbox.xmax-bbox.xmin,bbox.ymax-bbox.ymin,bbox.zmax-bbox.zmin];
      const display=dimensions.map(v=>Number((v/units[input.request.units].factor).toPrecision(7)));
      const volume=Number((report.volume_mm3/units[input.request.units].factor**3).toPrecision(7));
      return `${name} · ${input.preview.catalog[name]} · ${dimensions.join(" × ")} mm${input.request.units==="mm" ? "" : ` · ${display.join(" × ")} ${input.request.units}`} · volumen ${report.volume_mm3} mm³${input.request.units==="mm" ? "" : ` · ${volume} ${input.request.units}³`}`;
    }).join("\n") : "";
    element("approval-note").textContent="La operación se liga al original capturado, catálogo y geometría medidos. Se revalidan antes de editar; el original y los objetos no seleccionados se conservan.";
  }
  if (konnect) {
    const input=job?.snapshot.input.konnect_version ? job.snapshot.input : undefined;
    element("manual-dimensions").hidden=true;element("runtime").hidden=true;element("model-controls").hidden=true;element("model-response").hidden=true;
    element("workflow-note").textContent="Describe el componente que quieres modificar en una copia del esquema.";
    element<HTMLTextAreaElement>("prompt").placeholder="Quiero cambiar una resistencia, pero no sé cómo se llama en el dibujo";
    element("conversation-note").textContent="Abre el esquema y pide el cambio por su tipo y valor, por ejemplo «cambia la resistencia de 10k a 22k». Si hay varias, elige una en la lista. Claude Code o Codex pueden ayudarte con peticiones abiertas usando las herramientas de Hazlo. Confirmas y apruebas la propuesta aquí; Hazlo vuelve a leer la copia guardada por KiCad.";
    element("review-note").textContent="Revisa el valor actual y el nuevo. Para cambiar la propuesta, cancélala y prepara otra.";
    element("approve").textContent=input?.pcb_move ? "Aprobar movimiento y revisión de la PCB" : "Aprobar edición del esquema";
    element("operation-preview").textContent=input ? konnectDescription(input) : "Abre el esquema para descubrir sus componentes y elegir qué cambiar.";
    element("code-preview").hidden=!input;element("code-preview").textContent=input?.source.files.map(f=>`${f.path} · ${f.bytes} bytes`).join("\n") ?? "";
    element("approval-note").textContent=input?.pcb_move ? "Se conserva el original. Konnect mueve solo este componente; KiCad rellena zonas, guarda una copia de revisión y genera DRC y exportaciones. Consulta los errores antes de fabricar." : "Se conserva el original y se verifica que solo cambie el valor aprobado. No se controla hardware.";
  }
  if(pcbWorkflow){
    const input=job?.snapshot.input.pcb_move ? job.snapshot.input : undefined;
    element("manual-dimensions").hidden=true;element("runtime").hidden=true;element("model-controls").hidden=true;element("model-response").hidden=true;
    element("workflow-note").textContent="Describe qué necesitas reorganizar en tu placa. Primero veremos sus componentes y sus posiciones actuales.";
    element<HTMLTextAreaElement>("prompt").placeholder="Quiero mover un componente, pero no sé su identificador ni su posición";
    element("conversation-note").textContent="La lista reúne valores, huellas y ubicaciones medidas. Claude Code o Codex pueden consultar la PCB elegida y preparar un movimiento con destino y unidades confirmados. Apruebas el cambio aquí; Hazlo vuelve a leer la copia guardada por KiCad antes del siguiente cambio. Guarda primero los cambios de la PCB abierta.";
    element("review-note").textContent="Confirma el componente y la posición actual → nueva. Para cambiarlos, cancela y prepara otra propuesta.";
    element("approve").textContent="Aprobar movimiento y revisión de la PCB";
    element("operation-preview").textContent=input ? konnectDescription(input) : "Abre la PCB para elegir un componente sin conocer su identificador.";
    element("code-preview").hidden=!input;element("code-preview").textContent=input?.source.files.map(file=>`${file.path} · ${file.bytes} bytes`).join("\n") ?? "";
    element("approval-note").textContent="Se conserva el original. Konnect mueve solo este componente; KiCad rellena zonas, guarda una copia de revisión y genera DRC y exportaciones. Consulta los errores antes de fabricar.";
  }
  const previousOperation = !!job && !active && !job.busy;
  element("preview-heading").textContent=previousOperation ? "Última operación del proyecto" : "Vista de la propuesta";
  element("approval-note").hidden=!active && !job?.busy;
  if(previousOperation){
    element("review-note").textContent="Esta es una operación anterior. Sus archivos y detalles están en Resultados. Prepara una nueva propuesta con los datos confirmados para revisar y aprobar otro cambio.";
    element("operation-preview").textContent=runStatus(job.snapshot);
    element("code-preview").hidden=true;
  }
  element("status").textContent = job ? runStatus(job.snapshot) : state.project ? "Describe tu idea en el chat o ingresa las medidas manualmente." : "Crea o abre un proyecto para comenzar la conversación.";
  const completed = job?.snapshot.phase === "completed";
  const verified = completed || !!job?.snapshot.audit?.files_verified;
  element("generation-result").textContent = verified ? "Completada" : job?.busy ? "En curso" : job?.snapshot.phase === "failed" ? "Falló; revisa el detalle" : "Sin archivos nuevos";
  element("validation-result").textContent = verified ? (d ? "FCStd y STEP verificados" : job?.snapshot.input.agent_version ? "Original y copia del sketch verificados" : "Sketch, HEX y ELF verificados") : "Sin artefactos nuevos aceptados";
  if (verified && job?.snapshot.input.cadcraft_version) element("validation-result").textContent = "Identidades y archivos del dibujo verificados; consulta el informe";
  if (verified && job?.snapshot.input.kicad_version) element("validation-result").textContent = "Archivos e informe verificados; consulta los errores, advertencias, exclusiones y comprobaciones desactivadas";
  if (verified && job?.snapshot.input.orca_version) element("validation-result").textContent = "3MF, dimensiones, perfiles, unidades y G-code verificados; consulta el informe";
  if (verified && job?.snapshot.input.cadpilot_version) element("validation-result").textContent="Catálogo y geometría medidos, originales y archivos de la copia verificados";
  if (verified && job?.snapshot.input.konnect_version) element("validation-result").textContent=job.snapshot.input.pcb_move ? "Movimiento, PCB guardada y exportaciones verificados; consulta errores, advertencias y comprobaciones desactivadas DRC" : job.snapshot.input.schema_version===2 ? "Original conservado, esquema completo y valor leído por KiCad verificados" : "Original conservado, cambio de valor y copia completa verificados · versión anterior";
  element("publication-result").textContent = completed ? "Archivos disponibles en el proyecto" : verified && job?.snapshot.phase === "failed" ? "Falló; archivos de trabajo conservados" : "Sin publicación nueva";
  element("artifacts").replaceChildren();
  if (job?.snapshot.phase === "completed") artifactButtons(job.snapshot, element("artifacts"));
  element("history").replaceChildren();
  for (const previous of state.recovered) {
    if (previous.input.job_id === job?.snapshot.input.job_id) continue;
    if (previous.phase === "completed") artifactButtons(previous, element("history"));
    else { const row = document.createElement("p"); row.textContent = `${runStatus(previous)}${historyText(previous)}${previous.audit?.error ? ` · ${previous.audit.error}` : ""}`; element("history").append(row); }
  }
  preview();
  if(previousOperation)document.querySelector<SVGElement>(".preview svg")!.style.display="none";
  if(agentChat){
    element("model-controls").hidden=true;element("model-response").hidden=true;
    element("conversation-note").textContent=`${codexChat ? "Codex" : "Claude"} reúne tu intención y puede preparar piezas rectangulares, código Arduino Uno, cambios CADPilot, dibujos CADCraft, esquemas y movimientos PCB, revisiones KiCad y nuevos laminados OrcaSlicer usando el contexto elegido y resultados verificados. Revisa cada propuesta aquí antes de aprobar; estas operaciones trabajan con copias guardadas.`;
    element<HTMLButtonElement>("infer").disabled=commandBusy || !!state.chat?.busy || !state.project || (codexChat ? !state.codex || !login?.connected : !state.claude) || !!state.inference?.busy || loginBusy;
    element<HTMLButtonElement>("cancel-inference").disabled=commandBusy || !state.chat?.busy || !!state.chat.cancelled;
  }
}

function showError(message: string) { element("error").textContent = message; element("error").hidden = false; }
function applyJob(job: Job) {
  if (state.job && state.job.snapshot.input.job_id === job.snapshot.input.job_id && state.job.sequence > job.sequence) return;
  const changed = state.job?.snapshot.input.job_id !== job.snapshot.input.job_id || state.job?.sequence !== job.sequence;
  if (changed && job.error) showError(job.error);
  state.job = job; render();
}
function applySnapshot(next: DesktopSnapshot) {
  const previous = state;
  if(next.codex_login && previous.codex_login && next.codex_login.sequence<previous.codex_login.sequence)next.codex_login=previous.codex_login;
  if(next.chat && previous.chat && next.chat.id===previous.chat.id && next.chat.sequence<previous.chat.sequence)return;
  if(!next.agent_tools_enabled){bridgeConfig=undefined;element("agent-tools-config").hidden=true;element("claude-tools-config").textContent="";element("codex-tools-config").textContent="";}
  if (next.job && previous.job && next.job.snapshot.input.job_id === previous.job.snapshot.input.job_id && next.job.sequence < previous.job.sequence) return;
  if (next.inference && previous.inference && next.inference.id === previous.inference.id && next.inference.sequence < previous.inference.sequence) return;
  if(JSON.stringify(next.project)!==JSON.stringify(previous.project)) {
    for(const id of ["length","width","height"])element<HTMLInputElement>(id).value="";
    resetPilotSelection();
    newChatSession=false;answeredChat=undefined;
    bridgeConfig=undefined;
    element("agent-tools-config").hidden=true;
    element("claude-tools-config").textContent="";element("codex-tools-config").textContent="";
    pcb=undefined;element<HTMLSelectElement>("pcb-component").replaceChildren(new Option("Primero abre una PCB",""));element("pcb-discovery").textContent="";for(const id of ["pcb-x","pcb-y"])element<HTMLInputElement>(id).value="";
    schematic=undefined;
    element<HTMLSelectElement>("schematic-component").replaceChildren(new Option("Primero abre un esquema", ""));
    element("schematic-discovery").textContent="";
    element<HTMLInputElement>("konnect-reference").value="";
    element("opening-status").hidden = true;
    element("opening-status").textContent = "";
    element<HTMLTextAreaElement>("sketch-source").value = "";
  }
  if (next.job && (!state.job || next.job.snapshot.input.job_id !== state.job.snapshot.input.job_id)) {
    if (next.job.snapshot.input.dimensions_mm) {
      setDimensionFields(next.job.snapshot.input.dimensions_mm);
      element<HTMLDetailsElement>("manual-dimensions").open = true;
      selectedWorkflow = "freecad";
    } else if (next.job.snapshot.input.konnect_version) {
      selectedWorkflow=next.job.snapshot.input.pcb_move ? "pcb" : "konnect";
      element<HTMLInputElement>("konnect-reference").value=next.job.snapshot.input.pcb_move?.reference ?? next.job.snapshot.input.edit!.reference;
      element<HTMLInputElement>("konnect-value").value=next.job.snapshot.input.edit?.value ?? "";
    } else if (next.job.snapshot.input.cadcraft_version) {
      selectedWorkflow = "drawing";
    } else if (next.job.snapshot.input.kicad_version) {
      selectedWorkflow = "kicad";
      element<HTMLSelectElement>("kicad-unit").value = next.job.snapshot.input.units;
    } else if (next.job.snapshot.input.cadpilot_version) {
      selectedWorkflow="cadpilot";
      const request=next.job.snapshot.input.request;
      element<HTMLSelectElement>("cadpilot-operation").value=request.operation;
      element<HTMLSelectElement>("cadpilot-unit").value=request.units;
      pilotFormUnit=request.units;
      element<HTMLInputElement>("cadpilot-object").value=request.object;
      for (const [key,value] of Object.entries(request.parameters)) element<HTMLInputElement>(`cadpilot-${key==="radius" ? "diameter" : key}`).value=String(key==="radius" ? value*2 : value);
    } else if (next.job.snapshot.input.orca_version) {
      selectedWorkflow="slice";
      element<HTMLSelectElement>("slice-unit").value=next.job.snapshot.input.units;
      element<HTMLSelectElement>("slice-bed").value=next.job.snapshot.input.bed;
      element<HTMLInputElement>("slice-bed-temperature").value=String(next.job.snapshot.input.settings.bed_c);
    } else {
      element<HTMLTextAreaElement>("sketch-source").value = typeof next.job.snapshot.input.source === "string" ? next.job.snapshot.input.source : "";
      selectedWorkflow = next.job.snapshot.input.agent_version ? "agent" : "arduino";
    }
    element<HTMLSelectElement>("workflow").value = selectedWorkflow;
    dirty = false;
  }
  state = next;
  if (next.job?.error && (previous.job?.snapshot.input.job_id !== next.job.snapshot.input.job_id || previous.job?.sequence !== next.job.sequence)) showError(next.job.error);
  if (next.inference?.error && (previous.inference?.id !== next.inference.id || previous.inference?.sequence !== next.inference.sequence)) showError(next.inference.error);
  render();
}
function resetInference() { state.inference = null; }
element("assistant-provider").addEventListener("change",()=>{assistantProvider=element<HTMLSelectElement>("assistant-provider").value;turns=[];answeredChat=state.chat?.id;newChatSession=false;renderedConversation="";render();});
element("select-claude").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_claude"));}));

element("select-codex").addEventListener("click",()=>void action(async()=>{projectRevision++;codexLoginRevision++;applySnapshot(await invoke("select_codex"));}));
element("connect-codex").addEventListener("click",()=>void action(async()=>{
  const revision=++codexLoginRevision;
  codexLoginChannel=new Channel<CodexLoginView>();
  codexLoginChannel.onmessage=view=>{if(revision!==codexLoginRevision)return;
    if(state.codex_login && state.codex_login.sequence>view.sequence)return;
    state.codex_login=view;render();
  };
  applySnapshot(await invoke("connect_codex",{events:codexLoginChannel}));
}));
element("open-codex-login").addEventListener("click",()=>void action(async()=>{if(state.codex_login)await invoke("open_codex_login",{id:state.codex_login.id});}));
element("cancel-codex-login").addEventListener("click",()=>void action(async()=>{if(state.codex_login){const view=await invoke<CodexLoginView>("cancel_codex_login",{id:state.codex_login.id});if(state.codex_login.id===view.id && state.codex_login.sequence<=view.sequence)state.codex_login=view;}}));

element("enable-agent-tools").addEventListener("click",()=>void action(async()=>{
  projectRevision++;
  bridgeConfig=await invoke<BridgeConfig>("set_agent_tools",{enabled:true});
  const config=bridgeConfig;
  applySnapshot(await invoke("get_snapshot"));
  if(!state.agent_tools_enabled)return;
  element("claude-tools-config").textContent=JSON.stringify({mcpServers:{hazlomexa:{type:"stdio",...config}}},null,2);
  element("codex-tools-config").textContent=`[mcp_servers.hazlomexa]\ncommand = ${JSON.stringify(config.command)}\n\n[mcp_servers.hazlomexa.env]\n${Object.entries(config.env).map(([key,value])=>`${key} = ${JSON.stringify(value)}`).join("\n")}`;
  element("agent-tools-config").hidden=false;
}));
element("disable-agent-tools").addEventListener("click",()=>void action(async()=>{
  projectRevision++;await invoke("set_agent_tools",{enabled:false});applySnapshot(await invoke("get_snapshot"));
}));
async function action<T>(work: () => Promise<T>): Promise<T | undefined> {
  if (commandBusy) return;
  commandBusy = true; element("error").hidden = true; render();
  try { return await work(); } catch (error) { if (String(error) !== "Selección cancelada.") showError(String(error)); return undefined; }
  finally { commandBusy = false; render(); }
}

for (const [id, create] of [["create-project", true], ["open-project", false]] as const) {
  element(id).addEventListener("click", () => void action(async () => { const next = await invoke<DesktopSnapshot>("select_project", { create }); projectRevision++; applySnapshot(next); dirty = false; }));
}
element("select-freecad").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("select_freecad")); }));
element("select-arduino").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("select_arduino")); }));
element("connect-agent").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("connect_agent")); }));
element("select-cadcraft").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("select_cadcraft")); }));
element("select-kicad").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("select_kicad")); }));
element("select-orca").addEventListener("click", () => void action(async () => { applySnapshot(await invoke("select_orca")); }));
element("select-pilot-freecad").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_freecad"));}));
element("cadpilot-operation").addEventListener("change",render);
element("cadpilot-component").addEventListener("change",pilotObjectChanged);
element("cadpilot-unit").addEventListener("change",()=>{
  const next=element<HTMLSelectElement>("cadpilot-unit").value as Unit;
  for(const key of ["length","width","height","diameter","x","y","z"]){
    const field=element<HTMLInputElement>(`cadpilot-${key}`);
    if(field.value.trim() && Number.isFinite(Number(field.value)))field.value=String(Number((Number(field.value)*units[pilotFormUnit].factor/units[next].factor).toPrecision(12)));
  }
  pilotFormUnit=next;
});
element("cadpilot-new-source").addEventListener("click",()=>{resetPilotSelection();render();});
element("cadpilot-form").addEventListener("submit",event=>{
  event.preventDefault();
  void action(async()=>{
    const operation=element<HTMLSelectElement>("cadpilot-operation").value as CadpilotRequest["operation"];
    const keys=operation==="inspect" ? [] : operation==="edit_box" ? ["length","width","height"] : operation==="add_box" ? ["length","width","height","x","y","z"] : ["radius","height","x","y","z"];
    const parameters:Record<string,number>={};
    for(const key of keys){const value=element<HTMLInputElement>(`cadpilot-${key==="radius" ? "diameter" : key}`).value.trim();if(!value || !Number.isFinite(Number(value))) throw new Error("Confirma las medidas y la posición de la copia. Si todavía no las conoces, mide e inspecciona primero.");parameters[key]=key==="radius" ? Number(value)/2 : Number(value);}
    const request:CadpilotRequest={operation,object:operation==="inspect" ? "Document" : pilotBase && operation==="edit_box" ? element<HTMLSelectElement>("cadpilot-component").value : element<HTMLInputElement>("cadpilot-object").value,units:element<HTMLSelectElement>("cadpilot-unit").value as Unit,parameters};
    applySnapshot(pilotBase ? await invoke("prepare_cadpilot_followup",{projectId:pilotBase.project_id,baseJobId:pilotBase.job_id,resultSha256:pilotBase.sha256,request}) : await invoke("prepare_cadpilot",{request}));
  });
});
element("slice-form").addEventListener("submit",event=>{
  event.preventDefault();
  void action(async()=>{
    const value=element<HTMLInputElement>("slice-bed-temperature").value.trim();
    const temperature=Number(value);
    if(!value || !Number.isFinite(temperature) || temperature<0 || temperature>200) throw new Error("Confirma la temperatura inicial de cama entre 0 y 200 °C.");
    applySnapshot(await invoke("prepare_slice",{request:{units:element<HTMLSelectElement>("slice-unit").value,bed:element<HTMLSelectElement>("slice-bed").value,bed_temperature_c:temperature}}));
  });
});
element("kicad-form").addEventListener("submit", event => {
  event.preventDefault();
  void action(async () => { applySnapshot(await invoke("prepare_kicad", {units: element<HTMLSelectElement>("kicad-unit").value})); });
});
element("select-konnect").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_konnect"));}));
element("select-konnect-kicad").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_kicad"));}));
element("konnect-form").addEventListener("submit",event=>{
  event.preventDefault();void action(async()=>{
    const reference=element<HTMLInputElement>("konnect-reference").value.trim(),value=element<HTMLInputElement>("konnect-value").value.trim();
    applySnapshot(await invoke(schematic ? "prepare_discovered_schematic" : "prepare_konnect",schematic ? {selection:schematic.id,reference,value} : {units:"mm",reference,value}));
  });
});
function renderPcbPosition() {
  const component=pcb?.components.find(fp=>fp.uuid===element<HTMLSelectElement>("pcb-component").value);
  const factor=pcbFactors[element<HTMLSelectElement>("pcb-unit").value];
  const value=(v:number)=>Number((v/factor).toPrecision(10));
  element("pcb-current").textContent=component ? `Posición actual desde el origen: X ${value(component.x_mm)}, Y ${value(component.y_mm)} ${element<HTMLSelectElement>("pcb-unit").value} · orientación ${component.rotation_deg}°` : "Elige un componente para ver su posición actual.";
}
element("select-pcb-konnect").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_konnect"));}));
element("select-pcb-kicad").addEventListener("click",()=>void action(async()=>{applySnapshot(await invoke("select_kicad"));}));
element("discover-pcb").addEventListener("click",()=>void action(async()=>{
  const revision=projectRevision;const found=await invoke<DiscoveredPcb>("discover_pcb");
  if(revision!==projectRevision || found.project_id!==state.project?.[0])return;
  pcb=found;const choices=element<HTMLSelectElement>("pcb-component");choices.replaceChildren(new Option("Elige un componente",""));
  for(const component of found.components){
    const namespace=component.library.split(":")[0];
    const kinds: Record<string,string>={Resistor_SMD:"Resistencia",Resistor_THT:"Resistencia",Capacitor_SMD:"Condensador",Capacitor_THT:"Condensador",TestPoint:"Punto de prueba"};
    const kind=Object.hasOwn(kinds,namespace) ? kinds[namespace] : component.library;
    choices.add(new Option(`${kind} · ${component.value || "sin valor"} · ${component.layer==="B.Cu" ? "cara inferior" : component.layer==="F.Cu" ? "cara superior" : component.layer} · (${component.x_mm}, ${component.y_mm}) mm (${component.reference})`,component.uuid));
  }
  for(const id of ["pcb-x","pcb-y"])element<HTMLInputElement>(id).value="";
  element("pcb-discovery").textContent=`${found.name}: ${found.components.length} componentes medidos. Elige cuál quieres mover.`;
}));
element("pcb-component").addEventListener("change",()=>{for(const id of ["pcb-x","pcb-y"])element<HTMLInputElement>(id).value="";render();});
element("pcb-unit").addEventListener("change",()=>{
  const next=element<HTMLSelectElement>("pcb-unit").value;
  for(const id of ["pcb-x","pcb-y"]){const input=element<HTMLInputElement>(id);if(input.value.trim() && Number.isFinite(Number(input.value)))input.value=String(Number((Number(input.value)*pcbFactors[pcbUnit]/pcbFactors[next]).toPrecision(12)));}
  pcbUnit=next;render();
});
element("pcb-form").addEventListener("submit",event=>{
  event.preventDefault();void action(async()=>{
    if(!pcb || pcb.project_id!==state.project?.[0])throw new Error("Abre la PCB del proyecto actual.");
    const component=element<HTMLSelectElement>("pcb-component").value;
    if(!component || ["pcb-x","pcb-y"].some(id=>!element<HTMLInputElement>(id).value.trim()))throw new Error("Elige un componente y confirma la posición nueva.");
    const x=Number(element<HTMLInputElement>("pcb-x").value),y=Number(element<HTMLInputElement>("pcb-y").value);
    if(!Number.isFinite(x) || !Number.isFinite(y))throw new Error("Confirma una posición finita.");
    applySnapshot(await invoke("prepare_pcb_move",{selection:pcb.id,component,units:element<HTMLSelectElement>("pcb-unit").value,x,y}));
  });
});
element("discover-schematic").addEventListener("click",()=>void action(async()=>{
  const revision=projectRevision;
  const found=await invoke<DiscoveredSchematic>("discover_schematic");
  if(revision!==projectRevision || found.project_id!==state.project?.[0]) return;
  schematic=found;
  const choices=element<HTMLSelectElement>("schematic-component");choices.replaceChildren(new Option("Elige un componente", ""));
  for(const component of found.components) {
    const kind=schematicKind(component.library);
    const option=new Option(`${kind} · ${component.value || "sin valor"} (${component.reference})${component.reference.startsWith("#") ? " · solo lectura" : ""}`,component.reference);
    option.disabled=component.reference.startsWith("#");choices.add(option);
  }
  element<HTMLInputElement>("konnect-reference").value="";
  element("schematic-discovery").textContent=`${found.name}: ${found.components.length} componentes en esta hoja. Elige cuál quieres cambiar; las hojas secundarias se inspeccionan por separado.`;
}));
element("schematic-component").addEventListener("change",()=>{
  const component=schematic?.components.find(c=>c.reference===element<HTMLSelectElement>("schematic-component").value);
  element<HTMLInputElement>("konnect-reference").value=component?.reference ?? "";
  element("schematic-discovery").textContent=component ? `Su valor actual es ${component.value || "vacío"}. ¿Qué valor necesitas?` : "Elige un componente del esquema.";
});
element("drawing-add-circle").addEventListener("change", render);
element("drawing-circle-unit").addEventListener("change",()=>{
  const next=element<HTMLSelectElement>("drawing-circle-unit").value as Unit;
  for(const id of ["drawing-x","drawing-y","drawing-diameter"]){const input=element<HTMLInputElement>(id);if(input.value.trim() && Number.isFinite(Number(input.value)))input.value=String(Number((Number(input.value)*units[drawingCircleUnit].factor/units[next].factor).toPrecision(12)));}
  drawingCircleUnit=next;
});
element("drawing-form").addEventListener("submit", event => {
  event.preventDefault();
  void action(async () => {
    const add = element<HTMLInputElement>("drawing-add-circle").checked;
    if (add && ["drawing-x", "drawing-y", "drawing-diameter"].some(id => !element<HTMLInputElement>(id).value.trim())) throw new Error("Indica la posición horizontal y vertical del centro y el diámetro antes de seleccionar el original.");
    const convert=(value:number)=>value*units[drawingCircleUnit].factor/units[element<HTMLSelectElement>("drawing-unit").value as Unit].factor;
    const circle = add ? {center:[convert(Number(element<HTMLInputElement>("drawing-x").value)), convert(Number(element<HTMLInputElement>("drawing-y").value))], radius:convert(Number(element<HTMLInputElement>("drawing-diameter").value))/2} : null;
    if (circle && (!circle.center.every(Number.isFinite) || !Number.isFinite(circle.radius) || circle.radius <= 0)) throw new Error("Indica un centro finito y un diámetro positivo.");
    applySnapshot(await invoke("prepare_drawing", {request:{output_format:element<HTMLSelectElement>("drawing-format").value, units:element<HTMLSelectElement>("drawing-unit").value, circle, allow_loss:element<HTMLInputElement>("drawing-loss").checked}}));
  });
});
element("select-agent-sketch").addEventListener("click", () => void action(async () => {
  applySnapshot(await invoke("select_agent_sketch"));
  element<HTMLTextAreaElement>("sketch-source").value = state.agent_source?.source ?? "";
  dirty = true;
}));
element("workflow").addEventListener("change", () => {
  selectedWorkflow = element<HTMLSelectElement>("workflow").value as typeof selectedWorkflow;
  if(assistantProvider === "guided"){turns = []; resetInference();}
  dirty = false; renderedConversation = "";
  render();
});
element("model-profile").addEventListener("change", render);
element("unit").addEventListener("change", () => {
  const next=element<HTMLSelectElement>("unit").value as Unit;
  for(const id of ["length","width","height"]){const input=element<HTMLInputElement>(id);if(input.value.trim() && Number.isFinite(Number(input.value)))input.value=String(convertDimensions({length:Number(input.value),width:0,height:0},selectedUnit,next).length);}
  selectedUnit=next;render();
});
element("new-conversation").addEventListener("click", () => {
  newChatSession=true;answeredChat=state.chat?.id;
  turns = []; resetInference(); element<HTMLTextAreaElement>("prompt").value = ""; render();
});
element("inference-form").addEventListener("submit", event => {
  event.preventDefault();
  void action(async () => {
    const revision = projectRevision;
    const requestRevision = ++inferenceRevision;
    const content = element<HTMLTextAreaElement>("prompt").value.trim();
    const nextTurns: Turn[] = [...turns, { role: "user", content }];
    if(assistantProvider!=="guided"){
      const ready=assistantProvider==="codex" ? state.codex_conversation_ready : state.claude_conversation_ready;
      const startNew=newChatSession || !ready;
      const prompt=agentPrompt(content,startNew ? turns : []);
      turns=nextTurns;answeredChat=undefined;element<HTMLTextAreaElement>("prompt").value="";
      chatChannel=new Channel<ChatView>();
      chatChannel.onmessage=view=>{if(revision!==projectRevision || requestRevision!==inferenceRevision || view.project_id!==state.project?.[0])return;
        if(state.chat?.id===view.id && state.chat.sequence>view.sequence)return;
        state.chat=view;render();if(!view.busy)void invoke<DesktopSnapshot>("get_snapshot").then(next=>{if(revision===projectRevision && requestRevision===inferenceRevision)applySnapshot(next);}).catch(error=>showError(String(error)));
      };
      const next=await invoke<DesktopSnapshot>(assistantProvider==="codex" ? "start_codex_chat" : "start_claude_chat",{prompt,newSession:startNew,events:chatChannel});newChatSession=false;
      if(next.chat?.id===state.chat?.id && next.chat && state.chat && next.chat.sequence<state.chat.sequence)next.chat=state.chat;
      applySnapshot(next);return;
    }
    if (selectedWorkflow === "cadpilot") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns=[...nextTurns,{role:"assistant",content:"Empecemos por inspeccionar el FCStd si no conoces sus medidas o nombres internos. Después elige la caja que quieres editar, o un nombre nuevo para añadir caja o cilindro. Confirma dimensiones y posición en mm, cm, m, pulgadas o pies; revisaremos el catálogo medido y la operación sobre la copia antes de aprobar."}];
      element<HTMLTextAreaElement>("prompt").value="";return;
    }
    if (selectedWorkflow === "slice") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns=[...nextTurns,{role:"assistant",content:"¿Cuáles son las unidades originales del STL, la máquina y boquilla, el material y la superficie de cama? Confirma su temperatura inicial en °C. Selecciona el modelo y los tres perfiles con sus herencias; revisaremos el tamaño en milímetros y las condiciones antes de crear 3MF y G-code."}];
      element<HTMLTextAreaElement>("prompt").value="";return;
    }
    if(selectedWorkflow==="pcb"){
      if(new TextEncoder().encode(JSON.stringify(nextTurns)).length>7168)throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      const component=pcb?.components.find(fp=>fp.uuid===element<HTMLSelectElement>("pcb-component").value);
      turns=[...nextTurns,{role:"assistant",content:component ? `Seleccionaste ${component.value || component.library}. ¿Dónde quieres colocarlo respecto al origen de la PCB? Su posición actual es (${component.x_mm}, ${component.y_mm}) mm. Confirma la posición nueva en mm, pulgadas o mils; revisaremos el movimiento y posibles conexiones rotas antes de aprobar.` : pcb ? "La PCB ya tiene una lista de componentes con sus valores, caras y posiciones. Elige cuál quieres mover y describe dónde lo necesitas. Si la ubicación es ambigua, falta confirmarla antes de crear una propuesta." : "Empecemos sin conocer identificadores ni medidas: pulsa Abrir PCB y ver sus componentes. Revisaremos sus valores y posiciones actuales antes de elegir qué mover."}];
      element<HTMLTextAreaElement>("prompt").value="";return;
    }
    if (selectedWorkflow === "konnect") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      const locked=state.job?.busy || state.job?.snapshot.phase==="awaiting_approval";
      const guided:ReturnType<typeof guidedSchematic>=locked ? {reply:"Cancela o termina la propuesta actual antes de cambiar sus parámetros."} : schematic ? guidedSchematic(content,schematic.components,element<HTMLSelectElement>("schematic-component").value) : {reply:"Podemos empezar sin conocer nombres ni valores: pulsa Abrir esquema y ver qué contiene. Verás sus componentes y valores actuales antes de decidir qué cambiar."};
      if (guided.reference && guided.value) {
        element<HTMLSelectElement>("schematic-component").value=guided.reference;
        element<HTMLSelectElement>("schematic-component").dispatchEvent(new Event("change"));
        element<HTMLInputElement>("konnect-value").value=guided.value;
      }
      turns=[...nextTurns,{role:"assistant",content:guided.reply}];
      element<HTMLTextAreaElement>("prompt").value="";return;
    }
    if (selectedWorkflow === "kicad") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns = [...nextTurns, {role:"assistant",content:"¿Revisaremos la hoja principal del esquema o una PCB? Reúne el proyecto, hojas secundarias, bibliotecas locales y reglas en la carpeta que seleccionarás. Elige mm, pulgadas o mils para el informe. La propuesta mostrará todos los archivos capturados antes de aprobar ERC/DRC y exportación."}];
      element<HTMLTextAreaElement>("prompt").value = "";
      return;
    }
    if (selectedWorkflow === "drawing") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns = [...nextTurns, {role:"assistant",content:"Reunamos el archivo original, el formato que necesitas y sus unidades. Para añadir un círculo, necesitamos su ancho completo (diámetro) y dónde irá su centro. Puedes dar esas medidas en milímetros o pulgadas. Si todavía no las sabes, inspeccionemos primero el dibujo. Selecciona el original; confirmarás la operación antes de crear la copia."}];
      element<HTMLTextAreaElement>("prompt").value = "";
      return;
    }
    if (selectedWorkflow === "agent") {
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns = [...nextTurns, {role:"assistant",content:state.agent_source ? `Cambio anotado para ${state.agent_source.source_name}. Ajusta el código de la copia y prepara su edición; revisaremos todo antes de aprobar.` : "Primero selecciona el sketch original y conecta Arduino Agent. Después ajusta el código de la copia y prepara la edición para revisarla."}];
      element<HTMLTextAreaElement>("prompt").value = "";
      return;
    }
    if (selectedWorkflow === "arduino") {
      // Reserve room for the fixed reply so the saved conversation stays bounded.
      if (new TextEncoder().encode(JSON.stringify(nextTurns)).length > 7168) throw new Error("La conversación alcanzó el límite. Empieza una conversación nueva.");
      turns = nextTurns;
      const locked = state.job?.busy || state.job?.snapshot.phase === "awaiting_approval";
      const guided = locked ? { reply: "Cancela o termina la propuesta actual antes de cambiar el código. Después podemos confirmar el LED y los tiempos." } : guidedBlink(nextTurns);
      if (guided.source) { element<HTMLTextAreaElement>("sketch-source").value = guided.source; dirty = true; }
      turns.push({ role: "assistant", content: guided.reply });
      element<HTMLTextAreaElement>("prompt").value = "";
      return;
    }
    const prompt = conversationPrompt(nextTurns);
    resetInference();
    turns = nextTurns;
    answeredInference = undefined;
    element<HTMLTextAreaElement>("prompt").value = "";
    render();
    inferenceChannel = new Channel<Inference>();
    inferenceChannel.onmessage = view => {
      if (revision !== projectRevision || requestRevision !== inferenceRevision || (state.inference && state.inference.id !== view.id) || (state.inference?.sequence ?? 0) > view.sequence) return;
      if (view.error && (state.inference?.id !== view.id || state.inference?.sequence !== view.sequence)) showError(view.error);
      state.inference = view; render();
      if (!view.busy) void invoke<DesktopSnapshot>("get_snapshot").then(next => { if (revision === projectRevision && requestRevision === inferenceRevision && next.inference?.id === view.id) applySnapshot(next); }).catch(error => showError(String(error)));
    };
    const next = await invoke<DesktopSnapshot>("start_inference", { profile: element<HTMLSelectElement>("model-profile").value, prompt, events: inferenceChannel });
    // A fast stream may arrive before the command result.
    if (next.inference?.id === state.inference?.id && next.inference && state.inference && next.inference.sequence < state.inference.sequence) next.inference = state.inference;
    applySnapshot(next);
  });
});
element("manual-dimensions").addEventListener("toggle", preview);
element("cancel-inference").addEventListener("click", () => void action(async () => {
  if(assistantProvider!=="guided" && state.chat){state.chat=await invoke("cancel_chat",{id:state.chat.id});return;}
  if (state.inference) { state.inference = await invoke("cancel_inference", { id: state.inference.id }); }
}));
element("dimensions-form").addEventListener("submit", event => {
  event.preventDefault();
  void action(async () => {
    if(!dimensionsConfirmed())throw new Error("Confirma largo, ancho y alto. Cada medida debe equivaler a 1–200 mm; las medidas desconocidas pueden quedar vacías mientras conversamos.");
    // A manual proposal supersedes pending inference snapshots from the same project.
    projectRevision++; inferenceRevision++;
    const current = state.job;
    const job: Job = current?.snapshot.phase === "awaiting_approval"
      ? await invoke("edit_proposal", { jobId: current.snapshot.input.job_id, dimensions: dimensions() })
      : await invoke("start_proposal", { dimensions: dimensions() });
    const d = job.snapshot.input.dimensions_mm!;
    turns.push({ role: "user", content: `Medidas ingresadas manualmente para un sólido rectangular: largo ${d.length} mm, ancho ${d.width} mm, alto ${d.height} mm.` });
    dirty = false; applyJob(job);
    applySnapshot(await invoke("get_snapshot"));
  });
});
for (const id of ["length", "width", "height"]) element(id).addEventListener("input", () => { dirty = true; render(); });
element("sketch-source").addEventListener("input", () => { dirty = true; render(); });
element("arduino-board").addEventListener("change", () => { dirty = true; render(); });
element("arduino-form").addEventListener("submit", event => {
  event.preventDefault();
  void action(async () => {
    projectRevision++; inferenceRevision++;
    if (selectedWorkflow === "agent") {
      const source = element<HTMLTextAreaElement>("sketch-source").value;
      const current = state.job;
      const job: Job = current?.snapshot.phase === "awaiting_approval"
        ? await invoke("edit_agent_proposal", {jobId:current.snapshot.input.job_id,source})
        : await invoke("start_agent_edit", {source});
      dirty = false; applyJob(job); applySnapshot(await invoke("get_snapshot"));
      return;
    }
    const args = { fqbn: element<HTMLSelectElement>("arduino-board").value, source: element<HTMLTextAreaElement>("sketch-source").value };
    const current = state.job;
    const job: Job = current?.snapshot.phase === "awaiting_approval"
      ? await invoke("edit_compile", { jobId: current.snapshot.input.job_id, ...args })
      : await invoke("start_compile", args);
    dirty = false; applyJob(job); applySnapshot(await invoke("get_snapshot"));
  });
});
element("approve").addEventListener("click", () => void action(async () => {
  if (!state.job?.approval_id || dirty) return;
  channel = new Channel<Job>();
  const jobId = state.job.snapshot.input.job_id;
  channel.onmessage = job => { if (state.job?.snapshot.input.job_id === jobId) applyJob(job); };
  applyJob(await invoke("approve_operation", { approvalId: state.job.approval_id, events: channel }));
}));
element("cancel").addEventListener("click", () => void action(async () => {
  if (state.job) applyJob(await invoke("cancel_job", { jobId: state.job.snapshot.input.job_id }));
}));

void action(async () => {
  applySnapshot(await invoke("get_snapshot"));
  if (state.inference) element<HTMLSelectElement>("model-profile").value = state.inference.profile;
  if (state.job?.snapshot.input.dimensions_mm) setDimensionFields(state.job.snapshot.input.dimensions_mm);
});
// Reconnect/reload reads authoritative state; polling never starts or approves a job.
setInterval(() => {
  const revision = projectRevision;
  if (!commandBusy && (state.codex_login?.busy || state.chat?.busy || state.agent_tools_enabled || state.job?.busy || state.inference?.busy)) void invoke<DesktopSnapshot>("get_snapshot").then(next => {
    if (revision !== projectRevision) return;
    if (next.job && state.job && next.job.snapshot.input.job_id === state.job.snapshot.input.job_id && next.job.sequence < state.job.sequence) return;
    if (next.inference && state.inference && next.inference.id === state.inference.id && next.inference.sequence < state.inference.sequence) return;
    applySnapshot(next);
  }).catch(error => showError(String(error)));
}, 1000);
