//! Arduino Agent's native editor companion with revision-bound, human-approved edits.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json,Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
};
use uuid::Uuid;

const SOURCE_LIMIT: usize = 65536;
const FRAME_LIMIT: usize = 512 * 1024;

// Connection secrets are deliberately neither serializable nor printable.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    editor_version: u32,
    instance_id: Uuid,
    pipe: String,
    token: String,
}
impl Connection {
    pub(crate) fn validate_for(&self, before: &Observation) -> Result<(), Failure> {
        self.validate()?;
        before.validate()?;
        if self.instance_id == before.instance_id {
            Ok(())
        } else {
            Err(Failure::Refused)
        }
    }
    fn validate(&self) -> Result<(), Failure> {
        if self.editor_version != 1
            || self.instance_id.is_nil()
            || self.pipe != format!(r"\\.\pipe\hazlomexa-arduino-{}", self.instance_id)
            || self.token.len() != 64
            || !self.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Failure::Refused);
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, Failure> {
        crate::project::valid_location(path).map_err(|_| Failure::Refused)?;
        let _parents = crate::project::pin_lineage(path.parent().ok_or(Failure::Refused)?)
            .map_err(|_| Failure::Refused)?;
        let bytes = crate::kicad_project::read_regular(path, 4096).map_err(|_| Failure::Refused)?;
        let connection: Self = serde_json::from_slice(&bytes).map_err(|_| Failure::Refused)?;
        connection.validate()?;
        Ok(connection)
    }
}

pub fn default_directory() -> Result<PathBuf, String> {
    let profile = std::env::var_os("USERPROFILE")
        .ok_or("No se encontró el perfil local de Arduino Agent.")?;
    let root = PathBuf::from(profile).join(".arduinoIDE/hazlomexa-editor");
    crate::project::valid_location(&root)
        .map_err(|_| "El perfil de Arduino Agent debe ser local.")?;
    Ok(root)
}

pub(crate) const COMPANION_FILES: [(&str, &[u8]); 3] = [
    ("package.json", include_bytes!("../resources/arduino-editor/package.json")),
    ("extension.cjs", include_bytes!("../resources/arduino-editor/extension.cjs")),
    ("core.cjs", include_bytes!("../resources/arduino-editor/core.cjs")),
];

