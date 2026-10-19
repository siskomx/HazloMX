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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub live_pcb_version: u32,
    pub job_id: Uuid,
    pub board_name: String,
    pub description: String,
    pub units: crate::kicad::Units,
    pub before: Observation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movement: Option<crate::konnect::FootprintMove>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub export_copy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sha256: Option<String>,
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        self.before.validate(self.before.job_id)?;
        if self.export_copy {
            if self.movement.is_some() { return Err("Exportar requiere una propuesta separada sin movimiento.".into()); }
        } else {
            let movement=self.movement.as_ref().ok_or("Falta el movimiento aprobado.")?;
            movement.validate()?;
            let target=self.before.components.iter().find(|c| c.uuid==movement.uuid
                && c.reference==movement.reference).ok_or("Falta el componente de la lectura nativa.")?;
            if target.locked || target.x_mm!=movement.expected_x_mm || target.y_mm!=movement.expected_y_mm {
                return Err("El componente requiere una lectura actual y editable.".into());
            }
        }
        if self.schema_version!=1 || self.live_pcb_version!=1 || self.job_id.is_nil()
            || self.board_name.is_empty() || self.board_name.len()>1024
            || self.board_name.chars().any(|c|c.is_control() || matches!(c,'/'|'\\'|':'))
            || !self.board_name.ends_with(".kicad_pcb")
            || self.description.trim().is_empty() || self.description.len()>1024
            || self.description.chars().any(char::is_control)
            || self.receipt_sha256.as_ref().is_some_and(|s|!crate::kicad_project::valid_hash(s))
            || serde_json::to_vec(self).map_err(|_|"Propuesta PCB no compatible.")?.len()>if self.receipt_sha256.is_some() {60128} else {60000}
        {
            return Err("La propuesta PCB requiere una lectura actual y un componente editable.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub native_pcb_version: u32,
    pub job_id: Uuid,
    pub status: String,
    pub before_sha256: String,
    pub after: serde_json::Value,
    pub native_commit_confirmed: bool,
    pub saved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_sha256: Option<String>,
}
pub fn validate_result(input:&Input, result:&Receipt) -> Result<(),String> {
    input.validate()?;
    if result.native_pcb_version!=1 || result.job_id!=input.job_id || result.status!="completed"
        || result.before_sha256!=input.before.state_sha256 || result.saved
        || result.native_commit_confirmed==input.export_copy
        || if input.export_copy {result.exported_sha256.as_deref()!=Some(input.before.state_sha256.as_str())} else {result.exported_sha256.is_some()}
    {
        return Err("KiCad no confirmó el cambio aprobado.".into());
    }
    let mut after=result.after.clone();
    let map=after.as_object_mut().ok_or("Falta la lectura final nativa.")?;
    if map.len()!=5 || map.contains_key("job_id") || map.contains_key("native_pcb_version") {
        return Err("La lectura final nativa no es compatible.".into());
    }
    map.insert("job_id".into(),serde_json::json!(input.job_id));
    map.insert("native_pcb_version".into(),serde_json::json!(1));
    let after:Observation=serde_json::from_value(after).map_err(|_|"La lectura final nativa no es compatible.")?;
    after.validate(input.job_id)?;
    if after.instance_sha256!=input.before.instance_sha256
        || after.document_sha256!=input.before.document_sha256
        || (after.state_sha256==input.before.state_sha256)!=input.export_copy
    {
        return Err("La identidad o el estado final de la PCB no coincide.".into());
    }
    let mut expected=input.before.components.clone();
    if let Some(movement)=&input.movement {
        let target=expected.iter_mut().find(|c|c.uuid==movement.uuid).ok_or("Falta el componente aprobado.")?;
        target.x_mm=movement.x_mm; target.y_mm=movement.y_mm;
    }
    expected.sort_by_key(|c|c.uuid);
    let mut actual=after.components;actual.sort_by_key(|c|c.uuid);
    if actual!=expected { return Err("La lectura final no coincide con el movimiento aprobado.".into()); }
    Ok(())
}

pub(crate) fn validate_artifacts(directory:&Path, input:&Input, result:&Receipt) -> Result<(),String> {
    validate_result(input,result)?;
    if input.export_copy {
        let bytes=crate::kicad_project::read_regular(&directory.join("Board.kicad_pcb"),2*1024*1024)?;
        if crate::kicad_project::digest(&bytes)!=input.before.state_sha256 {
            return Err("La PCB exportada no coincide con el estado aprobado.".into());
        }
    }
    Ok(())
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
    crate::project::valid_location(board).map_err(|_|"Selecciona una PCB local válida.")?;
    let job = Uuid::new_v4();
    let request=serde_json::json!({"action":"inspect","job_id":job,"board_path":board});
    let bytes=invoke(worker,&request,connection,profile,cancel,None,None)?;
    parse_observation(&bytes,job).map_err(|_|"KiCad no confirmó el estado de la PCB abierta. No se sustituirá por el archivo guardado.".into())
}

struct Execution<'a>{verifier:&'a Path,stage:&'a Path,input:&'a Input}
fn completion_identity(input:&Input)->Result<String,String>{
    let mut input=input.clone();input.receipt_sha256=None;
    serde_json::to_vec(&input).map(|bytes|crate::kicad_project::digest(&bytes)).map_err(|_|"No se pudo identificar la operación PCB.".into())
}
fn invoke(
    worker:&[u8], request:&serde_json::Value, connection:&Connection, profile:&Path,
    cancel:Arc<AtomicBool>, execution:Option<Execution<'_>>, dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>,
) -> Result<Vec<u8>,String> {
    use crate::{kicad_project as source, process::{run_owned_input, StopReason}};
    const FAILED: &str = "KiCad no confirmó el estado de la PCB abierta. No se sustituirá por el archivo guardado.";
    connection.validate()?;
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
    let request=serde_json::to_vec(request).map_err(|_|FAILED)?;
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
    let verifier=execution.as_ref().map(|e|e.verifier);
    let _stage_pins=execution.as_ref().map(|e|crate::project::pin_lineage(e.stage).map_err(|_|FAILED)).transpose()?;
    if let Some(execution)=&execution {
        variables.insert("HAZLOMEXA_PCB_COMPLETION".into(),execution.stage.join("pcb-completion.json").into_os_string());
        variables.insert("HAZLOMEXA_PCB_INPUT_SHA256".into(),completion_identity(execution.input)?.into());
    }
    let _verifier_pins=verifier.map(|path|crate::project::pin_lineage(path.parent().ok_or(FAILED)?)
        .map_err(|_|FAILED)).transpose()?;
    let _verifier=verifier.map(|path|source::open_regular(path,512*1024*1024)).transpose().map_err(|_|FAILED)?;
    if let Some(path)=verifier {
        variables.insert("HAZLOMEXA_PCB_VERIFIER".into(),path.as_os_str().to_owned());
    }
    let output=if let Some(dispatch)=dispatch {
        let permit=dispatch.enter()?;
        crate::process::run_owned_input_permitted(&executable,&[],profile,&variables,cancel.clone(),Duration::from_secs(30),(&request,permit))
    } else {run_owned_input(&executable,&[],profile,&variables,cancel.clone(),Duration::from_secs(30),&request)}.map_err(|_|FAILED)?;
    if cancel.load(Ordering::SeqCst) || output.reason == StopReason::Cancelled {
        return Err("Lectura nativa cancelada.".into());
    }
    if output.reason != StopReason::Completed || output.exit_code != Some(0)
        || output.truncated || !output.stderr.is_empty() || output.stdout.len() > 60000
    {
        return Err(FAILED.into());
    }
    Ok(output.stdout)
}

