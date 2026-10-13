use rmcp::{
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    RoleClient, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

pub const VERSION: &str = "0.7.1";
const LIMIT: usize = 65536;

#[derive(Clone, Serialize)]
pub struct Connection {
    pub version: String,
    pub protocol_version: String,
    #[serde(skip)]
    token: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub agent_version: String,
    pub source_name: String,
    pub original_source: String,
    pub source: String,
}
impl EditInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.job_id.is_nil()
            || self.agent_version != VERSION
            || self.source_name.len() > 255
            || !self.source_name.to_lowercase().ends_with(".ino")
            || self.source_name.contains(['/', '\\', ':', '"', '\0'])
            || self.source.trim().is_empty()
            || self.source.len() > LIMIT
            || self.original_source.len() > LIMIT
            || self.source.contains('\0')
            || self.original_source.contains('\0')
        {
            return Err("Seleccione un sketch .ino y código UTF-8 de hasta 64 KiB para Arduino Agent 0.7.1.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EditResult {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub agent_version: String,
    pub protocol_version: String,
    pub artifacts: [String; 2],
}

fn selected_document(uri: &str, expected: &Path) -> bool {
    reqwest::Url::parse(uri)
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .is_some_and(|actual| {
            actual
                .as_os_str()
                .eq_ignore_ascii_case(expected.as_os_str())
        })
}

fn read_source(path: &Path) -> Result<String, String> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.len() > LIMIT as u64
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("El sketch debe ser un archivo regular de hasta 64 KiB, sin enlaces.".into());
    }
    let mut text = String::new();
    file.take((LIMIT + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|_| "El sketch debe usar UTF-8.".to_string())?;
    if text.len() > LIMIT || text.contains('\0') {
        return Err("Sketch no permitido.".into());
    }
    Ok(text)
}

pub fn selected_source(path: &Path) -> Result<(String, String), String> {
    crate::project::valid_location(path).map_err(|e| e.to_string())?;
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ino"))
    {
        return Err("Seleccione un archivo .ino local.".into());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Nombre de sketch no válido.")?
        .to_string();
    let source = read_source(path)?;
    let input = EditInput {
        schema_version: 1,
        job_id: Uuid::new_v4(),
        agent_version: VERSION.into(),
        source_name: name.clone(),
        original_source: source.clone(),
        source: source.clone(),
    };
    input.validate()?;
    Ok((name, source))
}

fn token() -> Result<String, String> {
    let profile = std::env::var_os("USERPROFILE").ok_or("No se encontró el perfil de Windows.")?;
    let file = File::open(PathBuf::from(profile).join(".arduinoIDE/mcp-token"))
        .map_err(|_| "Abra Arduino Agent 0.7.1 antes de conectar.".to_string())?;
    let mut value = String::new();
    file.take(4097)
        .read_to_string(&mut value)
        .map_err(|_| "Token local de Agent no válido.".to_string())?;
    let value = value.trim();
    if !(32..=4096).contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'-' | b'_'))
    {
        return Err("Token local de Agent no válido.".into());
    }
    Ok(value.to_string())
}

impl Connection {
    pub fn verify_token(&self) -> Result<(), String> {
        if token()? != self.token {
            return Err(
                "La conexión de Agent cambió. Conecte de nuevo y vuelva a revisar la propuesta."
                    .into(),
            );
        }
        Ok(())
    }
}