/// Explicit desktop setup only; agents cannot install or replace editor plugins.
pub fn install_companion(profile: &Path) -> Result<bool, String> {
    use std::{fs, io::Write};
    let invalid = "Abre Arduino Agent una vez antes de instalar su complemento. Su perfil debe estar en una carpeta local sin enlaces.";
    let _profile = crate::project::pin_lineage(profile).map_err(|_| invalid)?;
    let plugins = profile.join("plugins");
    match fs::create_dir(&plugins) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err("No se pudo preparar la carpeta de complementos de Arduino Agent.".into()),
    }
    let _plugins = crate::project::pin_directory(&plugins, false).map_err(|_| invalid)?;
    let destination = plugins.join("hazlomexa-arduino-editor");
    if destination.try_exists().map_err(|_| invalid)? || profile.join(".hazlomexa-editor-update.json").try_exists().map_err(|_|invalid)? {
        return crate::agent_companion::upgrade(profile,&COMPANION_FILES);
    }
    // Publish the manifest with the complete directory, never an incomplete plugin.
    let staging = profile.join(format!(".hazlomexa-editor-install-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|_| "No se pudo preparar el complemento local.")?;
    let result = (|| -> std::io::Result<()> {
        for (name, bytes) in COMPANION_FILES {
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(staging.join(name))?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        fs::rename(&staging, &destination)
    })();
    if result.is_err() {
        for (name, _) in COMPANION_FILES { let _ = fs::remove_file(staging.join(name)); }
        let _ = fs::remove_dir(&staging);
        return Err("No se pudo instalar el complemento. No se reemplazó ningún complemento existente.".into());
    }
    Ok(true)
}

#[derive(Clone, Serialize)]
pub struct DiscoveredEditor {
    pub id: Uuid,
    pub observation: Observation,
    #[serde(skip)]
    connection: Connection,
}
impl DiscoveredEditor {
    pub fn connection(&self) -> &Connection {
        &self.connection
    }
}

/// Read-only connection discovery; it never installs a plugin, deletes records or edits a sketch.
pub async fn discover(
    root: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<DiscoveredEditor>, String> {
    let invalid = "No se pudieron leer los registros locales del complemento de Arduino Agent.";
    if cancel.load(Ordering::SeqCst) {
        return Err("Conexión cancelada.".into());
    }
    crate::project::valid_location(root).map_err(|_| invalid)?;
    let _parents = match crate::project::pin_lineage(root) {
        Ok(pins) => pins,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(invalid.into()),
    };
    let mut tasks = tokio::task::JoinSet::new();
    for (count, entry) in std::fs::read_dir(root).map_err(|_| invalid)?.enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("Conexión cancelada.".into());
        }
        if count >= 64 {
            return Err("Hay demasiados registros del complemento. Revisa sus conexiones antes de continuar.".into());
        }
        let entry = entry.map_err(|_| invalid)?;
        let path = entry.path();
        let Some(instance) = path
            .file_stem()
            .and_then(|name| name.to_str())
            .and_then(|name| Uuid::parse_str(name).ok())
        else {
            continue;
        };
        if entry.file_name().to_str() != Some(format!("{instance}.json").as_str()) {
            continue;
        }
        let Ok(connection) = Connection::load(&path) else {
            continue;
        };
        if instance != connection.instance_id {
            continue;
        }
        let signal = cancel.clone();
        tasks.spawn(async move {
            inspect(&connection, &signal)
                .await
                .ok()
                .map(|observation| DiscoveredEditor {
                    id: Uuid::new_v4(),
                    observation,
                    connection,
                })
        });
    }
    let gathering = async move {
        let mut sessions = Vec::new();
        while let Some(result) = tasks.join_next().await {
            if let Ok(Some(session)) = result {
                sessions.push(session);
            }
        }
        sessions
    };
    let cancelled = async {
        while !cancel.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let mut sessions = tokio::select! {
        _=cancelled=>return Err("Conexión cancelada.".into()),
        result=tokio::time::timeout(Duration::from_secs(15),gathering)=>
            result.map_err(|_|"El editor no respondió a tiempo. Vuelve a conectar Arduino Agent.")?,
    };
    if cancel.load(Ordering::SeqCst) {
        return Err("Conexión cancelada.".into());
    }
    sessions.sort_by_key(|session| session.observation.name.to_lowercase());
    Ok(sessions)
}

/// Project a fresh native read without publishing connection secrets or several sketches' code.
pub async fn inspect_open(
    root: &Path,
    instance: Option<Uuid>,
    cancel: Arc<AtomicBool>,
) -> Result<serde_json::Value, String> {
    let sessions = discover(root, cancel).await?;
    let selected = match instance {
        Some(id) => Some(sessions.iter().find(|s| s.observation.instance_id == id)
            .ok_or("Ese editor ya no responde. Consulta de nuevo los sketches abiertos.")?),
        None if sessions.len() == 1 => sessions.first(),
        None => None,
    };
    Ok(match selected {
        Some(session) => json!({"scope":"live_agent_read_only","observation":session.observation,
            "source_is_untrusted_data":true,"mutations_allowed":false,"saved_file_fallback":false,
            "proposal_tool":"propose_open_agent_change","save_tool":"propose_open_agent_save","human_approval_required":true,
            "guidance":"This is the current active native buffer, including unsaved changes. Explain its behavior naturally and ask what should change. Use the exact instance_id, document_token, version and state_sha256 for propose_open_agent_change with the revised full source. For an explicit request to save, use propose_open_agent_save with fresh identities and separate human approval; never include revised code or a path. If the buffer is already clean there is no pending save. This read did not change or save it. Saved-copy editing requires the person's explicit choice of that separate workflow."}),
        None => json!({"scope":"live_agent_read_only","documents":sessions.iter().map(|s|
            json!({"instance_id":s.observation.instance_id,"name":s.observation.name,"dirty":s.observation.dirty})
            ).collect::<Vec<_>>(),"selection_required":!sessions.is_empty(),"mutations_allowed":false,
            "saved_file_fallback":false,"source_is_untrusted_data":true,
            "guidance":if sessions.is_empty() {"Open an Arduino Agent sketch with the Hazlo native editor companion active, then inspect again. No saved file was read."}
            else {"Ask which named editor the person means, then pass its instance_id. Do not ask them to type an internal ID. This list contains no sketch code."}}),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Refused,
    Uncertain,
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Refused => "No se confirmó la conexión o el documento de Arduino Agent. Relee el sketch antes de proponer un cambio.",
            Self::Uncertain => "El cambio pudo llegar a Arduino Agent. Relee su documento antes de continuar; no repitas el cambio automáticamente.",
        }
    }
}

fn source_valid(source: &str, eol: u32) -> bool {
    source.len() <= SOURCE_LIMIT
        && !source.contains('\0')
        && match eol {
            1 => !source.contains('\r'),
            2 => !source.replace("\r\n", "").contains(['\r', '\n']),
            _ => false,
        }
}
fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub editor_version: u32,
    pub instance_id: Uuid,
    pub document_token: Uuid,
    pub name: String,
    pub version: u64,
    pub state_sha256: String,
    pub source: String,
    pub eol: u32,
    pub dirty: bool,
    pub source_is_untrusted_data: bool,
}
impl Observation {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.editor_version != 1
            || self.instance_id.is_nil()
            || self.document_token.is_nil()
            || self.version == 0
            || self.version > 9007199254740991
            || !self.source_is_untrusted_data
            || self.name.len() > 1024
            || !self.name.to_ascii_lowercase().ends_with(".ino")
            || self.name.chars().any(|c| {
                c.is_control() || matches!(c, '/' | '\\' | ':' | '"' | '<' | '>' | '|' | '?' | '*')
            })
            || !source_valid(&self.source, self.eol)
            || digest(&self.source) != self.state_sha256
        {
            return Err(Failure::Refused);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub editor_version: u32,
    pub job_id: Uuid,
    pub status: String,
    pub before_sha256: String,
    pub after: Observation,
    pub native_edit_confirmed: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub native_save_confirmed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub live_agent_version: u32,
    pub job_id: Uuid,
    pub description: String,
    pub before: Observation,
    pub source: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub save: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sha256: Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agreement:Option<crate::native_tools::Agreement>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub native_completion_version:Option<u32>,
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(agreement)=&self.agreement{agreement.validate()?;}
        self.before.validate().map_err(|error| error.message())?;
        if self.schema_version != 1
            || self.native_completion_version.is_some_and(|version|version!=1)
            || self.live_agent_version != 1
            || self.job_id.is_nil()
            || (!self.save && self.before.version >= 9007199254740991)
            || self.description.trim().is_empty()
            || self.description.len() > 1024
            || self.description.chars().any(char::is_control)
            || if self.save { self.source != self.before.source || !self.before.dirty } else { self.source == self.before.source }
            || !source_valid(&self.source, self.before.eol)
            || self
                .receipt_sha256
                .as_ref()
                .is_some_and(|hash| !crate::kicad_project::valid_hash(hash))
            || serde_json::to_vec(self)
                .map_err(|_| "Propuesta no compatible.")?
                .len()
                > 900000
        {
            return Err(
                "La propuesta requiere el sketch abierto y un cambio de código revisable.".into(),
            );
        }
        Ok(())
    }
}

pub fn validate_report(input: &Input, report: &serde_json::Value) -> Result<(), String> {
    input.validate()?;
    let mut expanded = report.clone();
    let after = expanded
        .get_mut("after")
        .and_then(|value| value.as_object_mut())
        .ok_or("Falta la lectura final del sketch.")?;
    if after.contains_key("source") {
        return Err("Informe nativo no compatible.".into());
    }
    // The journal retains the approved source; the small report retains its independently verified native digest.
    after.insert("source".into(), json!(input.source));
    let receipt: Receipt =
        serde_json::from_value(expanded).map_err(|_| "Informe nativo no compatible.")?;
    receipt
        .validate_operation(input.job_id, &input.before, &input.source, input.save)
        .map_err(|error| error.message().into())
}

pub(crate) fn run_gated(input:&Input,connection:&Connection,stage:&Path,cancel:&AtomicBool,dispatch:&Arc<crate::live_dispatch::Dispatch>)->Result<(),String>{
    let dispatch=Some(dispatch);
    input.validate()?;
    connection
        .validate_for(&input.before)
        .map_err(|error| error.message())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "No se pudo iniciar la conexión al editor.")?;
    let receipt = if input.save {
        runtime.block_on(save_with_dispatch(connection,input.job_id,&input.before,cancel,dispatch))
    } else { runtime.block_on(apply_with_dispatch(
            connection,
            input.job_id,
            &input.before,
            &input.source,
            cancel,
            dispatch,
        )) }.map_err(|error| error.message())?;
    let report=normalized_report(input,receipt)?;
    dispatch.ok_or("Falta el envío original del editor.")?.retain_agent_completion(input,&report)?;
    retain_report(&report,stage,false)
}
fn normalized_report(input:&Input,receipt:Receipt)->Result<Value,String>{
    receipt.validate_operation(input.job_id,&input.before,&input.source,input.save).map_err(|error|error.message())?;
    let mut report = serde_json::to_value(receipt).map_err(|_| Failure::Uncertain.message())?;
    report["after"]
        .as_object_mut()
        .ok_or(Failure::Uncertain.message())?
        .remove("source");
    validate_report(input, &report)?;
    Ok(report)
}
fn retain_report(report:&Value,stage:&Path,recovery:bool)->Result<(),String>{
    if recovery && stage.join("result.json").try_exists().map_err(|_|"No se pudo comprobar el resultado local.")?{
        let saved:serde_json::Value=crate::project::read_json(&stage.join("result.json")).map_err(|_|"El informe local cambió; se conserva sin reemplazarlo.")?;
        return if &saved==report{Ok(())}else{Err("El informe local no coincide con el recibo nativo. Se conserva el trabajo y la pausa.".into())};
    }
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| Failure::Uncertain.message())?;
    crate::kicad_project::write_new(&stage.join("result.json"), &bytes)
        .map_err(|_| Failure::Uncertain.message().to_string())
}
pub(crate) fn recover_result(input:&Input,connection:&Connection,stage:&Path,cancel:&AtomicBool)->Result<bool,String>{
    input.validate()?;
    if recover_recorded_result(stage,input)?{return Ok(true);}
    let record=crate::live_dispatch::read(stage,&crate::project::OperationInput::LiveAgent(input.clone()))?;
    if record.as_ref().is_some_and(|record|record.outcome==crate::native_journal::Outcome::Pending) || input.native_completion_version==Some(1) && record.is_none(){return Err("Falta el envío original admitido del editor; no se adoptó otro resultado.".into());}
    connection.validate_for(&input.before).map_err(|error|error.message())?;
    let mut original=json!({"action":if input.save{"save"}else{"apply"},"job_id":input.job_id,"instance_id":input.before.instance_id,"document_token":input.before.document_token,"version":input.before.version,"state_sha256":input.before.state_sha256});
    if !input.save{original["source"]=json!(input.source);}
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_|"No se pudo iniciar la recuperación nativa.")?;
    let value:serde_json::Value=runtime.block_on(exchange(connection,json!({"action":"recover","original":original}),false,cancel,Duration::from_secs(15))).map_err(|_|"El editor no conserva un resultado comprobado de esa operación. Se conserva el trabajo y la pausa; no repitas la edición.")?;
    if value==json!({"editor_recovery_version":1,"status":"running"}){return Ok(false);}
    let receipt:Receipt=serde_json::from_value(value).map_err(|_|"El recibo nativo no es compatible; se conserva la pausa.")?;
    let report=normalized_report(input,receipt)?;
    if record.is_some(){crate::live_dispatch::retain_agent_completion(stage,input,&report)?;}
    retain_report(&report,stage,true)?;Ok(true)
}

