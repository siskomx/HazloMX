#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop_codex;
mod desktop_ollama;

use hazlomexa::{
    agent, cadcraft, cadpilot, claude, codex,
    host_tools::{
        DiscoveredCadDocument, DiscoveredDrawing, DiscoveredLivePcb, DiscoveredPcb,
        DiscoveredSchematic, DiscoveredSlice, HostTools,
    },
    jobs::{AgentSource, ArduinoRuntime, InferenceView, JobView, ModelProfile, Workflow},
    kicad, orca,
    project::{ArtifactFormat, Project, Snapshot},
    tool_bridge::{BridgeConfig, ToolBridge},
    Dimensions,
};
use serde::Serialize;
use std::{
    os::windows::ffi::OsStrExt,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{ipc::Channel, Manager, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

struct AppState {
    workflow: Arc<Workflow>,
    freecad: Mutex<PathBuf>,
    tools: Arc<HostTools>,
    bridge: Mutex<Option<ToolBridge>>,
    claude: Mutex<Option<claude::Runtime>>,
    codex: Mutex<Option<codex::Runtime>>,
    codex_login: Arc<Mutex<Option<desktop_codex::Login>>>,
    // shortcut: native thread IDs last one run; persist only after account-bound restart resume passes.
    codex_session: Mutex<Option<(Uuid, Uuid, Option<String>)>>,
    chat: Arc<Mutex<Option<ChatTurn>>>,
    chat_session: Mutex<Option<(Uuid, Uuid, bool)>>,
}
#[derive(Clone, Serialize)]
struct ChatView {
    provider: &'static str,
    id: Uuid,
    project_id: Uuid,
    sequence: u64,
    busy: bool,
    cancelled: bool,
    text: String,
    error: Option<String>,
    question: Option<ChatQuestion>,
}
#[derive(Clone, Serialize)]
struct ChatQuestion {
    id: Uuid,
    questions: Vec<codex::Question>,
}
struct PendingQuestion {
    request: codex::QuestionRequest,
    answer: tokio::sync::oneshot::Sender<std::collections::BTreeMap<String, String>>,
}
struct ChatTurn {
    view: ChatView,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    question: Option<PendingQuestion>,
}

#[derive(Serialize)]
struct DesktopSnapshot {
    project: Option<(Uuid, PathBuf)>,
    job: Option<JobView>,
    inference: Option<InferenceView>,
    recovered: Vec<Snapshot>,
    freecad: PathBuf,
    freecad_available: bool,
    arduino: Option<ArduinoRuntime>,
    agent: Option<agent::Connection>,
    agent_source: Option<AgentSource>,
    cadcraft: Option<PathBuf>,
    kicad: Option<PathBuf>,
    konnect: Option<PathBuf>,
    orca: Option<PathBuf>,
    agent_tools_enabled: bool,
    claude: Option<claude::Runtime>,
    codex: Option<codex::Runtime>,
    codex_login: Option<desktop_codex::LoginView>,
    chat: Option<ChatView>,
    claude_conversation_ready: bool,
    codex_conversation_ready: bool,
}

fn snapshot(state: &AppState) -> Result<DesktopSnapshot, String> {
    let view = state.workflow.view()?;
    let freecad = state.freecad.lock().map_err(|e| e.to_string())?.clone();
    let project_id = view.project.as_ref().map(|project| project.0);
    let claude_conversation_ready = state
        .chat_session
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|session| Some(session.0) == project_id && session.2);
    let codex_conversation_ready = state
        .codex_session
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|session| Some(session.0) == project_id && session.2.is_some());
    Ok(DesktopSnapshot {
        claude_conversation_ready,
        codex_conversation_ready,
        project: view.project,
        job: view.job,
        inference: view.inference,
        recovered: view.history,
        freecad_available: freecad.is_file(),
        freecad,
        arduino: state.workflow.arduino_runtime()?,
        agent: state.workflow.agent_connection()?,
        agent_source: state.workflow.agent_source()?,
        cadcraft: state.workflow.cadcraft_runtime()?,
        kicad: state.workflow.kicad_runtime()?,
        konnect: state.workflow.konnect_runtime()?,
        orca: state.workflow.orca_runtime()?,
        claude: state.claude.lock().map_err(|e| e.to_string())?.clone(),
        codex: state.codex.lock().map_err(|e| e.to_string())?.clone(),
        codex_login: state
            .codex_login
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .map(|login| login.view.clone()),
        chat: state
            .chat
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .map(|turn| turn.view.clone()),
        agent_tools_enabled: state
            .bridge
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .is_some_and(|b| b.config().is_ok()),
    })
}

