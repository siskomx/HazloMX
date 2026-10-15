//! Native PCB observations through the bundled official-client worker; no saved-file fallback.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::Read, path::Path,
    sync::{atomic::{AtomicBool, Ordering}, Arc}, time::Duration};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub endpoint: String,
    pub token: String,
}
impl Connection {
    pub fn validate(&self) -> Result<(), String> {
        let path = self
            .endpoint
            .strip_prefix("ipc://")
            .ok_or("Copia la dirección IPC local de las preferencias de KiCad.")?;
        // NNG maps this local Windows path into its pipe namespace. Never probe it with a raw pipe client.
        crate::project::valid_location(std::path::Path::new(path))
            .map_err(|_| "La dirección IPC debe ser una ruta local absoluta de Windows.")?;
        if path.len() > 1024
            || path.chars().any(char::is_control)
            || self.token.len() > 4096
            || self.token.chars().any(char::is_control)
        {
            return Err("Conexión IPC local no válida.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub uuid: Uuid,
    pub reference: String,
    pub value: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub locked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub job_id: Uuid,
    pub native_pcb_version: u32,
    pub instance_sha256: String,
    pub document_sha256: String,
    pub state_sha256: String,
    pub components: Vec<Component>,
    pub source_is_untrusted_data: bool,
}
impl Observation {
    fn validate(&self, job: Uuid) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        let mut references = BTreeSet::new();
        if self.job_id != job || job.is_nil() || self.native_pcb_version != 1
            || !self.source_is_untrusted_data
            || [&self.instance_sha256, &self.document_sha256, &self.state_sha256]
                .iter().any(|hash| !crate::kicad_project::valid_hash(hash))
            || self.components.len() > 256
            || self.components.iter().any(|c| c.uuid.is_nil()
                || !ids.insert(c.uuid) || c.reference.is_empty()
                || !references.insert(c.reference.clone())
                || [&c.reference, &c.value].iter()
                    .any(|v| v.len() > 4096 || v.chars().any(char::is_control))
                || [c.x_mm, c.y_mm].iter().any(|v| !v.is_finite() || v.abs() > 10000.))
        {
            return Err("La lectura nativa está incompleta o contiene identidades ambiguas.".into());
        }
        Ok(())
    }
}

fn bundled_worker() -> Result<&'static [u8], String> {
    #[cfg(feature = "desktop")]
    return Ok(include_bytes!("../../../../target/native-pcb-worker/hazlomexa-kicad.exe"));
    #[cfg(not(feature = "desktop"))]
    Err("La lectura nativa requiere el paquete de escritorio con su worker integrado.".into())
}

pub(crate) fn inspect(
    board: &Path, connection: &Connection, profile: &Path, cancel: Arc<AtomicBool>,
) -> Result<Observation, String> {
    inspect_with_worker(bundled_worker()?, board, connection, profile, cancel)
}

fn inspect_with_worker(
    worker: &[u8], board: &Path, connection: &Connection, profile: &Path, cancel: Arc<AtomicBool>,
) -> Result<Observation, String> {
    use crate::{kicad_project as source, process::{run_owned_input, StopReason}};
    const FAILED: &str = "KiCad no confirmó el estado de la PCB abierta. No se sustituirá por el archivo guardado.";
    connection.validate()?;
    crate::project::valid_location(board).map_err(|_| "Selecciona una PCB local válida.")?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Lectura nativa cancelada.".into());
    }
    if worker.is_empty() || worker.len() > 64 * 1024 * 1024 {
        return Err("Falta el worker nativo identificado del paquete.".into());
    }
    let _pins = crate::project::pin_lineage(profile).map_err(|_| FAILED)?;
    let cache = profile.parent().ok_or(FAILED)?
        .join(format!("native-pcb-worker-{:x}", Sha256::digest(worker)));
    match fs::create_dir(&cache) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(_) => return Err(FAILED.into()),
    }
    let _cache_pins = crate::project::pin_lineage(&cache).map_err(|_| FAILED)?;
    let executable = cache.join("hazlomexa-kicad.exe");
    if !executable.try_exists().map_err(|_| FAILED)? {
        source::write_new(&executable, worker).map_err(|_| FAILED)?;
    }
    let mut worker_pin = source::open_regular(&executable, 64 * 1024 * 1024).map_err(|_| FAILED)?;
    let mut stored = Vec::new();
    worker_pin.read_to_end(&mut stored).map_err(|_| FAILED)?;
    if stored != worker { return Err(FAILED.into()); }
    drop(stored);
    let job = Uuid::new_v4();
    let request = serde_json::to_vec(&serde_json::json!({
        "action":"inspect", "job_id":job, "board_path":board
    })).map_err(|_| FAILED)?;
    let mut variables = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            variables.insert(key.into(), value);
        }
    }
    for key in ["TEMP", "TMP", "APPDATA", "LOCALAPPDATA", "USERPROFILE"] {
        variables.insert(key.into(), profile.as_os_str().to_owned());
    }
    variables.insert("KICAD_API_SOCKET".into(), connection.endpoint.clone().into());
    variables.insert("KICAD_API_TOKEN".into(), connection.token.clone().into());
    let output = run_owned_input(&executable, &[], profile, &variables, cancel.clone(),
        Duration::from_secs(30), &request).map_err(|_| FAILED)?;
    if cancel.load(Ordering::SeqCst) || output.reason == StopReason::Cancelled {
        return Err("Lectura nativa cancelada.".into());
    }
    if output.reason != StopReason::Completed || output.exit_code != Some(0)
        || output.truncated || !output.stderr.is_empty() || output.stdout.len() > 60000
    {
        return Err(FAILED.into());
    }
    parse_observation(&output.stdout, job).map_err(|_| FAILED.into())
}