#[derive(Clone)]
pub(crate) struct Binding {
    pub board:std::path::PathBuf,
    pub connection:Connection,
}
pub(crate) fn run_gated(input:&Input,binding:&Binding,stage:&Path,profile:&Path,cancel:Arc<AtomicBool>,dispatch:&Arc<crate::live_dispatch::Dispatch>)->Result<(),String>{
    let dispatch=Some(dispatch);
    input.validate()?;
    binding.connection.validate()?;
    crate::project::valid_location(&binding.board).map_err(|_|"La PCB seleccionada cambió.")?;
    fs::create_dir(profile).map_err(|_|"No se pudo preparar la operación nativa.")?;
    #[cfg(not(test))]
    let verifier=std::env::current_exe().map_err(|_|"Falta el verificador nativo de Hazlo.")?;
    #[cfg(test)]
    let verifier=Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("target/debug/hazlomexa.exe");
    let mut request=serde_json::json!({"action":"move","job_id":input.job_id,"board_path":binding.board,
        "instance_sha256":input.before.instance_sha256,"document_sha256":input.before.document_sha256,
        "state_sha256":input.before.state_sha256,"movement":input.movement});
    if input.export_copy {
        request["action"]=serde_json::json!("export");
        request.as_object_mut().unwrap().remove("movement");
        request["destination"]=serde_json::json!(stage.join("Board.kicad_pcb"));
    }
    let failed="KiCad no confirmó el resultado de la edición. Vuelve a leer el documento antes de continuar; no repitas la operación a ciegas.";
    let bytes=invoke(bundled_worker()?,&request,&binding.connection,profile,cancel,Some(Execution{verifier:&verifier,stage,input}),dispatch)
        .map_err(|_|failed)?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reply { ok:bool, result:Receipt }
    let reply:Reply=serde_json::from_slice(&bytes).map_err(|_|failed)?;
    if !reply.ok {return Err(failed.into());}
    validate_artifacts(stage,input,&reply.result).map_err(|_|failed)?;
    let report=serde_json::to_vec_pretty(&reply.result).map_err(|_|failed)?;
    crate::kicad_project::write_new(&stage.join("result.json"),&report).map_err(|_|failed)?;
    Ok(())
}

