//! Recover native checks of an admitted, confirmed owned copy; never dispatch its edit.
use crate::{kicad_project as source,konnect_pcb::{Input,ResultDocument},kicad_run,project::{atomic_json,pin_lineage,directory_identity}};
use serde::{Serialize,Deserialize};
use std::{collections::BTreeSet,fs::{self,File},path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,Ordering}}};
use uuid::Uuid;
const JOURNAL:&str="pcb-copy.json";
const EDIT:&str="pcb-copy-edit.json";
const LIMIT:u64=1024*1024;
const PAUSE:&str="La copia se conserva para revisión. No repitas el movimiento ni reemplaces los archivos.";
fn plain(path:&Path)->Result<PathBuf,String>{
    let text=path.to_str().ok_or(PAUSE)?;
    Ok(if matches!(path.components().next(),Some(std::path::Component::Prefix(p)) if matches!(p.kind(),std::path::Prefix::VerbatimDisk(_))){PathBuf::from(text.strip_prefix("\\\\?\\").ok_or(PAUSE)?)}else{path.to_owned()})
}
pub(crate) struct Exclusive(windows_sys::Win32::Foundation::HANDLE);
impl Exclusive{
    pub(crate) fn acquire(input:&Input)->Result<Self,String>{
        Self::for_job(input.job_id)
    }
    pub(crate) fn for_job(job_id:Uuid)->Result<Self,String>{
        use windows_sys::Win32::{System::Threading::{CreateMutexW,WaitForSingleObject},Foundation::{CloseHandle,WAIT_OBJECT_0,WAIT_ABANDONED}};
        // Retain the historical prefix so earlier hosts share the same per-job lock.
        let name=format!("Local\\HazloPcbCopy-{}",job_id).encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        // The session-local OS mutex bounds concurrent recovery, including native waits.
        let handle=unsafe{CreateMutexW(std::ptr::null(),0,name.as_ptr())};if handle.is_null(){return Err(PAUSE.into());}
        match unsafe{WaitForSingleObject(handle,0)}{WAIT_OBJECT_0|WAIT_ABANDONED=>Ok(Self(handle)),_=>{unsafe{CloseHandle(handle)};Err("La copia todavía está en uso. Conserva el trabajo y consulta su estado antes de continuar.".into())}}
    }
}
impl Drop for Exclusive{fn drop(&mut self){unsafe{windows_sys::Win32::System::Threading::ReleaseMutex(self.0);windows_sys::Win32::Foundation::CloseHandle(self.0);}}}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Record{version:u32,input_sha256:String,attempt:Uuid,edit_sha256:Option<String>,checks:Vec<Check>,report_sha256:Option<String>}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Check{folder:String,input:source::KicadInput,report_sha256:Option<String>,#[serde(default,skip_serializing_if="Option::is_none")]approval:Option<Approval>}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval{job_id:Uuid,input_sha256:String}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit{version:u32,input_sha256:String,attempt:Uuid,directory:(u32,u32,u32),files:Vec<source::CapturedFile>}
pub(crate) fn identity(input:&Input)->Result<String,String>{let mut input=input.clone();input.receipt_sha256=None;serde_json::to_vec(&input).map(|b|source::digest(&b)).map_err(|_|PAUSE.into())}
fn folder(value:&str)->Result<(),String>{let id=value.strip_prefix("Verification-").and_then(|s|Uuid::parse_str(s).ok()).ok_or(PAUSE)?;if value!=format!("Verification-{id}"){return Err(PAUSE.into());}Ok(())}
fn read(stage:&Path,input:&Input)->Result<Record,String>{
    input.validate()?;
    if input.copy_checkpoint_version!=Some(1){return Err(PAUSE.into());}
    let record:Record=serde_json::from_slice(&source::read_regular(&stage.join(JOURNAL),LIMIT)?).map_err(|_|PAUSE)?;
    if record.version!=1 || record.input_sha256!=identity(input)? || record.attempt.is_nil() || record.checks.len()>2
        || [&record.edit_sha256,&record.report_sha256].iter().any(|h|h.as_ref().is_some_and(|s|!source::valid_hash(s)))
        || (!record.checks.is_empty() && record.edit_sha256.is_none()) {return Err(PAUSE.into());}
    let mut names=BTreeSet::new();
    for check in &record.checks{folder(&check.folder)?;check.input.validate()?;
        if !names.insert(&check.folder) || check.input.job_id!=input.job_id || check.input.entry!=input.source.entry || check.input.units!=input.source.units
            || check.input.layers!=input.source.layers || check.report_sha256.as_ref().is_some_and(|h|!source::valid_hash(h))
            || check.approval.as_ref().is_some_and(|a|a.job_id.is_nil() || a.job_id==input.job_id || !source::valid_hash(&a.input_sha256)){return Err(PAUSE.into());}}
    Ok(record)
}
fn save(stage:&Path,record:&Record)->Result<(),String>{atomic_json(&stage.join(JOURNAL),record,true).map_err(|_|PAUSE.into())}
pub(crate) fn begin(stage:&Path,input:&Input)->Result<(),String>{
    input.validate()?;
    let record=Record{version:1,input_sha256:identity(input)?,attempt:Uuid::new_v4(),edit_sha256:None,checks:vec![],report_sha256:None};
    atomic_json(&stage.join(JOURNAL),&record,false).map_err(|_|PAUSE.into())
}
fn edited(stage:&Path,input:&Input)->Result<(Vec<File>,Edit),String>{
    let mut pins=pin_lineage(&stage.join("Edited")).map_err(|_|PAUSE)?;
    let directory=directory_identity(pins.last().ok_or(PAUSE)?).map_err(|_|PAUSE)?;
    source::validate_capture(&stage.join("Original"),&input.source)?;
    let mut files=Vec::new();
    source::portable_files(&stage.join("Original"),&input.source,&AtomicBool::new(false),|rel,before|{
        let path=stage.join("Edited").join(rel);pins.push(source::open_regular(&path,source::FILE_LIMIT)?);
        let after=source::read_regular(&path,source::FILE_LIMIT)?;
        if rel==source::relative(&input.source.entry)?{crate::konnect::verify_footprint_move(before,&after,&input.pcb_move)?;}
        else if after!=before{return Err(PAUSE.into());}
        files.push(source::CapturedFile{path:rel.to_str().ok_or(PAUSE)?.replace('\\',"/"),bytes:after.len() as u64,sha256:source::digest(&after)});Ok(())
    })?;
    files.sort_by(|a,b|a.path.cmp(&b.path));
    let actual=kicad_run::files(&stage.join("Edited"))?.into_iter().map(|p|p.strip_prefix(stage.join("Edited")).map(|r|r.to_string_lossy().replace('\\',"/")).map_err(|_|PAUSE)).collect::<Result<BTreeSet<_>,_>>()?;
    if actual!=files.iter().map(|f|f.path.clone()).collect(){return Err(PAUSE.into());}
    Ok((pins,Edit{version:1,input_sha256:identity(input)?,attempt:Uuid::nil(),directory,files}))
}
// The caller invokes this only after dispatch's native acknowledgment, checks and process closure.
pub(crate) fn retain_edit(stage:&Path,input:&Input)->Result<(),String>{
    let stage=stage.canonicalize().map_err(|_|PAUSE)?;let mut record=read(&stage,input)?;
    if record.edit_sha256.is_some(){return Err(PAUSE.into());}
    let (_pins,mut proof)=edited(&stage,input)?;proof.attempt=record.attempt;
    let bytes=serde_json::to_vec(&proof).map_err(|_|PAUSE)?;source::write_new(&stage.join(EDIT),&bytes)?;
    record.edit_sha256=Some(source::digest(&bytes));save(&stage,&record)
}
fn retained_edit(stage:&Path,input:&Input,record:&Record)->Result<(Vec<File>,Edit),String>{
    if record.edit_sha256.is_none(){return Err("Falta el vínculo original de edición confirmada y cierre del proceso. Se conserva la copia; no repitas el movimiento ni adoptes sus archivos como prueba de esa llamada.".into());}
    let bytes=source::read_regular(&stage.join(EDIT),LIMIT)?;
    if record.edit_sha256.as_deref()!=Some(source::digest(&bytes).as_str()){return Err(PAUSE.into());}
    let saved:Edit=serde_json::from_slice(&bytes).map_err(|_|PAUSE)?;
    let (pins,current)=edited(stage,input)?;
    if saved.version!=1 || saved.input_sha256!=current.input_sha256 || saved.attempt!=record.attempt || saved.directory!=current.directory || saved.files!=current.files{return Err(PAUSE.into());}
    Ok((pins,saved))
}
pub(crate) fn verification_name(directory:&Path,input:&Input)->Result<String,String>{
    let result:ResultDocument=serde_json::from_slice(&source::read_regular(&directory.canonicalize().map_err(|_|PAUSE)?.join("result.json"),LIMIT)?).map_err(|_|PAUSE)?;
    let name=result.verification_directory.unwrap_or_else(||"Verification".into());
    if input.copy_checkpoint_version.is_some(){folder(&name)?;}else if name!="Verification"{return Err(PAUSE.into());}Ok(name)
}
pub(crate) fn filtered_files(stage:&Path,input:&Input,selected:&str)->Result<Vec<PathBuf>,String>{
    let record=read(stage,input)?;
    let ignored=record.checks.iter().filter(|c|c.folder!=selected).map(|c|c.folder.as_str()).collect::<BTreeSet<_>>();
    kicad_run::files(stage)?.into_iter().filter_map(|p|{
        let rel=match p.strip_prefix(stage){Ok(p)=>p,Err(_)=>return Some(Err(PAUSE.into()))};
        if rel==Path::new(JOURNAL) || rel==Path::new("result.json") || rel.components().next().is_some_and(|c|ignored.contains(c.as_os_str().to_str().unwrap_or(""))){None}else{Some(Ok(p))}
    }).collect()
}
fn checked_report(stage:&Path,input:&Input,record:&Record,name:&str)->Result<kicad_run::KicadResult,String>{
    let (_pins,proof)=retained_edit(stage,input,record)?;
    let check=record.checks.iter().find(|c|c.folder==name).ok_or(PAUSE)?;
    if check.input.files!=proof.files{return Err(PAUSE.into());}
    let directory=stage.join(name);let bytes=source::read_regular(&directory.join("result.json"),LIMIT)?;
    if check.report_sha256.as_deref()!=Some(source::digest(&bytes).as_str()){return Err(PAUSE.into());}
    let result=serde_json::from_slice(&bytes).map_err(|_|PAUSE)?;
    kicad_run::validate_artifacts(&directory,&check.input,&result)?;Ok(result)
}
pub(crate) fn validate(stage:&Path,input:&Input,result:&ResultDocument)->Result<(),String>{
    let stage=stage.canonicalize().map_err(|_|PAUSE)?;let record=read(&stage,input)?;
    let name=result.verification_directory.as_deref().ok_or(PAUSE)?;folder(name)?;
    let actual=checked_report(&stage,input,&record,name)?;
    if serde_json::to_value(actual).map_err(|_|PAUSE)?!=serde_json::to_value(&result.verification).map_err(|_|PAUSE)? {return Err(PAUSE.into());}
    let check=record.checks.iter().find(|c|c.folder==name).ok_or(PAUSE)?;
    if serde_json::to_value(&check.input).map_err(|_|PAUSE)?!=serde_json::to_value(&result.verification_input).map_err(|_|PAUSE)?{return Err(PAUSE.into());}
    if let Some(hash)=&record.report_sha256{
        let bytes=serde_json::to_vec_pretty(result).map_err(|_|PAUSE)?;
        if source::digest(&bytes)!=*hash || input.receipt_sha256.as_ref().is_some_and(|h|h!=hash){return Err(PAUSE.into());}
        let path=stage.join("result.json");if path.try_exists().map_err(|_|PAUSE)? && source::read_regular(&path,LIMIT)?!=bytes{return Err(PAUSE.into());}
    }else if input.receipt_sha256.is_some(){return Err(PAUSE.into());}Ok(())
}
pub(crate) fn finish(stage:&Path,input:&Input,cli:Option<&Path>,profile:&Path,cancel:Arc<AtomicBool>)->Result<ResultDocument,String>{
    finish_gated(stage,input,cli,profile,cancel,None)
}
pub(crate) fn finish_gated(stage:&Path,input:&Input,cli:Option<&Path>,profile:&Path,cancel:Arc<AtomicBool>,gate:Option<crate::process::DispatchGate<'_>>)->Result<ResultDocument,String>{
    finish_impl(stage,input,cli,profile,cancel,(gate,None))
}
pub(crate) fn finish_approved(stage:&Path,input:&Input,cli:&Path,profile:&Path,cancel:Arc<AtomicBool>,gate:crate::process::DispatchGate<'_>,approval:&crate::result_checks::Input)->Result<ResultDocument,String>{
    finish_impl(stage,input,Some(cli),profile,cancel,(Some(gate),Some(approval)))
}
fn finish_impl(stage:&Path,input:&Input,cli:Option<&Path>,profile:&Path,cancel:Arc<AtomicBool>,control:(Option<crate::process::DispatchGate<'_>>,Option<&crate::result_checks::Input>))->Result<ResultDocument,String>{
    let (gate,approval)=control;
    let _exclusive=Exclusive::acquire(input)?;
    let _stage_pins=pin_lineage(stage).map_err(|_|PAUSE)?;let stage=stage.canonicalize().map_err(|_|PAUSE)?;
    let mut record=read(&stage,input)?;let (_edit_pins,proof)=retained_edit(&stage,input,&record)?;
    let mut selected=record.checks.iter().find(|c|c.report_sha256.is_some()).map(|c|c.folder.clone());
    if let Some(approval)=approval{
        approval_target(input,&record,approval)?;
        if record.checks.len()==approval.attempts_before && selected.is_none(){}
        else{approved_check(&stage,input,approval)?;}
    }
    if selected.is_none(){
        if let Some(gate)=gate{drop(gate.enter(&cancel)?);}
        let cli=cli.ok_or("La copia editada está conservada; falta comprobarla con KiCad CLI. No repitas el movimiento.")?;
        if cancel.load(Ordering::SeqCst){return Err(PAUSE.into());}
        if record.checks.len()>=2{return Err("Se agotaron los dos intentos de comprobación. Se conservaron las copias; amplía el alcance en el chat sin repetir el movimiento.".into());}
        let _runtime=kicad_run::runtime(cli,&cancel)?;
        let native_source=plain(&stage.join("Edited"))?;
        let mut expected=input.source.clone();expected.schema_version=2;expected.source_root=native_source.to_str().ok_or(PAUSE)?.replace('\\',"/");expected.files=proof.files.clone();
        let name=format!("Verification-{}",Uuid::new_v4());let directory=stage.join(&name);
        let owner=approval.map(|a|a.identity().map(|input_sha256|Approval{job_id:a.job_id,input_sha256})).transpose()?;
        record.checks.push(Check{folder:name.clone(),input:expected.clone(),report_sha256:None,approval:owner});save(&stage,&record)?;
        fs::create_dir(&directory).map_err(|_|PAUSE)?;
        fs::create_dir(profile).map_err(|_|PAUSE)?;let _profile=pin_lineage(profile).map_err(|_|PAUSE)?;
        let source_input=source::capture(&native_source,&native_source.join(source::relative(&input.source.entry)?),&profile.join("import"),input.job_id,input.source.units,cancel.clone())?;
        if serde_json::to_value(&source_input).map_err(|_|PAUSE)?!=serde_json::to_value(&expected).map_err(|_|PAUSE)?{return Err(PAUSE.into());}
        let result=kicad_run::run_gated(cli,&profile.join("import"),&directory,&profile.join("native"),&source_input,cancel,gate)?;
        kicad_run::validate_artifacts(&directory,&source_input,&result)?;
        let bytes=source::read_regular(&directory.join("result.json"),LIMIT)?;
        if serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|_|PAUSE)?!=serde_json::to_value(&result).map_err(|_|PAUSE)?{return Err(PAUSE.into());}
        record.checks.last_mut().ok_or(PAUSE)?.report_sha256=Some(source::digest(&bytes));save(&stage,&record)?;selected=Some(name);
    }
    let name=selected.ok_or(PAUSE)?;let verification=checked_report(&stage,input,&record,&name)?;
    let verification_input=record.checks.iter().find(|c|c.folder==name).ok_or(PAUSE)?.input.clone();
    let files=filtered_files(&stage,input,&name)?.into_iter().map(|p|Ok(kicad_run::OutputFile{path:p.strip_prefix(&stage).map_err(|_|PAUSE)?.to_string_lossy().replace('\\',"/"),sha256:source::digest(&source::read_regular(&p,source::FILE_LIMIT)?)})).collect::<Result<Vec<_>,String>>()?;
    let result=ResultDocument{schema_version:1,job_id:input.job_id,konnect_version:input.konnect_version.clone(),protocol_version:"2025-06-18".into(),verification_input,verification,files,verification_directory:Some(name)};
    crate::konnect_pcb::validate_artifacts(&stage,input,&result)?;
    let bytes=serde_json::to_vec_pretty(&result).map_err(|_|PAUSE)?;if bytes.len() as u64>LIMIT{return Err(PAUSE.into());}
    let path=stage.join("result.json");
    if path.try_exists().map_err(|_|PAUSE)?{if source::read_regular(&path,LIMIT)?!=bytes{return Err(PAUSE.into());}}else{source::write_new(&path,&bytes)?;}
    let hash=source::digest(&bytes);if record.report_sha256.as_ref().is_some_and(|h|h!=&hash){return Err(PAUSE.into());}
    record.report_sha256=Some(hash);save(&stage,&record)?;Ok(result)
}

fn approval_target(input:&Input,record:&Record,approval:&crate::result_checks::Input)->Result<(),String>{
    approval.validate()?;
    if approval.target_job_id!=input.job_id || approval.target_input_sha256!=identity(input)? || record.edit_sha256.as_deref()!=Some(approval.edit_sha256.as_str()){return Err(PAUSE.into());}Ok(())
}
pub(crate) fn pending_checkpoint(stage:&Path,input:&Input)->Result<(String,usize),String>{
    let _exclusive=Exclusive::acquire(input)?;let _pins=pin_lineage(stage).map_err(|_|PAUSE)?;
    let stage=stage.canonicalize().map_err(|_|PAUSE)?;let record=read(&stage,input)?;let _edit=retained_edit(&stage,input,&record)?;
    if record.checks.iter().any(|c|c.report_sha256.is_some()) || record.report_sha256.is_some(){return Err("La copia ya tiene comprobaciones vinculadas; recupera su informe local sin nuevos pasos.".into());}
    if record.checks.len()>=2{return Err("Se agotaron los dos intentos. Se conserva la copia; hace falta revisar un alcance de recursos distinto.".into());}
    Ok((record.edit_sha256.ok_or(PAUSE)?,record.checks.len()))
}
pub(crate) fn approved_check(stage:&Path,input:&Input,approval:&crate::result_checks::Input)->Result<(),String>{
    let stage=stage.canonicalize().map_err(|_|PAUSE)?;let record=read(&stage,input)?;approval_target(input,&record,approval)?;
    if record.checks.len()!=approval.attempts_before+1{return Err(PAUSE.into());}
    let check=record.checks.last().ok_or(PAUSE)?;let owner=check.approval.as_ref().ok_or(PAUSE)?;
    if owner.job_id!=approval.job_id || owner.input_sha256!=approval.identity()? || check.report_sha256.is_none(){return Err("La comprobación de esta aprobación no está confirmada; no se repetirá su intento.".into());}
    checked_report(&stage,input,&record,&check.folder)?;Ok(())
}

#[cfg(test)]
pub(crate) mod tests{
    use super::*;
    #[test]
    fn native_source_metadata_uses_plain_drive_spelling(){
        assert_eq!(plain(Path::new(r"\\?\C:\owned\Edited")).unwrap(),Path::new(r"C:\owned\Edited"));
        assert_eq!(plain(Path::new(r"C:\owned\Edited")).unwrap(),Path::new(r"C:\owned\Edited"));
    }
    pub(crate) fn fixture()->(PathBuf,PathBuf,Input){
        let root=std::env::temp_dir().join(format!("hazlo-pcb-copy-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();
        let original=root.join("source");fs::create_dir(&original).unwrap();
        let id=Uuid::new_v4();let board=format!(r#"(kicad_pcb (layers (0 "F.Cu" signal) (44 "Edge.Cuts" user)) (footprint "Device:R" (layer "F.Cu") (uuid "{id}") (at 10 20) (property "Reference" "R1") (property "Value" "10k")))"#);
        fs::write(original.join("Board.kicad_pcb"),&board).unwrap();
        let job=Uuid::new_v4();let captured=root.join("import");let input=Input{schema_version:1,job_id:job,konnect_version:crate::konnect_run::VERSION.into(),
            source:source::capture(&original,&original.join("Board.kicad_pcb"),&captured,job,crate::kicad::Units::Mm,Arc::new(AtomicBool::new(false))).unwrap(),
            pcb_move:crate::konnect::FootprintMove{uuid:id,reference:"R1".into(),expected_x_mm:10.,expected_y_mm:20.,x_mm:11.,y_mm:20.},native_origin:None,agreement:None,copy_checkpoint_version:Some(1),receipt_sha256:None};
        let stage=root.join("stage");fs::create_dir(&stage).unwrap();fs::create_dir(stage.join("Original")).unwrap();
        fs::write(stage.join("Original/Board.kicad_pcb"),&board).unwrap();source::working_copy(&stage.join("Original"),&stage.join("Edited"),&input.source,&AtomicBool::new(false)).unwrap();
        source::write_new(&stage.join("input.json"),&serde_json::to_vec(&input).unwrap()).unwrap();begin(&stage,&input).unwrap();
        fs::write(stage.join("Edited/Board.kicad_pcb"),board.replace("(at 10 20)","(at 11 20)")).unwrap();(root,stage,input)
    }
    #[test]
    fn unlinked_plausible_edit_is_never_adopted_or_replayed(){
        let (root,stage,input)=fixture();let record=read(&stage,&input).unwrap();let (_pins,mut proof)=edited(&stage,&input).unwrap();proof.attempt=record.attempt;drop(_pins);
        source::write_new(&stage.join(EDIT),&serde_json::to_vec(&proof).unwrap()).unwrap();
        let before=fs::read(stage.join("Edited/Board.kicad_pcb")).unwrap();
        assert!(finish(&stage,&input,None,&root.join("unused"),Arc::new(AtomicBool::new(false))).unwrap_err().contains("Falta el vínculo original"));
        assert_eq!(fs::read(stage.join("Edited/Board.kicad_pcb")).unwrap(),before);assert!(!root.join("unused").exists());fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn confirmed_edit_preserves_user_changes_and_rejects_scope_changes(){
        let (root,stage,input)=fixture();retain_edit(&stage,&input).unwrap();
        assert!(finish(&stage,&input,None,&root.join("unused"),Arc::new(AtomicBool::new(false))).unwrap_err().contains("falta comprobarla"));
        let mut other=input.clone();other.pcb_move.x_mm=12.;assert!(read(&stage,&other).is_err());
        let path=stage.join("Edited/Board.kicad_pcb");fs::write(&path,b"preserve later manual work").unwrap();
        assert!(finish(&stage,&input,None,&root.join("unused"),Arc::new(AtomicBool::new(false))).is_err());assert_eq!(fs::read(path).unwrap(),b"preserve later manual work");fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lost_edit_link_keeps_raw_proof_but_cannot_adopt_it_after_restart(){
        let (root,stage,input)=fixture();let held=source::open_regular(&stage.join(JOURNAL),LIMIT).unwrap();
        assert!(retain_edit(&stage,&input).is_err());assert!(stage.join(EDIT).exists());drop(held);
        assert!(read(&stage,&input).unwrap().edit_sha256.is_none());
        assert!(finish(&stage,&input,None,&root.join("unused"),Arc::new(AtomicBool::new(false))).is_err());fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn attempt_limit_prevents_native_start_and_keeps_partial_directories(){
        let (root,stage,input)=fixture();retain_edit(&stage,&input).unwrap();let mut record=read(&stage,&input).unwrap();
        for _ in 0..2{let folder=format!("Verification-{}",Uuid::new_v4());fs::create_dir(stage.join(&folder)).unwrap();fs::write(stage.join(&folder).join("partial.native"),b"preserve partial").unwrap();record.checks.push(Check{folder,input:input.source.clone(),report_sha256:None,approval:None});}
        save(&stage,&record).unwrap();assert!(finish(&stage,&input,Some(&root.join("absent-cli.exe")),&root.join("unused"),Arc::new(AtomicBool::new(false))).unwrap_err().contains("dos intentos"));
        for check in record.checks{assert_eq!(fs::read(stage.join(check.folder).join("partial.native")).unwrap(),b"preserve partial");}assert!(!root.join("unused").exists());fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn check_resume_is_bound_to_one_attempt_and_never_replays_an_unconfirmed_check(){
        let (root,stage,input)=fixture();retain_edit(&stage,&input).unwrap();let (edit_sha256,attempts_before)=pending_checkpoint(&stage,&input).unwrap();
        let approval=crate::result_checks::Input{result_checks_version:1,program:None,job_id:Uuid::new_v4(),target_job_id:input.job_id,target_input_sha256:identity(&input).unwrap(),edit_sha256,attempts_before,agreement:crate::native_tools::tests::agreement_fixture(),receipt_sha256:None};
        let proof=fs::read(stage.join(EDIT)).unwrap();let mut record=read(&stage,&input).unwrap();
        record.checks.push(Check{folder:format!("Verification-{}",Uuid::new_v4()),input:input.source.clone(),report_sha256:None,approval:Some(Approval{job_id:approval.job_id,input_sha256:approval.identity().unwrap()})});save(&stage,&record).unwrap();
        let gate=std::sync::Mutex::new(());let paused=AtomicBool::new(false);
        assert!(finish_approved(&stage,&input,&root.join("must-not-start.exe"),&root.join("unused"),Arc::new(AtomicBool::new(false)),crate::process::DispatchGate{mutex:&gate,paused:&paused},&approval).unwrap_err().contains("no está confirmada"));
        assert!(!root.join("unused").exists());assert_eq!(fs::read(stage.join(EDIT)).unwrap(),proof);assert_eq!(read(&stage,&input).unwrap().checks.len(),1);
        let mut changed=approval.clone();changed.job_id=Uuid::new_v4();assert!(approved_check(&stage,&input,&changed).is_err());changed=approval.clone();changed.agreement.outcome="Changed result".into();assert!(approved_check(&stage,&input,&changed).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paused_checks_preserve_the_edit_and_do_not_consume_an_attempt(){
        let (root,stage,input)=fixture();retain_edit(&stage,&input).unwrap();let proof=fs::read(stage.join(EDIT)).unwrap();
        let gate=std::sync::Mutex::new(());let paused=AtomicBool::new(true);
        assert!(finish_gated(&stage,&input,Some(&root.join("must-not-start.exe")),&root.join("unused"),Arc::new(AtomicBool::new(false)),Some(crate::process::DispatchGate{mutex:&gate,paused:&paused})).unwrap_err().contains("pausados"));
        assert!(read(&stage,&input).unwrap().checks.is_empty());assert_eq!(fs::read(stage.join(EDIT)).unwrap(),proof);assert!(!root.join("unused").exists());fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_recovery_is_refused_without_waiting_or_consuming_attempts(){
        let (root,_stage,input)=fixture();let guard=Exclusive::acquire(&input).unwrap();let other=input.clone();
        assert!(std::thread::spawn(move||Exclusive::acquire(&other).is_err()).join().unwrap());drop(guard);
        assert!(std::thread::spawn(move||Exclusive::acquire(&input).is_ok()).join().unwrap());fs::remove_dir_all(root).unwrap();
    }
}
