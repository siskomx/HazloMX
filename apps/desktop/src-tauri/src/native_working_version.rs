//! FreeCAD's host-owned native copy and isolated reopen check. No agent-supplied paths/code.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
const LIMIT: u64 = 32 * 1024 * 1024;
const CORE: &str = include_str!("../resources/freecad/native_copy.py");
const POLICY: &str = include_str!("../resources/freecad/cadpilot_policy.py");
const CHECK:&str="import json,hashlib,sys\nfrom pathlib import Path\nprofile=Path(PROFILE_FROM_HOST)\nsys.path.insert(0,str(profile))\nfrom native_copy import verify\nroot=Path.cwd()\nexpected=json.loads((root/'capture.json').read_text(encoding='utf-8'))\nresult=verify(root/expected['file']['name'],expected)\npayload=json.dumps(result,allow_nan=False,sort_keys=True,separators=(',',':')).encode()\nwith open(profile/'produced.json','xb') as output: output.write(payload)\nprint('HAZLO_VERSION_NONCE:'+hashlib.sha256(payload).hexdigest())\n";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub verifier_sha256: String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub reader_budget_version:Option<u32>,
}
impl Plan {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::kicad_project::valid_hash(&self.verifier_sha256) || self.reader_budget_version.is_some_and(|version|version!=1) {
            return Err("El verificador nativo no tiene una identidad válida.".into());
        }
        Ok(())
    }
}
pub fn plan(executable: &Path) -> Result<Plan, String> {
    Ok(Plan{verifier_sha256:crate::kicad_project::digest(&crate::kicad_project::read_regular(executable,LIMIT).map_err(|_|"No se encontró FreeCADCmd para comprobar la nueva versión nativa; conserva el trabajo y revisa la instalación detectada.")?),reader_budget_version:Some(1)})
}
pub(crate) fn glue() -> String {
    CORE.replace(
        "from cadpilot_policy import NATIVE_TYPES, PROPERTY_TYPES, document",
        "",
    )
}
fn file_name(value: &Value) -> Result<&str, String> {
    let name = value
        .as_str()
        .ok_or("Falta el nombre de la versión nativa.")?;
    let device = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let numbered = ["COM", "LPT"].iter().any(|prefix| {
        device.strip_prefix(prefix).is_some_and(|suffix| {
            suffix.chars().count() == 1
                && suffix
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '¹' | '²' | '³'))
        })
    });
    if name.is_empty()
        || name.len() > 256
        || name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        || name.chars().any(char::is_control)
        || !name.to_ascii_lowercase().ends_with(".fcstd")
        || matches!(
            device.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        )
        || numbered
    {
        return Err("El nombre de la versión nativa no es compatible.".into());
    }
    Ok(name)
}
fn receipt(directory: &Path, result: &Value) -> Result<(Value, Value), String> {
    let canonical=directory.canonicalize().map_err(|_|"Falta la carpeta de la copia conservada.")?;let directory=canonical.as_path();
    let info = &result["working_version"];
    if info["state"] != "captured" {
        return Err("La edición nativa está confirmada, pero no se capturó una versión verificable. Conserva el trabajo; no repitas la edición.".into());
    }
    let bytes =
        crate::kicad_project::read_regular(&directory.join("capture.json"), 2 * 1024 * 1024)?;
    if info["capture_sha256"].as_str() != Some(crate::kicad_project::digest(&bytes).as_str()) {
        return Err("El manifiesto de captura cambió; no se abrirá otra entrada.".into());
    }
    let capture: Value =
        serde_json::from_slice(&bytes).map_err(|_| "El manifiesto de captura no es válido.")?;
    if capture["version"] != 1
        || capture["source_preserved"] != true
        || capture["reopen_checked"] != false
        || capture["file"] != info["file"]
    {
        return Err("La captura no coincide con el recibo nativo original.".into());
    }
    let file = crate::kicad_project::read_regular(
        &directory.join(file_name(&capture["file"]["name"])?),
        LIMIT,
    )?;
    if capture["file"]["bytes"].as_u64() != Some(file.len() as u64)
        || capture["file"]["sha256"].as_str() != Some(crate::kicad_project::digest(&file).as_str())
    {
        return Err("La versión nativa cambió; se conserva sin reemplazarla.".into());
    }
    Ok((capture, info.clone()))
}
pub(crate) fn validate_artifacts(
    directory: &Path,
    result: &Value,
    expected_hash: &str,
) -> Result<Value, String> {
    let root = directory.join("NativeVersion").canonicalize().map_err(|_|"Falta la copia nativa conservada.")?;
    let (capture, _) = receipt(&root, result)?;
    let bytes =
        crate::kicad_project::read_regular(&root.join("verification.json"), 2 * 1024 * 1024)?;
    let verified: Value =
        serde_json::from_slice(&bytes).map_err(|_| "La comprobación nativa no es válida.")?;
    if crate::kicad_project::digest(&bytes) != expected_hash
        || verified["version"] != 1
        || verified["file"] != capture["file"]
        || [
            "source_preserved",
            "reopen_checked",
            "headless",
            "native_properties_checked",
            "native_geometry_checked",
            "native_primitives_recomputed",
        ]
        .iter()
        .any(|key| verified[key] != true)
        || verified["runtime_version"] != "1.1.4"
        || verified["units"] != "mm"
        || verified["coordinate_frame"] != "document_world"
        || verified["dependency_scope"] != "self_contained_native_primitives"
        || verified["external_dependencies"] != json!([])
        || verified["gui_state_checked"] != false
        || verified["physical_function_checked"] != false
    {
        return Err(
            "Faltan las comprobaciones de la versión nativa; la tarea sigue pendiente.".into(),
        );
    }
    Ok(verified)
}
pub(crate) fn pin_verifier(executable: &Path, plan: &Plan) -> Result<Vec<fs::File>, String> {
    plan.validate()?;
    let mut pins = crate::project::pin_lineage(
        executable
            .parent()
            .ok_or("Falta la carpeta del verificador.")?,
    )
    .map_err(|_| "No se pudo fijar el verificador.")?;
    pins.push(crate::kicad_project::open_regular(executable, LIMIT)?);
    if crate::kicad_project::digest(&crate::kicad_project::read_regular(executable, LIMIT)?)
        != plan.verifier_sha256
    {
        return Err("El verificador cambió desde la aprobación. No se abrió la copia ni se repitió la edición.".into());
    }
    Ok(pins)
}

