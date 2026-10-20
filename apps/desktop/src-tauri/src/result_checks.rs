//! Fresh approval for checks of an immutable retained result, never its original edit.
use crate::{kicad_project as source,project::{Project,OperationInput,Snapshot,Phase},pcb_copy_recovery as copy};
use serde::{Serialize,Deserialize};
use std::{path::{Path,PathBuf},sync::{Arc,atomic::AtomicBool}};
use uuid::Uuid;
const LIMIT:u64=65536;
const PAUSE:&str="La copia o su alcance cambió. Se conserva el trabajo; prepara una nueva revisión sin repetir la edición.";
#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input{
    pub result_checks_version:u32,pub job_id:Uuid,pub target_job_id:Uuid,
    pub target_input_sha256:String,pub edit_sha256:String,pub attempts_before:usize,
    pub agreement:crate::native_tools::Agreement,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub receipt_sha256:Option<String>,
}
impl Input{
    pub fn validate(&self)->Result<(),String>{
        self.agreement.validate()?;
        if self.result_checks_version!=1 || self.job_id.is_nil() || self.target_job_id.is_nil() || self.job_id==self.target_job_id
            || !source::valid_hash(&self.target_input_sha256) || !source::valid_hash(&self.edit_sha256) || self.attempts_before>=2
            || self.receipt_sha256.as_ref().is_some_and(|h|!source::valid_hash(h)){return Err(PAUSE.into());}Ok(())
    }
    pub(crate) fn identity(&self)->Result<String,String>{let mut approved=self.clone();approved.receipt_sha256=None;serde_json::to_vec(&approved).map(|b|source::digest(&b)).map_err(|_|PAUSE.into())}
}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Report{version:u32,job_id:Uuid,target_job_id:Uuid,target_result_sha256:String}
pub(crate) fn directory(project:&Project,id:Uuid)->Result<PathBuf,String>{let stage=project.store("staging").join(id.to_string());Ok(if stage.try_exists().map_err(|_|PAUSE)?{stage}else{project.artifact_directory(id)})}
fn target(project:&Project,input:&Input)->Result<(Snapshot,crate::konnect_pcb::Input,PathBuf),String>{
    input.validate()?;
    let snapshot=project.history().map_err(|_|PAUSE)?.into_iter().find(|s|s.input.job_id()==input.target_job_id).ok_or(PAUSE)?;
    let OperationInput::KonnectPcb(original)=&snapshot.input else{return Err(PAUSE.into());};
    if copy::identity(original)?!=input.target_input_sha256{return Err(PAUSE.into());}
    Ok((snapshot.clone(),original.clone(),directory(project,input.target_job_id)?))
}
pub(crate) fn prepare(project:&Project,target_job_id:Uuid,agreement:crate::native_tools::Agreement)->Result<Input,String>{
    let snapshot=project.history().map_err(|_|PAUSE)?.into_iter().find(|s|s.input.job_id()==target_job_id).ok_or(PAUSE)?;
    if matches!(snapshot.phase,Phase::Running|Phase::AwaitingApproval|Phase::Completed){return Err("Ese trabajo no necesita esta reanudación; consulta su estado y resultado conservado.".into());}
    let OperationInput::KonnectPcb(original)=&snapshot.input else{return Err("Este resultado todavía necesita su ruta específica de revisión.".into());};
    let (edit_sha256,attempts_before)=copy::pending_checkpoint(&directory(project,target_job_id)?,original)?;
    let input=Input{result_checks_version:1,job_id:Uuid::new_v4(),target_job_id,target_input_sha256:copy::identity(original)?,edit_sha256,attempts_before,agreement,receipt_sha256:None};input.validate()?;Ok(input)
}
pub(crate) fn run(project:&Project,input:&Input,cli:&Path,stage:&Path,profile:&Path,cancel:Arc<AtomicBool>,gate:crate::process::DispatchGate<'_>)->Result<(),String>{
    let (_snapshot,original,directory)=target(project,input)?;
    let mut approved=input.clone();approved.receipt_sha256=None;
    source::write_new(&stage.join("input.json"),&serde_json::to_vec(&approved).map_err(|_|PAUSE)?)?;
    copy::finish_approved(&directory,&original,cli,profile,cancel,gate,input)?;
    let (mut snapshot,_,_)=target(project,input)?;snapshot.input.retain_receipt(&directory).map_err(|_|PAUSE)?;
    project.validate_outputs(&directory,&snapshot.input).map_err(|_|PAUSE)?;project.save(&snapshot).map_err(|_|PAUSE)?;
    project.recover_except(Some(input.job_id)).map_err(|_|PAUSE)?;
    recover(project,stage,input)
}
fn report(project:&Project,input:&Input)->Result<Report,String>{
    let (snapshot,original,directory)=target(project,input)?;
    if snapshot.phase!=Phase::Completed{return Err("La comprobación se conserva; su publicación original sigue pendiente.".into());}
    project.validate_outputs(&directory,&snapshot.input).map_err(|_|PAUSE)?;
    copy::approved_check(&directory,&original,input)?;
    let bytes=source::read_regular(&directory.join("result.json"),1024*1024)?;
    let hash=source::digest(&bytes);if original.receipt_sha256.as_deref()!=Some(hash.as_str()){return Err(PAUSE.into());}
    Ok(Report{version:1,job_id:input.job_id,target_job_id:input.target_job_id,target_result_sha256:hash})
}
pub(crate) fn recover(project:&Project,stage:&Path,input:&Input)->Result<(),String>{
    let _pins=crate::project::pin_lineage(stage).map_err(|_|PAUSE)?;
    let bytes=serde_json::to_vec_pretty(&report(project,input)?).map_err(|_|PAUSE)?;
    let path=stage.join("result.json");if path.try_exists().map_err(|_|PAUSE)?{if source::read_regular(&path,LIMIT)?!=bytes{return Err(PAUSE.into());}}else{source::write_new(&path,&bytes)?;}Ok(())
}
pub(crate) fn validate(project:&Project,stage:&Path,input:&Input)->Result<(),String>{
    let mut approved=input.clone();approved.receipt_sha256=None;
    if source::read_regular(&stage.join("input.json"),LIMIT)?!=serde_json::to_vec(&approved).map_err(|_|PAUSE)?{return Err(PAUSE.into());}
    let bytes=source::read_regular(&stage.join("result.json"),LIMIT)?;
    if bytes!=serde_json::to_vec_pretty(&report(project,input)?).map_err(|_|PAUSE)? || input.receipt_sha256.as_ref().is_some_and(|h|h!=&source::digest(&bytes)){return Err(PAUSE.into());}
    let files=crate::kicad_run::files(stage)?;if files.len()!=2{return Err(PAUSE.into());}Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn resume_scope_keeps_the_original_edit_and_rejects_changed_or_unknown_targets(){
        let (root,stage,original)=copy::tests::fixture();copy::retain_edit(&stage,&original).unwrap();
        let folder=root.join("task");std::fs::create_dir(&folder).unwrap();let project=Project::create(&folder).unwrap();
        let target_stage=project.store("staging").join(original.job_id.to_string());
        assert!(stage.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()) && target_stage.parent().unwrap().canonicalize().unwrap().starts_with(project.root.canonicalize().unwrap()));std::fs::rename(stage,&target_stage).unwrap();
        let snapshot=project.snapshot(OperationInput::KonnectPcb(original.clone()),Phase::Cancelled).unwrap();project.save(&snapshot).unwrap();
        let plan=prepare(&project,original.job_id,crate::native_tools::tests::agreement_fixture()).unwrap();
        assert_ne!(plan.job_id,original.job_id);assert_eq!(plan.attempts_before,0);assert_eq!(plan.target_input_sha256,copy::identity(&original).unwrap());
        let mut changed=plan.clone();changed.target_input_sha256="a".repeat(64);assert!(target(&project,&changed).is_err());
        changed=plan.clone();changed.target_job_id=Uuid::new_v4();assert!(target(&project,&changed).is_err());
        changed=plan.clone();changed.attempts_before=2;assert!(changed.validate().is_err());
        let bytes=std::fs::read(target_stage.join("Edited/Board.kicad_pcb")).unwrap();let proof=std::fs::read(target_stage.join("pcb-copy-edit.json")).unwrap();
        assert!(recover(&project,&target_stage,&plan).is_err());assert_eq!(std::fs::read(target_stage.join("Edited/Board.kicad_pcb")).unwrap(),bytes);assert_eq!(std::fs::read(target_stage.join("pcb-copy-edit.json")).unwrap(),proof);
        std::fs::write(target_stage.join("Edited/Board.kicad_pcb"),b"preserve manual correction").unwrap();assert!(prepare(&project,original.job_id,plan.agreement).is_err());
        assert_eq!(std::fs::read(target_stage.join("Edited/Board.kicad_pcb")).unwrap(),b"preserve manual correction");drop(project);std::fs::remove_dir_all(root).unwrap();
    }
}