pub(crate) fn recover_result(input:&Input,directory:&Path)->Result<bool,String>{
    input.validate()?;
    let _pins=crate::project::pin_lineage(directory).map_err(|_|"No se pudo fijar el resultado PCB local.")?;
    let path=directory.join("pcb-completion.json");
    if !path.try_exists().map_err(|_|"No se pudo comprobar el recibo PCB.")?{return Err("Falta el recibo original de finalización de KiCad. Se conserva la pausa; no repitas la edición.".into());}
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Completion{version:u32,input_sha256:String,result:Receipt}
    let bytes=crate::kicad_project::read_regular(&path,65536)?;
    let proof:Completion=serde_json::from_slice(&bytes).map_err(|_|"El recibo PCB no es compatible; se conserva la pausa.")?;
    if proof.version!=1 || proof.input_sha256!=completion_identity(input)?{return Err("El recibo PCB pertenece a otra operación; se conserva la pausa.".into());}
    validate_artifacts(directory,input,&proof.result)?;
    let report=serde_json::to_vec_pretty(&proof.result).map_err(|_|"No se pudo conservar el recibo PCB.")?;
    let result=directory.join("result.json");
    if result.try_exists().map_err(|_|"No se pudo comprobar el informe PCB local.")?{
        let saved:serde_json::Value=serde_json::from_slice(&crate::kicad_project::read_regular(&result,65536)?).map_err(|_|"El informe PCB cambió; se conserva sin reemplazarlo.")?;
        if saved!=serde_json::to_value(&proof.result).map_err(|_|"No se pudo comprobar el recibo PCB.")?{return Err("El informe PCB cambió; se conserva sin reemplazarlo.".into());}
    }else{crate::kicad_project::write_new(&result,&report).map_err(|_|"El recibo sigue local; no se pudo conservar su informe. No repitas la edición.")?;}
    Ok(true)
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
    pub(crate) fn move_fixture() -> Input {
        let id=Uuid::parse_str("1390739f-0b91-4d50-b8ae-92acd725ec51").unwrap();
        let mut before=fixture();
        before.components.push(Component {uuid:id,reference:"R1".into(),value:"10k".into(),x_mm:10.,y_mm:20.,locked:false});
        Input {schema_version:1,live_pcb_version:1,job_id:Uuid::new_v4(),board_name:"Owned.kicad_pcb".into(),description:"Move the resistor right by one inch".into(),
            units:crate::kicad::Units::In,before,export_copy:false,movement:Some(crate::konnect::FootprintMove {
                uuid:id,reference:"R1".into(),expected_x_mm:10.,expected_y_mm:20.,x_mm:35.4,y_mm:22.5
            }),receipt_sha256:None}
    }
    pub(crate) fn receipt_fixture(input:&Input) -> Receipt {
        let mut after=serde_json::to_value(&input.before).unwrap();
        let map=after.as_object_mut().unwrap();
        map.remove("job_id");map.remove("native_pcb_version");
        if let Some(movement)=&input.movement {
            after["state_sha256"]=json!("d".repeat(64));
            after["components"][0]["x_mm"]=json!(movement.x_mm);
            after["components"][0]["y_mm"]=json!(movement.y_mm);
        }
        Receipt {native_pcb_version:1,job_id:input.job_id,status:"completed".into(),
            before_sha256:input.before.state_sha256.clone(),after,native_commit_confirmed:!input.export_copy,saved:false,
            exported_sha256:input.export_copy.then(||input.before.state_sha256.clone())}
    }
    #[test]
    fn recovery_requires_the_exact_retained_completion_and_preserves_existing_reports(){
        for scenario in ["missing","pending","wrong_input","wrong_job","mismatch","changed_report","success"]{
            let root=std::env::temp_dir().join(format!("hazlo-pcb-proof-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();let input=move_fixture();let mut receipt=receipt_fixture(&input);
            if scenario=="wrong_job"{receipt.job_id=Uuid::new_v4();}else if scenario=="mismatch"{receipt.after["components"][0]["value"]=json!("manual change");}
            let proof=json!({"version":1,"input_sha256":if scenario=="wrong_input"{"f".repeat(64)}else{completion_identity(&input).unwrap()},"result":receipt});
            if !matches!(scenario,"missing"|"pending"){fs::write(root.join("pcb-completion.json"),serde_json::to_vec(&proof).unwrap()).unwrap();}else if scenario=="pending"{fs::write(root.join("pcb-completion.json.pending"),serde_json::to_vec(&proof).unwrap()).unwrap();}
            if scenario=="changed_report"{fs::write(root.join("result.json"),b"preserve manual report").unwrap();}
            let result=recover_result(&input,&root);
            if scenario=="success"{assert!(result.unwrap());let before=fs::read(root.join("result.json")).unwrap();assert!(recover_result(&input,&root).unwrap());assert_eq!(fs::read(root.join("result.json")).unwrap(),before);let saved:Receipt=crate::project::read_json(&root.join("result.json")).unwrap();validate_result(&input,&saved).unwrap();}
            else{assert!(result.is_err(),"{scenario}");if scenario=="changed_report"{assert_eq!(fs::read(root.join("result.json")).unwrap(),b"preserve manual report");}else{assert!(!root.join("result.json").exists());}}
            assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn exports_require_separate_approval_and_exact_preserved_native_state_and_bytes() {
        let mut input=move_fixture();
        let legacy=serde_json::to_value(&input).unwrap();
        assert!(legacy.get("export_copy").is_none());
        serde_json::from_value::<Input>(legacy).unwrap().validate().unwrap();
        input.export_copy=true;
        assert!(input.validate().is_err());
        input.movement=None;
        input.validate().unwrap();
        for field in ["changed_state","commit","saved","missing_hash","wrong_hash","changed_component"] {
            let mut receipt=receipt_fixture(&input);
            match field {
                "changed_state"=>receipt.after["state_sha256"]=json!("d".repeat(64)),
                "commit"=>receipt.native_commit_confirmed=true,
                "saved"=>receipt.saved=true,
                "missing_hash"=>receipt.exported_sha256=None,
                "wrong_hash"=>receipt.exported_sha256=Some("d".repeat(64)),
                _=>receipt.after["components"][0]["value"]=json!("changed"),
            }
            assert!(validate_result(&input,&receipt).is_err(),"{field}");
        }
        input.export_copy=false;
        assert!(input.validate().is_err());
    }
    #[test]
    fn approved_moves_and_receipts_require_exact_native_identity_and_geometry() {
        let input=move_fixture();
        input.validate().unwrap();
        validate_result(&input,&receipt_fixture(&input)).unwrap();
        for change in ["locked","stale_position","invalid_state","noop","sub_nm"] {
            let mut bad=input.clone();
            match change {
                "locked"=>bad.before.components[0].locked=true,
                "stale_position"=>bad.movement.as_mut().unwrap().expected_x_mm=9.,
                "invalid_state"=>bad.before.state_sha256="bad".into(),
                "noop"=>{bad.movement.as_mut().unwrap().x_mm=10.;bad.movement.as_mut().unwrap().y_mm=20.;},
                _=>bad.movement.as_mut().unwrap().x_mm=35.4000004,
            }
            assert!(bad.validate().is_err(),"{change}");
        }
        for field in ["wrong_job","wrong_before","wrong_instance","wrong_document","same_state","other_value","unconfirmed","saved"] {
            let mut bad=receipt_fixture(&input);
            match field {
                "wrong_job"=>bad.job_id=Uuid::new_v4(),
                "wrong_before"=>bad.before_sha256="e".repeat(64),
                "wrong_instance"=>bad.after["instance_sha256"]=json!("e".repeat(64)),
                "wrong_document"=>bad.after["document_sha256"]=json!("e".repeat(64)),
                "same_state"=>bad.after["state_sha256"]=json!(input.before.state_sha256),
                "other_value"=>bad.after["components"][0]["value"]=json!("unapproved"),
                "unconfirmed"=>bad.native_commit_confirmed=false,
                _=>bad.saved=true,
            }
            assert!(validate_result(&input,&bad).is_err(),"{field}");
        }
        let mut raw=serde_json::to_value(&input).unwrap();raw["board_path"]=json!("C:/private/board.kicad_pcb");
        assert!(serde_json::from_value::<Input>(raw).is_err());
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
