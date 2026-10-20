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
    pub result_checks_version:u32,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub program:Option<crate::host_tools::Program>,
    pub job_id:Uuid,pub target_job_id:Uuid,
    pub target_input_sha256:String,pub edit_sha256:String,pub attempts_before:usize,
    pub agreement:crate::native_tools::Agreement,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub receipt_sha256:Option<String>,
}
impl Input{
    pub fn validate(&self)->Result<(),String>{
        self.agreement.validate()?;
        if !matches!((self.result_checks_version,self.program),(1,None)|(2,Some(crate::host_tools::Program::Agent))|(3,Some(crate::host_tools::Program::Freecad))) || self.job_id.is_nil() || self.target_job_id.is_nil() || self.job_id==self.target_job_id
            || !source::valid_hash(&self.target_input_sha256) || !source::valid_hash(&self.edit_sha256) || self.attempts_before>=2
            || self.receipt_sha256.as_ref().is_some_and(|h|!source::valid_hash(h)){return Err(PAUSE.into());}Ok(())
    }
    pub(crate) fn identity(&self)->Result<String,String>{let mut approved=self.clone();approved.receipt_sha256=None;serde_json::to_vec(&approved).map(|b|source::digest(&b)).map_err(|_|PAUSE.into())}
}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Report{version:u32,job_id:Uuid,target_job_id:Uuid,target_result_sha256:String}
pub(crate) fn directory(project:&Project,id:Uuid)->Result<PathBuf,String>{let stage=project.store("staging").join(id.to_string());Ok(if stage.try_exists().map_err(|_|PAUSE)?{stage}else{project.artifact_directory(id)})}