fn completion_identity(input:&Input)->Result<String,String>{let mut input=input.clone();input.receipt_sha256=None;Ok(crate::kicad_project::digest(&serde_json::to_vec(&input).map_err(|_|"Entrada del editor no válida.")?))}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion{version:u32,input_sha256:String,result:Value}
pub(crate) fn write_native_completion(directory:&Path,input:&Input,report:&Value)->Result<String,String>{
    validate_report(input,report)?;
    let _pins=crate::project::pin_lineage(directory).map_err(|_|"No se pudo fijar el recibo local del editor.")?;
    let bytes=serde_json::to_vec(&Completion{version:1,input_sha256:completion_identity(input)?,result:report.clone()}).map_err(|_|"No se pudo conservar el recibo del editor.")?;
    if bytes.len()>65536{return Err("El recibo del editor supera el límite local; no repitas la edición.".into());}
    let path=directory.join("agent-completion.json");
    if path.try_exists().map_err(|_|"No se pudo comprobar el recibo del editor.")?{if crate::kicad_project::read_regular(&path,65536)?!=bytes{return Err("El recibo original del editor cambió; se conserva sin reemplazarlo.".into());}}
    else{crate::kicad_project::write_new(&path,&bytes).map_err(|_|"La respuesta del editor no pudo conservarse; no repitas la edición.")?;}
    Ok(crate::kicad_project::digest(&bytes))
}
pub(crate) fn confirmed_completion(directory:&Path,input:&Input,hash:&str)->Result<Value,String>{
    let _pins=crate::project::pin_lineage(directory).map_err(|_|"No se pudo fijar el recibo del editor.")?;
    let _pin=crate::kicad_project::open_regular(&directory.join("agent-completion.json"),65536)?;
    let bytes=crate::kicad_project::read_regular(&directory.join("agent-completion.json"),65536)?;
    if !crate::kicad_project::valid_hash(hash) || crate::kicad_project::digest(&bytes)!=hash{return Err("El recibo del editor cambió; se conserva la pausa sin repetir la operación.".into());}
    let proof:Completion=serde_json::from_slice(&bytes).map_err(|_|"Recibo del editor no compatible.")?;
    if proof.version!=1 || proof.input_sha256!=completion_identity(input)?{return Err("El recibo no pertenece a la operación aprobada del editor.".into());}
    validate_report(input,&proof.result)?;Ok(proof.result)
}
pub(crate) fn recover_recorded_result(directory:&Path,input:&Input)->Result<bool,String>{
    let Some(record)=crate::live_dispatch::read(directory,&crate::project::OperationInput::LiveAgent(input.clone()))? else{return Ok(false);};
    let Some(hash)=record.agent_completion_sha256 else{return Ok(false);};
    let report=confirmed_completion(directory,input,&hash)?;
    let bytes=serde_json::to_vec_pretty(&report).map_err(|_|"Recibo del editor no compatible.")?;
    let path=directory.join("result.json");
    if path.try_exists().map_err(|_|"No se pudo comprobar el informe del editor.")?{if crate::kicad_project::read_regular(&path,65536)?!=bytes{return Err("El informe del editor cambió; se conserva sin reemplazarlo.".into());}}
    else{crate::project::atomic_json(&path,&report,false).map_err(|_|"El recibo sigue local; no se pudo recuperar su informe. No repitas la edición.")?;}Ok(true)
}
impl Receipt {
    pub fn validate(&self, job: Uuid, before: &Observation, source: &str) -> Result<(), Failure> {
        self.validate_operation(job,before,source,false)
    }
    fn validate_operation(&self, job: Uuid, before: &Observation, source: &str, saving: bool) -> Result<(), Failure> {
        let valid = before.validate().is_ok()
            && self.after.validate().is_ok()
            && !job.is_nil()
            && self.editor_version == 1
            && self.job_id == job
            && self.status == "completed"
            && self.native_edit_confirmed != saving
            && self.native_save_confirmed == saving
            && self.before_sha256 == before.state_sha256
            && if saving { source == before.source && before.dirty } else { source != before.source }
            && self.after.instance_id == before.instance_id
            && self.after.document_token == before.document_token
            && self.after.name == before.name
            && self.after.eol == before.eol
            && if saving { self.after.version == before.version && !self.after.dirty } else { self.after.version > before.version }
            && self.after.source == source;
        if valid {
            Ok(())
        } else {
            Err(Failure::Uncertain)
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply<T> {
    ok: bool,
    result: Option<T>,
    outcome: Option<String>,
    error: Option<String>,
}

async fn exchange<T: DeserializeOwned>(
    connection: &Connection,
    request: serde_json::Value,
    mutation: bool,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<T, Failure> {
    exchange_with_dispatch(connection,request,mutation,cancel,timeout,None).await
}
async fn exchange_with_dispatch<T:DeserializeOwned>(connection:&Connection,request:serde_json::Value,mutation:bool,cancel:&AtomicBool,timeout:Duration,dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>)->Result<T,Failure>{
    connection.validate()?;
    if cancel.load(Ordering::SeqCst) {
        return Err(Failure::Refused);
    }
    let mut frame = serde_json::to_vec(&json!({"token":connection.token,"request":request}))
        .map_err(|_| Failure::Refused)?;
    frame.push(b'\n');
    if frame.len() > FRAME_LIMIT {
        return Err(Failure::Refused);
    }
    let mut pipe=ClientOptions::new().open(&connection.pipe).map_err(|_|Failure::Refused)?;
    let mut transfer={
        let permit=dispatch.map(|d|d.enter()).transpose().map_err(|_|Failure::Refused)?;
        let transfer=tokio::spawn(async move {
            pipe.write_all(&frame).await.map_err(|_|())?;
            let mut bytes=Vec::new();
            (&mut pipe).take((FRAME_LIMIT+1) as u64).read_to_end(&mut bytes).await.map_err(|_|())?;
            if bytes.len()>FRAME_LIMIT{return Err(());}
            Ok::<Vec<u8>,()>(bytes)
        });
        drop(permit);
        transfer
    };
    let sent=mutation;
    let cancelled=async {while !cancel.load(Ordering::SeqCst){tokio::time::sleep(Duration::from_millis(20)).await;}};
    let bytes=tokio::select! {
        _=cancelled=>None,
        result=tokio::time::timeout(timeout,&mut transfer)=>result.ok().and_then(Result::ok).and_then(Result::ok),
    };
    transfer.abort();
    let reply=bytes.and_then(|bytes|serde_json::from_slice::<Reply<T>>(&bytes).ok());
    let failure = if sent {
        Failure::Uncertain
    } else {
        Failure::Refused
    };
    match reply {
        Some(Reply {
            ok: true,
            result: Some(value),
            outcome: None,
            error: None,
        }) => Ok(value),
        Some(Reply {
            ok: false,
            result: None,
            outcome: Some(outcome),
            error: Some(error),
        }) if outcome == "refused" && error == "native_editor_not_confirmed" => {
            Err(Failure::Refused)
        }
        _ => Err(failure),
    }
}

pub async fn inspect(connection: &Connection, cancel: &AtomicBool) -> Result<Observation, Failure> {
    let result: Observation = exchange(
        connection,
        json!({"action":"inspect"}),
        false,
        cancel,
        Duration::from_secs(15),
    )
    .await?;
    result.validate()?;
    if result.instance_id != connection.instance_id {
        return Err(Failure::Refused);
    }
    Ok(result)
}

// Callers must obtain human approval bound to this exact observation before calling apply.
pub async fn apply(
    connection: &Connection,
    job: Uuid,
    before: &Observation,
    source: &str,
    cancel: &AtomicBool,
) -> Result<Receipt, Failure> {
    apply_with_dispatch(connection,job,before,source,cancel,None).await
}
async fn apply_with_dispatch(connection:&Connection,job:Uuid,before:&Observation,source:&str,cancel:&AtomicBool,dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>)->Result<Receipt,Failure>{
    before.validate()?;
    if job.is_nil()
        || before.instance_id != connection.instance_id
        || source == before.source
        || !source_valid(source, before.eol)
    {
        return Err(Failure::Refused);
    }
    let receipt:Receipt = exchange_with_dispatch(connection, json!({"action":"apply","job_id":job,
        "instance_id":before.instance_id,"document_token":before.document_token,"version":before.version,
        "state_sha256":before.state_sha256,"source":source}), true, cancel, Duration::from_secs(15),dispatch).await?;
    receipt.validate(job, before, source)?;
    Ok(receipt)
}

// A save is a separately approved effect; it never changes source or supplies a path.
pub async fn save(connection:&Connection,job:Uuid,before:&Observation,cancel:&AtomicBool) -> Result<Receipt,Failure> {
    save_with_dispatch(connection,job,before,cancel,None).await
}
async fn save_with_dispatch(connection:&Connection,job:Uuid,before:&Observation,cancel:&AtomicBool,dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>)->Result<Receipt,Failure>{
    connection.validate_for(before)?;
    if job.is_nil() || !before.dirty { return Err(Failure::Refused); }
    let receipt:Receipt=exchange_with_dispatch(connection,json!({"action":"save","job_id":job,
        "instance_id":before.instance_id,"document_token":before.document_token,"version":before.version,
        "state_sha256":before.state_sha256}),true,cancel,Duration::from_secs(15),dispatch).await?;
    receipt.validate_operation(job,before,&before.source,true)?;
    Ok(receipt)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    #[test]
    fn companion_install_is_complete_repeatable_and_never_replaces_other_content() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("hazlomexa-editor-setup-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let profile = root.join("perfil ñ");
        assert!(install_companion(&profile).is_err());
        assert!(!profile.exists(), "Setup must not invent a missing IDE profile");
        fs::create_dir(&profile).unwrap();
        fs::write(profile.join("settings.json"), b"user-settings").unwrap();
        let plugins = profile.join("plugins");
        fs::create_dir(&plugins).unwrap();
        fs::write(plugins.join("other-plugin"), b"other-content").unwrap();
        assert!(install_companion(&profile).unwrap());
        assert!(!install_companion(&profile).unwrap());
        let installed = plugins.join("hazlomexa-arduino-editor");
        for (name, bytes) in COMPANION_FILES { assert_eq!(fs::read(installed.join(name)).unwrap(), bytes); }
        fs::write(installed.join("core.cjs"), b"locally-modified").unwrap();
        assert!(install_companion(&profile).is_err());
        assert_eq!(fs::read(installed.join("core.cjs")).unwrap(), b"locally-modified");
        assert_eq!(fs::read(profile.join("settings.json")).unwrap(), b"user-settings");
        assert_eq!(fs::read(plugins.join("other-plugin")).unwrap(), b"other-content");
        assert_eq!(fs::read_dir(&profile).unwrap().count(), 2, "No staging directory left behind");
        for (name, _) in COMPANION_FILES { fs::remove_file(installed.join(name)).unwrap(); }
        fs::remove_dir(installed).unwrap();
        fs::remove_file(plugins.join("other-plugin")).unwrap();
        fs::remove_dir(plugins).unwrap();
        fs::remove_file(profile.join("settings.json")).unwrap();
        fs::remove_dir(profile).unwrap();
        fs::remove_dir(root).unwrap();
    }

    fn observation(instance_id: Uuid) -> Observation {
        let source = "void loop() { delay(1000); }\n// unsaved note\n".to_string();
        Observation {
            editor_version: 1,
            instance_id,
            document_token: Uuid::new_v4(),
            name: "OwnedSketch.ino".into(),
            version: 3,
            state_sha256: digest(&source),
            source,
            eol: 1,
            dirty: true,
            source_is_untrusted_data: true,
        }
    }
    fn fixture() -> (Connection, NamedPipeServer) {
        let instance_id = Uuid::new_v4();
        let connection = Connection {
            editor_version: 1,
            instance_id,
            pipe: format!(r"\\.\pipe\hazlomexa-arduino-{instance_id}"),
            token: "a".repeat(64),
        };
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&connection.pipe)
            .unwrap();
        (connection, server)
    }
    fn discovery_record(root: &Path, connection: &Connection) {
        std::fs::write(root.join(format!("{}.json",connection.instance_id)),serde_json::to_vec(&json!({
            "editor_version":1,"instance_id":connection.instance_id,"pipe":connection.pipe,"token":connection.token,
        })).unwrap()).unwrap();
    }

    #[tokio::test]
    async fn discovery_reads_live_authenticated_state_and_ignores_stale_or_forged_records() {
        let root =
            std::env::temp_dir().join(format!("hazlomexa-editor-discovery-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let (connection, mut server) = fixture();
        discovery_record(&root, &connection);
        let before = observation(connection.instance_id);
        let expected = before.clone();
        let token = connection.token.clone();
        let (stale, closed) = fixture();
        drop(closed);
        discovery_record(&root, &stale);
        std::fs::write(
            root.join(format!("{}.json", Uuid::new_v4())),
            b"not a connection",
        )
        .unwrap();
        std::fs::write(
            root.join("Sketch.ino"),
            b"Saved text is not live editor state",
        )
        .unwrap();
        let forged = root.join(format!("{}.json", Uuid::new_v4()));
        std::fs::write(
            &forged,
            serde_json::to_vec(
                &json!({"editor_version":1,"instance_id":connection.instance_id,
            "pipe":connection.pipe,"token":connection.token}),
            )
            .unwrap(),
        )
        .unwrap();
        let worker = tokio::spawn(async move {
            assert_eq!(
                receive(&mut server).await,
                json!({"token":token,"request":{"action":"inspect"}})
            );
            server
                .write_all(&serde_json::to_vec(&json!({"ok":true,"result":expected})).unwrap())
                .await
                .unwrap();
        });
        let sessions = discover(&root, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap();
        worker.await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].observation.state_sha256, before.state_sha256);
        sessions[0].connection().validate_for(&before).unwrap();
        let public = serde_json::to_value(&sessions).unwrap();
        assert!(public[0].get("connection").is_none());
        assert!(!public.to_string().contains(&connection.token));
        assert!(!public.to_string().contains(&connection.pipe));
        assert_eq!(
            std::fs::read(root.join("Sketch.ino")).unwrap(),
            b"Saved text is not live editor state"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 5);
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancelled_discovery_returns_no_partial_selection_and_preserves_records() {
        let root =
            std::env::temp_dir().join(format!("hazlomexa-editor-discovery-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let (connection, mut server) = fixture();
        discovery_record(&root, &connection);
        let cancel = Arc::new(AtomicBool::new(true));
        assert!(discover(&root, cancel.clone()).await.is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(50), server.connect())
                .await
                .is_err()
        );
        cancel.store(false, Ordering::SeqCst);
        let signal = cancel.clone();
        let (stop, done) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            signal.store(true, Ordering::SeqCst);
            let _ = done.await;
        });
        assert!(discover(&root, cancel).await.is_err());
        stop.send(()).unwrap();
        worker.await.unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn absent_and_overfull_registries_are_bounded_without_creating_or_deleting_them() {
        assert!(default_directory()
            .unwrap()
            .ends_with(".arduinoIDE/hazlomexa-editor"));
        let root =
            std::env::temp_dir().join(format!("hazlomexa-editor-discovery-{}", Uuid::new_v4()));
        assert!(discover(&root, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap()
            .is_empty());
        assert!(!root.exists());
        std::fs::create_dir(&root).unwrap();
        for index in 0..65 {
            std::fs::write(root.join(format!("note-{index}")), b"retained").unwrap();
        }
        assert!(discover(&root, Arc::new(AtomicBool::new(false)))
            .await
            .is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 65);
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }
    async fn receive(server: &mut NamedPipeServer) -> serde_json::Value {
        server.connect().await.unwrap();
        let mut bytes = Vec::new();
        while bytes.len() <= FRAME_LIMIT {
            let byte = server.read_u8().await.unwrap();
            if byte == b'\n' {
                return serde_json::from_slice(&bytes).unwrap();
            }
            bytes.push(byte);
        }
        panic!("Request must be bounded");
    }
    fn receipt(job: Uuid, before: &Observation, source: &str) -> serde_json::Value {
        let mut after = before.clone();
        after.version += 1;
        after.source = source.into();
        after.state_sha256 = digest(source);
        json!({"editor_version":1,"job_id":job,"status":"completed","before_sha256":before.state_sha256,
            "after":after,"native_edit_confirmed":true})
    }

    pub(crate) fn job_fixture() -> Input {
        let before = observation(Uuid::new_v4());
        Input {native_completion_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),
            schema_version: 1,
            live_agent_version: 1,
            job_id: Uuid::new_v4(),
            description: "Make the LED blink faster".into(),
            source: before.source.replace("1000", "500"),
            save: false,
            before,
            receipt_sha256: None,
        }
    }
    pub(crate) fn connection_fixture(input: &Input) -> Connection {
        Connection {
            editor_version: 1,
            instance_id: input.before.instance_id,
            pipe: format!(r"\\.\pipe\hazlomexa-arduino-{}", input.before.instance_id),
            token: "b".repeat(64),
        }
    }
    pub(crate) fn report_fixture(input: &Input) -> serde_json::Value {
        let mut report = if input.save {
            let mut after=input.before.clone();after.dirty=false;
            json!({"editor_version":1,"job_id":input.job_id,"status":"completed","before_sha256":input.before.state_sha256,
                "after":after,"native_edit_confirmed":false,"native_save_confirmed":true})
        } else { receipt(input.job_id, &input.before, &input.source) };
        report["after"].as_object_mut().unwrap().remove("source");
        report
    }

    #[test]
    fn native_save_requires_unchanged_dirty_source_and_a_distinct_confirmed_receipt() {
        let legacy=job_fixture();
        let json=serde_json::to_value(&legacy).unwrap();
        assert!(json.get("save").is_none());
        assert!(!serde_json::from_value::<Input>(json).unwrap().save);
        let mut input=legacy;
        input.save=true;input.source=input.before.source.clone();
        input.validate().unwrap();
        let report=report_fixture(&input);validate_report(&input,&report).unwrap();
        for field in ["dirty","version","document_token","state_sha256"] {
            let mut altered=report.clone();
            altered["after"][field]=match field {"dirty"=>json!(true),"version"=>json!(input.before.version+1),
                "document_token"=>json!(Uuid::new_v4()),_=>json!("0".repeat(64))};
            assert!(validate_report(&input,&altered).is_err(),"{field}");
        }
        let mut altered=report.clone();altered["native_save_confirmed"]=json!(false);
        assert!(validate_report(&input,&altered).is_err());
        altered=report;altered["native_edit_confirmed"]=json!(true);
        assert!(validate_report(&input,&altered).is_err());
        input.source.push_str("// changed");assert!(input.validate().is_err());
        input.source=input.before.source.clone();input.before.dirty=false;assert!(input.validate().is_err());
    }

    #[test]
    fn full_size_escaped_sketches_fit_the_journal_and_small_report_without_losing_verification() {
        let mut input = job_fixture();
        input.before.source = "\u{1}".repeat(SOURCE_LIMIT);
        input.before.state_sha256 = digest(&input.before.source);
        input.source = "\u{2}".repeat(SOURCE_LIMIT);
        input.validate().unwrap();
        let report = report_fixture(&input);
        assert!(serde_json::to_vec_pretty(&report).unwrap().len() < 65536);
        assert!(!report["after"].as_object().unwrap().contains_key("source"));
        validate_report(&input, &report).unwrap();
        let mut changed = report.clone();
        changed["after"]["state_sha256"] = json!("0".repeat(64));
        assert!(validate_report(&input, &changed).is_err());
        changed = report;
        changed["after"]["source"] = json!(input.source);
        assert!(validate_report(&input, &changed).is_err());
    }

    #[test]
    fn untrusted_native_observations_need_exact_source_identity_and_bounds() {
        let before = observation(Uuid::new_v4());
        before.validate().unwrap();
        let original = serde_json::to_value(&before).unwrap();
        for (field, value) in [
            ("editor_version", json!(2)),
            ("instance_id", json!(Uuid::nil())),
            ("document_token", json!(Uuid::nil())),
            ("name", json!("../foreign.ino")),
            ("version", json!(0)),
            ("version", json!(9007199254740992u64)),
            ("state_sha256", json!("0".repeat(64))),
            ("source_is_untrusted_data", json!(false)),
            ("source", json!("x".repeat(SOURCE_LIMIT + 1))),
            ("eol", json!(2)),
        ] {
            let mut value_copy = original.clone();
            value_copy[field] = value;
            let invalid: Observation = serde_json::from_value(value_copy).unwrap();
            assert_eq!(invalid.validate(), Err(Failure::Refused), "{field}");
        }
        assert!(!source_valid("bad\0", 1));
        assert!(!source_valid("a\rb", 1));
        assert!(source_valid("a\r\nb\r\n", 2));
        assert!(source_valid("", 1));
    }

    #[test]
    fn completed_receipts_bind_the_job_original_and_exact_native_result() {
        let before = observation(Uuid::new_v4());
        let job = Uuid::new_v4();
        let source = before.source.replace("1000", "500");
        let original = receipt(job, &before, &source);
        serde_json::from_value::<Receipt>(original.clone())
            .unwrap()
            .validate(job, &before, &source)
            .unwrap();
        for (field, value) in [
            ("job_id", json!(Uuid::new_v4())),
            ("before_sha256", json!("0".repeat(64))),
            ("status", json!("failed")),
            ("native_edit_confirmed", json!(false)),
            ("editor_version", json!(2)),
        ] {
            let mut invalid = original.clone();
            invalid[field] = value;
            assert_eq!(
                serde_json::from_value::<Receipt>(invalid)
                    .unwrap()
                    .validate(job, &before, &source),
                Err(Failure::Uncertain)
            );
        }
        for (field, value) in [
            ("instance_id", json!(Uuid::new_v4())),
            ("document_token", json!(Uuid::new_v4())),
            ("name", json!("Other.ino")),
            ("version", json!(before.version)),
            ("source", json!("changed by user\n")),
        ] {
            let mut invalid = original.clone();
            invalid["after"][field] = value;
            assert_eq!(
                serde_json::from_value::<Receipt>(invalid)
                    .unwrap()
                    .validate(job, &before, &source),
                Err(Failure::Uncertain)
            );
        }
    }
    #[tokio::test]
    async fn confirmed_edit_and_save_survive_report_failure_after_the_pipe_closes(){
        use std::{fs,sync::{Mutex,atomic::AtomicUsize}};
        for saving in [false,true]{
            let root=std::env::temp_dir().join(format!("hazlo-agent-completion-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();let(connection,mut server)=fixture();let mut input=job_fixture();input.native_completion_version=Some(1);input.before=observation(connection.instance_id);input.save=saving;input.source=if saving{input.before.source.clone()}else{input.before.source.replace("1000","500")+"// PRIVATE_NATIVE_SOURCE_SENTINEL\n"};let mut reply=report_fixture(&input);reply["after"]["source"]=json!(input.source);let expected=input.clone();let writes=Arc::new(AtomicUsize::new(0));let count=writes.clone();
            let native=tokio::spawn(async move{let message=receive(&mut server).await;assert_eq!(message["request"]["action"],if saving{"save"}else{"apply"});assert_eq!(message["request"]["job_id"],json!(expected.job_id));count.fetch_add(1,Ordering::SeqCst);server.write_all(&serde_json::to_vec(&json!({"ok":true,"result":reply})).unwrap()).await.unwrap();});
            let operation=crate::project::OperationInput::LiveAgent(input.clone());let fence=crate::live_dispatch::Dispatch::start(&root,&operation,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();fs::write(root.join("result.json"),b"preserve occupied report").unwrap();let directory=root.clone();let checked=input.clone();let connected=connection.clone();let dispatch=fence.clone();let result=tokio::task::spawn_blocking(move||run_gated(&checked,&connected,&directory,&AtomicBool::new(false),&dispatch)).await.unwrap();native.await.unwrap();assert!(result.is_err());assert_eq!(writes.load(Ordering::SeqCst),1);assert_eq!(fs::read(root.join("result.json")).unwrap(),b"preserve occupied report");let record=crate::live_dispatch::read(&root,&operation).unwrap().unwrap();assert!(record.agent_completion_sha256.is_some());assert_eq!(record.summary(&operation).acknowledged,1);assert_eq!(record.summary(&operation).uncertain,0);assert!(record.summary(&operation).needs_review);assert!(fence.enter().is_err());
            let proof=fs::read_to_string(root.join("agent-completion.json")).unwrap();assert!(!proof.contains("PRIVATE_NATIVE_SOURCE_SENTINEL")&&!proof.contains(&connection.token));fs::rename(root.join("result.json"),root.join("preserved.native")).unwrap();let mut disconnected=connection;disconnected.editor_version=0;assert!(recover_result(&input,&disconnected,&root,&AtomicBool::new(false)).unwrap());assert!(recover_result(&input,&disconnected,&root,&AtomicBool::new(false)).unwrap());assert_eq!(writes.load(Ordering::SeqCst),1);let report:Value=crate::project::read_json(&root.join("result.json")).unwrap();validate_report(&input,&report).unwrap();assert_eq!(fs::read(root.join("preserved.native")).unwrap(),b"preserve occupied report");drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn completion_scope_preserves_legacy_inputs_and_requires_original_admission(){
        use std::{fs,sync::Mutex};
        use std::os::windows::fs::OpenOptionsExt;
        let mut input=job_fixture();let legacy=serde_json::to_value(&input).unwrap();assert!(legacy.get("native_completion_version").is_none());assert_eq!(serde_json::to_value(serde_json::from_value::<Input>(legacy.clone()).unwrap()).unwrap(),legacy);let old=completion_identity(&input).unwrap();input.native_completion_version=Some(1);input.validate().unwrap();assert_ne!(completion_identity(&input).unwrap(),old);let mut changed=input.clone();changed.native_completion_version=Some(2);assert!(changed.validate().is_err());
        let root=std::env::temp_dir().join(format!("hazlo-agent-anchor-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();let report=report_fixture(&input);let original=crate::project::OperationInput::LiveAgent(input.clone());let hash=write_native_completion(&root,&input,&report).unwrap();assert!(!recover_recorded_result(&root,&input).unwrap());assert!(crate::live_dispatch::retain_agent_completion(&root,&input,&report).is_err());let fence=crate::live_dispatch::Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();assert!(fence.retain_agent_completion(&input,&report).is_err());drop(fence.enter().unwrap());
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(root.join("live-dispatch.json")).unwrap();assert!(fence.retain_agent_completion(&input,&report).is_err());assert!(crate::live_dispatch::read(&root,&original).unwrap().unwrap().agent_completion_sha256.is_none());assert_eq!(crate::kicad_project::digest(&fs::read(root.join("agent-completion.json")).unwrap()),hash);drop(lock);
        fence.retain_agent_completion(&input,&report).unwrap();fence.retain_agent_completion(&input,&report).unwrap();assert_eq!(crate::live_dispatch::read(&root,&original).unwrap().unwrap().agent_completion_sha256.as_deref(),Some(hash.as_str()));changed=input.clone();changed.agreement.as_mut().unwrap().outcome.push_str(" changed");assert!(crate::live_dispatch::read(&root,&crate::project::OperationInput::LiveAgent(changed)).is_err());assert!(fence.enter().is_err());drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn recovery_pipe_looks_up_exact_native_attempts_and_preserves_local_reports(){
        for scenario in ["edit","save","running","wrong_job","changed_report"]{
            let root=std::env::temp_dir().join(format!("hazlo-editor-recovery-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
            let (connection,mut server)=fixture();let mut input=job_fixture();input.before=observation(connection.instance_id);input.source=input.before.source.replace("1000","500");
            if scenario=="save"{input.save=true;input.source=input.before.source.clone();}
            let mut raw=report_fixture(&input);raw["after"]["source"]=json!(input.source);if scenario=="wrong_job"{raw["job_id"]=json!(Uuid::new_v4());}
            let manual=if scenario=="changed_report"{let mut changed=report_fixture(&input);changed["after"]["version"]=json!(input.before.version+100);validate_report(&input,&changed).unwrap();let bytes=serde_json::to_vec_pretty(&changed).unwrap();std::fs::write(root.join("result.json"),&bytes).unwrap();Some(bytes)}else{None};
            let expected=input.clone();let token=connection.token.clone();let worker=tokio::spawn(async move{
                let message=receive(&mut server).await;let mut original=json!({"action":if expected.save{"save"}else{"apply"},"job_id":expected.job_id,"instance_id":expected.before.instance_id,"document_token":expected.before.document_token,"version":expected.before.version,"state_sha256":expected.before.state_sha256});if !expected.save{original["source"]=json!(expected.source);}
                assert_eq!(message,json!({"token":token,"request":{"action":"recover","original":original}}));
                let result=if scenario=="running"{json!({"editor_recovery_version":1,"status":"running"})}else{raw};server.write_all(&serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap()).await.unwrap();
            });
            let directory=root.clone();let checked=input.clone();let result=tokio::task::spawn_blocking(move||recover_result(&checked,&connection,&directory,&AtomicBool::new(false))).await.unwrap();worker.await.unwrap();
            match scenario{
                "running"=>{assert!(!result.unwrap());assert!(!root.join("result.json").exists());},
                "wrong_job"=>{assert!(result.is_err());assert!(!root.join("result.json").exists());},
                "changed_report"=>{assert!(result.is_err());assert_eq!(std::fs::read(root.join("result.json")).unwrap(),manual.unwrap());},
                _=>{assert!(result.unwrap());let report:serde_json::Value=crate::project::read_json(&root.join("result.json")).unwrap();assert!(report["after"].get("source").is_none());validate_report(&input,&report).unwrap();},
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn actual_pipe_inspection_and_apply_validate_the_full_native_contract() {
        let (connection, mut server) = fixture();
        let before = observation(connection.instance_id);
        let expected = before.clone();
        let token = connection.token.clone();
        let worker = tokio::spawn(async move {
            let request = receive(&mut server).await;
            assert_eq!(
                request,
                json!({"token":token,"request":{"action":"inspect"}})
            );
            server
                .write_all(&serde_json::to_vec(&json!({"ok":true,"result":expected})).unwrap())
                .await
                .unwrap();
        });
        let observed = inspect(&connection, &AtomicBool::new(false)).await.unwrap();
        worker.await.unwrap();
        assert_eq!(observed.state_sha256, before.state_sha256);
        assert!(observed.dirty);
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&connection.pipe)
            .unwrap();
        let job = Uuid::new_v4();
        let source = before.source.replace("1000", "500");
        let result = receipt(job, &before, &source);
        let expected = before.clone();
        let edited = source.clone();
        let worker = tokio::spawn(async move {
            let request = receive(&mut server).await;
            assert_eq!(
                request["request"],
                json!({"action":"apply","job_id":job,"instance_id":expected.instance_id,
                "document_token":expected.document_token,"version":expected.version,"state_sha256":expected.state_sha256,"source":edited})
            );
            server
                .write_all(&serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap())
                .await
                .unwrap();
        });
        let result = apply(&connection, job, &before, &source, &AtomicBool::new(false))
            .await
            .unwrap();
        worker.await.unwrap();
        assert_eq!(result.after.source, source);
        assert!(result.after.source.contains("unsaved note"));
    }

    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    async fn live_chat_correction_fences_pipe_dispatch_and_preserves_inflight_reply() {
        use crate::{jobs::Workflow,project::{Project,Phase,OperationInput},tasks::Request};
        use std::{future::Future,sync::atomic::AtomicUsize};
        for saving in [false,true] {for scenario in ["before","during","stop"] {
            let root=std::env::temp_dir().join(format!("hazlo-agent-correction-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
            assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
            let workflow=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap());workflow.select_project(Project::create(&root).unwrap()).unwrap();
            let selected=workflow.view().unwrap().project.unwrap();let task=workflow.task_state().unwrap().unwrap().active;
            workflow.task_request(selected.clone(),Request::Message{id:task.id,revision:task.revision,text:"Preserve the sketch and make the light blink faster".into(),history:json!({"version":1,"turns":[]}).to_string(),correction_of:None}).unwrap();
            let task=workflow.task_state().unwrap().unwrap().active;
            let (connection,mut server)=fixture();let mut input=job_fixture();input.before=observation(connection.instance_id);input.save=saving;input.source=if saving{input.before.source.clone()}else{input.before.source.replace("1000","500")};let id=input.job_id;
            let pending=workflow.propose_live_agent(selected.clone(),input.clone(),connection).unwrap();let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
            let correction="Keep my manual changes; make the next part smaller";
            let request=||Request::Message{id:task.id,revision:task.revision,text:correction.into(),history:json!({"version":1,"turns":[{"role":"user","content":correction}]}).to_string(),correction_of:Some(id)};
            if scenario=="before" {workflow.task_request(selected.clone(),request()).unwrap();}
            let (ready,ready_rx)=tokio::sync::oneshot::channel();let (received,received_rx)=tokio::sync::oneshot::channel();let (release,release_rx)=tokio::sync::oneshot::channel();let mut release=Some(release);
            let writes=Arc::new(AtomicUsize::new(0));let observed=writes.clone();let native=input.clone();
            let server_task=tokio::spawn(async move {
                {
                    let mut connecting=Box::pin(server.connect());let mut ready=Some(ready);
                    std::future::poll_fn(|cx|{let result=connecting.as_mut().poll(cx);if let Some(signal)=ready.take(){let _=signal.send(());}result}).await.unwrap();
                }
                let mut frame=Vec::new();
                loop {let mut byte=[0];let count=server.read(&mut byte).await.unwrap();if count==0{return;}
                    if byte[0]==b'\n'{break;}frame.push(byte[0]);assert!(frame.len()<FRAME_LIMIT);}
                let request:serde_json::Value=serde_json::from_slice(&frame).unwrap();assert_eq!(request["request"]["job_id"],id.to_string());
                observed.fetch_add(1,Ordering::SeqCst);let _=received.send(());let _=release_rx.await;
                let mut result=report_fixture(&native);result["after"]["source"]=json!(native.source);
                let _=server.write_all(&serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap()).await;
            });
            ready_rx.await.unwrap();let runner=workflow.clone();let worker=tokio::task::spawn_blocking(move||runner.execute(dispatch));
            if scenario!="before" {
                tokio::time::timeout(Duration::from_secs(5),received_rx).await.unwrap().unwrap();
                let started=std::time::Instant::now();workflow.task_request(selected.clone(),request()).unwrap();assert!(started.elapsed()<Duration::from_secs(1),"Correction waited for the native reply");
                if scenario=="stop" {workflow.cancel(id).unwrap();}else{release.take().unwrap().send(()).unwrap();}
            }
            let done=tokio::time::timeout(Duration::from_secs(5),worker).await.unwrap().unwrap().unwrap();
            assert_eq!(done.snapshot.phase,if scenario=="during"{Phase::Completed}else{Phase::Cancelled});
            assert_eq!(writes.load(Ordering::SeqCst),usize::from(scenario!="before"));drop(release);server_task.await.unwrap();
            let summary=workflow.native_recovery().unwrap();assert_eq!(summary[0].needs_review,scenario=="stop");assert_eq!(summary[0].not_dispatched,usize::from(scenario=="before"));
            assert_eq!(workflow.task_state().unwrap().unwrap().active.latest_correction.unwrap().text,correction);
            assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
            assert_eq!(matches!(done.snapshot.input,OperationInput::LiveAgent(ref i) if i.receipt_sha256.is_some()),scenario=="during");
            drop(workflow);let project=Project::open(&root).unwrap();let recovered=project.recover().unwrap();assert_eq!(recovered[0].phase,done.snapshot.phase);assert_eq!(project.native_recovery(None).unwrap()[0].needs_review,scenario=="stop");drop(project);
            std::fs::remove_dir_all(root).unwrap();
        }}
    }

    #[tokio::test]
    async fn refusal_is_distinct_from_lost_malformed_or_unconfirmed_edit_replies() {
        for (reply, expected) in [
            (
                Some(json!({"ok":false,"outcome":"refused","error":"native_editor_not_confirmed"})),
                Failure::Refused,
            ),
            (
                Some(
                    json!({"ok":false,"outcome":"uncertain","error":"native_editor_not_confirmed"}),
                ),
                Failure::Uncertain,
            ),
            (
                Some(json!({"ok":true,"result":{"unexpected":"reply"}})),
                Failure::Uncertain,
            ),
            (None, Failure::Uncertain),
        ] {
            let (connection, mut server) = fixture();
            let before = observation(connection.instance_id);
            let worker = tokio::spawn(async move {
                receive(&mut server).await;
                if let Some(reply) = reply {
                    server
                        .write_all(&serde_json::to_vec(&reply).unwrap())
                        .await
                        .unwrap();
                }
            });
            let result = apply(
                &connection,
                Uuid::new_v4(),
                &before,
                &before.source.replace("1000", "500"),
                &AtomicBool::new(false),
            )
            .await;
            assert_eq!(result.unwrap_err(), expected);
            worker.await.unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_before_dispatch_never_connects_but_after_dispatch_is_uncertain() {
        let (connection, mut server) = fixture();
        let before = observation(connection.instance_id);
        let source = before.source.replace("1000", "500");
        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        assert_eq!(
            apply(&connection, Uuid::new_v4(), &before, &source, &cancel)
                .await
                .unwrap_err(),
            Failure::Refused
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), server.connect())
                .await
                .is_err()
        );
        cancel.store(false, Ordering::SeqCst);
        let signal = cancel.clone();
        let (stop, done) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            signal.store(true, Ordering::SeqCst);
            let _ = done.await;
        });
        assert_eq!(
            apply(&connection, Uuid::new_v4(), &before, &source, &cancel)
                .await
                .unwrap_err(),
            Failure::Uncertain
        );
        stop.send(()).unwrap();
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn held_and_oversized_replies_cannot_escape_the_transport_limits() {
        let (connection, mut server) = fixture();
        let (stop, done) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            let _ = done.await;
        });
        let result = exchange::<Receipt>(
            &connection,
            json!({"action":"apply"}),
            true,
            &AtomicBool::new(false),
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(result.unwrap_err(), Failure::Uncertain);
        stop.send(()).unwrap();
        worker.await.unwrap();
        let (connection, mut server) = fixture();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            let _ = server.write_all(&vec![b' '; FRAME_LIMIT + 1]).await;
        });
        assert_eq!(
            inspect(&connection, &AtomicBool::new(false))
                .await
                .unwrap_err(),
            Failure::Refused
        );
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn discovery_refuses_remote_or_mismatched_pipes_and_oversized_files() {
        let (mut connection, _server) = fixture();
        connection.pipe = r"\\remote\pipe\other".into();
        assert_eq!(connection.validate(), Err(Failure::Refused));
        connection.pipe = format!(r"\\.\pipe\hazlomexa-arduino-{}", Uuid::new_v4());
        assert_eq!(connection.validate(), Err(Failure::Refused));
        let root =
            std::env::temp_dir().join(format!("hazlomexa-native-discovery-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("connection.json");
        let (connection, _server) = fixture();
        std::fs::write(&path,serde_json::to_vec(&json!({"editor_version":1,"instance_id":connection.instance_id,"pipe":connection.pipe,"token":connection.token})).unwrap()).unwrap();
        assert_eq!(
            Connection::load(&path).unwrap().instance_id,
            connection.instance_id
        );
        std::fs::write(&path, vec![b' '; 4097]).unwrap();
        assert!(Connection::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
