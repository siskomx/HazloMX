use super::{snapshot, AppState, ChatTurn, ChatView, DesktopSnapshot};
use hazlomexa::{jobs::ModelProfile, ollama};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{ipc::Channel, State};
use uuid::Uuid;

#[tauri::command]
pub(crate) async fn start_ollama_chat(
    state: State<'_, AppState>,
    profile: ModelProfile,
    program: hazlomexa::host_tools::Program,
    prompt: String,
    events: Channel<ChatView>,
) -> Result<DesktopSnapshot, String> {
    if prompt.trim().is_empty() || prompt.len() > 8192 {
        return Err("Escribe una solicitud de hasta 8 KiB.".into());
    }
    let view = state.workflow.view()?;
    let project = view.project.ok_or("Selecciona un proyecto.")?;
    if view.inference.is_some_and(|inference| inference.busy) {
        return Err("Detén la solicitud anterior a Ollama antes de conversar.".into());
    }
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
    let client = profile.client()?;
    let id = Uuid::new_v4();
    let provider = match profile {
        ModelProfile::LocalExperimental => "ollama_local",
        ModelProfile::Cloud => "ollama_cloud",
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    *current = Some(ChatTurn {
        view: ChatView {
            provider,
            id,
            project_id: project.0,
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
    let chat = state.chat.clone();
    let host = state.tools.clone();
    let workflow = state.workflow.clone();
    tauri::async_runtime::spawn(async move {
        let result = async {
        let tools = hazlomexa::host_tools::McpTools::for_program(host, project.clone(), program)?;
        ollama::chat::run_scoped(
            client,
            workflow.clone(),
            tools,
            prompt,
            cancelled.clone(),
            |text| {
                if let Ok(mut current) = chat.lock() {
                    if let Some(turn) = current
                        .as_mut()
                        .filter(|turn| turn.view.id == id && !turn.view.cancelled)
                    {
                        turn.view.text = text;
                        turn.view.sequence += 1;
                        if workflow.view().ok().and_then(|view| view.project).as_ref()
                            == Some(&project)
                        {
                            let _ = events.send(turn.view.clone());
                        }
                    }
                }
            },
        )
        .await
        }.await;
        if let Ok(mut current) = chat.lock() {
            if let Some(turn) = current.as_mut().filter(|turn| turn.view.id == id) {
                turn.view.busy = false;
                turn.view.sequence += 1;
                if cancelled.load(Ordering::SeqCst) {
                    turn.view.cancelled = true;
                    turn.view.error = None;
                } else {
                    match result {
                        Ok(text) => turn.view.text = text,
                        Err(error) => turn.view.error = Some(error),
                    }
                }
                if workflow.view().ok().and_then(|view| view.project).as_ref() == Some(&project) {
                    let _ = events.send(turn.view.clone());
                }
            }
        }
    });
    snapshot(&state)
}