fn original_identity(input:&OperationInput)->Result<String,String>{match input{
    OperationInput::KonnectPcb(original)=>copy::identity(original),
    OperationInput::LiveAgent(original)=>crate::agent_editor::completion_identity(original),
    OperationInput::LiveCad(original)=>crate::cadpilot_live::completion_input(original),
    _=>Err(PAUSE.into()),
}}
fn target(project:&Project,input:&Input)->Result<(Snapshot,PathBuf),String>{
    input.validate()?;
    let snapshot=project.history().map_err(|_|PAUSE)?.into_iter().find(|s|s.input.job_id()==input.target_job_id).ok_or(PAUSE)?;
    if original_identity(&snapshot.input)?!=input.target_input_sha256
        || !matches!((&snapshot.input,input.program),(OperationInput::KonnectPcb(_),None)|(OperationInput::LiveAgent(_),Some(crate::host_tools::Program::Agent))|(OperationInput::LiveCad(_),Some(crate::host_tools::Program::Freecad))){return Err(PAUSE.into());}
    Ok((snapshot,directory(project,input.target_job_id)?))
}
pub(crate) fn prepare(project:&Project,target_job_id:Uuid,agreement:crate::native_tools::Agreement)->Result<Input,String>{
    let snapshot=project.history().map_err(|_|PAUSE)?.into_iter().find(|s|s.input.job_id()==target_job_id).ok_or(PAUSE)?;
    if matches!(snapshot.phase,Phase::Running|Phase::AwaitingApproval|Phase::Completed){return Err("Ese trabajo no necesita esta reanudación; consulta su estado y resultado conservado.".into());}
    let directory=directory(project,target_job_id)?;
    let (program,(edit_sha256,attempts_before))=match &snapshot.input{
        OperationInput::KonnectPcb(original)=>(None,copy::pending_checkpoint(&directory,original)?),
        OperationInput::LiveAgent(original)=>(Some(crate::host_tools::Program::Agent),crate::agent_editor::pending_copy_check(&directory,original)?),
        OperationInput::LiveCad(original)=>(Some(crate::host_tools::Program::Freecad),crate::native_working_version::pending_copy_check(&directory,original)?),
        _=>return Err("Este resultado todavía necesita su ruta específica de revisión.".into()),
    };
    let input=Input{result_checks_version:match program{Some(crate::host_tools::Program::Freecad)=>3,Some(_)=>2,None=>1},program,job_id:Uuid::new_v4(),target_job_id,target_input_sha256:original_identity(&snapshot.input)?,edit_sha256,attempts_before,agreement,receipt_sha256:None};input.validate()?;Ok(input)
}
fn publish_target(project:&Project,input:&Input)->Result<(),String>{
    let (mut snapshot,directory)=target(project,input)?;snapshot.input.retain_receipt(&directory).map_err(|_|PAUSE)?;
    project.validate_outputs(&directory,&snapshot.input).map_err(|_|PAUSE)?;project.save(&snapshot).map_err(|_|PAUSE)?;
    project.recover_except(Some(input.job_id)).map_err(|_|PAUSE)?;Ok(())
}
pub(crate) fn run(project:&Project,input:&Input,cli:Option<&Path>,stage:&Path,profile:&Path,cancel:Arc<AtomicBool>,gate:crate::process::DispatchGate<'_>)->Result<(),String>{
    let (snapshot,directory)=target(project,input)?;
    let mut approved=input.clone();approved.receipt_sha256=None;
    source::write_new(&stage.join("input.json"),&serde_json::to_vec(&approved).map_err(|_|PAUSE)?)?;
    match &snapshot.input{
        OperationInput::KonnectPcb(original)=>{copy::finish_approved(&directory,original,cli.ok_or("Falta el lector KiCad compatible." )?,profile,cancel,gate,input)?;},
        OperationInput::LiveAgent(original)=>crate::agent_editor::finish_approved_copy_check(&directory,original,profile,cancel,gate,input)?,
        OperationInput::LiveCad(original)=>crate::native_working_version::finish_approved_copy_check(cli.ok_or("Falta el lector FreeCAD compatible.")?,&directory,original,profile,(cancel,gate),input)?,
        _=>return Err(PAUSE.into()),
    }
    publish_target(project,input)?;recover(project,stage,input)
}
// Explicit recovery repairs only the exact completed approval link, never launches a reader.
pub(crate) fn recover_target(project:&Project,input:&Input)->Result<(),String>{
    let (snapshot,directory)=target(project,input)?;
    if let OperationInput::LiveAgent(original)=&snapshot.input{
        crate::agent_editor::recover_approved_copy_check(&directory,original,input)?;
        publish_target(project,input)?;
    }else if let OperationInput::LiveCad(original)=&snapshot.input{
        crate::native_working_version::recover_approved_copy_check(&directory,original,input)?;publish_target(project,input)?;
    }Ok(())
}
fn report(project:&Project,input:&Input)->Result<Report,String>{
    let (snapshot,directory)=target(project,input)?;
    if snapshot.phase!=Phase::Completed{return Err("La comprobación se conserva; su publicación original sigue pendiente.".into());}
    project.validate_outputs(&directory,&snapshot.input).map_err(|_|PAUSE)?;
    match &snapshot.input{
        OperationInput::KonnectPcb(original)=>copy::approved_check(&directory,original,input)?,
        OperationInput::LiveAgent(original)=>{crate::agent_editor::approved_copy_check(&directory,original,input)?;},
        OperationInput::LiveCad(original)=>{crate::native_working_version::approved_copy_check(&directory,original,input)?;},
        _=>return Err(PAUSE.into()),
    }
    let bytes=source::read_regular(&directory.join("result.json"),1024*1024)?;
    let hash=source::digest(&bytes);if snapshot.input.receipt_sha256()!=Some(hash.as_str()){return Err(PAUSE.into());}
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
    fn arduino_resume_binds_the_retained_tree_and_never_requires_its_source_editor(){
        let (root,stage,original)=crate::agent_editor::tests::retained_copy_fixture();
        let folder=root.join("task");std::fs::create_dir(&folder).unwrap();let project=Project::create(&folder).unwrap();let task=crate::tasks::Store::new(&project).unwrap().state().unwrap().active;
        let target_stage=project.store("staging").join(original.job_id.to_string());assert!(stage.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()) && target_stage.parent().unwrap().canonicalize().unwrap().starts_with(project.root.canonicalize().unwrap()));std::fs::rename(stage,&target_stage).unwrap();
        let mut snapshot=project.snapshot(OperationInput::LiveAgent(original.clone()),Phase::Cancelled).unwrap();snapshot.audit.as_mut().unwrap().task=Some(task.binding());snapshot.audit.as_mut().unwrap().native_attempted=Some(true);snapshot.audit.as_mut().unwrap().source=crate::project::RunSource::Mcp;project.save(&snapshot).unwrap();
        let plan=prepare(&project,original.job_id,crate::native_tools::tests::agreement_fixture()).unwrap();assert_eq!(plan.result_checks_version,2);assert_eq!(plan.program,Some(crate::host_tools::Program::Agent));assert_eq!(plan.attempts_before,0);assert_eq!(plan.target_input_sha256,crate::agent_editor::completion_identity(&original).unwrap());
        let saved=serde_json::to_vec(&plan).unwrap();assert!(target(&project,&plan).is_ok());std::fs::rename(root.join("OwnedSketch"),root.join("offline-source")).unwrap();assert_eq!(prepare(&project,original.job_id,plan.agreement.clone()).unwrap().edit_sha256,plan.edit_sha256);
        let mut changed=plan.clone();changed.program=None;assert!(changed.validate().is_err());changed=plan.clone();changed.result_checks_version=1;assert!(changed.validate().is_err());changed=plan.clone();changed.target_input_sha256="a".repeat(64);assert!(target(&project,&changed).is_err());assert_eq!(serde_json::to_vec(&plan).unwrap(),saved);
        let copy=std::fs::read_dir(&target_stage).unwrap().filter_map(Result::ok).find(|e|e.file_name().to_string_lossy().starts_with("SketchVersion-")).unwrap().path().join("OwnedSketch/Closed.bin");std::fs::write(&copy,b"preserve later manual work").unwrap();assert!(prepare(&project,original.job_id,plan.agreement).is_err());assert_eq!(std::fs::read(copy).unwrap(),b"preserve later manual work");
        drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }

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
