//! Bounded native-reader admissions and original completion links; no native dispatch.
use serde::{Serialize,Deserialize};
use std::{fs,path::{Path,PathBuf}};
use crate::kicad_project::{read_regular,write_new};
const ERROR:&str="La comprobación conserva sus archivos. No repitas un intento incierto.";
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Owner{pub job_id:uuid::Uuid,pub input_sha256:String}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Readback{pub version:u32,pub capture_sha256:String,
    #[serde(default,skip_serializing_if="Option::is_none")]pub approval:Option<Owner>,
    #[serde(default,skip_serializing_if="Option::is_none")]pub result_sha256:Option<String>}
pub(crate) fn read(destination:&Path)->Result<Vec<Readback>,String>{
    let destination=destination.canonicalize().map_err(|_|ERROR)?;
    let mut records=Vec::new();
    for entry in fs::read_dir(&destination).map_err(|_|ERROR)?{
        let entry=entry.map_err(|_|ERROR)?;let name=entry.file_name();let name=name.to_str().ok_or(ERROR)?;
        if !name.starts_with("readback-"){continue;}
        let id=name.strip_prefix("readback-").and_then(|s|s.strip_suffix(".json")).and_then(|s|uuid::Uuid::parse_str(s).ok()).ok_or(ERROR)?;
        if name!=format!("readback-{id}.json"){return Err(ERROR.into());}
        let record:Readback=serde_json::from_slice(&read_regular(&entry.path(),4096).map_err(|_|ERROR)?).map_err(|_|ERROR)?;
        if record.version!=1 || !crate::kicad_project::valid_hash(&record.capture_sha256)
            || record.result_sha256.as_ref().is_some_and(|h|!crate::kicad_project::valid_hash(h))
            || record.approval.as_ref().is_some_and(|a|a.job_id.is_nil() || !crate::kicad_project::valid_hash(&a.input_sha256)){return Err(ERROR.into());}
        records.push(record);if records.len()>2{return Err(ERROR.into());}
    }Ok(records)
}


pub(crate) fn count(root:&Path,capture:&str)->Result<usize,String>{
    let records=read(root)?;if !crate::kicad_project::valid_hash(capture) || records.iter().any(|r|r.capture_sha256!=capture){return Err(ERROR.into());}Ok(records.len())
}
pub(crate) struct Ticket{path:PathBuf,record:Readback}
pub(crate) fn begin(root:&Path,capture:&str,approval:Option<&crate::result_checks::Input>)->Result<Ticket,String>{
    let root=root.canonicalize().map_err(|_|ERROR)?;let count=count(&root,capture)?;
    if count>=2{return Err("Se agotaron las dos comprobaciones registradas. Se conservó la copia; no repitas la edición.".into());}
    if let Some(approval)=approval{approval.validate()?;if approval.attempts_before!=count{return Err(ERROR.into());}}
    let record=Readback{version:1,capture_sha256:capture.into(),approval:approval.map(|a|a.identity().map(|identity|Owner{job_id:a.job_id,input_sha256:identity})).transpose()?,result_sha256:None};
    let path=root.join(format!("readback-{}.json",uuid::Uuid::new_v4()));write_new(&path,&serde_json::to_vec(&record).map_err(|_|ERROR)?)?;Ok(Ticket{path,record})
}
pub(crate) fn complete(mut ticket:Ticket,result:&str)->Result<(),String>{
    if !crate::kicad_project::valid_hash(result) || read_regular(&ticket.path,4096)?!=serde_json::to_vec(&ticket.record).map_err(|_|ERROR)?{return Err(ERROR.into());}
    ticket.record.result_sha256=Some(result.into());crate::project::atomic_json(&ticket.path,&ticket.record,true).map_err(|_|ERROR.into())
}
pub(crate) fn approved(root:&Path,capture:&str,approval:&crate::result_checks::Input)->Result<String,String>{
    approval.validate()?;if count(root,capture)?!=approval.attempts_before+1{return Err(ERROR.into());}
    let identity=approval.identity()?;let records=read(root)?;let owned=records.iter().filter(|r|r.approval.as_ref().is_some_and(|a|a.job_id==approval.job_id && a.input_sha256==identity)).collect::<Vec<_>>();
    if owned.len()!=1{return Err(ERROR.into());}owned[0].result_sha256.clone().ok_or(ERROR.into())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn readback_links_are_owned_bounded_and_never_adopt_unknown_or_changed_tickets(){
        let root=std::env::temp_dir().join(format!("hazlo-reader-ledger-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
        let input=crate::result_checks::Input{result_checks_version:3,program:Some(crate::host_tools::Program::Freecad),job_id:uuid::Uuid::new_v4(),target_job_id:uuid::Uuid::new_v4(),target_input_sha256:"b".repeat(64),edit_sha256:"c".repeat(64),attempts_before:0,agreement:crate::native_tools::tests::agreement_fixture(),receipt_sha256:None};
        let capture="a".repeat(64);let ticket=begin(&root,&capture,Some(&input)).unwrap();assert!(approved(&root,&capture,&input).is_err());assert!(begin(&root,&capture,Some(&input)).is_err());
        complete(ticket,&"d".repeat(64)).unwrap();assert_eq!(approved(&root,&capture,&input).unwrap(),"d".repeat(64));
        let mut other=input.clone();other.job_id=uuid::Uuid::new_v4();assert!(approved(&root,&capture,&other).is_err());assert!(approved(&root,&"e".repeat(64),&input).is_err());
        let second=begin(&root,&capture,None).unwrap();let path=second.path.clone();fs::write(&path,b"preserve changed ticket").unwrap();assert!(complete(second,&"f".repeat(64)).is_err());assert_eq!(fs::read(path).unwrap(),b"preserve changed ticket");
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
}
