//! Project files stay in the native sketch format; the installed CLI checks the new copy.
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::{collections::{BTreeMap,BTreeSet},ffi::OsString,fs::{self,File},io::Read,
    os::windows::fs::MetadataExt,path::{Path,PathBuf},sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
use crate::{kicad_project::{digest,relative,read_regular,write_new,open_regular_allow_empty},project::{pin_directory,pin_lineage,valid_location,directory_identity}};
const FILE_LIMIT:u64=8*1024*1024;
const TOTAL_LIMIT:usize=32*1024*1024;
const MANIFEST_LIMIT:u64=512*1024;
const ERROR:&str="La copia del proyecto sigue pendiente. Se conservó el trabajo; no repitas la edición.";

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan{version:u32,verifier:PathBuf,verifier_sha256:String,source_root_id:(u32,u32,u32),sketch_name:String,excluded:Vec<String>}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Link{folder:String,sha256:String}
#[derive(Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct Entry{path:String,bytes:u64,sha256:String,native_buffer_utf8:bool}
#[derive(Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Capture{version:u32,input_sha256:String,buffer_sha256:String,source_root_id:(u32,u32,u32),sketch_name:String,
    directories:Vec<String>,files:Vec<Entry>,excluded:Vec<String>,encoding_policy:String}
struct Tree{_pins:Vec<File>,files:BTreeMap<String,Vec<u8>>,directories:BTreeSet<String>}
pub(crate) struct Scope<'a>{pub input_sha256:&'a str,pub buffer_sha256:&'a str,pub buffers:&'a [(String,String)]}
fn stop(cancel:&AtomicBool)->Result<(),String>{if cancel.load(Ordering::SeqCst){Err(ERROR.into())}else{Ok(())}}
fn checked_relative(value:&str)->Result<PathBuf,String>{
    let path=relative(value)?;
    if value.split('/').any(|p|p.is_empty() || p=="." || p=="..") {return Err(ERROR.into());}
    valid_location(&Path::new("C:/native-copy").join(&path)).map_err(|_|ERROR)?;Ok(path)
}
impl Plan{
    pub(crate) fn validate(&self)->Result<(),String>{
        valid_location(&self.verifier).map_err(|_|ERROR)?;
        checked_relative(&self.sketch_name)?;
        if self.version!=1 || self.sketch_name.contains('/') || self.sketch_name.len()>128
            || !self.verifier.file_name().is_some_and(|n|n.eq_ignore_ascii_case("arduino-cli.exe"))
            || !crate::kicad_project::valid_hash(&self.verifier_sha256) || self.excluded.len()>2 {return Err(ERROR.into());}
        for name in &self.excluded{checked_relative(name)?;}
        Ok(())
    }
    fn main_name(&self)->String{format!("{}.ino",self.sketch_name)}
}
impl Link{
    fn validate(&self)->Result<(),String>{
        let id=self.folder.strip_prefix("SketchVersion-").and_then(|s|uuid::Uuid::parse_str(s).ok()).ok_or(ERROR)?;
        if self.folder!=format!("SketchVersion-{id}") || !crate::kicad_project::valid_hash(&self.sha256){return Err(ERROR.into());}Ok(())
    }
}
fn excluded(name:&str,excluded:&[String])->bool{excluded.iter().any(|p|name.eq_ignore_ascii_case(p)
    || name.to_ascii_lowercase().starts_with(&(p.to_ascii_lowercase()+"/")))}