type Session = RunningService<RoleClient, ()>;
fn authenticated_health(bytes: &[u8]) -> bool {
    if bytes.len() > 4096 {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .is_some_and(|health| {
            health["status"].as_str() == Some("ok")
                && health["server"].as_str() == Some("arduino-agent")
                && health["version"].as_str() == Some(VERSION)
                && health["auth"].as_bool() == Some(true)
        })
}
async fn session(token: String) -> Result<Session, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .pool_max_idle_per_host(0)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "No se pudo crear la conexión local.".to_string())?;
    let mut health = client
        .get("http://127.0.0.1:3847/health")
        .send()
        .await
        .map_err(|_| "Abra Arduino Agent 0.7.1 y conecte de nuevo.".to_string())?;
    if !health.status().is_success() {
        return Err("Agent no confirmó su estado.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = health
        .chunk()
        .await
        .map_err(|_| "Agent no confirmó su estado.".to_string())?
    {
        if chunk.len() > 4096usize.saturating_sub(bytes.len()) {
            return Err("Respuesta de estado de Agent demasiado grande.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !authenticated_health(&bytes) {
        return Err("Se requiere Arduino Agent 0.7.1 con autenticación activada.".into());
    }
    let mut config = StreamableHttpClientTransportConfig::with_uri("http://127.0.0.1:3847/mcp");
    config.auth_header = Some(token);
    config.allow_stateless = false;
    config.reinit_on_expired_session = false;
    config.max_sse_event_size = 1048576;
    config.max_concurrent_requests = 1;
    let service = tokio::time::timeout(
        Duration::from_secs(20),
        ().serve(StreamableHttpClientTransport::with_client(client, config)),
    )
    .await
    .map_err(|_| "Agent no respondió a tiempo.".to_string())?
    .map_err(|_| {
        "No se pudo conectar con Arduino Agent. Revise que esté abierto y autenticado.".to_string()
    })?;
    let info = service
        .peer_info()
        .ok_or("Agent no confirmó su identidad.")?;
    if !info
        .server_info
        .as_ref()
        .is_some_and(|server| server.name == "arduino-agent" && server.version == VERSION)
        || !matches!(info.protocol_version.as_str(), "2025-03-26" | "2025-11-25")
    {
        return Err(
            "Esta integración requiere Arduino Agent 0.7.1 con un protocolo verificado.".into(),
        );
    }
    let tools = service
        .list_all_tools()
        .await
        .map_err(|_| "No se pudo verificar el catálogo de Agent.".to_string())?;
    if !tools.iter().any(|tool| tool.name == "execute_tool") {
        return Err("El catálogo de Agent no es compatible.".into());
    }
    Ok(service)
}

pub async fn connect() -> Result<Connection, String> {
    let secret = token()?;
    let service = session(secret.clone()).await?;
    let protocol_version = service.peer_info().unwrap().protocol_version.to_string();
    service
        .cancel()
        .await
        .map_err(|_| "No se pudo cerrar la comprobación de Agent.".to_string())?;
    Ok(Connection {
        version: VERSION.into(),
        protocol_version,
        token: secret,
    })
}

// Only these software actions exist at the host boundary; the UI never supplies router/tool names.
async fn sketch(
    service: &Session,
    action: &str,
    path: &Path,
    content: Option<&str>,
) -> Result<serde_json::Value, String> {
    if !matches!(action, "get_content" | "set_content" | "open")
        || (action == "set_content") != content.is_some()
    {
        return Err("Operación de sketch no permitida.".into());
    }
    let mut params =
        json!({"action":action,"path":path.to_str().ok_or("Ruta de copia no válida.")?});
    if let Some(content) = content {
        params["content"] = json!(content);
    }
    let arguments = json!({"tool_name":"arduino_sketch","params":params})
        .as_object()
        .unwrap()
        .clone();
    let result = service.call_tool(CallToolRequestParams::new("execute_tool").with_arguments(arguments)).await.map_err(|_|"Agent no confirmó la operación; la copia se conserva. No repita una escritura sin revisarla.".to_string())?;
    if result.is_error == Some(true) || result.content.len() != 1 {
        return Err("Agent rechazó la operación del sketch; la copia se conserva.".into());
    }
    let text = result.content[0]
        .as_text()
        .ok_or("Respuesta de Agent no compatible.")?;
    if text.text.len() > 1048576 {
        return Err("Respuesta de Agent demasiado grande.".into());
    }
    serde_json::from_str(&text.text).map_err(|_| "Respuesta de Agent no compatible.".into())
}

pub fn validate_artifacts(
    directory: &Path,
    input: &EditInput,
    result: &EditResult,
) -> Result<(), String> {
    input.validate()?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || result.agent_version != VERSION
        || !matches!(
            result.protocol_version.as_str(),
            "2025-03-26" | "2025-11-25"
        )
        || result.artifacts != ["Original.ino", "Sketch.ino"]
        || read_source(&directory.join("Original.ino"))? != input.original_source
        || read_source(&directory.join("Sketch.ino"))? != input.source
    {
        return Err("Los archivos de Agent no coinciden con la propuesta aprobada.".into());
    }
    Ok(())
}

pub fn edit(
    connection: &Connection,
    stage: &Path,
    input: &EditInput,
    cancelled: Arc<AtomicBool>,
) -> Result<EditResult, String> {
    input.validate()?;
    connection.verify_token()?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Operación cancelada.".into());
    }
    if !stage.is_absolute()
        || fs::read_dir(stage)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("El directorio de Agent debe ser una reserva vacía.".into());
    }
    // Keep potentially late server effects in a fresh copy outside published project artifacts.
    let work = std::env::temp_dir().join(format!("hazlomexa-agent-{}", input.job_id));
    crate::project::valid_location(&work).map_err(|e| e.to_string())?;
    fs::create_dir(&work).map_err(|_| {
        "Ya existe la copia de esta operación; revise el historial antes de reintentar.".to_string()
    })?;
    let directory = work.join("Sketch");
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("Sketch.ino");
    fs::write(&path, &input.original_source).map_err(|e| e.to_string())?;
    let _work_pin = crate::project::pin_directory(&work, false).map_err(|e| e.to_string())?;
    let _directory_pin =
        crate::project::pin_directory(&directory, false).map_err(|e| e.to_string())?;
    let _file_pin = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
        .map_err(|e| e.to_string())?;
    fs::write(
        stage.join("input.json"),
        serde_json::to_vec_pretty(input).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        stage.join("working-copy.json"),
        serde_json::to_vec(&json!({"path":path})).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = runtime.block_on(async {
        let operation = async {
            let service = session(connection.token.clone()).await?;
            let before = sketch(&service,"get_content",&path,None).await?;
            if before["content"].as_str() != Some(input.original_source.as_str()) || before["path"].as_str() != path.to_str() { return Err("Agent no confirmó la copia original.".into()); }
            sketch(&service,"set_content",&path,Some(&input.source)).await?;
            let after = sketch(&service,"get_content",&path,None).await?;
            if after["content"].as_str() != Some(input.source.as_str()) || after["path"].as_str() != path.to_str() || read_source(&path)? != input.source { return Err("La copia editada no coincide con el código aprobado.".into()); }
            let opened = sketch(&service,"open",&path,None).await?;
            if !opened["mainFileUri"].as_str().is_some_and(|uri|selected_document(uri,&path)) { return Err("Agent no confirmó el documento seleccionado.".into()); }
            let protocol_version = service.peer_info().unwrap().protocol_version.to_string();
            service.cancel().await.map_err(|_|"Agent no confirmó el cierre de sesión.".to_string())?;
            Ok::<EditResult,String>(EditResult {schema_version:1,job_id:input.job_id,agent_version:VERSION.into(),protocol_version,artifacts:["Original.ino".into(),"Sketch.ino".into()]})
        };
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(60),operation) => result.map_err(|_|"Agent no confirmó la operación a tiempo; revise la copia antes de reintentar.".to_string())?,
            _ = async { while !cancelled.load(Ordering::SeqCst) {tokio::time::sleep(Duration::from_millis(20)).await;} } => Err("Operación cancelada. Agent puede terminar de modificar su copia; no se publicarán resultados.".into())
        }
    })?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Operación cancelada; la copia se conserva sin publicar.".into());
    }
    fs::write(stage.join("Original.ino"), &input.original_source).map_err(|e| e.to_string())?;
    fs::write(stage.join("Sketch.ino"), &input.source).map_err(|e| e.to_string())?;
    validate_artifacts(stage, input, &result)?;
    fs::write(
        stage.join("result.json"),
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> EditInput {
        EditInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            agent_version: VERSION.into(),
            source_name: "Original.ino".into(),
            original_source: "void setup() {} void loop() {}".into(),
            source: "void setup() {} void loop() { delay(500); }".into(),
        }
    }
    fn root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("hazlomexa-agent-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        root
    }
    #[test]
    fn rejects_unknown_effects_paths_and_large_or_invalid_sources() {
        let mut input = input();
        input.validate().unwrap();
        let mut value = serde_json::to_value(&input).unwrap();
        value["tool"] = json!("arduino_upload");
        assert!(serde_json::from_value::<EditInput>(value).is_err());
        input.source_name = "../Original.ino".into();
        assert!(input.validate().is_err());
        input.source_name = "Original.ino".into();
        input.source = "á".repeat(32769);
        assert!(input.validate().is_err());
        let root = root();
        let path = root.join("Original.ino");
        fs::write(&path, [255]).unwrap();
        assert!(selected_source(&path).is_err());
        fs::write(&path, "á".repeat(32768)).unwrap();
        assert_eq!(selected_source(&path).unwrap().1.len(), 65536);
        assert!(selected_source(Path::new("relative.ino")).is_err());
        assert!(selected_source(Path::new("\\\\server\\share\\Original.ino")).is_err());
        let connection = Connection {
            version: VERSION.into(),
            protocol_version: "2025-11-25".into(),
            token: "private-token-must-not-appear".into(),
        };
        assert!(!serde_json::to_string(&connection)
            .unwrap()
            .contains("private-token"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn requires_identified_authenticated_health_before_mcp_dispatch() {
        let mut health = json!({"status":"ok","server":"arduino-agent","version":"0.7.1","uptime":0,"auth":true});
        assert!(authenticated_health(&serde_json::to_vec(&health).unwrap()));
        health["auth"] = json!(false);
        assert!(!authenticated_health(&serde_json::to_vec(&health).unwrap()));
        health["auth"] = json!(true);
        health["version"] = json!("unknown");
        assert!(!authenticated_health(&serde_json::to_vec(&health).unwrap()));
        assert!(!authenticated_health(&vec![b' '; 4097]));
    }
    #[test]
    fn selected_document_decodes_uri_and_requires_exact_windows_path() {
        let path = Path::new("C:\\Users\\á name\\Sketch\\Sketch.ino");
        assert!(selected_document(
            "file:///c%3A/Users/%C3%A1%20name/Sketch/Sketch.ino",
            path
        ));
        assert!(selected_document(
            "file:///C:/Users/%C3%A1%20name/Sketch/Sketch.ino",
            path
        ));
        assert!(!selected_document(
            "file:///C:/Users/%C3%A1%20name/Other/Sketch.ino",
            path
        ));
        assert!(!selected_document(
            "https://example.invalid/Sketch.ino",
            path
        ));
        assert!(!selected_document("file://server/share/Sketch.ino", path));
    }
    #[test]
    fn accepted_files_must_match_both_approved_sources() {
        let root = root();
        let input = input();
        fs::write(root.join("Original.ino"), &input.original_source).unwrap();
        fs::write(root.join("Sketch.ino"), &input.source).unwrap();
        let result = EditResult {
            schema_version: 1,
            job_id: input.job_id,
            agent_version: VERSION.into(),
            protocol_version: "2025-11-25".into(),
            artifacts: ["Original.ino".into(), "Sketch.ino".into()],
        };
        validate_artifacts(&root, &input, &result).unwrap();
        fs::write(root.join("Sketch.ino"), "changed").unwrap();
        assert!(validate_artifacts(&root, &input, &result).is_err());
        fs::write(root.join("Sketch.ino"), &input.source).unwrap();
        fs::write(root.join("Original.ino"), "changed").unwrap();
        assert!(validate_artifacts(&root, &input, &result).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Requires the authenticated pinned Agent in an owned Windows guest"]
    fn actual_owned_agent_edits_only_copy_and_verifies_selected_document() {
        assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let connection = runtime.block_on(connect()).unwrap();
        let root = root();
        let input = input();
        let original = root.join("Original.ino");
        fs::write(&original, &input.original_source).unwrap();
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let result = edit(
            &connection,
            &stage,
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        validate_artifacts(&stage, &input, &result).unwrap();
        assert_eq!(
            fs::read_to_string(&original).unwrap(),
            input.original_source
        );
        let cancelled = root.join("cancelled");
        fs::create_dir(&cancelled).unwrap();
        let mut cancelled_input = input.clone();
        cancelled_input.job_id = Uuid::new_v4();
        assert!(edit(
            &connection,
            &cancelled,
            &cancelled_input,
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(fs::read_dir(&cancelled).unwrap().next().is_none());
        fs::write("C:\\AgentProbe\\output\\adapter-result.json",serde_json::to_vec_pretty(&json!({"pass":true,"result":result,"original_preserved":true,"cancel_before_effects":true,"root":root})).unwrap()).unwrap();
    }
}
