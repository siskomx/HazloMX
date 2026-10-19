use super::{
    snapshot, AppState, ChatQuestion, ChatTurn, ChatView, DesktopSnapshot, PendingQuestion,
};

#[tauri::command]
pub(crate) fn answer_codex_question(
    state: State<AppState>,
    id: Uuid,
    question_id: Uuid,
    answers: std::collections::BTreeMap<String, String>,
) -> Result<ChatView, String> {
    answer_owned_question(&state,id,question_id,answers)
}
pub(crate) fn answer_owned_question(state:&AppState,id:Uuid,question_id:Uuid,answers:std::collections::BTreeMap<String,String>)->Result<ChatView,String>{
    let scope=state.workflow.conversation_scope()?.ok_or("Abre una conversación antes de responder.")?;
    let mut current = state.chat.lock().map_err(|e| e.to_string())?;
    let turn = current
        .as_mut()
        .filter(|turn| {
            turn.view.id == id
                && turn.scope.project==scope.project && turn.scope.task.as_ref().map(|task|task.id)==scope.task.as_ref().map(|task|task.id) && scope.provider.as_deref()==Some("codex") && turn.view.provider=="codex"
                && turn.view.busy
                && !turn.view.cancelled
                && turn
                    .view
                    .question
                    .as_ref()
                    .is_some_and(|question| question.id == question_id)
        })
        .ok_or("Esta pregunta ya no está activa.")?;
    let body=turn.question
        .as_ref()
        .ok_or("Esta pregunta ya no está activa.")?
        .request
        .retained_answers(question_id,&answers)?;
    let task=turn.scope.task.as_ref().ok_or("La conversación cambió; no se respondió otra pregunta.")?;
    let input=hazlomexa::tasks::ChatCorrectionInput{id:task.id,revision:task.revision,chat_id:id,correction_id:question_id,text:String::new()};
    state.workflow.answer_chat(&turn.scope,&input,&body,&turn.view.text)?;
    // A fresh turn uses the durable answers; the old tool bridge keeps its superseded scope.
    turn.cancel.store(true,Ordering::SeqCst);turn.view.cancelled=true;turn.question=None;
    turn.view.question = None;
    turn.view.sequence += 1;
    let view=turn.view.clone();drop(current);
    *state.codex_session.lock().map_err(|_|"No se pudo invalidar el contexto anterior.")?=None;
    *state.chat_session.lock().map_err(|_|"No se pudo invalidar el contexto anterior.")?=None;
    Ok(view)
}
use hazlomexa::{claude, codex, tool_bridge::ToolBridge};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tauri::{ipc::Channel, Manager, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub(crate) struct LoginView {
    id: Uuid,
    sequence: u64,
    pub busy: bool,
    cancelled: bool,
    connected: bool,
    auth_url: Option<String>,
    error: Option<String>,
}
pub(crate) struct Login {
    pub view: LoginView,
    cancel: Arc<AtomicBool>,
}
pub(crate) fn agent_busy(state: &AppState) -> Result<bool, String> {
    let chat = state
        .chat
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .is_some_and(|turn| turn.view.busy);
    Ok(chat
        || state
            .codex_login
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .is_some_and(|login| login.view.busy))
}
fn home(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let home = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("codex-client");
    std::fs::create_dir_all(home.join("workspace")).map_err(|e| e.to_string())?;
    Ok(home)
}

#[tauri::command]
pub(crate) async fn select_codex(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<DesktopSnapshot, String> {
    if agent_busy(&state)? {
        return Err("Detén la conversación o conexión antes de cambiar Codex.".into());
    }
    let runtime = tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_title("Selecciona Codex 0.159.2 · codex.exe")
            .add_filter("Codex", &["exe"])
            .blocking_pick_file()
            .ok_or("Selección cancelada.")?
            .into_path()
            .map_err(|e| e.to_string())?;
        codex::identify(path)
    })
    .await
    .map_err(|e| e.to_string())??;
    if agent_busy(&state)? {
        return Err("Detén la conversación o conexión antes de cambiar Codex.".into());
    }
    *state.codex_session.lock().map_err(|e| e.to_string())? = None;
    *state.codex_login.lock().map_err(|e| e.to_string())? = None;
    *state.codex.lock().map_err(|e| e.to_string())? = Some(runtime);
    snapshot(&state)
}

