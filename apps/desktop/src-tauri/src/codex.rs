//! Codex app-server framing and turn state. A completed chat is not a verified design.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const RECORD_LIMIT: usize = 1024 * 1024;
const TEXT_LIMIT: usize = 256 * 1024;

pub const BASE_CONFIG: &str = r#"cli_auth_credentials_store = "file"
approval_policy = "never"
sandbox_mode = "read-only"
web_search = "disabled"
project_doc_max_bytes = 0
[analytics]
enabled = false
[features]
shell_tool = false
unified_exec = false
multi_agent = false
hooks = false
plugins = false
remote_plugin = false
sleep_tool = false
request_permissions_tool = false
default_mode_request_user_input = true
tool_suggest = false
view_image = false
goals = false
image_generation = false
code_mode = false
code_mode_host = false
skill_search = false
skill_mcp_dependency_install = false
browser_use = false
browser_use_external = false
computer_use = false
in_app_browser = false
apps = false
realtime_conversation = false
[agents]
enabled = false
[apps._default]
enabled = false
"#;

pub fn configuration(bridge: &crate::tool_bridge::BridgeConfig) -> Result<String, String> {
    let command = serde_json::to_string(&bridge.command).map_err(|e| e.to_string())?;
    let env = bridge
        .env
        .iter()
        .map(|(key, value)| {
            if !["HAZLOMEXA_PIPE", "HAZLOMEXA_TOKEN"].contains(&key.as_str()) {
                return Err("La conexión de Codex contiene una variable no autorizada.".to_owned());
            }
            Ok(format!(
                "{key} = {}",
                serde_json::to_string(value).map_err(|e| e.to_string())?
            ))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("\n");
    if bridge.env.len() != 2 || !std::path::Path::new(&bridge.command).is_absolute() {
        return Err("Falta la conexión local de Codex con Hazlo.".into());
    }
    Ok(format!(
        r#"{BASE_CONFIG}
[mcp_servers.hazlomexa]
command = {command}
required = true
startup_timeout_sec = 20
[mcp_servers.hazlomexa.env]
{env}
"#
    ))
}

#[derive(Clone, Serialize)]
pub struct Runtime {
    pub executable: std::path::PathBuf,
    pub sha256: String,
    pub version: String,
}

pub fn identify(executable: std::path::PathBuf) -> Result<Runtime, String> {
    let parent = executable.parent().ok_or("Falta la carpeta de Codex.")?;
    let _lineage = crate::project::pin_lineage(parent).map_err(|e| e.to_string())?;
    if executable
        .file_name()
        .is_none_or(|name| !name.eq_ignore_ascii_case("codex.exe"))
    {
        return Err("Selecciona el ejecutable instalado codex.exe.".into());
    }
    let sha256 = crate::claude::fingerprint(&executable)?;
    let result = crate::process::run_owned(
        &executable,
        &["--version".into()],
        parent,
        &crate::claude::environment(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::time::Duration::from_secs(10),
    )
    .map_err(|e| e.to_string())?;
    if result.exit_code != Some(0)
        || result.truncated
        || String::from_utf8_lossy(&result.stdout).trim() != "codex-cli 0.159.2"
        || crate::claude::fingerprint(&executable)? != sha256
    {
        return Err("Esta conexión requiere Codex 0.159.2, la versión verificada.".into());
    }
    Ok(Runtime {
        executable,
        sha256,
        version: "0.159.2".into(),
    })
}

pub struct TurnRequest {
    pub runtime: Runtime,
    pub home: std::path::PathBuf,
    pub directory: std::path::PathBuf,
    pub resume: Option<String>,
    pub prompt: String,
    pub cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
pub struct TurnResult {
    pub thread_id: String,
    pub view: StreamView,
}

struct Client {
    child: Box<dyn process_wrap::tokio::ChildWrapper>,
    input: tokio::process::ChildStdin,
    output: tokio::io::BufReader<tokio::process::ChildStdout>,
    stderr: tokio::task::JoinHandle<()>,
    _pins: Vec<std::fs::File>,
}
impl Client {
    fn spawn(
        runtime: &Runtime,
        home: &std::path::Path,
        directory: &std::path::Path,
    ) -> Result<Self, String> {
        use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
        use std::process::Stdio;
        use tokio::io::BufReader;
        if runtime.version != "0.159.2"
            || !runtime.executable.is_absolute()
            || !home.is_absolute()
            || !directory.is_absolute()
        {
            return Err("La conexión de Codex no es válida.".into());
        }
        let mut pins = crate::project::pin_lineage(home).map_err(|e| e.to_string())?;
        pins.extend(crate::project::pin_lineage(directory).map_err(|e| e.to_string())?);
        pins.extend(
            crate::project::pin_lineage(
                runtime
                    .executable
                    .parent()
                    .ok_or("Falta la carpeta de Codex.")?,
            )
            .map_err(|e| e.to_string())?,
        );
        pins.push(crate::kicad_project::open_regular(
            &runtime.executable,
            512 * 1024 * 1024,
        )?);
        pins.push(crate::kicad_project::open_regular(
            &home.join("config.toml"),
            65536,
        )?);
        if crate::claude::fingerprint(&runtime.executable)? != runtime.sha256 {
            return Err("Codex cambió. Selecciónalo de nuevo.".into());
        }
        let mut command = tokio::process::Command::new(&runtime.executable);
        command
            .args(["app-server", "--stdio"])
            .current_dir(directory)
            .env_clear()
            .envs(crate::claude::environment())
            .env("CODEX_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut command = CommandWrap::from(command);
        let mut flags = CreationFlags(Default::default());
        flags.0 .0 = 0x08000000;
        command.wrap(flags).wrap(KillOnDrop).wrap(JobObject);
        let mut child = command.spawn().map_err(|_| "No se pudo iniciar Codex.")?;
        let input = child.stdin().take().ok_or("Falta la entrada de Codex.")?;
        let output = BufReader::new(child.stdout().take().ok_or("Falta la salida de Codex.")?);
        let mut errors = child
            .stderr()
            .take()
            .ok_or("Falta el canal de errores de Codex.")?;
        let stderr = tokio::spawn(async move {
            let mut buffer = [0; 8192];
            while let Ok(count) = errors.read(&mut buffer).await {
                if count == 0 {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            stderr,
            _pins: pins,
        })
    }
    async fn stop(&mut self) -> Result<(), String> {
        use std::time::Duration;
        let _ = tokio::time::timeout(Duration::from_millis(250), self.input.shutdown()).await;
        let stopped = Box::into_pin(self.child.kill()).await;
        let waited = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await;
        if tokio::time::timeout(Duration::from_secs(5), &mut self.stderr)
            .await
            .is_err()
        {
            self.stderr.abort();
        }
        stopped.map_err(|_| "No se pudo detener el cliente de Codex.")?;
        waited
            .map_err(|_| "Codex no confirmó su cierre.")?
            .map_err(|_| "No se pudo confirmar el cierre de Codex.")?;
        Ok(())
    }
}

pub async fn run(request: TurnRequest, emit: impl FnMut(StreamView)) -> Result<TurnResult, String> {
    run_with_questions(request, emit, |_| async {
        Err("Esta conexión no tiene una interfaz para responder preguntas.".into())
    })
    .await
}

pub async fn run_with_questions<
    F: std::future::Future<Output = Result<BTreeMap<String, String>, String>>,
>(
    request: TurnRequest,
    mut emit: impl FnMut(StreamView),
    mut ask: impl FnMut(QuestionRequest) -> F,
) -> Result<TurnResult, String> {
    use std::{sync::atomic::Ordering, time::Duration};
    if request.cancelled.load(Ordering::SeqCst) {
        return Err("Solicitud detenida.".into());
    }
    if request.runtime.version != "0.159.2"
        || !request.runtime.executable.is_absolute()
        || !request.home.is_absolute()
        || !request.directory.is_absolute()
        || request.prompt.trim().is_empty()
        || request.prompt.len() > 8192
        || request
            .resume
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
    {
        return Err("Selecciona Codex y escribe una solicitud de hasta 8 KiB.".into());
    }
    let mut client = Client::spawn(&request.runtime, &request.home, &request.directory)?;
    let (input, output) = (&mut client.input, &mut client.output);
    let work = async {
        let mut notifications = Vec::new();
        rpc(&mut *input,&mut *output,1,"initialize",serde_json::json!({"clientInfo":{"name":"hazlomexa","version":"0.1.0"},"capabilities":{"experimentalApi":true}}),&mut notifications).await?;
        input
            .write_all(b"{\"method\":\"initialized\",\"params\":{}}\n")
            .await
            .map_err(|_| "No se pudo inicializar Codex.")?;
        let account = rpc(
            &mut *input,
            &mut *output,
            2,
            "account/read",
            serde_json::json!({"refreshToken":false}),
            &mut notifications,
        )
        .await?;
        let connected = account_connected(&account)?;
        if account["requiresOpenaiAuth"] != false && !connected {
            return Err(
                "Inicia sesión en el cliente de Codex para esta conexión antes de conversar."
                    .into(),
            );
        }
        let (method, params) = if let Some(thread) = &request.resume {
            (
                "thread/resume",
                serde_json::json!({"threadId":thread,"cwd":request.directory,"approvalPolicy":"never","sandbox":"read-only","baseInstructions":crate::claude::INSTRUCTIONS}),
            )
        } else {
            (
                "thread/start",
                serde_json::json!({"cwd":request.directory,"ephemeral":false,"approvalPolicy":"never","sandbox":"read-only","baseInstructions":crate::claude::INSTRUCTIONS}),
            )
        };
        let thread = rpc(
            &mut *input,
            &mut *output,
            3,
            method,
            params,
            &mut notifications,
        )
        .await?;
        let thread_id = thread["thread"]["id"]
            .as_str()
            .ok_or("Falta la sesión de Codex.")?
            .to_owned();
        if request.resume.as_ref().is_some_and(|id| id != &thread_id) {
            return Err("Codex retomó otra sesión.".into());
        }
        let status = rpc(
            &mut *input,
            &mut *output,
            4,
            "mcpServerStatus/list",
            serde_json::json!({"threadId":thread_id,"detail":"toolsAndAuthOnly"}),
            &mut notifications,
        )
        .await?;
        let expected = crate::host_tools::catalog();
        if status["data"].as_array().is_none_or(|servers| {
            servers.len() != 1
                || servers[0]["name"] != "hazlomexa"
                || servers[0]["runtimeStatus"] != "connected"
                || servers[0]["tools"].as_object().is_none_or(|tools| {
                    tools.len() != expected.len()
                        || expected
                            .iter()
                            .any(|tool| !tools.contains_key(tool.name.as_ref()))
                })
        }) {
            return Err("Codex no conectó exclusivamente el catálogo de Hazlo.".into());
        }
        let turn=rpc(&mut *input,&mut *output,5,"turn/start",serde_json::json!({"threadId":thread_id,"input":[{"type":"text","text":request.prompt,"text_elements":[]}]}),&mut notifications).await?;
        let turn_id = turn["turn"]["id"]
            .as_str()
            .ok_or("Falta el turno de Codex.")?;
        let mut decoder = TurnDecoder::new(thread_id.clone(), turn_id.into())?;
        let mut queued = notifications.into_iter();
        let mut partial_record = Vec::new();
        let mut total = 0usize;
        for _ in 0..1000 {
            if decoder.view().complete {
                return Ok(TurnResult {
                    thread_id,
                    view: decoder.finish()?,
                });
            }
            let event = match queued.next() {
                Some(event) => event,
                None => read_record_buffered(&mut *output, &mut partial_record).await?,
            };
            total =
                total.saturating_add(serde_json::to_vec(&event).map_err(|e| e.to_string())?.len());
            if total > 4 * RECORD_LIMIT {
                return Err("Codex superó el límite de eventos del turno.".into());
            }
            if event["method"] == "item/tool/requestUserInput" {
                let question = parse_questions(&event, &thread_id, turn_id)?;
                let waiting = ask(question.clone());
                tokio::pin!(waiting);
                let answers = loop {
                    let incoming = match queued.next() {
                        Some(event) => event,
                        None => tokio::select! {
                            answers = &mut waiting => break answers?,
                            incoming = read_record_buffered(&mut *output, &mut partial_record) => incoming?,
                        },
                    };
                    total = total.saturating_add(
                        serde_json::to_vec(&incoming)
                            .map_err(|e| e.to_string())?
                            .len(),
                    );
                    if total > 4 * RECORD_LIMIT {
                        return Err("Codex superó el límite de eventos del turno.".into());
                    }
                    if incoming["method"] == "serverRequest/resolved"
                        && incoming["params"]["threadId"] == thread_id
                        && incoming["params"]["requestId"] == question.request_id
                    {
                        return Err(
                            "Codex cerró esta pregunta. Vuelve a enviar tu solicitud.".into()
                        );
                    }
                    if decoder.observe(&incoming)? {
                        emit(decoder.view());
                    }
                    if decoder.view().complete {
                        return Ok(TurnResult {
                            thread_id,
                            view: decoder.finish()?,
                        });
                    }
                };
                let result = question.answer(answers)?;
                input
                    .write_all(
                        format!(
                            "{}\n",
                            serde_json::json!({"id":question.request_id,"result":result})
                        )
                        .as_bytes(),
                    )
                    .await
                    .map_err(|_| "No se pudo enviar la respuesta a Codex.")?;
                continue;
            }
            if decoder.observe(&event)? {
                emit(decoder.view());
            }
        }
        Err("Codex superó el límite de eventos del turno.".into())
    };
    let interrupted = async {
        loop {
            if request.cancelled.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    let result = tokio::select! {
        _=interrupted=>Err("Solicitud detenida.".into()),
        response=tokio::time::timeout(Duration::from_secs(300),work)=>match response { Ok(result)=>result, Err(_)=>Err("Codex superó el tiempo de respuesta. La solicitud se detuvo.".into()) },
    };
    client.stop().await?;
    result
}

#[derive(Clone, Serialize, Default)]
pub struct LoginView {
    pub connected: bool,
    pub auth_url: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
}
#[derive(Clone)]
pub struct QuestionRequest {
    pub request_id: Value,
    pub questions: Vec<Question>,
}
impl QuestionRequest {
    pub fn answer(&self, answers: BTreeMap<String, String>) -> Result<Value, String> {
        if answers.len() != self.questions.len() {
            return Err("Responde cada pregunta antes de continuar.".into());
        }
        let mut result = BTreeMap::new();
        for question in &self.questions {
            let answer = answers
                .get(&question.id)
                .ok_or("Falta una respuesta.")?
                .trim();
            if answer.is_empty()
                || answer.len() > 2048
                || (!question.is_other
                    && question.options.as_ref().is_some_and(|options| {
                        !options.iter().any(|option| option.label == answer)
                    }))
            {
                return Err(
                    "Elige una opción válida o escribe una respuesta de hasta 2 KiB.".into(),
                );
            }
            result.insert(question.id.clone(), serde_json::json!({"answers":[answer]}));
        }
        Ok(serde_json::json!({"answers":result}))
    }
}
fn parse_questions(event: &Value, thread: &str, turn: &str) -> Result<QuestionRequest, String> {
    let params = &event["params"];
    let id = event
        .get("id")
        .ok_or("Falta el identificador de la pregunta.")?;
    if event["method"] != "item/tool/requestUserInput"
        || params["threadId"] != thread
        || params["turnId"] != turn
        || params["isBlocking"].as_bool().is_none()
        || (!id.is_u64()
            && !id
                .as_str()
                .is_some_and(|id| !id.is_empty() && id.len() <= 128))
    {
        return Err("Codex envió una pregunta para otra sesión o de un tipo no admitido.".into());
    }
    let questions: Vec<Question> = serde_json::from_value(params["questions"].clone())
        .map_err(|_| "Codex envió preguntas no válidas.")?;
    if questions.is_empty() || questions.len() > 3 || text(params, "itemId")?.len() > 128 {
        return Err("Codex superó el límite de preguntas.".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for question in &questions {
        if question.is_secret
            || question.id.is_empty()
            || question.id.len() > 64
            || !ids.insert(&question.id)
            || question.header.len() > 128
            || question.question.trim().is_empty()
            || question.question.len() > 4096
            || question.options.as_ref().is_some_and(|options| {
                options.is_empty()
                    || options.len() > 8
                    || options.iter().any(|option| {
                        option.label.trim().is_empty()
                            || option.label.len() > 256
                            || option.description.len() > 2048
                    })
            })
        {
            return Err(
                "Codex envió una pregunta secreta, repetida o fuera de los límites admitidos."
                    .into(),
            );
        }
    }
    Ok(QuestionRequest {
        request_id: id.clone(),
        questions,
    })
}

fn account_connected(value: &Value) -> Result<bool, String> {
    if value["requiresOpenaiAuth"].as_bool().is_none() {
        return Err("Codex no confirmó el estado de autenticación.".into());
    }
    let account = value
        .get("account")
        .ok_or("Falta el estado de cuenta de Codex.")?;
    if account.is_null() {
        return Ok(false);
    }
    match account["type"].as_str() {
        Some("chatgpt" | "apiKey") => Ok(true),
        _ => Err("Codex devolvió un tipo de cuenta no admitido por esta conexión.".into()),
    }
}

pub fn login_url(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| "Codex devolvió un enlace de inicio de sesión no válido.")?;
    if value.len() > 8192
        || url.scheme() != "https"
        || !matches!(url.host_str(), Some("auth.openai.com" | "chatgpt.com"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return Err("Codex devolvió un enlace fuera del inicio de sesión oficial.".into());
    }
    Ok(url.into())
}

/// Codex owns its login and credential storage; Hazlo receives status and an official URL.
pub async fn authenticate(
    runtime: Runtime,
    home: std::path::PathBuf,
    directory: std::path::PathBuf,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    mut emit: impl FnMut(LoginView),
) -> Result<LoginView, String> {
    use std::{sync::atomic::Ordering, time::Duration};
    if cancelled.load(Ordering::SeqCst) {
        return Err("Conexión detenida.".into());
    }
    let mut client = Client::spawn(&runtime, &home, &directory)?;
    let (input, output) = (&mut client.input, &mut client.output);
    let mut login_id = None;
    let work = async {
        let mut notifications = Vec::new();
        rpc(
            &mut *input,
            &mut *output,
            1,
            "initialize",
            serde_json::json!({"clientInfo":{"name":"hazlomexa","version":"0.1.0"}}),
            &mut notifications,
        )
        .await?;
        input
            .write_all(b"{\"method\":\"initialized\",\"params\":{}}\n")
            .await
            .map_err(|_| "No se pudo inicializar Codex.")?;
        let account = rpc(
            &mut *input,
            &mut *output,
            2,
            "account/read",
            serde_json::json!({"refreshToken":false}),
            &mut notifications,
        )
        .await?;
        if account_connected(&account)? {
            return Ok(LoginView {
                connected: true,
                auth_url: None,
            });
        }
        let started = rpc(
            &mut *input,
            &mut *output,
            3,
            "account/login/start",
            serde_json::json!({"type":"chatgpt"}),
            &mut notifications,
        )
        .await?;
        let id = text(&started, "loginId")?.to_owned();
        if started["type"] != "chatgpt" || id.len() > 128 {
            return Err("Codex inició otro tipo de conexión.".into());
        }
        let url = login_url(text(&started, "authUrl")?)?;
        login_id = Some(id.clone());
        emit(LoginView {
            connected: false,
            auth_url: Some(url),
        });
        let mut total = 0usize;
        for _ in 0..1000 {
            let event = if !notifications.is_empty() {
                notifications.remove(0)
            } else {
                read_record(&mut *output).await?
            };
            total =
                total.saturating_add(serde_json::to_vec(&event).map_err(|e| e.to_string())?.len());
            if total > 4 * RECORD_LIMIT || event.get("id").is_some() {
                return Err("Codex produjo una respuesta de conexión no compatible.".into());
            }
            if event["method"] != "account/login/completed" {
                continue;
            }
            let result = &event["params"];
            if result["loginId"] != id {
                return Err("Codex respondió a otro inicio de sesión.".into());
            }
            if result["success"] != true {
                return Err(
                    "Codex no completó el inicio de sesión. Vuelve a conectar para reintentar."
                        .into(),
                );
            }
            let account = rpc(
                &mut *input,
                &mut *output,
                4,
                "account/read",
                serde_json::json!({"refreshToken":false}),
                &mut Vec::new(),
            )
            .await?;
            if !account_connected(&account)? {
                return Err("Codex no confirmó una cuenta conectada.".into());
            }
            return Ok(LoginView {
                connected: true,
                auth_url: None,
            });
        }
        Err("Codex superó el límite de eventos de conexión.".into())
    };
    let interrupted = async {
        loop {
            if cancelled.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    let result = tokio::select! {
        _=interrupted=>Err("Conexión detenida.".into()),
        response=tokio::time::timeout(Duration::from_secs(300),work)=>match response {Ok(result)=>result,Err(_)=>Err("El inicio de sesión de Codex caducó. Vuelve a conectar para reintentar.".into())},
    };
    if result.is_err() {
        if let Some(id) = login_id {
            let cancel = format!(
                "{}\n",
                serde_json::json!({"id":99,"method":"account/login/cancel","params":{"loginId":id}})
            );
            let _ = tokio::time::timeout(
                Duration::from_millis(250),
                client.input.write_all(cancel.as_bytes()),
            )
            .await;
        }
    }
    client.stop().await?;
    if let Ok(view) = &result {
        emit(view.clone());
    }
    result
}

/// Bound allocation before parsing, including a record without its terminating newline.
pub async fn read_record<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Value, String> {
    let mut bytes = Vec::new();
    read_record_buffered(reader, &mut bytes).await
}
// Keep partial bytes when a human answer interrupts the pending read.
async fn read_record_buffered<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    bytes: &mut Vec<u8>,
) -> Result<Value, String> {
    let count = reader
        .take((RECORD_LIMIT + 1 - bytes.len()) as u64)
        .read_until(b'\n', bytes)
        .await
        .map_err(|_| "No se pudo leer la respuesta de Codex.")?;
    if count == 0 || bytes.len() > RECORD_LIMIT || bytes.last() != Some(&b'\n') {
        return Err("Codex cerró un evento o superó el límite de lectura.".into());
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "Codex produjo un evento JSON no válido.")?;
    bytes.clear();
    if !value.is_object() {
        return Err("Codex produjo un evento que no es un objeto.".into());
    }
    Ok(value)
}

/// Preserve notifications that arrive before the corresponding RPC acknowledgment.
pub async fn rpc(
    input: &mut (impl AsyncWrite + Unpin),
    output: &mut (impl AsyncBufRead + Unpin),
    id: u64,
    method: &str,
    params: Value,
    notifications: &mut Vec<Value>,
) -> Result<Value, String> {
    input
        .write_all(
            format!(
                "{}\n",
                serde_json::json!({"id":id,"method":method,"params":params})
            )
            .as_bytes(),
        )
        .await
        .map_err(|_| "No se pudo enviar la solicitud a Codex.")?;
    let read = async {
        let mut total = 0usize;
        for _ in 0..1000 {
            let message = read_record(output).await?;
            total = total.saturating_add(
                serde_json::to_vec(&message)
                    .map_err(|e| e.to_string())?
                    .len(),
            );
            if total > 4 * RECORD_LIMIT || notifications.len() >= 512 {
                return Err("Codex superó el límite de eventos de la solicitud.".into());
            }
            if message.get("method").is_some() {
                if message.get("id").is_some() {
                    if method == "turn/start" && message["method"] == "item/tool/requestUserInput" {
                        notifications.push(message);
                        continue;
                    }
                    input.write_all(format!("{}\n", serde_json::json!({"id":message["id"],"error":{"code":-32601,"message":"Hazlo does not authorize client actions"}})).as_bytes()).await.map_err(|_| "No se pudo rechazar la acción del cliente.")?;
                    return Err("Codex solicitó una acción del cliente no autorizada.".into());
                }
                notifications.push(message);
            } else if message["id"] == id {
                if message.get("error").is_some() || message.get("result").is_none() {
                    return Err(format!("Codex rechazó la solicitud {method}."));
                }
                return Ok(message["result"].clone());
            } else {
                return Err("Codex respondió a una solicitud diferente.".into());
            }
        }
        Err("Codex superó el límite de eventos de la solicitud.".into())
    };
    tokio::time::timeout(std::time::Duration::from_secs(45), read)
        .await
        .map_err(|_| "Codex superó el tiempo de respuesta.".to_owned())?
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamView {
    pub text: String,
    pub complete: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

pub struct TurnDecoder {
    thread: String,
    turn: String,
    messages: BTreeMap<String, String>,
    order: Vec<String>,
    view: StreamView,
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= TEXT_LIMIT)
        .ok_or_else(|| format!("Codex produjo un campo {key} no válido."))
}

impl TurnDecoder {
    pub fn new(thread: String, turn: String) -> Result<Self, String> {
        if thread.is_empty() || turn.is_empty() || thread.len() > 128 || turn.len() > 128 {
            return Err("La sesión y el turno de Codex no son válidos.".into());
        }
        Ok(Self {
            thread,
            turn,
            messages: BTreeMap::new(),
            order: Vec::new(),
            view: StreamView::default(),
        })
    }

    pub fn view(&self) -> StreamView {
        self.view.clone()
    }

    fn bind(&self, params: &Value) -> Result<(), String> {
        let turn = params.get("turnId").unwrap_or(&params["turn"]["id"]);
        if params["threadId"] != self.thread || turn != &self.turn {
            return Err("Codex respondió para otra sesión o turno.".into());
        }
        if self.view.complete {
            return Err("Codex envió eventos después de terminar el turno.".into());
        }
        Ok(())
    }

    fn message(&mut self, id: &str, value: &str, replace: bool) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 {
            return Err("Codex produjo un identificador de mensaje no válido.".into());
        }
        if value.len() > TEXT_LIMIT {
            return Err("Codex superó el límite de texto.".into());
        }
        if !self.messages.contains_key(id) {
            if self.messages.len() >= 256 {
                return Err("Codex superó el límite de mensajes del turno.".into());
            }
            self.order.push(id.into());
        }
        let current = self.messages.entry(id.into()).or_default();
        if replace {
            *current = value.into();
        } else {
            if current.len().saturating_add(value.len()) > TEXT_LIMIT {
                return Err("Codex superó el límite de texto.".into());
            }
            current.push_str(value);
        }
        let total: usize = self.messages.values().map(String::len).sum();
        if total.saturating_add(self.order.len().saturating_sub(1) * 2) > TEXT_LIMIT {
            return Err("Codex superó el límite de texto.".into());
        }
        self.view.text = self
            .order
            .iter()
            .map(|id| self.messages[id].as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(())
    }

    fn item(&mut self, item: &Value, completed: bool) -> Result<(), String> {
        let id = text(item, "id")?;
        match text(item, "type")? {
            "agentMessage" => {
                let value = item["text"].as_str().ok_or("Falta el texto de Codex.")?;
                if completed {
                    self.message(id, value, true)?;
                }
            }
            "mcpToolCall" => {
                let tool = text(item, "tool")?;
                if item["server"] != "hazlomexa"
                    || !crate::host_tools::catalog()
                        .iter()
                        .any(|t| t.name.as_ref() == tool)
                {
                    return Err("Codex anunció una herramienta ajena a Hazlo.".into());
                }
            }
            "userMessage" | "reasoning" | "plan" | "contextCompaction" => {}
            _ => return Err("Codex anunció una acción no admitida por el chat de Hazlo.".into()),
        }
        Ok(())
    }

    /// RPC responses are handled separately: an acknowledgment cannot complete a turn.
    /// This checks notifications; it does not prevent a client from executing builtin tools.
    pub fn observe(&mut self, event: &Value) -> Result<bool, String> {
        if event.get("id").is_some() {
            if event.get("method").is_some() {
                return Err("Codex solicitó una acción del cliente no autorizada.".into());
            }
            return Ok(false);
        }
        let method = text(event, "method")?;
        let params = &event["params"];
        match method {
            "item/agentMessage/delta" => {
                self.bind(params)?;
                let delta = params["delta"]
                    .as_str()
                    .ok_or("Fragmento de Codex no válido.")?;
                self.message(text(params, "itemId")?, delta, false)?;
            }
            "item/started" | "item/completed" => {
                self.bind(params)?;
                self.item(&params["item"], method == "item/completed")?;
            }
            "turn/started" => {
                self.bind(params)?;
                if params["turn"]["status"] != "inProgress" {
                    return Err("Codex inició un turno en un estado no válido.".into());
                }
            }
            "turn/completed" => {
                self.bind(params)?;
                let turn = &params["turn"];
                let items = turn["items"]
                    .as_array()
                    .ok_or("Faltan los mensajes finales de Codex.")?;
                if items.len() > 256 {
                    return Err("Codex superó el límite de elementos.".into());
                }
                for item in items {
                    self.item(item, true)?;
                }
                match turn["status"].as_str() {
                    Some("completed") if turn["error"].is_null() => {}
                    Some("interrupted") => self.view.cancelled = true,
                    Some("failed") => self.view.error = Some("Codex no completó la solicitud. Comprueba el inicio de sesión y el estado del cliente antes de reintentar.".into()),
                    _ => return Err("Codex no confirmó un estado final válido.".into()),
                }
                self.view.complete = true;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn finish(&self) -> Result<StreamView, String> {
        if !self.view.complete {
            return Err("Codex cerró la respuesta antes de terminar el turno.".into());
        }
        Ok(self.view())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn questions_bind_the_turn_and_validate_each_human_answer() {
        let event = serde_json::json!({"id":7,"method":"item/tool/requestUserInput","params":{"threadId":"thread","turnId":"turn","itemId":"item","isBlocking":true,"questions":[{"id":"units","header":"Units","question":"Which units?","options":[{"label":"mm","description":"Metric"},{"label":"in","description":"Inches"}]}]}});
        let request = super::parse_questions(&event, "thread", "turn").unwrap();
        assert_eq!(
            request
                .answer(std::collections::BTreeMap::from([(
                    "units".into(),
                    "mm".into()
                )]))
                .unwrap(),
            serde_json::json!({"answers":{"units":{"answers":["mm"]}}})
        );
        for answer in ["", "cm"] {
            assert!(request
                .answer(std::collections::BTreeMap::from([(
                    "units".into(),
                    answer.into()
                )]))
                .is_err());
        }
        assert!(request.answer(std::collections::BTreeMap::new()).is_err());
        assert!(super::parse_questions(&event, "foreign", "turn").is_err());
        let mut ordinary = event.clone();
        ordinary["params"]["isBlocking"] = serde_json::json!(false);
        assert!(super::parse_questions(&ordinary, "thread", "turn").is_ok());
        for (field, value) in [
            ("isSecret", serde_json::json!(true)),
            ("question", serde_json::json!("x".repeat(4097))),
        ] {
            let mut invalid = event.clone();
            invalid["params"]["questions"][0][field] = value;
            assert!(super::parse_questions(&invalid, "thread", "turn").is_err());
        }
        let mut invalid = event.clone();
        invalid["params"]["questions"]
            .as_array_mut()
            .unwrap()
            .push(event["params"]["questions"][0].clone());
        assert!(super::parse_questions(&invalid, "thread", "turn").is_err());
    }

    #[tokio::test]
    async fn question_wait_keeps_partial_records_after_cancelled_reads() {
        use tokio::io::AsyncWriteExt;
        let (mut writer, reader) = tokio::io::duplex(128);
        let mut reader = tokio::io::BufReader::new(reader);
        writer.write_all(b"{\"id\":").await.unwrap();
        let mut partial = Vec::new();
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(20),
            super::read_record_buffered(&mut reader, &mut partial)
        )
        .await
        .is_err());
        writer.write_all(b"7}\n").await.unwrap();
        assert_eq!(
            super::read_record_buffered(&mut reader, &mut partial)
                .await
                .unwrap(),
            serde_json::json!({"id":7})
        );
        assert!(partial.is_empty());
    }
    use super::*;
    use serde_json::json;

    fn decoder() -> TurnDecoder {
        TurnDecoder::new("thread".into(), "turn".into()).unwrap()
    }
    fn notification(method: &str, extra: Value) -> Value {
        let mut params = json!({"threadId":"thread","turnId":"turn"});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"method":method,"params":params})
    }
    fn terminal(status: &str, items: Value) -> Value {
        json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":status,"items":items}}})
    }

    #[test]
    fn captured_installed_client_turns_preserve_real_status_and_text() {
        for (fixture, failed) in [
            (include_str!("../../../../docs/research/codex-runner-2026-10-12/actual-failed-events.json"),true),
            (include_str!("../../../../docs/research/codex-runner-2026-10-12/actual-completed-events.json"),false),
        ] {
            let events:Vec<Value> = serde_json::from_str(fixture).unwrap();
            let started=events.iter().find(|event|event["method"]=="turn/started").unwrap();
            let mut decoder=TurnDecoder::new(started["params"]["threadId"].as_str().unwrap().into(),started["params"]["turn"]["id"].as_str().unwrap().into()).unwrap();
            for event in events { decoder.observe(&event).unwrap(); }
            let view=decoder.finish().unwrap();
            assert_eq!(view.error.is_some(),failed);
            assert!(!view.cancelled);
            assert_eq!(view.text,if failed { "" } else { "¿Qué quieres cambiar?" });
        }
    }

    #[tokio::test]
    async fn rpc_preserves_early_notifications_and_rejects_other_actions() {
        let data = b"{\"method\":\"item/agentMessage/delta\",\"params\":{\"delta\":\"early\"}}\n{\"id\":1,\"result\":{\"ok\":true}}\n";
        let mut notifications = Vec::new();
        let value = rpc(
            &mut tokio::io::sink(),
            &mut data.as_slice(),
            1,
            "turn/start",
            json!({}),
            &mut notifications,
        )
        .await
        .unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(notifications[0]["params"]["delta"], "early");
        let mut bytes=b"{\"id\":8,\"method\":\"item/tool/requestUserInput\",\"params\":{}}\n{\"id\":1,\"result\":{}}\n".as_slice();
        let mut queued = Vec::new();
        rpc(
            &mut tokio::io::sink(),
            &mut bytes,
            1,
            "turn/start",
            json!({}),
            &mut queued,
        )
        .await
        .unwrap();
        assert_eq!(queued[0]["id"], 8);
        for response in [
            json!({"id":2,"result":{}}),
            json!({"id":1,"error":{"message":"secret"}}),
            json!({"id":1}),
            json!({"id":8,"method":"requestApproval","params":{}}),
        ] {
            let bytes = format!("{response}\n").into_bytes();
            let error = rpc(
                &mut tokio::io::sink(),
                &mut bytes.as_slice(),
                1,
                "turn/start",
                json!({}),
                &mut Vec::new(),
            )
            .await
            .unwrap_err();
            assert!(!error.contains("secret"));
        }
    }

    #[tokio::test]
    async fn pre_cancelled_runner_never_reads_or_launches_runtime() {
        let result = run(
            TurnRequest {
                runtime: Runtime {
                    executable: "C:/missing/codex.exe".into(),
                    sha256: "0".repeat(64),
                    version: "0.159.2".into(),
                },
                home: "C:/missing".into(),
                directory: "C:/missing".into(),
                resume: None,
                prompt: "x".into(),
                cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            },
            |_| panic!("Cancelled turn emitted a reply"),
        )
        .await;
        assert_eq!(result.err().unwrap(), "Solicitud detenida.");
    }

    #[test]
    fn connection_config_rejects_extra_environment_and_disables_native_actions() {
        let mut bridge = crate::tool_bridge::BridgeConfig {
            command: "C:/owned/hazlomexa-tools.exe".into(),
            env: BTreeMap::from([
                ("HAZLOMEXA_PIPE".into(), "local".into()),
                ("HAZLOMEXA_TOKEN".into(), "0".repeat(32)),
            ]),
        };
        let config = configuration(&bridge).unwrap();
        for feature in [
            "shell_tool",
            "unified_exec",
            "view_image",
            "goals",
            "browser_use",
            "computer_use",
            "hooks",
        ] {
            assert!(config.contains(&format!("{feature} = false")));
        }
        bridge.env.insert("FOREIGN".into(), "x".into());
        assert!(configuration(&bridge).is_err());
    }

    #[test]
    fn login_links_are_official_https_and_credentials_stay_with_codex() {
        assert!(!account_connected(&json!({"account":null,"requiresOpenaiAuth":true})).unwrap());
        assert!(account_connected(
            &json!({"account":{"type":"chatgpt"},"requiresOpenaiAuth":true})
        )
        .unwrap());
        for account in [
            json!({}),
            json!({"requiresOpenaiAuth":true}),
            json!({"account":{},"requiresOpenaiAuth":true}),
            json!({"account":{"type":"foreign"},"requiresOpenaiAuth":true}),
        ] {
            assert!(account_connected(&account).is_err());
        }
        assert!(BASE_CONFIG.contains("cli_auth_credentials_store = \"file\""));
        for url in [
            "https://auth.openai.com/oauth/authorize?state=fixture",
            "https://chatgpt.com/auth/login",
        ] {
            assert!(login_url(url).is_ok());
        }
        for url in [
            "http://auth.openai.com/oauth/authorize",
            "https://auth.openai.com.foreign.test/",
            "https://user:password@auth.openai.com/",
            "https://auth.openai.com:444/",
            "https://auth.openai.com/#fragment",
            "file:///C:/private",
        ] {
            assert!(login_url(url).is_err());
        }
    }

    #[tokio::test]
    async fn bounded_records_reject_incomplete_and_non_object_json() {
        for data in [
            b"{}".to_vec(),
            b"[]\n".to_vec(),
            b"bad\n".to_vec(),
            vec![b' '; RECORD_LIMIT + 1],
        ] {
            assert!(read_record(&mut data.as_slice()).await.is_err());
        }
        let mut bytes = b"{\"id\":1,\"result\":{}}\n{\"method\":\"notice\"}\n".as_slice();
        assert_eq!(read_record(&mut bytes).await.unwrap()["id"], 1);
        assert_eq!(read_record(&mut bytes).await.unwrap()["method"], "notice");
        assert!(read_record(&mut bytes).await.is_err());
    }

    #[test]
    fn final_messages_replace_deltas_without_duplicate_text() {
        let mut d = decoder();
        assert!(!d
            .observe(&json!({"id":1,"result":{"turn":{"status":"inProgress"}}}))
            .unwrap());
        assert!(d.finish().is_err());
        for delta in ["¿Qué ", "quieres crear?"] {
            d.observe(&notification(
                "item/agentMessage/delta",
                json!({"itemId":"message","delta":delta}),
            ))
            .unwrap();
        }
        assert_eq!(d.view().text, "¿Qué quieres crear?");
        let item = json!({"id":"message","type":"agentMessage","text":"¿Qué quieres cambiar?"});
        d.observe(&notification("item/completed", json!({"item":item})))
            .unwrap();
        d.observe(&terminal("completed", json!([item]))).unwrap();
        assert_eq!(d.finish().unwrap().text, "¿Qué quieres cambiar?");
        assert!(d.observe(&terminal("completed", json!([]))).is_err());
    }

    #[test]
    fn identity_actions_failures_and_limits_remain_separate() {
        let mut wrong = notification("item/agentMessage/delta", json!({"itemId":"m","delta":"x"}));
        wrong["params"]["threadId"] = json!("foreign");
        assert!(decoder().observe(&wrong).is_err());
        for kind in [
            "commandExecution",
            "fileChange",
            "dynamicToolCall",
            "webSearch",
        ] {
            assert!(decoder()
                .observe(&notification(
                    "item/started",
                    json!({"item":{"id":"a","type":kind}})
                ))
                .is_err());
        }
        assert!(decoder()
            .observe(&json!({"id":2,"method":"item/commandExecution/requestApproval","params":{}}))
            .is_err());
        for (server, tool) in [
            ("foreign", "get_workflow_status"),
            ("hazlomexa", "approve_operation"),
        ] {
            assert!(decoder()
                .observe(&notification(
                    "item/started",
                    json!({"item":{"id":"a","type":"mcpToolCall","server":server,"tool":tool}})
                ))
                .is_err());
        }
        assert!(decoder().observe(&notification("item/started",json!({"item":{"id":"a","type":"mcpToolCall","server":"hazlomexa","tool":"read_verified_result"}}))).is_ok());
        for status in ["failed", "interrupted"] {
            let mut d = decoder();
            d.observe(&terminal(status, json!([]))).unwrap();
            let view = d.finish().unwrap();
            assert_eq!(view.cancelled, status == "interrupted");
            assert_eq!(view.error.is_some(), status == "failed");
        }
        assert!(decoder()
            .observe(&terminal("inProgress", json!([])))
            .is_err());
        let mut contradictory = terminal("completed", json!([]));
        contradictory["params"]["turn"]["error"] = json!({"message":"failed"});
        assert!(decoder().observe(&contradictory).is_err());
        assert!(decoder()
            .observe(&terminal(
                "completed",
                json!([{"id":"a","type":"fileChange"}])
            ))
            .is_err());
        assert!(decoder()
            .observe(&notification(
                "item/agentMessage/delta",
                json!({"itemId":"a","delta":"x".repeat(TEXT_LIMIT + 1)})
            ))
            .is_err());
        assert!(decoder()
            .observe(&terminal(
                "completed",
                json!([{"id":"a","type":"agentMessage","text":"x".repeat(TEXT_LIMIT + 1)}])
            ))
            .is_err());
    }
}