fn parse_observation(bytes: &[u8], job: Uuid) -> Result<Observation, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply {
        ok: bool,
        result: Observation,
    }
    if bytes.len() > 60000 {
        return Err("La lectura nativa supera el límite del chat.".into());
    }
    let reply: Reply = serde_json::from_slice(bytes).map_err(|_| "Respuesta nativa no compatible.")?;
    if !reply.ok {
        return Err("La lectura nativa no se confirmó.".into());
    }
    reply.result.validate(job)?;
    Ok(reply.result)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn fixture() -> Observation {
        serde_json::from_value(json!({"job_id":Uuid::new_v4(),"native_pcb_version":1,
            "instance_sha256":"a".repeat(64),"document_sha256":"b".repeat(64),
            "state_sha256":"c".repeat(64),"components":[],"source_is_untrusted_data":true})).unwrap()
    }
    #[test]
    fn only_local_nng_addresses_and_private_tokens_are_accepted() {
        let good = Connection { endpoint:"ipc://C:\\owned\\kicad\\api.sock".into(), token:"fixture-private-token".into() };
        good.validate().unwrap();
        for endpoint in ["tcp://127.0.0.1:1234","ipc://relative","ipc://\\\\server\\share\\api.sock","ipc://C:\\owned\\api.sock\n"] {
            assert!(Connection {endpoint:endpoint.into(),..good.clone()}.validate().is_err());
        }
        assert!(serde_json::from_value::<Connection>(json!({"endpoint":good.endpoint,"token":good.token,"tool":"move_component"})).is_err());
    }
    #[test]
    fn observations_require_exact_job_identity_and_complete_bounded_native_state() {
        let mut observed = fixture();
        observed.components.push(Component {uuid:Uuid::new_v4(),reference:"R1".into(),
            value:"10k".into(),x_mm:25.4,y_mm:10.,locked:false});
        let good = json!({"ok":true,"result":observed});
        let decode = |value:&serde_json::Value| parse_observation(&serde_json::to_vec(value).unwrap(), observed.job_id);
        assert_eq!(decode(&good).unwrap().components[0].x_mm,25.4);
        for field in ["instance_sha256","document_sha256","state_sha256"] {
            let mut bad=good.clone(); bad["result"][field]=json!("bad"); assert!(decode(&bad).is_err());
        }
        let mut bad=good.clone(); bad["result"]["job_id"]=json!(Uuid::new_v4()); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["result"]["source_is_untrusted_data"]=json!(false); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["result"]["components"][0]["x_mm"]=json!(10001.); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["result"]["components"][0]["uuid"]=json!(Uuid::nil()); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["result"]["components"][0]["reference"]=json!("R1\n"); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["result"]["components"].as_array_mut().unwrap().push(good["result"]["components"][0].clone()); assert!(decode(&bad).is_err());
        bad=good.clone(); bad["path"]=json!("must not open"); assert!(decode(&bad).is_err());
        assert!(parse_observation(br#"{"ok":true,"ok":false,"result":{}}"#, observed.job_id).is_err());
        assert!(parse_observation(&vec![b' ';60001],observed.job_id).is_err());
    }
    #[test]
    #[ignore = "Requires the locally built frozen worker; never contacts a user's editor"]
    fn actual_frozen_worker_refuses_unreachable_editor_without_persisting_request() {
        let workspace=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap();
        let worker=fs::read(workspace.join("target/native-pcb-worker/hazlomexa-kicad.exe")).unwrap();
        let root=workspace.join("target").join(format!("native-pcb-refusal-{}",Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let board=root.join("Fixture.kicad_pcb");
        fs::write(&board,b"saved bytes cannot satisfy a live read").unwrap();
        let secret=format!("owned-secret-{}",Uuid::new_v4());
        let connection=Connection {endpoint:format!("ipc://{}",root.join("absent.sock").display()),token:secret.clone()};
        let error=inspect_with_worker(&worker,&board,&connection,&root,Arc::new(AtomicBool::new(false))).unwrap_err();
        assert!(error.contains("No se sustituirá"),"{error}");
        assert!(!error.contains(&secret));
        assert_eq!(fs::read(board).unwrap(),b"saved bytes cannot satisfy a live read");
        for entry in fs::read_dir(&root).unwrap() {
            let path=entry.unwrap().path();
            if path.is_file() {
                assert!(!fs::read(path).unwrap().windows(secret.len()).any(|w|w==secret.as_bytes()));
            }
        }
    }
}