#[tauri::command]
pub(crate) async fn connect_codex(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    events: Channel<LoginView>,
) -> Result<DesktopSnapshot, String> {
    let runtime = state
        .codex
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Primero elige Codex instalado.")?;
    let chat = state.chat.lock().map_err(|e| e.to_string())?;
    if chat.as_ref().is_some_and(|turn| turn.view.busy)
        || state.workflow.view()?.inference.is_some_and(|i| i.busy)
    {
        return Err("Detén la conversación antes de conectar la cuenta.".into());
    }
    let mut login = state.codex_login.lock().map_err(|e| e.to_string())?;
    if login.as_ref().is_some_and(|login| login.view.busy) {
        return Err("La conexión de Codex ya está en curso.".into());
    }
    let sequence = login.as_ref().map_or(1, |login| login.view.sequence + 1);
    let home = home(&app)?;
    std::fs::write(home.join("config.toml"), codex::BASE_CONFIG).map_err(|e| e.to_string())?;
    let id = Uuid::new_v4();
    let cancelled = Arc::new(AtomicBool::new(false));
    *login = Some(Login {
        view: LoginView {
            id,
            sequence,
            busy: true,
            cancelled: false,
            connected: false,
            auth_url: None,
            error: None,
        },
        cancel: cancelled.clone(),
    });
    drop(login);
    drop(chat);
    let slot = state.codex_login.clone();
    tauri::async_runtime::spawn(async move {
        let result = codex::authenticate(
            runtime,
            home.clone(),
            home.join("workspace"),
            cancelled.clone(),
            |update| {
                if let Ok(mut slot) = slot.lock() {
                    if let Some(login) = slot
                        .as_mut()
                        .filter(|login| login.view.id == id && !login.view.cancelled)
                    {
                        login.view.connected = update.connected;
                        login.view.auth_url = update.auth_url;
                        login.view.sequence += 1;
                        let _ = events.send(login.view.clone());
                    }
                }
            },
        )
        .await;
        if let Ok(mut slot) = slot.lock() {
            if let Some(login) = slot.as_mut().filter(|login| login.view.id == id) {
                login.view.busy = false;
                login.view.auth_url = None;
                login.view.sequence += 1;
                if cancelled.load(Ordering::SeqCst) {
                    login.view.cancelled = true;
                    login.view.connected = false;
                    login.view.error = None;
                } else {
                    match result {
                        Ok(view) => login.view.connected = view.connected,
                        Err(error) => {
                            login.view.connected = false;
                            login.view.error = Some(error);
                        }
                    }
                }
                let _ = events.send(login.view.clone());
            }
        }
    });
    snapshot(&state)
}

#[tauri::command]
pub(crate) fn cancel_codex_login(state: State<AppState>, id: Uuid) -> Result<LoginView, String> {
    let mut login = state.codex_login.lock().map_err(|e| e.to_string())?;
    let login = login
        .as_mut()
        .filter(|login| login.view.id == id && login.view.busy)
        .ok_or("La conexión cambió o ya terminó.")?;
    login.cancel.store(true, Ordering::SeqCst);
    login.view.cancelled = true;
    login.view.auth_url = None;
    login.view.sequence += 1;
    Ok(login.view.clone())
}