pub(crate) fn checkpoint(directory:&Path,result:&Value)->Result<(String,usize),String>{
    let root=directory.join("NativeVersion");let pins=crate::project::pin_lineage(&root).map_err(|_|"No se pudo fijar la copia conservada.")?;
    let (capture,_)=receipt(&root,result)?;let captured=result["working_version"]["capture_sha256"].as_str().ok_or("Falta la captura original.")?;
    let identity=crate::project::directory_identity(pins.last().ok_or("Falta la carpeta nativa.")?).map_err(|_|"No se pudo comprobar la identidad de la copia.")?;
    let hash=crate::kicad_project::digest(&serde_json::to_vec(&json!({"capture_sha256":captured,"directory":identity,"file":capture["file"]})).map_err(|_|"La captura no es válida.")?);
    Ok((hash,crate::native_readback::count(&root,captured)?))
}
fn original(directory:&Path,input:&crate::cadpilot_live::Input)->Result<(crate::live_dispatch::Record,Value),String>{
    input.working_version.as_ref().ok_or("La operación no incluía una copia nativa.")?.validate()?;
    let record=crate::live_dispatch::read(directory,&crate::project::OperationInput::LiveCad(input.clone()))?.ok_or("Falta el envío original.")?;
    let result=crate::cadpilot_live::read_native_completion(directory,input,record.native_completion_sha256.as_deref().ok_or("Falta el vínculo de finalización original.")?)?.ok_or("Falta el recibo original.")?;
    checkpoint(directory,&result)?;Ok((record,result))
}
pub(crate) fn pending_copy_check(directory:&Path,input:&crate::cadpilot_live::Input)->Result<(String,usize),String>{
    let _exclusive=crate::pcb_copy_recovery::Exclusive::for_job(input.job_id)?;let (record,result)=original(directory,input)?;
    if record.working_version_sha256.is_some(){return Err("La copia ya tiene una comprobación vinculada; recupera su resultado local.".into());}
    if !matches!(record.outcome,crate::native_journal::Outcome::Dispatched|crate::native_journal::Outcome::Uncertain){return Err("Falta reconciliar el envío original; no se adoptó otra comprobación.".into());}
    let checkpoint=checkpoint(directory,&result)?;if checkpoint.1>=2{return Err("Se agotaron las dos comprobaciones registradas. Se conserva la copia; una ampliación necesita otra aprobación.".into());}Ok(checkpoint)
}
pub(crate) fn approved_copy_check(directory:&Path,input:&crate::cadpilot_live::Input,approval:&crate::result_checks::Input)->Result<String,String>{
    approval.validate()?;let (_,result)=original(directory,input)?;let (identity,count)=checkpoint(directory,&result)?;
    if approval.program!=Some(crate::host_tools::Program::Freecad) || approval.target_job_id!=input.job_id || approval.target_input_sha256!=crate::cadpilot_live::completion_input(input)? || identity!=approval.edit_sha256 || count!=approval.attempts_before+1{return Err("La comprobación no pertenece a esta copia y aprobación.".into());}
    let hash=crate::native_readback::approved(&directory.join("NativeVersion"),result["working_version"]["capture_sha256"].as_str().ok_or("Falta la captura original.")?,approval)?;
    validate_artifacts(directory,&result,&hash)?;Ok(hash)
}
fn retain_approved(directory:&Path,input:&crate::cadpilot_live::Input,hash:&str)->Result<(),String>{
    let (record,_)=original(directory,input)?;
    if record.working_version_sha256.as_deref()!=Some(hash){crate::live_dispatch::retain_working_version(directory,input,hash)?;}
    if !crate::cadpilot_live::recover_recorded_result(directory,input)?{return Err("Falta recuperar el informe original de la copia comprobada.".into());}
    let mut checked=crate::project::OperationInput::LiveCad(input.clone());checked.retain_receipt(directory).map_err(|_|"Falta el informe original.")?;crate::live_dispatch::recover_checked(directory,&checked)
}
pub(crate) fn finish_approved_copy_check(executable:&Path,directory:&Path,input:&crate::cadpilot_live::Input,profile:&Path,control:(Arc<AtomicBool>,crate::process::DispatchGate<'_>),approval:&crate::result_checks::Input)->Result<(),String>{
    let _exclusive=crate::pcb_copy_recovery::Exclusive::for_job(input.job_id)?;approval.validate()?;let (record,result)=original(directory,input)?;
    if approval.program!=Some(crate::host_tools::Program::Freecad) || approval.target_job_id!=input.job_id || approval.target_input_sha256!=crate::cadpilot_live::completion_input(input)?{return Err("La aprobación no pertenece a esta copia.".into());}
    let (identity,count)=checkpoint(directory,&result)?;if identity!=approval.edit_sha256{return Err("La copia cambió; se conserva sin reemplazarla.".into());}
    let hash=if count==approval.attempts_before && record.working_version_sha256.is_none(){verify(executable,input.working_version.as_ref().unwrap(),directory,profile,&result,(control.0,Some(control.1)),Some(approval))?}else{approved_copy_check(directory,input,approval)?};
    retain_approved(directory,input,&hash)
}
pub(crate) fn recover_approved_copy_check(directory:&Path,input:&crate::cadpilot_live::Input,approval:&crate::result_checks::Input)->Result<(),String>{
    let _exclusive=crate::pcb_copy_recovery::Exclusive::for_job(input.job_id)?;let hash=approved_copy_check(directory,input,approval)?;retain_approved(directory,input,&hash)
}
pub(crate) fn verify(
    executable: &Path,
    plan: &Plan,
    directory: &Path,
    profile: &Path,
    result: &Value,
    control: (Arc<AtomicBool>,Option<crate::process::DispatchGate<'_>>),
    approval:Option<&crate::result_checks::Input>,
) -> Result<String, String> {
    let (cancel,gate)=control;
    drop(gate.map(|g|g.enter(&cancel)).transpose()?);
    if cancel.load(Ordering::SeqCst) {
        return Err("Se detuvo la comprobación; la copia y la edición se conservan.".into());
    }
    if !executable.is_absolute() || !directory.is_absolute() || !profile.is_absolute() {
        return Err("La verificación necesita ubicaciones locales reservadas por Hazlo.".into());
    }
    plan.validate()?;
    if approval.is_none() && plan.reader_budget_version.is_none(){return Err("Esta aprobación anterior no registra un presupuesto de comprobaciones. Conserva la copia y prepara una nueva aprobación solo para comprobarla.".into());}
    let root = directory.join("NativeVersion").canonicalize().map_err(|_|"Falta la copia nativa conservada.")?;
    let mut pins =
        crate::project::pin_lineage(&root).map_err(|_| "No se pudo fijar la versión nativa.")?;
    pins.push(crate::kicad_project::open_regular(
        &root.join("capture.json"),
        2 * 1024 * 1024,
    )?);
    let (capture, _) = receipt(&root, result)?;
    pins.push(crate::kicad_project::open_regular(
        &root.join(file_name(&capture["file"]["name"])?),
        LIMIT,
    )?);
    pins.extend(pin_verifier(executable, plan)?);
    if let Some(approval)=approval{approval.validate()?;if approval.program!=Some(crate::host_tools::Program::Freecad) || checkpoint(directory,result)?!=(approval.edit_sha256.clone(),approval.attempts_before){return Err("La copia o los intentos cambiaron; no se envió otra comprobación.".into());}}
    let capture_hash=result["working_version"]["capture_sha256"].as_str().ok_or("Falta el vínculo original de captura.")?;
    let ticket=crate::native_readback::begin(&root,capture_hash,approval)?;
    fs::create_dir(profile)
        .map_err(|_| "No se pudo reservar el perfil de verificación; el trabajo sigue local.")?;
    let profile = crate::freecad::configuration_path(profile)
        .map_err(|_| "No se pudo preparar el perfil nativo aislado.")?;
    pins.extend(crate::project::pin_lineage(&profile).map_err(|_| "No se pudo fijar el perfil de verificación.")?);
    let mut env = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for (key, name) in [
        ("FREECAD_USER_HOME", "home"),
        ("FREECAD_USER_DATA", "data"),
        ("FREECAD_USER_TEMP", "temp"),
    ] {
        let path = profile.join(name);
        fs::create_dir(&path).map_err(|_| "No se pudo preparar el perfil de verificación.")?;
        env.insert(key.into(), path.into_os_string());
    }
    for key in [
        "TEMP",
        "TMP",
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        env.insert(key.into(), profile.join("temp").into_os_string());
    }
    let nonce = uuid::Uuid::new_v4().to_string();
    let marker = format!("HAZLO_VERSION_{nonce}:");
    let check = CHECK.replace("HAZLO_VERSION_NONCE:", &marker).replace(
        "PROFILE_FROM_HOST",
        &serde_json::to_string(&profile).map_err(|_| "Perfil de verificación no válido.")?,
    );
    for (name, body) in [
        ("native_copy.py", CORE),
        ("cadpilot_policy.py", POLICY),
        ("verify-copy.FCMacro", check.as_str()),
    ] {
        crate::kicad_project::write_new(&profile.join(name), body.as_bytes())?;
        pins.push(crate::kicad_project::open_regular(
            &profile.join(name),
            2 * 1024 * 1024,
        )?);
    }
    let args = [
        "--user-cfg".into(),
        profile.join("user.cfg").into_os_string(),
        "--system-cfg".into(),
        profile.join("system.cfg").into_os_string(),
        profile.join("verify-copy.FCMacro").into_os_string(),
    ];
    let permit=gate.map(|g|g.enter(&cancel)).transpose()?;
    // The installed launcher rejects verbatim drive cwd spelling; file pins keep it fixed.
    let native_root=if matches!(root.components().next(),Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::VerbatimDisk(_))){std::path::PathBuf::from(root.to_str().and_then(|p|p.strip_prefix("\\\\?\\")).ok_or("No se pudo preparar la carpeta nativa.")?)}else{root.clone()};
    let response = crate::process::run_owned_permitted(
        executable,
        &args,
        &native_root,
        &env,
        cancel.clone(),
        Duration::from_secs(90),
        (false,permit),
    )
    .map_err(|_| "No se pudo comprobar la copia; la edición no se repitió.")?;
    if cancel.load(Ordering::SeqCst)
        || response.reason != crate::process::StopReason::Completed
        || response.exit_code != Some(0)
    {
        return Err("La comprobación de la copia quedó pendiente; conserva el trabajo y no repitas la edición.".into());
    }
    let stdout =
        String::from_utf8(response.stdout).map_err(|_| "La comprobación no confirmó su recibo.")?;
    let hashes: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix(&marker))
        .collect();
    if hashes.len() != 1 || !crate::kicad_project::valid_hash(hashes[0]) {
        return Err("El verificador no confirmó un recibo único; la copia queda pendiente.".into());
    }
    let bytes =
        crate::kicad_project::read_regular(&profile.join("produced.json"), 2 * 1024 * 1024)?;
    let hash = crate::kicad_project::digest(&bytes);
    if hash != hashes[0] {
        return Err("El recibo de verificación cambió; no se adoptó otro contenido.".into());
    }
    let path = root.join("verification.json");
    if path
        .try_exists()
        .map_err(|_| "No se pudo comprobar el recibo local.")?
    {
        if crate::kicad_project::read_regular(&path, 2 * 1024 * 1024)? != bytes {
            return Err("La verificación guardada cambió; se conservó sin reemplazarla.".into());
        }
    } else {
        crate::kicad_project::write_new(&path, &bytes)?;
    }
    validate_artifacts(directory, result, &hash)?;
    crate::native_readback::complete(ticket,&hash)?;
    Ok(hash)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn retained_fixture(source:Option<&Path>,reader:Option<&Path>,recorded_budget:bool)->(std::path::PathBuf,std::path::PathBuf,crate::cadpilot_live::Input,std::path::PathBuf){
        use crate::{project::OperationInput,live_dispatch::Dispatch};use std::sync::Mutex;
        let root=std::env::temp_dir().join(format!("hazlo-freecad-resume-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();let stage=root.join("stage");fs::create_dir(&stage).unwrap();let native=stage.join("NativeVersion");fs::create_dir(&native).unwrap();
        let executable=if let Some(reader)=reader{reader.to_owned()}else{let reader=root.join("FreeCADCmd.exe");fs::write(&reader,b"owned placeholder, never execute").unwrap();reader};
        let capture=if let Some(source)=source{let bytes=crate::kicad_project::read_regular(&source.join("capture.json"),2*1024*1024).unwrap();let capture:Value=serde_json::from_slice(&bytes).unwrap();let name=file_name(&capture["file"]["name"]).unwrap();fs::copy(source.join("version").join(name),native.join(name)).unwrap();fs::write(native.join("capture.json"),&bytes).unwrap();capture}else{
            let bytes=b"owned native fixture bytes";fs::write(native.join("Owned.FCStd"),bytes).unwrap();let capture=json!({"version":1,"source_preserved":true,"reopen_checked":false,"file":{"name":"Owned.FCStd","bytes":bytes.len(),"sha256":crate::kicad_project::digest(bytes)}});fs::write(native.join("capture.json"),serde_json::to_vec(&capture).unwrap()).unwrap();capture};
        let (mut input,mut result)=crate::cadpilot_live::tests::completed_fixture();input.working_version=Some(plan(&executable).unwrap());if !recorded_budget{input.working_version.as_mut().unwrap().reader_budget_version=None;}
        result["working_version"]=json!({"state":"captured","capture_sha256":crate::kicad_project::digest(&fs::read(native.join("capture.json")).unwrap()),"file":capture["file"]});
        let operation=OperationInput::LiveCad(input.clone());let fence=Dispatch::start(&stage,&operation,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();drop(fence.enter().unwrap());fence.retain_native_completion(&input,&result).unwrap();drop(fence);
        (root,stage,input,executable)
    }
    #[test]
    fn legacy_reader_budget_stays_absent_and_unknown_versions_are_refused(){
        let bytes=serde_json::to_vec(&json!({"verifier_sha256":"a".repeat(64)})).unwrap();let mut old:Plan=serde_json::from_slice(&bytes).unwrap();old.validate().unwrap();assert!(old.reader_budget_version.is_none());assert_eq!(serde_json::to_vec(&old).unwrap(),bytes);
        old.reader_budget_version=Some(1);old.validate().unwrap();assert_ne!(serde_json::to_vec(&old).unwrap(),bytes);for version in [0,2,u32::MAX]{old.reader_budget_version=Some(version);assert!(old.validate().is_err());}
    }
    #[test]
    fn legacy_freecad_check_requires_fresh_scope_and_cannot_retrofit_the_original_input(){
        let(root,stage,mut input,reader)=retained_fixture(None,None,false);let profile=root.join("not-started-reader");let (_,result)=original(&stage,&input).unwrap();
        let error=verify(&reader,input.working_version.as_ref().unwrap(),&stage,&profile,&result,(Arc::new(AtomicBool::new(false)),None),None).unwrap_err();assert!(error.contains("nueva aprobación"));assert!(!profile.exists());assert_eq!(pending_copy_check(&stage,&input).unwrap().1,0);
        let original=fs::read(stage.join("native-completion.json")).unwrap();input.working_version.as_mut().unwrap().reader_budget_version=Some(1);assert!(pending_copy_check(&stage,&input).is_err());assert_eq!(fs::read(stage.join("native-completion.json")).unwrap(),original);assert_eq!(fs::read(stage.join("NativeVersion/Owned.FCStd")).unwrap(),b"owned native fixture bytes");assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retained_native_copy_binds_identity_and_unknown_attempts_cannot_be_replayed(){
        let(root,stage,input,executable)=retained_fixture(None,None,true);let (identity,attempts)=pending_copy_check(&stage,&input).unwrap();assert_eq!(attempts,0);
        let plan=crate::result_checks::Input{result_checks_version:3,program:Some(crate::host_tools::Program::Freecad),job_id:uuid::Uuid::new_v4(),target_job_id:input.job_id,target_input_sha256:crate::cadpilot_live::completion_input(&input).unwrap(),edit_sha256:identity.clone(),attempts_before:0,agreement:crate::native_tools::tests::agreement_fixture(),receipt_sha256:None};
        let(_,result)=original(&stage,&input).unwrap();let capture=result["working_version"]["capture_sha256"].as_str().unwrap();let _unknown=crate::native_readback::begin(&stage.join("NativeVersion"),capture,Some(&plan)).unwrap();
        let mutex=std::sync::Mutex::new(());let paused=AtomicBool::new(false);let gate=crate::process::DispatchGate{mutex:&mutex,paused:&paused};let profile=root.join("not-started");assert!(finish_approved_copy_check(&executable,&stage,&input,&profile,(Arc::new(AtomicBool::new(false)),gate),&plan).is_err());assert!(!profile.exists());assert_eq!(crate::native_readback::count(&stage.join("NativeVersion"),capture).unwrap(),1);
        let moved=root.join("moved-stage");assert!(stage.canonicalize().unwrap().starts_with(root.canonicalize().unwrap())&&moved.parent().unwrap().canonicalize().unwrap()==root.canonicalize().unwrap());fs::rename(&stage,&moved).unwrap();assert_eq!(checkpoint(&moved,&result).unwrap().0,identity);
        fs::write(moved.join("NativeVersion/Owned.FCStd"),b"preserve manual native changes").unwrap();assert!(pending_copy_check(&moved,&input).is_err());assert_eq!(fs::read(moved.join("NativeVersion/Owned.FCStd")).unwrap(),b"preserve manual native changes");assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires the identified portable FreeCAD GUI and Cmd; creates only an owned offscreen fixture"]
    fn real_gui_copy_preserves_modified_undo_and_original_then_reopens() {
        let executable = std::path::PathBuf::from(
            std::env::var_os("HAZLOMEXA_NATIVE_COPY_VERIFIER").expect("Set the identified runtime"),
        );
        let gui = executable.parent().unwrap().join("bin/FreeCAD.exe");
        assert!(gui.is_file());
        let root = std::env::temp_dir().join(format!("hazlo-gui-copy-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let profile = root.join("gui-profile");
        fs::create_dir(&profile).unwrap();
        let profile = crate::freecad::configuration_path(&profile).unwrap();
        for (name, body) in [("native_copy.py", CORE), ("cadpilot_policy.py", POLICY)] {
            fs::write(profile.join(name), body).unwrap();
        }
        let body = include_str!("../../../../tests/cadpilot/probe_gui_copy.py");
        let script = format!(
            "import sys\nsys.path.insert(0,{})\n{body}",
            serde_json::to_string(&profile).unwrap()
        );
        fs::write(profile.join("capture.FCMacro"), script).unwrap();
        let mut env = BTreeMap::<OsString, OsString>::new();
        for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.into(), value);
            }
        }
        for key in [
            "FREECAD_USER_HOME",
            "FREECAD_USER_DATA",
            "FREECAD_USER_TEMP",
            "TEMP",
            "TMP",
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
        ] {
            env.insert(key.into(), profile.clone().into_os_string());
        }
        env.insert("QT_QPA_PLATFORM".into(), "offscreen".into());
        env.insert(
            "HAZLO_OWNED_COPY".into(),
            root.file_name().unwrap().to_owned(),
        );
        let args = [
            "--user-cfg".into(),
            profile.join("user.cfg").into_os_string(),
            "--system-cfg".into(),
            profile.join("system.cfg").into_os_string(),
            profile.join("capture.FCMacro").into_os_string(),
        ];
        let response = crate::process::run_owned(
            &gui,
            &args,
            &root,
            &env,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(60),
        )
        .unwrap();
        assert_eq!(
            response.reason,
            crate::process::StopReason::Completed,
            "Owned GUI fixture did not complete; private files retained"
        );
        assert_eq!(response.exit_code, Some(0));
        fs::write(root.join("stdout.private.txt"), &response.stdout).unwrap();
        fs::write(root.join("stderr.private.txt"), &response.stderr).unwrap();
        let proof: Value = crate::project::read_json(&root.join("gui-proof.json"))
            .expect("GUI state proof missing; private files retained");
        for key in [
            "modified_preserved",
            "undo_preserved",
            "original_preserved",
            "native_state_preserved",
        ] {
            assert_eq!(proof[key], true);
        }
        let capture = crate::kicad_project::read_regular(
            &root.join("NativeVersion/capture.json"),
            2 * 1024 * 1024,
        )
        .unwrap();
        let captured: Value = serde_json::from_slice(&capture).unwrap();
        let result = json!({"working_version":{"state":"captured","capture_sha256":crate::kicad_project::digest(&capture),"file":captured["file"]}});
        let mutex=std::sync::Mutex::new(());let paused=AtomicBool::new(false);
        let gate=crate::process::DispatchGate{mutex:&mutex,paused:&paused};
        let hash = verify(
            &executable,
            &plan(&executable).unwrap(),
            &root,
            &root.join("check-profile"),
            &result,
            (Arc::new(AtomicBool::new(false)),Some(gate)),
            None,
        )
        .unwrap();
        validate_artifacts(&root, &result, &hash).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paused_native_reader_does_not_touch_the_retained_version_or_start_a_profile(){
        let root=std::env::temp_dir().join(format!("hazlo-cad-reader-pause-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
        let exe=root.join("FreeCADCmd.exe");fs::write(&exe,b"owned verifier identity").unwrap();let plan=plan(&exe).unwrap();
        fs::write(root.join("retained.FCStd"),b"preserve native copy").unwrap();let mutex=std::sync::Mutex::new(());let paused=AtomicBool::new(true);
        let gate=crate::process::DispatchGate{mutex:&mutex,paused:&paused};let profile=root.join("not-started");
        let error=verify(&exe,&plan,&root,&profile,&json!({}),(Arc::new(AtomicBool::new(false)),Some(gate)),None).unwrap_err();
        assert!(error.contains("pausados"));assert!(!profile.exists());assert_eq!(fs::read(root.join("retained.FCStd")).unwrap(),b"preserve native copy");
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_runtime_and_cancellation_are_checked_before_native_execution() {
        for value in [
            json!("../Other.FCStd"),
            json!("CON.FCStd"),
            json!("LPT¹.FCStd"),
            json!("file:stream.FCStd"),
            json!("bad?.FCStd"),
            json!("Other.py"),
        ] {
            assert!(file_name(&value).is_err());
        }
        assert_eq!(
            file_name(&json!("Owned part.FCStd")).unwrap(),
            "Owned part.FCStd"
        );
        let root =
            std::env::temp_dir().join(format!("hazlo-version-scope-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let exe = root.join("FreeCADCmd.exe");
        fs::write(&exe, b"original runtime identity").unwrap();
        let plan = plan(&exe).unwrap();
        fs::write(&exe, b"changed runtime identity").unwrap();
        assert!(pin_verifier(&exe, &plan).is_err());
        let profile = root.join("not-started");
        assert!(verify(
            &exe,
            &plan,
            &root,
            &profile,
            &json!({}),
            (Arc::new(AtomicBool::new(true)),None),
            None,
        )
        .is_err());
        assert!(!profile.exists());
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Requires an owned five-primitive copy fixture and explicitly identified FreeCADCmd"]
    fn real_native_working_version_verifier_uses_host_pins_and_a_bound_process_receipt() {
        let source = std::path::PathBuf::from(
            std::env::var_os("HAZLOMEXA_NATIVE_COPY_FIXTURE")
                .expect("Set the owned fixture directory"),
        );
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        assert_eq!(
            source.canonicalize().unwrap().parent(),
            Some(workspace.join("target").canonicalize().unwrap().as_path())
        );
        assert!(source
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("native-copy-"));
        let proof: Value = crate::project::read_json(&source.join("capture-proof.json")).unwrap();
        assert_eq!(proof["original_unchanged"], true);
        let capture =
            crate::kicad_project::read_regular(&source.join("capture.json"), 2 * 1024 * 1024)
                .unwrap();
        let captured: Value = serde_json::from_slice(&capture).unwrap();
        assert_eq!(
            captured["expected"]["objects"].as_object().unwrap().len(),
            5
        );
        let name = file_name(&captured["file"]["name"]).unwrap();
        let executable = std::path::PathBuf::from(
            std::env::var_os("HAZLOMEXA_NATIVE_COPY_VERIFIER").expect("Set the identified runtime"),
        );
        let root =
            std::env::temp_dir().join(format!("hazlo-version-host-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let native = root.join("NativeVersion");
        fs::create_dir(&native).unwrap();
        fs::write(native.join("capture.json"), &capture).unwrap();
        fs::copy(source.join("version").join(name), native.join(name)).unwrap();
        let result = json!({"working_version":{"state":"captured","capture_sha256":crate::kicad_project::digest(&capture),"file":captured["file"]}});
        let plan = plan(&executable).unwrap();
        let mutex=std::sync::Mutex::new(());let paused=AtomicBool::new(false);
        let gate=crate::process::DispatchGate{mutex:&mutex,paused:&paused};
        let hash = verify(
            &executable,
            &plan,
            &root,
            &root.join("profile"),
            &result,
            (Arc::new(AtomicBool::new(false)),Some(gate)),
            None,
        )
        .unwrap();
        validate_artifacts(&root, &result, &hash).unwrap();
        let report: Value = crate::project::read_json(&native.join("verification.json")).unwrap();
        assert_eq!(report["physical_function_checked"], false);
        assert_eq!(report["gui_state_checked"], false);
        assert_eq!(report["native_primitives_recomputed"], true);
        assert!(validate_artifacts(&root, &result, &"a".repeat(64)).is_err());
        let original = crate::kicad_project::read_regular(&native.join(name), LIMIT).unwrap();
        fs::write(
            native.join("verification.json"),
            b"preserve changed verification",
        )
        .unwrap();
        assert!(validate_artifacts(&root, &result, &hash).is_err());
        assert!(verify(&executable,&plan,&root,&root.join("read-only-recheck"),&result,(Arc::new(AtomicBool::new(false)),None),None).is_err());
        assert_eq!(fs::read(native.join("verification.json")).unwrap(),b"preserve changed verification");
        assert_eq!(
            crate::kicad_project::read_regular(&native.join(name), LIMIT).unwrap(),
            original
        );
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
}