#[tauri::command]
fn get_snapshot(state: State<AppState>) -> Result<DesktopSnapshot, String> {
    snapshot(&state)
}

#[tauri::command]
async fn discover_live_pcb(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection: hazlomexa::kicad_live::Connection,
) -> Result<DiscoveredLivePcb, String> {
    connection.validate()?;
    let project_id = state
        .workflow
        .view()?
        .project
        .ok_or("Selecciona un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let entry=app.dialog().file().set_title("Seleccione la PCB guardada que está abierta en ese KiCad; se leerá su estado vivo sin editar").add_filter("PCB KiCad",&["kicad_pcb"]).blocking_pick_file().ok_or("Selección cancelada.")?.into_path().map_err(|e|e.to_string())?;
        tools.select_live_pcb(project_id,entry,connection)
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
async fn select_claude(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    if state
        .chat
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|t| t.view.busy)
    {
        return Err("Detén la conversación antes de cambiar Claude.".into());
    }
    let runtime = tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Selecciona Claude Code 2.1.285 · claude.exe")
            .add_filter("Claude Code", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        claude::identify(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    if state
        .chat
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|t| t.view.busy)
    {
        return Err("Detén la conversación antes de cambiar Claude.".into());
    }
    *state.chat_session.lock().map_err(|e| e.to_string())? = None;
    *state.claude.lock().map_err(|e| e.to_string())? = Some(runtime);
    snapshot(&state)
}
#[tauri::command]
fn cancel_chat(state: State<AppState>, id: Uuid) -> Result<ChatView, String> {
    let mut chat = state.chat.lock().map_err(|e| e.to_string())?;
    let turn = chat
        .as_mut()
        .filter(|t| t.view.id == id)
        .ok_or("La conversación cambió.")?;
    turn.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    turn.view.cancelled = true;
    turn.view.question = None;
    turn.question = None;
    turn.view.sequence += 1;
    Ok(turn.view.clone())
}
#[tauri::command]
async fn start_claude_chat(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    new_session: bool,
    events: Channel<ChatView>,
) -> Result<DesktopSnapshot, String> {
    if prompt.trim().is_empty() || prompt.len() > 8192 {
        return Err("Escribe una solicitud de hasta 8 KiB.".into());
    }
    let view = state.workflow.view()?;
    if view.inference.is_some_and(|i| i.busy) {
        return Err("Detén la solicitud a Ollama antes de cambiar de asistente.".into());
    }
    let project = view.project.ok_or("Selecciona un proyecto.")?.0;
    let runtime = state
        .claude
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Selecciona Claude Code e inicia sesión en su cliente antes de conversar.")?;
    let mut current = state.chat.lock().map_err(|e| e.to_string())?;
    if current.as_ref().is_some_and(|t| t.view.busy)
        || state
            .codex_login
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .is_some_and(|login| login.view.busy)
    {
        return Err("Ya hay una conversación activa.".into());
    }
    let (session, resume) = {
        let mut saved = state.chat_session.lock().map_err(|e| e.to_string())?;
        if new_session || saved.as_ref().is_none_or(|s| s.0 != project) {
            *saved = Some((project, Uuid::new_v4(), false));
        }
        let (_, id, used) = saved.as_ref().unwrap();
        (*id, *used)
    };
    let id = Uuid::new_v4();
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("claude-sessions")
        .join(session.to_string());
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let helper = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("hazlomexa-tools.exe");
    *current = Some(ChatTurn {
        view: ChatView {
            provider: "claude",
            id,
            project_id: project,
            sequence: 1,
            busy: true,
            cancelled: false,
            text: String::new(),
            error: None,
            question: None,
        },
        cancel: cancelled.clone(),
        question: None,
    });
    drop(current);
    let tools = state.tools.clone();
    let chat = state.chat.clone();
    let workflow = state.workflow.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result=async {
            let executable=runtime.executable.clone();let expected=runtime.sha256.clone();
            if tauri::async_runtime::spawn_blocking(move||claude::fingerprint(&executable)).await.map_err(|e|e.to_string())??!=expected{return Err("Claude cambió. Selecciónalo de nuevo.".into());}
            let bridge=ToolBridge::start_project(tools,&helper,project)?;let config=bridge.config()?;let configuration=directory.join("mcp.json");
            std::fs::write(&configuration,serde_json::to_vec(&serde_json::json!({"mcpServers":{"hazlomexa":{"type":"stdio","command":config.command,"env":config.env}}})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            let response=claude::run(claude::TurnRequest{executable:runtime.executable,sha256:runtime.sha256,directory,configuration:configuration.clone(),session,resume,prompt,environment:claude::environment(),cancelled:cancelled.clone()},|update|{
                if let Ok(mut current)=chat.lock(){if let Some(turn)=current.as_mut().filter(|t|t.view.id==id){if !turn.view.cancelled{turn.view.text=update.text;turn.view.sequence+=1;let _=events.send(turn.view.clone());}}}
            }).await;
            let closed=bridge.close().await;let _=std::fs::remove_file(configuration);closed?;response
        }.await;
        let state = handle.state::<AppState>();
        let successful = result.as_ref().is_ok_and(|v| v.error.is_none())
            && !cancelled.load(std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut saved) = state.chat_session.lock() {
            if saved.as_ref().is_some_and(|s| s.1 == session) {
                if successful {
                    saved.as_mut().unwrap().2 = true;
                } else {
                    *saved = None;
                }
            }
        }
        if let Ok(mut current) = chat.lock() {
            if let Some(turn) = current.as_mut().filter(|t| t.view.id == id) {
                turn.view.busy = false;
                turn.view.sequence += 1;
                if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                    turn.view.cancelled = true;
                    turn.view.error = None;
                } else {
                    match result {
                        Ok(view) => {
                            turn.view.text = view.text;
                            turn.view.error = view.error;
                        }
                        Err(error) => turn.view.error = Some(error),
                    }
                }
                if workflow.view().ok().and_then(|v| v.project).map(|p| p.0) == Some(project) {
                    let _ = events.send(turn.view.clone());
                }
            }
        }
    });
    snapshot(&state)
}

#[tauri::command]
async fn set_agent_tools(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<Option<BridgeConfig>, String> {
    let mut bridge = state.bridge.lock().map_err(|e| e.to_string())?;
    if !enabled {
        *bridge = None;
        return Ok(None);
    }
    if state.workflow.view()?.project.is_none() {
        return Err("Selecciona un proyecto antes de conectar un agente.".into());
    }
    if let Some(current) = bridge.as_ref() {
        if let Ok(config) = current.config() {
            return Ok(Some(config));
        }
    }
    *bridge = None;
    let helper = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("hazlomexa-tools.exe");
    let connected = ToolBridge::start(state.tools.clone(), &helper)?;
    let config = connected.config()?;
    *bridge = Some(connected);
    Ok(Some(config))
}

#[tauri::command]
async fn select_cadcraft(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Seleccione cadcraft-cli.exe oficial Windows x64 0.5.0")
            .add_filter("CADCraft CLI", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || workflow.set_cadcraft(path))
        .await
        .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn select_orca(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Seleccione orca-slicer.exe oficial Windows x64 2.4.2")
            .add_filter("OrcaSlicer", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.set_orca(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn prepare_slice(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: orca::Request,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (root, model, profiles) = pick_slice_sources(&app)?;
        workflow.propose_slice(root, model, profiles, request)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}

fn pick_slice_sources(app: &tauri::AppHandle) -> Result<(PathBuf, PathBuf, [PathBuf; 3]), String> {
    let model = app
        .dialog()
        .file()
        .set_title(
            "Seleccione el STL para inspeccionarlo; su escala se confirmará en la conversación",
        )
        .add_filter("STL", &["stl"])
        .blocking_pick_file()
        .ok_or("Selección cancelada.")?
        .into_path()
        .map_err(|e| e.to_string())?;
    let root=app.dialog().file().set_title("Seleccione la carpeta de perfiles de OrcaSlicer que contiene machine, process y filament").blocking_pick_folder().ok_or("Selección cancelada.")?.into_path().map_err(|e|e.to_string())?;
    let mut selected = Vec::new();
    for (kind, label) in [
        ("machine", "máquina y boquilla"),
        ("process", "proceso"),
        ("filament", "material"),
    ] {
        let path = app
            .dialog()
            .file()
            .set_directory(root.join(kind))
            .set_title(format!("Seleccione el perfil JSON de {label}"))
            .add_filter("Perfil JSON", &["json"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        selected.push(path);
    }
    Ok((
        root,
        model,
        selected
            .try_into()
            .map_err(|_| "Faltan perfiles seleccionados.")?,
    ))
}
#[tauri::command]
async fn discover_slice(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiscoveredSlice, String> {
    let project_id = state
        .workflow
        .view()?
        .project
        .ok_or("Selecciona un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (root, model, profiles) = pick_slice_sources(&app)?;
        tools.select_slice(project_id, root, model, profiles)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn read_cadpilot_result(
    state: State<'_, AppState>,
    job_id: Uuid,
) -> Result<serde_json::Value, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools.call("read_verified_result", serde_json::json!({"job_id":job_id}))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn prepare_cadpilot_followup(
    state: State<'_, AppState>,
    project_id: Uuid,
    base_job_id: Uuid,
    result_sha256: String,
    request: cadpilot::Request,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        workflow.propose_cadpilot_followup(project_id, base_job_id, result_sha256, request)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn discover_cad_document(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiscoveredCadDocument, String> {
    let project_id = state
        .workflow
        .view()?
        .project
        .ok_or("Selecciona un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = app
            .dialog()
            .file()
            .set_title("Selecciona el documento FreeCAD guardado para inspeccionar")
            .add_filter("FreeCAD", &["FCStd"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let addon = app
            .dialog()
            .file()
            .set_title("Selecciona el paquete CADPilot 0.7.7")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        tools.select_cad_document(project_id, source, addon)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn prepare_cadpilot(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: cadpilot::Request,
) -> Result<DesktopSnapshot, String> {
    request.validate()?;
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = app
            .dialog()
            .file()
            .set_title("Seleccione el documento FCStd original para inspeccionar su copia")
            .add_filter("FreeCAD", &["FCStd"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let addon = app
            .dialog()
            .file()
            .set_title("Seleccione la carpeta del paquete CADPilot 0.7.7")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.propose_cadpilot(source, addon, request)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn select_kicad(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Seleccione kicad-cli.exe oficial Windows x64 10.0.7")
            .add_filter("KiCad CLI", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.set_kicad(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn prepare_kicad(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    units: kicad::Units,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move||{
        let root=app.dialog().file().set_title("Seleccione la carpeta que contiene el proyecto KiCad y todas sus dependencias locales").blocking_pick_folder().ok_or("Selección cancelada.")?.into_path().map_err(|e|e.to_string())?;
        let entry=app.dialog().file().set_directory(&root).set_title("Seleccione la hoja principal o PCB dentro de esa carpeta").add_filter("KiCad",&["kicad_sch","kicad_pcb"]).blocking_pick_file().ok_or("Selección cancelada.")?.into_path().map_err(|e|e.to_string())?;
        workflow.propose_kicad(root,entry,units)
    }).await.map_err(|e|e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn select_konnect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Seleccione konnect.exe Windows x64 0.13.0")
            .add_filter("Konnect", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.set_konnect(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn discover_pcb(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiscoveredPcb, String> {
    let project = state
        .workflow
        .view()?
        .project
        .ok_or("Seleccione un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = app
            .dialog()
            .file()
            .set_title("Selecciona la carpeta que contiene la PCB y sus archivos")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let entry = app
            .dialog()
            .file()
            .set_directory(&root)
            .set_title("Selecciona la PCB para ver sus componentes y posiciones")
            .add_filter("PCB KiCad", &["kicad_pcb"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        tools.select_pcb(project, root, entry)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn prepare_pcb_move(
    state: State<'_, AppState>,
    selection: Uuid,
    component: Uuid,
    units: kicad::Units,
    x: f64,
    y: f64,
) -> Result<DesktopSnapshot, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools.propose_pcb_move(selection, component, units, x, y)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn discover_schematic(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiscoveredSchematic, String> {
    let project_id = state
        .workflow
        .view()?
        .project
        .ok_or("Seleccione un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = app
            .dialog()
            .file()
            .set_title("Selecciona la carpeta que contiene el esquema y sus archivos")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let entry = app
            .dialog()
            .file()
            .set_directory(&root)
            .set_title("Selecciona el esquema para ver sus componentes")
            .add_filter("Esquema KiCad", &["kicad_sch"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        tools.select_schematic(project_id, root, entry)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn prepare_discovered_schematic(
    state: State<'_, AppState>,
    selection: Uuid,
    reference: String,
    value: String,
) -> Result<DesktopSnapshot, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || tools.propose_value(selection, reference, value))
        .await
        .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn prepare_konnect(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    units: kicad::Units,
    reference: String,
    value: String,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = app
            .dialog()
            .file()
            .set_title("Seleccione la carpeta del proyecto KiCad y sus dependencias locales")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let entry = app
            .dialog()
            .file()
            .set_directory(&root)
            .set_title("Seleccione el esquema que contiene el componente")
            .add_filter("Esquema KiCad", &["kicad_sch"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.propose_konnect_value(root, entry, units, reference, value)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}
#[tauri::command]
async fn read_kicad_summary(
    state: State<'_, AppState>,
    job_id: Uuid,
) -> Result<kicad::CheckSummary, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || workflow.kicad_summary(job_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn prepare_drawing(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: cadcraft::Request,
) -> Result<DesktopSnapshot, String> {
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Seleccione el dibujo original para trabajar en una copia")
            .add_filter("Dibujos", &["dxf", "dwg"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        workflow.propose_drawing(path, request)
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn discover_drawing(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DiscoveredDrawing, String> {
    let project_id = state
        .workflow
        .view()?
        .project
        .ok_or("Seleccione un proyecto.")?
        .0;
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Abrir dibujo para medir primero")
            .add_filter("Dibujos", &["dxf", "dwg"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        tools.select_drawing(project_id, path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn prepare_selected_drawing(
    state: State<'_, AppState>,
    selection: Uuid,
    request: cadcraft::Request,
) -> Result<DesktopSnapshot, String> {
    let tools = state.tools.clone();
    tauri::async_runtime::spawn_blocking(move || {
        tools.propose_selected_drawing(
            selection,
            request,
            false,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    snapshot(&state)
}

#[tauri::command]
async fn select_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    create: bool,
) -> Result<DesktopSnapshot, String> {
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Seleccione una carpeta local de proyecto")
            .blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or("Selección cancelada.")?
    .into_path()
    .map_err(|e| e.to_string())?;
    if !create && state.workflow.reopen_current(&path)?.is_some() {
        return snapshot(&state);
    }
    if state
        .chat
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|t| t.view.busy)
    {
        return Err("Detén la conversación antes de cambiar de proyecto.".into());
    }
    let project = if create {
        Project::create(&path)
    } else {
        Project::open(&path)
    }
    .map_err(|e| e.to_string())?;
    state.workflow.select_project(project)?;
    *state.chat_session.lock().map_err(|e| e.to_string())? = None;
    *state.codex_session.lock().map_err(|e| e.to_string())? = None;
    *state.chat.lock().map_err(|e| e.to_string())? = None;
    *state.bridge.lock().map_err(|e| e.to_string())? = None;
    snapshot(&state)
}

#[tauri::command]
async fn select_freecad(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let path = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Seleccione FreeCADCmd.exe de FreeCAD 1.1.4")
            .add_filter("FreeCAD", &["exe"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| e.to_string())?
    .ok_or("Selección cancelada.")?
    .into_path()
    .map_err(|e| e.to_string())?;
    state.workflow.set_executable(path.clone())?;
    *state.freecad.lock().map_err(|e| e.to_string())? = path;
    snapshot(&state)
}

#[tauri::command]
fn start_proposal(state: State<AppState>, dimensions: Dimensions) -> Result<JobView, String> {
    state.workflow.propose(dimensions)
}

#[tauri::command]
fn edit_proposal(
    state: State<AppState>,
    job_id: Uuid,
    dimensions: Dimensions,
) -> Result<JobView, String> {
    state.workflow.edit(job_id, dimensions)
}

#[tauri::command]
fn start_compile(state: State<AppState>, fqbn: String, source: String) -> Result<JobView, String> {
    state.workflow.propose_compile(fqbn, source)
}

#[tauri::command]
fn edit_compile(
    state: State<AppState>,
    job_id: Uuid,
    fqbn: String,
    source: String,
) -> Result<JobView, String> {
    state.workflow.edit_compile(job_id, fqbn, source)
}

#[tauri::command]
async fn select_arduino(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
        let executable = app
            .dialog()
            .file()
            .set_title("Seleccione Arduino CLI 1.5.1")
            .add_filter("Arduino CLI", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        let data = app
            .dialog()
            .file()
            .set_title("Seleccione la carpeta data preparada para Hazlo Mexa")
            .blocking_pick_folder()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        Ok((executable, data))
    })
    .await
    .map_err(|e| e.to_string())??;
    state.workflow.set_arduino(selected.0, selected.1)?;
    snapshot(&state)
}

#[tauri::command]
async fn connect_agent(state: State<'_, AppState>) -> Result<DesktopSnapshot, String> {
    let connection = agent::connect().await?;
    state.workflow.set_agent(connection)?;
    snapshot(&state)
}

#[tauri::command]
async fn select_agent_sketch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .add_filter("Sketch Arduino", &["ino"])
            .set_title("Seleccione el sketch original; se editará una copia")
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        agent::selected_source(&path)
    })
    .await
    .map_err(|e| e.to_string())??;
    state.workflow.set_agent_source(selected.0, selected.1)?;
    snapshot(&state)
}

#[tauri::command]
fn start_agent_edit(state: State<AppState>, source: String) -> Result<JobView, String> {
    state.workflow.propose_agent(source)
}

#[tauri::command]
fn edit_agent_proposal(
    state: State<AppState>,
    job_id: Uuid,
    source: String,
) -> Result<JobView, String> {
    state.workflow.edit_agent(job_id, source)
}

#[tauri::command]
fn approve_operation(
    state: State<AppState>,
    approval_id: Uuid,
    events: Channel<JobView>,
) -> Result<JobView, String> {
    let dispatch = state.workflow.begin(approval_id)?;
    let current = state
        .workflow
        .current()?
        .ok_or("No hay una operación activa.")?;
    let _ = events.send(current.clone());
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = workflow.execute(dispatch);
        if let Ok(Some(view)) = workflow.current() {
            let _ = events.send(view);
        }
        // A final snapshot-write failure is recoverable from the published directory.
        let _ = result;
    });
    Ok(current)
}

#[tauri::command]
fn cancel_job(state: State<AppState>, job_id: Uuid) -> Result<JobView, String> {
    state.workflow.cancel(job_id)
}

#[tauri::command]
fn start_inference(
    state: State<AppState>,
    profile: ModelProfile,
    prompt: String,
    events: Channel<InferenceView>,
) -> Result<DesktopSnapshot, String> {
    if state
        .chat
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|t| t.view.busy)
    {
        return Err("Detén la conversación con Claude antes de iniciar Ollama.".into());
    }
    let dispatch = state.workflow.start_inference(profile, &prompt)?;
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn(async move {
        let result = match dispatch.profile.client() {
            Ok(client) => {
                client
                    .propose(&prompt, dispatch.cancelled.clone(), |text| {
                        if let Ok(Some(view)) = workflow.inference_delta(&dispatch, &text) {
                            let _ = events.send(view);
                        }
                    })
                    .await
            }
            Err(error) => Err(error),
        };
        if let Ok(Some(view)) = workflow.finish_inference(dispatch, result) {
            let _ = events.send(view);
        }
    });
    snapshot(&state)
}

#[tauri::command]
fn cancel_inference(state: State<AppState>, id: Uuid) -> Result<InferenceView, String> {
    state.workflow.cancel_inference(id)
}

#[tauri::command]
fn open_artifact(
    state: State<AppState>,
    job_id: Uuid,
    format: ArtifactFormat,
) -> Result<Option<String>, String> {
    let artifact = state.workflow.artifact(job_id, format)?;
    if matches!(
        format,
        ArtifactFormat::Sketch
            | ArtifactFormat::SolidReport
            | ArtifactFormat::OriginalSketch
            | ArtifactFormat::DrawingReport
            | ArtifactFormat::KicadReport
            | ArtifactFormat::SliceReport
            | ArtifactFormat::CadpilotReport
            | ArtifactFormat::KonnectReport
    ) {
        return artifact.read_text().map(Some).map_err(|e| e.to_string());
    }
    // Explorer's /select parser requires native separators in nested artifact names.
    let artifact_path: PathBuf = artifact.path.components().collect();
    let system = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot missing")?);
    let gui = if matches!(format, ArtifactFormat::Fcstd | ArtifactFormat::Step) {
        state.workflow.gui_executable()?
    } else {
        system.join("explorer.exe")
    };
    let file: Vec<_> = gui.as_os_str().encode_wide().chain(Some(0)).collect();
    // Project path validation excludes quotes; preserve Unicode without a lossy string conversion.
    let prefix: Vec<u16> = if matches!(
        format,
        ArtifactFormat::Hex
            | ArtifactFormat::Elf
            | ArtifactFormat::Drawing
            | ArtifactFormat::OriginalDrawing
            | ArtifactFormat::DrawingPreview
            | ArtifactFormat::KicadSource
            | ArtifactFormat::KicadOutputs
            | ArtifactFormat::KonnectOriginal
            | ArtifactFormat::KonnectEdited
            | ArtifactFormat::KonnectNetlist
            | ArtifactFormat::OriginalMesh
            | ArtifactFormat::Sliced
            | ArtifactFormat::Gcode
            | ArtifactFormat::OriginalCad
    ) {
        "/select,".encode_utf16().collect()
    } else {
        Vec::new()
    };
    let parameters: Vec<u16> = prefix
        .into_iter()
        .chain(std::iter::once(34))
        .chain(artifact_path.as_os_str().encode_wide())
        .chain([34, 0])
        .collect();
    let verb: Vec<_> = "open".encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if result <= 32 {
        let target = if matches!(format, ArtifactFormat::Fcstd | ArtifactFormat::Step) {
            "FreeCAD"
        } else {
            "el archivo"
        };
        return Err(format!("Los archivos están guardados, pero Windows no pudo abrir {target} (código {result}). Revise la aplicación seleccionada."));
    }
    Ok(None)
}

fn main() {
    tauri::Builder::default().plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA missing")?);
            let freecad = std::env::var_os("HAZLOMEXA_FREECAD_CMD").map(PathBuf::from).unwrap_or_else(|| local.join("HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe"));
            let workflow = Arc::new(Workflow::new(freecad.clone(), data.join("freecad-profiles"))?);
            if let (Some(executable), Some(toolchain)) = (std::env::var_os("HAZLOMEXA_ARDUINO_CLI"), std::env::var_os("HAZLOMEXA_ARDUINO_DATA")) {
                // Optional runtime configuration must not prevent the existing FreeCAD workflow starting.
                let _ = workflow.set_arduino(PathBuf::from(executable), PathBuf::from(toolchain));
            }
            // CDP cannot operate native folder dialogs; debug tests seed a validated real project.
            #[cfg(debug_assertions)]
                if let Some(root) = std::env::var_os("HAZLOMEXA_SMOKE_PROJECT") {
                    let root = PathBuf::from(root);
                    let project = if root.join("hazlomx.json").exists() { Project::open(&root)? } else { Project::create(&root)? };
                    workflow.select_project(project)?;
                    tauri::WebviewWindowBuilder::new(app, "unprivileged", tauri::WebviewUrl::App("index.html".into()))
                        .title("Unprivileged IPC smoke test").visible(false).build()?;
                }
            let tools = Arc::new(HostTools::new(workflow.clone()));
            app.manage(AppState { workflow, freecad:Mutex::new(freecad), tools, bridge:Mutex::new(None), claude:Mutex::new(None), codex:Mutex::new(None), codex_login:Arc::new(Mutex::new(None)), codex_session:Mutex::new(None), chat:Arc::new(Mutex::new(None)), chat_session:Mutex::new(None) });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main app must also close hidden debug permission-test webviews.
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![get_snapshot, discover_cad_document, discover_slice, discover_live_pcb, discover_pcb, prepare_pcb_move, desktop_codex::answer_codex_question, desktop_codex::select_codex, desktop_codex::connect_codex, desktop_codex::cancel_codex_login, desktop_codex::open_codex_login, desktop_codex::start_codex_chat, select_claude, start_claude_chat, desktop_ollama::start_ollama_chat, cancel_chat, set_agent_tools, discover_schematic, prepare_discovered_schematic, select_konnect, prepare_konnect, read_cadpilot_result, prepare_cadpilot_followup, prepare_cadpilot, select_orca, prepare_slice, select_project, select_freecad, select_arduino, select_cadcraft, prepare_drawing, discover_drawing, prepare_selected_drawing, select_kicad, prepare_kicad, read_kicad_summary, connect_agent, select_agent_sketch, start_agent_edit, edit_agent_proposal, start_proposal, edit_proposal, start_compile, edit_compile, approve_operation, cancel_job, open_artifact, start_inference, cancel_inference])
        .run(tauri::generate_context!()).expect("Hazlo Mexa desktop could not start");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversation_readiness_requires_success_in_the_selected_project() {
        let root = std::env::temp_dir().join(format!("hazlomexa-chat-ready-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let project = Project::create(&root).unwrap();
        let project_id = project.id;
        let freecad = root.join("FreeCADCmd.exe");
        let workflow = Arc::new(Workflow::new(freecad.clone(), root.join("profiles")).unwrap());
        workflow.select_project(project).unwrap();
        let state = AppState {
            tools: Arc::new(HostTools::new(workflow.clone())),
            workflow,
            freecad: Mutex::new(freecad),
            bridge: Mutex::new(None),
            claude: Mutex::new(None),
            codex: Mutex::new(None),
            codex_login: Arc::new(Mutex::new(None)),
            codex_session: Mutex::new(None),
            chat: Arc::new(Mutex::new(None)),
            chat_session: Mutex::new(None),
        };
        let initial = snapshot(&state).unwrap();
        assert!(!initial.claude_conversation_ready && !initial.codex_conversation_ready);
        *state.chat_session.lock().unwrap() = Some((project_id, Uuid::new_v4(), false));
        *state.codex_session.lock().unwrap() = Some((project_id, Uuid::new_v4(), None));
        let pending = snapshot(&state).unwrap();
        assert!(!pending.claude_conversation_ready && !pending.codex_conversation_ready);
        state.chat_session.lock().unwrap().as_mut().unwrap().2 = true;
        state.codex_session.lock().unwrap().as_mut().unwrap().2 = Some("native-thread".into());
        let completed = snapshot(&state).unwrap();
        assert!(completed.claude_conversation_ready && completed.codex_conversation_ready);
        let other = root.join("other");
        std::fs::create_dir(&other).unwrap();
        state
            .workflow
            .select_project(Project::create(&other).unwrap())
            .unwrap();
        let foreign = snapshot(&state).unwrap();
        assert!(!foreign.claude_conversation_ready && !foreign.codex_conversation_ready);
    }
}