#[tauri::command]
pub(crate) fn open_codex_login(state: State<AppState>, id: Uuid) -> Result<(), String> {
    let login = state.codex_login.lock().map_err(|e| e.to_string())?;
    let url = login
        .as_ref()
        .filter(|login| login.view.id == id && login.view.busy && !login.view.cancelled)
        .and_then(|login| login.view.auth_url.as_deref())
        .ok_or("Vuelve a conectar para obtener un enlace vigente.")?;
    let url = codex::login_url(url)?;
    let url: Vec<_> = url.encode_utf16().chain(Some(0)).collect();
    let verb: Vec<_> = "open".encode_utf16().chain(Some(0)).collect();
    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    } as isize;
    if result <= 32 {
        return Err("Windows no pudo abrir el navegador para conectar Codex.".into());
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri IPC arguments are the desktop command contract.
pub(crate) async fn start_codex_chat(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    prompt: String,
    image: Option<hazlomexa::image_input::ImageInput>,
    task_user_request: Option<String>,
    program: hazlomexa::host_tools::Program,
    new_session: bool,
    events: Channel<ChatView>,
) -> Result<DesktopSnapshot, String> {
    let scope=state.workflow.conversation_scope()?.ok_or("Abre una conversación antes de iniciar la IA.")?;
    let (prompt,isolated)=state.workflow.task_prompt("codex",prompt,task_user_request,image.as_ref())?;
    let new_session=new_session || isolated;
    if prompt.trim().is_empty() || prompt.len() > 8192 {
        return Err("Escribe una solicitud de hasta 8 KiB.".into());
    }
    if let Some(image) = &image { image.validate()?; }
    let view = state.workflow.view()?;
    if view.inference.is_some_and(|i| i.busy) {
        return Err("Detén la solicitud a Ollama antes de cambiar de asistente.".into());
    }
    let selected_project = view.project.ok_or("Selecciona un proyecto.")?;
    let project = selected_project.0;
    if scope.project!=selected_project || state.workflow.conversation_scope()?.as_ref()!=Some(&scope){return Err("La conversación cambió antes de iniciar la IA.".into());}
    let runtime = state
        .codex
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Primero elige Codex instalado.")?;
    let mut current = state.chat.lock().map_err(|e| e.to_string())?;
    if current.as_ref().is_some_and(|turn| turn.view.busy)
        || state
            .codex_login
            .lock()
            .map_err(|e| e.to_string())?
            .as_ref()
            .is_some_and(|login| login.view.busy)
    {
        return Err("Detén la conversación o conexión actual antes de enviar.".into());
    }
    let (session, resume) = {
        let mut saved = state.codex_session.lock().map_err(|e| e.to_string())?;
        if new_session || saved.as_ref().is_none_or(|saved| saved.0 != scope) {
            *saved = Some((scope.clone(), Uuid::new_v4(), None));
        }
        let (_, id, thread) = saved.as_ref().unwrap();
        (*id, thread.clone())
    };
    let id = Uuid::new_v4();
    let cancelled = Arc::new(AtomicBool::new(false));
    let home = home(&app)?;
    let helper = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("hazlomexa-tools.exe");
    *current = Some(ChatTurn {
        scope:scope.clone(),
        view: ChatView {
            provider: "codex",
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
        let result = async {
            let executable = runtime.executable.clone();
            let expected = runtime.sha256.clone();
            if tauri::async_runtime::spawn_blocking(move || claude::fingerprint(&executable))
                .await
                .map_err(|e| e.to_string())??
                != expected
            {
                return Err("Codex cambió. Selecciónalo de nuevo.".into());
            }
            if workflow.conversation_scope()?.as_ref()!=Some(&scope){return Err("La conversación cambió; no se inició otra sesión de IA.".into());}
            let bridge = ToolBridge::start_chat(tools, &helper, selected_project, Some(program), "codex")?;
            let configuration = home.join("config.toml");
            let response = async {
                std::fs::write(&configuration, codex::configuration(&bridge.config()?)?)
                    .map_err(|e| e.to_string())?;
                codex::run_with_questions(
                    codex::TurnRequest {
                        expected_tools: bridge.tool_names().to_vec(),
                        runtime,
                        home: home.clone(),
                        directory: home.join("workspace"),
                        resume,
                        prompt,
                        image,
                        cancelled: cancelled.clone(),
                    },
                    |update| {
                        if let Ok(mut current) = chat.lock() {
                            if let Some(turn) = current
                                .as_mut()
                                .filter(|turn| turn.view.id == id && !turn.view.cancelled)
                            {
                                turn.view.text = update.text;
                                turn.view.sequence += 1;
                                let _ = events.send(turn.view.clone());
                            }
                        }
                    },
                    |request| {
                        let chat = chat.clone();
                        let events = events.clone();
                        let workflow = workflow.clone();
                        async move {
                            let (answer, receiver) = tokio::sync::oneshot::channel();
                            {
                                if workflow.view()?.project.map(|project| project.0)
                                    != Some(project)
                                {
                                    return Err("El proyecto cambió.".into());
                                }
                                let mut current = chat.lock().map_err(|e| e.to_string())?;
                                let turn = current
                                    .as_mut()
                                    .filter(|turn| {
                                        turn.view.id == id && turn.view.busy && !turn.view.cancelled
                                    })
                                    .ok_or("La conversación cambió.")?;
                                if turn.question.is_some() {
                                    return Err("Ya hay una pregunta pendiente.".into());
                                }
                                turn.view.question = Some(ChatQuestion {
                                    id: Uuid::new_v4(),
                                    questions: request.questions.clone(),
                                });
                                turn.question = Some(PendingQuestion { request, _answer:answer });
                                turn.view.sequence += 1;
                                events
                                    .send(turn.view.clone())
                                    .map_err(|_| "No se pudo mostrar la pregunta.")?;
                            }
                            receiver.await.map_err(|_| "La pregunta se cerró.".into())
                        }
                    },
                )
                .await
            }
            .await;
            let closed = bridge.close().await;
            let _ = std::fs::remove_file(configuration);
            closed?;
            response
        }
        .await;
        let state = handle.state::<AppState>();
        let successful = result
            .as_ref()
            .is_ok_and(|result| result.view.error.is_none() && !result.view.cancelled)
            && !cancelled.load(Ordering::SeqCst);
        let provenance=if successful{workflow.record_completed_reply(&scope,id,"codex",&result.as_ref().unwrap().view.text)}else{Ok(false)};
        let successful=successful && provenance.as_ref().is_ok_and(|saved|*saved);
        let provenance_error=provenance.err();
        if let Ok(mut saved) = state.codex_session.lock() {
            if saved.as_ref().is_some_and(|saved| saved.1 == session) {
                if successful && provenance_error.is_none() {
                    saved.as_mut().unwrap().2 = Some(result.as_ref().unwrap().thread_id.clone());
                } else {
                    *saved = None;
                }
            }
        }
        if let Ok(mut current) = chat.lock() {
            if let Some(turn) = current.as_mut().filter(|turn| turn.view.id == id) {
                turn.view.busy = false;
                turn.view.question = None;
                turn.question = None;
                turn.view.sequence += 1;
                if cancelled.load(Ordering::SeqCst) {
                    turn.view.cancelled = true;
                    turn.view.error = None;
                } else {
                    match result {
                        Ok(result) => {
                            turn.view.text = result.view.text;
                            turn.view.cancelled = result.view.cancelled;
                            turn.view.error = result.view.error.or(provenance_error);
                        }
                        Err(error) => turn.view.error = Some(error),
                    }
                }
                if workflow
                    .view()
                    .ok()
                    .and_then(|view| view.project)
                    .map(|project| project.0)
                    == Some(project)
                {
                    let _ = events.send(turn.view.clone());
                }
            }
        }
    });
    snapshot(&state)
}