fn names(root:&Path,excluded_paths:&[String],cancel:&AtomicBool)->Result<(BTreeSet<String>,BTreeSet<String>),String>{
    let mut files=BTreeSet::new();let mut directories=BTreeSet::new();let mut seen=BTreeSet::new();let mut pending=vec![(PathBuf::new(),0)];
    while let Some((relative_dir,depth))=pending.pop(){
        stop(cancel)?;if depth>16{return Err(ERROR.into());}
        for entry in fs::read_dir(root.join(&relative_dir)).map_err(|_|ERROR)?{
            stop(cancel)?;let entry=entry.map_err(|_|ERROR)?;let rel=relative_dir.join(entry.file_name());
            let name=rel.to_str().ok_or(ERROR)?.replace('\\',"/");checked_relative(&name)?;
            if excluded(&name,excluded_paths){continue;}
            if !seen.insert(name.to_lowercase()){return Err("El proyecto contiene nombres ambiguos para Windows. No se cambió ningún archivo.".into());}
            let metadata=fs::symlink_metadata(entry.path()).map_err(|_|ERROR)?;
            if metadata.file_attributes() & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT!=0{return Err(ERROR.into());}
            if metadata.is_dir(){directories.insert(name);pending.push((rel,depth+1));}
            else if metadata.is_file(){files.insert(name);}else{return Err(ERROR.into());}
            if files.len()>256 || directories.len()>128{return Err("El proyecto supera los límites de copia (256 archivos y 128 carpetas). El trabajo se conservó.".into());}
        }
    }Ok((files,directories))
}
fn tree(root:&Path,excluded_paths:&[String],cancel:&AtomicBool)->Result<Tree,String>{
    let mut pins=pin_lineage(root).map_err(|_|ERROR)?;
    // The native NT reader needs the extended Windows spelling for long project paths.
    let root=root.canonicalize().map_err(|_|ERROR)?;let root=root.as_path();let (file_names,directories)=names(root,excluded_paths,cancel)?;
    for dir in &directories{pins.push(pin_directory(&root.join(dir),false).map_err(|_|ERROR)?);}
    let mut files=BTreeMap::new();let mut total=0;
    for name in &file_names{
        stop(cancel)?;let mut file=open_regular_allow_empty(&root.join(name),FILE_LIMIT).map_err(|_|ERROR)?;
        let length=file.metadata().map_err(|_|ERROR)?.len();total+=length as usize;
        if total>TOTAL_LIMIT{return Err("El proyecto supera el límite de copia de 32 MiB. El trabajo se conservó.".into());}
        let mut bytes=Vec::new();Read::by_ref(&mut file).take(FILE_LIMIT+1).read_to_end(&mut bytes).map_err(|_|ERROR)?;
        if bytes.len() as u64!=length{return Err(ERROR.into());}pins.push(file);files.insert(name.clone(),bytes);
    }
    if names(root,excluded_paths,cancel)?!=(file_names,directories.clone()){return Err("Los archivos cambiaron durante la lectura. Conserva el trabajo y vuelve a inspeccionar; no repitas la edición.".into());}
    Ok(Tree{_pins:pins,files,directories})
}
fn profile(path:&Path)->Result<(BTreeMap<OsString,OsString>,Vec<File>),String>{
    fs::create_dir(path).map_err(|_|ERROR)?;let mut pins=pin_lineage(path).map_err(|_|ERROR)?;
    let mut env=BTreeMap::new();for key in ["SystemRoot","WINDIR","COMSPEC"]{if let Some(value)=std::env::var_os(key){env.insert(key.into(),value);}}
    for name in ["home","data","downloads","user","temp"]{fs::create_dir(path.join(name)).map_err(|_|ERROR)?;pins.push(pin_directory(&path.join(name),false).map_err(|_|ERROR)?);}
    for key in ["USERPROFILE","HOME","APPDATA","LOCALAPPDATA"]{env.insert(key.into(),path.join("home").into_os_string());}
    for key in ["TEMP","TMP"]{env.insert(key.into(),path.join("temp").into_os_string());}
    write_new(&path.join("arduino-cli.yaml"),&serde_json::to_vec(&json!({"directories":{"data":path.join("data"),"downloads":path.join("downloads"),"user":path.join("user")}})).map_err(|_|ERROR)?)?;
    pins.push(crate::kicad_project::open_regular(&path.join("arduino-cli.yaml"),65536).map_err(|_|ERROR)?);Ok((env,pins))
}
fn verifier_pins(plan:&Plan)->Result<Vec<File>,String>{
    plan.validate()?;let mut pins=pin_lineage(plan.verifier.parent().ok_or(ERROR)?).map_err(|_|ERROR)?;
    pins.push(crate::kicad_project::open_regular(&plan.verifier,64*1024*1024).map_err(|_|ERROR)?);
    let bytes=read_regular(&plan.verifier,64*1024*1024).map_err(|_|ERROR)?;
    if digest(&bytes)!=plan.verifier_sha256{return Err("El lector nativo cambió desde la aprobación. No se repitió ninguna edición.".into());}Ok(pins)
}
pub(crate) fn pin_source(plan:&Plan,root:&Path)->Result<Vec<File>,String>{
    let mut pins=pin_lineage(root).map_err(|_|ERROR)?;
    if directory_identity(pins.last().ok_or(ERROR)?).map_err(|_|ERROR)?!=plan.source_root_id{return Err("La carpeta nativa cambió de identidad. No se envió otro cambio.".into());}
    pins.extend(verifier_pins(plan)?);Ok(pins)
}
pub(crate) fn prepare(executable:&Path,root:&Path,project:&Path,before_name:&str,private_profile:&Path,cancel:Arc<AtomicBool>)->Result<Plan,String>{
    stop(&cancel)?;let pins=pin_lineage(root).map_err(|_|ERROR)?;
    let source_root_id=directory_identity(pins.last().ok_or(ERROR)?).map_err(|_|ERROR)?;
    let sketch_name=root.file_name().and_then(|n|n.to_str()).ok_or(ERROR)?.to_owned();
    let mut excluded_paths=Vec::new();
    let canonical_root=root.canonicalize().map_err(|_|ERROR)?;let canonical_project=project.canonicalize().map_err(|_|ERROR)?;
    if let Ok(relative_project)=canonical_project.strip_prefix(&canonical_root){
        for name in [".hazlomexa","hazlomx.json"]{excluded_paths.push(relative_project.join(name).to_str().ok_or(ERROR)?.replace('\\',"/"));}
    }
    let plan=Plan{version:1,verifier:executable.to_owned(),verifier_sha256:digest(&read_regular(executable,64*1024*1024).map_err(|_|ERROR)?),source_root_id,sketch_name,excluded:excluded_paths};
    let _verifier=verifier_pins(&plan)?;let snapshot=tree(root,&plan.excluded,&cancel)?;
    if !snapshot.files.keys().any(|n|n.eq_ignore_ascii_case(&plan.main_name())) && !before_name.eq_ignore_ascii_case(&plan.main_name()){return Err("La carpeta no tiene el sketch principal que espera el lector nativo. El trabajo se conservó antes de editar.".into());}
    drop(snapshot);drop(pins);let (env,_profile_pins)=profile(private_profile)?;
    let args=["--config-file".into(),private_profile.join("arduino-cli.yaml").into_os_string(),"version".into(),"--format".into(),"json".into()];
    let result=crate::process::run_owned(executable,&args,private_profile,&env,cancel,Duration::from_secs(15)).map_err(|_|ERROR)?;
    let identity:Value=serde_json::from_slice(&result.stdout).map_err(|_|ERROR)?;
    if result.reason!=crate::process::StopReason::Completed || result.exit_code!=Some(0) || result.truncated
        || identity["Application"]!="arduino-cli" || identity["VersionString"]!=crate::arduino::CLI_VERSION{return Err("No se confirmó el lector Arduino CLI compatible; no se cambió el sketch.".into());}
    Ok(plan)
}
pub(crate) fn capture<F:FnOnce()->Result<(),String>>(plan:&Plan,root:&Path,stage:&Path,scope:&Scope<'_>,cancel:&AtomicBool,confirm:F)->Result<(Link,Capture),String>{
    if !crate::kicad_project::valid_hash(scope.input_sha256) || !crate::kicad_project::valid_hash(scope.buffer_sha256){return Err(ERROR.into());}
    let _pins=pin_source(plan,root)?;let mut snapshot=tree(root,&plan.excluded,cancel)?;
    confirm()?;
    let expected_files=snapshot.files.keys().cloned().collect::<BTreeSet<_>>();
    if names(root,&plan.excluded,cancel)?!=(expected_files,snapshot.directories.clone()){return Err(ERROR.into());}
    let mut native_names=BTreeSet::new();
    for (requested_name,source) in scope.buffers{
        if source.len() as u64>FILE_LIMIT{return Err(ERROR.into());}
        let name=if let Some(existing)=snapshot.files.keys().find(|n|n.eq_ignore_ascii_case(requested_name)){existing.clone()}else{
            checked_relative(requested_name)?;
            let path=Path::new(requested_name);let parent=path.parent().filter(|p|!p.as_os_str().is_empty());
            if let Some(parent)=parent{
                let parent_name=parent.to_str().ok_or(ERROR)?.replace('\\',"/");
                let existing=snapshot.directories.iter().find(|n|n.eq_ignore_ascii_case(&parent_name)).ok_or(ERROR)?;
                format!("{existing}/{}",path.file_name().and_then(|n|n.to_str()).ok_or(ERROR)?)
            }else{requested_name.clone()}
        };
        checked_relative(&name)?;if excluded(&name,&plan.excluded) || !native_names.insert(name.to_lowercase()){return Err(ERROR.into());}
        snapshot.files.insert(name.clone(),source.as_bytes().to_vec());
        let mut parent=Path::new(&name).parent();while let Some(path)=parent.filter(|p|!p.as_os_str().is_empty()){
            snapshot.directories.insert(path.to_str().ok_or(ERROR)?.replace('\\',"/"));parent=path.parent();
        }
    }
    if snapshot.files.len()>256 || snapshot.files.values().map(Vec::len).sum::<usize>()>TOTAL_LIMIT || !snapshot.files.keys().any(|n|n.eq_ignore_ascii_case(&plan.main_name())){return Err(ERROR.into());}
    let attempts=fs::read_dir(stage).map_err(|_|ERROR)?.filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("SketchVersion-")).count();
    if attempts>=2{return Err("Se alcanzó el límite de dos intentos de copia del lote. Las copias existentes se conservaron; no repitas la edición.".into());}
    let folder=format!("SketchVersion-{}",uuid::Uuid::new_v4());let destination=stage.join(&folder);fs::create_dir(&destination).map_err(|_|ERROR)?;
    let mut output_pins=pin_lineage(&destination).map_err(|_|ERROR)?;let sketch=destination.join(&plan.sketch_name);fs::create_dir(&sketch).map_err(|_|ERROR)?;
    output_pins.push(pin_directory(&sketch,false).map_err(|_|ERROR)?);
    for dir in &snapshot.directories{stop(cancel)?;fs::create_dir(sketch.join(dir)).map_err(|_|ERROR)?;output_pins.push(pin_directory(&sketch.join(dir),false).map_err(|_|ERROR)?);}
    let mut files=Vec::new();for (name,body) in &snapshot.files{stop(cancel)?;write_new(&sketch.join(name),body).map_err(|_|ERROR)?;
        files.push(Entry{path:name.clone(),bytes:body.len() as u64,sha256:digest(body),native_buffer_utf8:native_names.contains(&name.to_lowercase())});}
    let capture=Capture{version:1,input_sha256:scope.input_sha256.into(),buffer_sha256:scope.buffer_sha256.into(),source_root_id:plan.source_root_id,sketch_name:plan.sketch_name.clone(),
        directories:snapshot.directories.into_iter().collect(),files,excluded:plan.excluded.clone(),encoding_policy:"native_buffers_utf8_closed_files_original_bytes".into()};
    let bytes=serde_json::to_vec(&capture).map_err(|_|ERROR)?;if bytes.len() as u64>MANIFEST_LIMIT{return Err(ERROR.into());}
    write_new(&destination.join("capture.json"),&bytes).map_err(|_|ERROR)?;
    Ok((Link{folder,sha256:digest(&bytes)},capture))
}
fn load(plan:&Plan,stage:&Path,link:&Link,scope:&Scope<'_>)->Result<(Capture,Tree),String>{
    plan.validate()?;link.validate()?;let destination=stage.join(&link.folder);let _pins=pin_lineage(&destination).map_err(|_|ERROR)?;
    let destination=destination.canonicalize().map_err(|_|ERROR)?;
    let bytes=read_regular(&destination.join("capture.json"),MANIFEST_LIMIT).map_err(|_|"No se pudo leer el manifiesto conservado de la copia.")?;
    if digest(&bytes)!=link.sha256{return Err("El manifiesto conservado cambió; no repitas la edición.".into());}let capture:Capture=serde_json::from_slice(&bytes).map_err(|_|"El manifiesto de copia no es válido.")?;
    if capture.version!=1 || capture.input_sha256!=scope.input_sha256 || capture.buffer_sha256!=scope.buffer_sha256 || capture.source_root_id!=plan.source_root_id
        || capture.sketch_name!=plan.sketch_name || capture.excluded!=plan.excluded || capture.encoding_policy!="native_buffers_utf8_closed_files_original_bytes"{return Err("La copia no coincide con el lote original; no repitas la edición.".into());}
    let snapshot=tree(&destination.join(&plan.sketch_name),&[],&AtomicBool::new(false)).map_err(|_|"No se pudo releer la carpeta de la copia conservada. No repitas la edición.")?;
    if snapshot.files.len()!=capture.files.len() || snapshot.directories.iter().cloned().collect::<Vec<_>>()!=capture.directories{return Err(ERROR.into());}
    let mut seen=BTreeSet::new();for entry in &capture.files{checked_relative(&entry.path)?;
        if !seen.insert(entry.path.to_lowercase()) || snapshot.files.get(&entry.path).is_none_or(|b|b.len() as u64!=entry.bytes || digest(b)!=entry.sha256){return Err(ERROR.into());}}
    let expected=scope.buffers.iter().map(|(name,source)|(name.to_lowercase(),digest(source.as_bytes()))).collect::<BTreeMap<_,_>>();
    for entry in &capture.files{if entry.native_buffer_utf8!=expected.contains_key(&entry.path.to_lowercase()) || expected.get(&entry.path.to_lowercase()).is_some_and(|hash|hash!=&entry.sha256){return Err(ERROR.into());}}
    if !expected.keys().all(|name|seen.contains(name)) || !snapshot.files.keys().any(|n|n.eq_ignore_ascii_case(&plan.main_name())){return Err(ERROR.into());}Ok((capture,snapshot))
}
pub(crate) fn validate_capture(plan:&Plan,stage:&Path,link:&Link,scope:&Scope<'_>)->Result<Capture,String>{load(plan,stage,link,scope).map(|(capture,_)|capture)}
pub(crate) fn matches_capture(link:&Link,capture:&Capture)->Result<(),String>{
    link.validate()?;if digest(&serde_json::to_vec(capture).map_err(|_|ERROR)?)!=link.sha256{return Err(ERROR.into());}Ok(())
}
pub(crate) fn entry(plan:&Plan,stage:&Path,link:&Link)->PathBuf{stage.join(&link.folder).join(&plan.sketch_name).join(plan.main_name())}
pub(crate) fn verify(plan:&Plan,stage:&Path,link:&Link,scope:&Scope<'_>,private_profile:&Path,control:(Arc<AtomicBool>,Option<crate::process::DispatchGate<'_>>))->Result<String,String>{
    let (cancel,gate)=control;
    stop(&cancel)?;drop(gate.map(|g|g.enter(&cancel)).transpose()?);let (_,snapshot)=load(plan,stage,link,scope)?;let _verifier=verifier_pins(plan)?;
    let destination=stage.join(&link.folder).canonicalize().map_err(|_|ERROR)?;
    let attempts=fs::read_dir(&destination).map_err(|_|ERROR)?.filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("readback-")).count();
    if attempts>=2{return Err("Se alcanzó el límite de dos comprobaciones nativas de la copia. Se conservó el trabajo; no repitas la edición.".into());}
    write_new(&destination.join(format!("readback-{}.json",uuid::Uuid::new_v4())),&serde_json::to_vec(&json!({"version":1,"capture_sha256":link.sha256})).map_err(|_|ERROR)?)?;
    let (env,_profile_pins)=profile(private_profile)?;let archive=private_profile.join("native-readback.zip");
    let sketch=stage.join(&link.folder).join(&plan.sketch_name).canonicalize().map_err(|_|ERROR)?;
    let args=["--config-file".into(),private_profile.join("arduino-cli.yaml").into_os_string(),"sketch".into(),"archive".into(),
        sketch.into_os_string(),archive.clone().into_os_string(),"--include-build-dir".into(),"--json".into()];
    let permit=gate.map(|g|g.enter(&cancel)).transpose()?;
    let output=crate::process::run_owned_permitted(&plan.verifier,&args,private_profile,&env,cancel.clone(),Duration::from_secs(60),(false,permit)).map_err(|_|ERROR)?;
    write_new(&private_profile.join("stdout.private"),&output.stdout).map_err(|_|ERROR)?;write_new(&private_profile.join("stderr.private"),&output.stderr).map_err(|_|ERROR)?;
    if output.reason!=crate::process::StopReason::Completed || output.exit_code!=Some(0) || output.truncated{return Err(ERROR.into());}
    let mut native=zip::ZipArchive::new(crate::kicad_project::open_regular(&archive,64*1024*1024).map_err(|_|ERROR)?).map_err(|_|ERROR)?;
    let mut seen=BTreeSet::new();for i in 0..native.len(){stop(&cancel)?;let mut file=native.by_index(i).map_err(|_|ERROR)?;
        let name=file.name().map_err(|_|ERROR)?.replace('\\',"/");let relative=name.strip_prefix(&(plan.sketch_name.clone()+"/")).ok_or(ERROR)?;
        checked_relative(relative)?;let expected=snapshot.files.get(relative).ok_or(ERROR)?;
        if file.is_dir() || file.size()!=expected.len() as u64 || !seen.insert(relative.to_owned()){return Err(ERROR.into());}
        let mut bytes=Vec::new();Read::by_ref(&mut file).take(FILE_LIMIT+1).read_to_end(&mut bytes).map_err(|_|ERROR)?;if &bytes!=expected{return Err(ERROR.into());}
    }
    if seen!=snapshot.files.keys().cloned().collect(){return Err(ERROR.into());}
    let proof=json!({"version":1,"capture_sha256":link.sha256,"verifier_sha256":plan.verifier_sha256,"runtime_version":crate::arduino::CLI_VERSION,
        "scope":"native_sketch_reader_roundtrip","file_count":seen.len(),"source_preserved":true,"native_reader_reopened":true,"native_file_bytes_checked":true,
        "encoding_policy":"native_buffers_utf8_closed_files_original_bytes","dependency_scope":"project_tree","external_dependencies_checked":false,
        "gui_state_checked":false,"compilation_checked":false,"physical_function_checked":false,"agreed_task_outcome_checked":false});
    let bytes=serde_json::to_vec(&proof).map_err(|_|ERROR)?;let path=destination.join("verification.json");
    if path.try_exists().map_err(|_|ERROR)?{if read_regular(&path,65536).map_err(|_|ERROR)?!=bytes{return Err(ERROR.into());}}
    else{write_new(&path,&bytes).map_err(|_|ERROR)?;}Ok(digest(&bytes))
}
pub(crate) fn validate_check(plan:&Plan,stage:&Path,link:&Link,scope:&Scope<'_>,hash:&str)->Result<Value,String>{
    let (capture,_snapshot)=load(plan,stage,link,scope)?;
    let folder=stage.join(&link.folder).canonicalize().map_err(|_|ERROR)?;let bytes=read_regular(&folder.join("verification.json"),65536).map_err(|_|ERROR)?;
    if !crate::kicad_project::valid_hash(hash) || digest(&bytes)!=hash{return Err(ERROR.into());}let proof:Value=serde_json::from_slice(&bytes).map_err(|_|ERROR)?;
    if proof["version"]!=1 || proof["capture_sha256"]!=link.sha256 || proof["verifier_sha256"]!=plan.verifier_sha256
        || proof["runtime_version"]!=crate::arduino::CLI_VERSION || proof["scope"]!="native_sketch_reader_roundtrip" || proof["file_count"]!=capture.files.len()
        || ["source_preserved","native_reader_reopened","native_file_bytes_checked"].iter().any(|key|proof[key]!=true)
        || ["external_dependencies_checked","gui_state_checked","compilation_checked","physical_function_checked","agreed_task_outcome_checked"].iter().any(|key|proof[key]!=false)
        || proof["encoding_policy"]!="native_buffers_utf8_closed_files_original_bytes" || proof["dependency_scope"]!="project_tree"{return Err(ERROR.into());}Ok(proof)
}

#[cfg(test)]
mod tests{
    use super::*;
    struct Fixture{root:PathBuf,source:PathBuf,stage:PathBuf,plan:Plan,buffers:Vec<(String,String)>}
    impl Fixture{
        fn new()->Self{
            let root=std::env::temp_dir().join(format!("hazlo-copy-{}",uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();let source=root.join("Sketch");let stage=root.join("stage");
            fs::create_dir(&source).unwrap();fs::create_dir(&stage).unwrap();
            fs::write(source.join("Sketch.ino"),"// disk original\r\n").unwrap();
            fs::write(source.join("Empty.h"),[]).unwrap();fs::create_dir(source.join("src")).unwrap();
            fs::create_dir(source.join("empty")).unwrap();fs::write(source.join("src/raw.bin"),[0,255,0]).unwrap();
            let verifier=root.join("arduino-cli.exe");fs::write(&verifier,b"owned verifier placeholder").unwrap();
            let plan=Plan{version:1,verifier,verifier_sha256:digest(b"owned verifier placeholder"),
                source_root_id:directory_identity(&pin_directory(&source,false).unwrap()).unwrap(),sketch_name:"Sketch".into(),excluded:vec![]};
            Self{root,source,stage,plan,buffers:vec![("Sketch.ino".into(),"// unsaved café\r\n".into()),("SRC/New.h".into(),"// unsaved new\n".into())]}
        }
        fn scope(&self)->Scope<'_>{Scope{input_sha256:"1111111111111111111111111111111111111111111111111111111111111111",buffer_sha256:"2222222222222222222222222222222222222222222222222222222222222222",buffers:&self.buffers}}
        fn capture(&self)->(Link,Capture){capture(&self.plan,&self.source,&self.stage,&self.scope(),&AtomicBool::new(false),||Ok(())).unwrap()}
    }
    impl Drop for Fixture{fn drop(&mut self){fs::remove_dir_all(&self.root).unwrap();}}

    #[test]
    fn paused_reader_keeps_the_copy_and_does_not_consume_a_check_attempt(){
        let f=Fixture::new();let (link,_)=f.capture();let before=fs::read(entry(&f.plan,&f.stage,&link)).unwrap();
        let mutex=std::sync::Mutex::new(());let paused=AtomicBool::new(true);
        let gate=crate::process::DispatchGate{mutex:&mutex,paused:&paused};let private=f.root.join("never-started-reader");
        assert!(verify(&f.plan,&f.stage,&link,&f.scope(),&private,(Arc::new(AtomicBool::new(false)),Some(gate))).is_err());
        assert!(!private.exists());assert!(!f.stage.join(&link.folder).join("verification.json").exists());
        assert_eq!(fs::read_dir(f.stage.join(&link.folder)).unwrap().count(),2);
        assert_eq!(fs::read(entry(&f.plan,&f.stage,&link)).unwrap(),before);validate_capture(&f.plan,&f.stage,&link,&f.scope()).unwrap();
    }

    #[test]
    fn copy_preserves_closed_bytes_and_originals_with_native_text_overlay(){
        let f=Fixture::new();let (link,captured)=f.capture();
        matches_capture(&link,&captured).unwrap();validate_capture(&f.plan,&f.stage,&link,&f.scope()).unwrap();
        let copy=entry(&f.plan,&f.stage,&link);let folder=copy.parent().unwrap();
        assert_eq!(fs::read(&copy).unwrap(),f.buffers[0].1.as_bytes());
        assert_eq!(fs::read(folder.join("src/New.h")).unwrap(),f.buffers[1].1.as_bytes());
        assert_eq!(fs::read(folder.join("src/raw.bin")).unwrap(),[0,255,0]);
        assert!(fs::read(folder.join("Empty.h")).unwrap().is_empty());assert!(folder.join("empty").is_dir());
        assert_eq!(fs::read(f.source.join("Sketch.ino")).unwrap(),b"// disk original\r\n");
        fs::remove_dir_all(&f.source).unwrap();validate_capture(&f.plan,&f.stage,&link,&f.scope()).unwrap();
        fs::write(folder.join("src/raw.bin"),[1,255,0]).unwrap();assert!(validate_capture(&f.plan,&f.stage,&link,&f.scope()).is_err());
    }
    #[test]
    fn copies_keep_partial_work_and_stop_after_two_attempts(){
        let f=Fixture::new();let (first,_)=f.capture();let (second,_)=f.capture();assert_ne!(first,second);
        assert!(capture(&f.plan,&f.source,&f.stage,&f.scope(),&AtomicBool::new(false),||Ok(())).is_err());
        validate_capture(&f.plan,&f.stage,&first,&f.scope()).unwrap();validate_capture(&f.plan,&f.stage,&second,&f.scope()).unwrap();
    }
    #[test]
    fn changed_verifier_cancellation_and_native_state_refuse_without_output(){
        let f=Fixture::new();
        assert!(capture(&f.plan,&f.source,&f.stage,&f.scope(),&AtomicBool::new(true),||Ok(())).is_err());
        assert!(capture(&f.plan,&f.source,&f.stage,&f.scope(),&AtomicBool::new(false),||Err("native buffers changed".into())).is_err());
        fs::write(&f.plan.verifier,b"different runtime").unwrap();
        assert!(capture(&f.plan,&f.source,&f.stage,&f.scope(),&AtomicBool::new(false),||Ok(())).is_err());
        assert_eq!(fs::read_dir(&f.stage).unwrap().count(),0);
    }
    #[test]
    fn task_storage_is_excluded_and_capture_scope_cannot_be_reassigned(){
        let mut f=Fixture::new();fs::create_dir(f.source.join(".hazlomexa")).unwrap();fs::write(f.source.join(".hazlomexa/private"),b"local evidence").unwrap();
        fs::write(f.source.join("hazlomx.json"),b"task configuration").unwrap();f.plan.excluded=vec![".hazlomexa".into(),"hazlomx.json".into()];
        let (link,_)=f.capture();let copy=entry(&f.plan,&f.stage,&link);assert!(!copy.parent().unwrap().join(".hazlomexa").exists());
        let mut scope=f.scope();scope.input_sha256=scope.buffer_sha256;assert!(validate_capture(&f.plan,&f.stage,&link,&scope).is_err());
    }
    #[test]
    fn long_windows_copy_paths_are_read_with_extended_spelling(){
        let mut f=Fixture::new();f.stage=f.stage.join("long-project-directory".repeat(6));fs::create_dir(&f.stage).unwrap();
        let (link,_)=f.capture();assert!(entry(&f.plan,&f.stage,&link).as_os_str().len()>260);
        validate_capture(&f.plan,&f.stage,&link,&f.scope()).unwrap();
    }
}
