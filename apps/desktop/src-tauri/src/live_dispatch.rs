//! One admitted native operation; content stays in its existing local result artifact.
use crate::{
    native_journal::{Outcome, Summary},
    project::{atomic_json, read_json, OperationInput},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
use uuid::Uuid;

const REFUSED: &str = "Los pasos pendientes están pausados. No se envió esta operación nativa.";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    version: u32,
    job_id: Uuid,
    input_sha256: String,
    pub outcome: Outcome,
    pub receipt_sha256: Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub refusal_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub native_completion_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub working_version_sha256:Option<String>,
    #[serde(default,skip_serializing_if="std::ops::Not::not")]
    pub drawing_completion_required:bool,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub drawing_completion_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub drawing_copy_capture_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub drawing_copy_check_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agent_completion_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agent_buffers_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agent_project_capture:Option<crate::arduino_version::Link>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agent_project_check_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub pcb_completion_sha256:Option<String>,
}
fn digest(input: &OperationInput) -> Result<String, String> {
    if !matches!(
        input,
        OperationInput::LiveCad(_)
            | OperationInput::LiveAgent(_)
            | OperationInput::LivePcb(_)
            | OperationInput::LiveDrawing(_)
    ) {
        return Err("Este registro requiere un adaptador nativo en vivo.".into());
    }
    let mut value = serde_json::to_value(input).map_err(|_| "Entrada nativa no válida.")?;
    value
        .as_object_mut()
        .ok_or("Entrada nativa no válida.")?
        .remove("receipt_sha256");
    Ok(crate::kicad_project::digest(
        &serde_json::to_vec(&value).map_err(|_| "Entrada nativa no válida.")?,
    ))
}
impl Record {
    pub fn summary(&self, input: &OperationInput) -> Summary {
        let finished = self.outcome == Outcome::Acknowledged;
        let refused=self.refusal_sha256.is_some();
        let observed=self.native_completion_sha256.is_some() || self.drawing_completion_sha256.is_some() || self.agent_completion_sha256.is_some() || self.pcb_completion_sha256.is_some();
        Summary {
            job_id: self.job_id,
            program: input.native_program(),
            kind: "live_adapter",
            acknowledged: usize::from(finished || observed),
            uncertain: usize::from(!refused && !observed && matches!(
                self.outcome,
                Outcome::Dispatched | Outcome::Uncertain
            )),
            not_dispatched: usize::from(self.outcome == Outcome::Pending || refused),
            finished,
            needs_review: !refused && matches!(self.outcome, Outcome::Dispatched | Outcome::Uncertain),
            outcome_checked: refused,
        }
    }
}
pub(crate) fn read(directory: &Path, input: &OperationInput) -> Result<Option<Record>, String> {
    let path = directory.join("live-dispatch.json");
    if !path
        .try_exists()
        .map_err(|_| "No se pudo comprobar el registro nativo.")?
    {
        return Ok(None);
    }
    let record: Record = read_json(&path).map_err(|_| "No se pudo leer el registro nativo.")?;
    if record.version != 1
        || record.job_id != input.job_id()
        || record.input_sha256 != digest(input)?
        || (record.outcome == Outcome::Acknowledged) != record.receipt_sha256.is_some()
        || record
            .receipt_sha256
            .as_ref()
            .is_some_and(|h| !crate::kicad_project::valid_hash(h))
    {
        return Err("El registro no coincide con la operación aprobada. No se repetirá.".into());
    }
    let mut retained_result=None;
    if matches!(input,OperationInput::LivePcb(pcb) if pcb.native_completion_version==Some(1)) && record.outcome==Outcome::Acknowledged && record.pcb_completion_sha256.is_none(){return Err("Falta el vínculo original de finalización PCB; se conserva la pausa.".into());}
    if let Some(hash)=&record.pcb_completion_sha256{
        let OperationInput::LivePcb(pcb)=input else{return Err("El recibo PCB no pertenece a esta operación.".into());};
        if record.outcome==Outcome::Pending || record.refusal_sha256.is_some(){return Err("El recibo PCB no pertenece a una llamada admitida.".into());}
        let report=crate::kicad_live::confirmed_completion(directory,pcb,hash)?;let expected=crate::kicad_project::digest(&serde_json::to_vec_pretty(&report).map_err(|_|"Informe PCB no válido.")?);
        if record.receipt_sha256.as_ref().is_some_and(|hash|hash!=&expected) || input.receipt_sha256().is_some_and(|hash|hash!=expected){return Err("El informe PCB no coincide con su resultado original.".into());}retained_result=Some(expected);
    }else if let OperationInput::LivePcb(pcb)=input{if record.outcome==Outcome::Acknowledged && !directory.join("result.json").try_exists().map_err(|_|"No se pudo comprobar el informe PCB.")?{
        let hash=record.receipt_sha256.as_deref().ok_or("Falta el informe PCB original.")?;crate::kicad_live::legacy_completed_report(directory,pcb,hash)?;retained_result=Some(hash.to_owned());
    }}
    if matches!(input,OperationInput::LiveAgent(agent) if agent.native_completion_version==Some(1)) && record.outcome==Outcome::Acknowledged && record.agent_completion_sha256.is_none(){return Err("Falta el recibo original del editor; se conserva la pausa.".into());}
    if let Some(hash)=&record.agent_completion_sha256{
        let OperationInput::LiveAgent(agent)=input else{return Err("El recibo del editor no pertenece a esta operación.".into());};
        if record.outcome==Outcome::Pending || record.refusal_sha256.is_some(){return Err("El recibo del editor no pertenece a una llamada admitida.".into());}
        let report=crate::agent_editor::confirmed_completion(directory,agent,hash)?;let expected=crate::kicad_project::digest(&serde_json::to_vec_pretty(&report).map_err(|_|"Informe del editor no válido.")?);
        if record.receipt_sha256.as_ref().is_some_and(|hash|hash!=&expected) || input.receipt_sha256().is_some_and(|hash|hash!=expected){return Err("El informe del editor no coincide con su resultado original.".into());}retained_result=Some(expected);
    }
    if let Some(hash)=&record.agent_buffers_sha256{
        let OperationInput::LiveAgent(agent)=input else{return Err("El checkpoint no pertenece al editor aprobado.".into());};
        let core=record.agent_completion_sha256.as_deref().ok_or("Falta el recibo original del checkpoint.")?;
        crate::agent_editor::confirmed_buffer_checkpoint(directory,agent,core,hash)?;
    }
    if matches!(input,OperationInput::LiveAgent(agent) if agent.buffer_checkpoint.is_some()) && record.outcome==Outcome::Acknowledged && record.agent_buffers_sha256.is_none(){return Err("La edición está confirmada, pero falta su checkpoint de buffers.".into());}
    if record.agent_project_capture.is_some() || record.agent_project_check_sha256.is_some(){
        let OperationInput::LiveAgent(agent)=input else{return Err("La copia no pertenece a Arduino Agent.".into());};
        let core=record.agent_completion_sha256.as_deref().ok_or("Falta el recibo original de la copia.")?;
        let buffers=record.agent_buffers_sha256.as_deref().ok_or("Falta el checkpoint original de la copia.")?;
        let link=record.agent_project_capture.as_ref().ok_or("Falta la captura de archivos de la copia.")?;
        crate::agent_editor::checked_working_version(directory,agent,core,buffers,link,record.agent_project_check_sha256.as_deref())?;
    }
    if matches!(input,OperationInput::LiveAgent(agent) if crate::agent_editor::working_plan(agent).is_some()) && record.outcome==Outcome::Acknowledged && record.agent_project_check_sha256.is_none(){return Err("La edición se conserva; falta comprobar su nueva copia de proyecto.".into());}
    if matches!(input,OperationInput::LiveDrawing(drawing) if drawing.native_completion_version==Some(1)) && !record.drawing_completion_required{return Err("La operación exige finalización nativa; no se puede degradar su registro.".into());}
    if record.drawing_completion_required && (!matches!(input,OperationInput::LiveDrawing(_)) || record.outcome==Outcome::Acknowledged && record.drawing_completion_sha256.is_none()){return Err("Falta el vínculo original de finalización CADCraft; se conserva la pausa.".into());}
    if let Some(hash)=&record.drawing_completion_sha256{
        let OperationInput::LiveDrawing(drawing)=input else{return Err("Recibo CADCraft incompatible.".into());};
        if matches!(record.outcome,Outcome::Pending) || record.refusal_sha256.is_some(){return Err("El recibo CADCraft no pertenece a una llamada admitida.".into());}
        let report=crate::cadcraft_live::confirmed_completion(directory,drawing,hash)?;
        let expected=crate::kicad_project::digest(&serde_json::to_vec_pretty(&report).map_err(|_|"Recibo CADCraft no válido.")?);
        if record.receipt_sha256.as_ref().is_some_and(|hash|hash!=&expected) || input.receipt_sha256().is_some_and(|hash|hash!=expected){return Err("El informe CADCraft no coincide con el resultado nativo original.".into());}retained_result=Some(expected);
    }
    if record.drawing_copy_capture_sha256.is_some() || record.drawing_copy_check_sha256.is_some(){
        let OperationInput::LiveDrawing(drawing)=input else{return Err("La copia no pertenece a CADCraft.".into());};
        if drawing.native_working_version!=Some(1) || record.drawing_completion_sha256.is_none(){return Err("Falta el resultado original de la copia CADCraft.".into());}
        let capture=record.drawing_copy_capture_sha256.as_deref().ok_or("Falta la captura original CADCraft.")?;
        crate::cadcraft_live::confirmed_working_capture(directory,drawing,capture)?;
        if let Some(checked)=&record.drawing_copy_check_sha256{crate::cadcraft_live::confirmed_working_version(directory,drawing,capture,checked)?;}
    }
    if matches!(input,OperationInput::LiveDrawing(drawing) if drawing.native_working_version.is_some()) && record.outcome==Outcome::Acknowledged && record.drawing_copy_check_sha256.is_none(){return Err("La copia CADCraft sigue pendiente de comprobar; no se completó el lote.".into());}
    if let Some(hash)=&record.native_completion_sha256{
        let OperationInput::LiveCad(cad)=input else{return Err("Recibo nativo incompatible; se conserva la pausa.".into());};
        if !crate::kicad_project::valid_hash(hash) || matches!(record.outcome,Outcome::Pending) || record.refusal_sha256.is_some(){return Err("El recibo nativo cambió; se conserva la pausa sin repetir la edición.".into());}
        let native=crate::cadpilot_live::read_native_completion(directory,cad,hash)?.ok_or("Falta el recibo nativo original.")?;
        let expected=crate::kicad_project::digest(&serde_json::to_vec_pretty(&native).map_err(|_|"Recibo nativo no válido.")?);if record.receipt_sha256.as_ref().is_some_and(|hash|hash!=&expected){return Err("El informe final no coincide con la respuesta nativa original.".into());}
        retained_result=Some(expected);
    }
    if let Some(hash) = &record.receipt_sha256 {
        let path=directory.join("result.json");
        let matches=if path.try_exists().map_err(|_|"No se pudo comprobar el informe nativo.")?{crate::kicad_project::digest(&crate::kicad_project::read_regular(&path,65536)?)==*hash}else{retained_result.as_ref()==Some(hash)};
        if !matches || input.receipt_sha256().is_some_and(|h|h!=hash){return Err("El resultado nativo cambió; se conserva la pausa.".into());}
    }
    if let Some(hash)=&record.working_version_sha256{
        let OperationInput::LiveCad(cad)=input else{return Err("La comprobación de copia no pertenece a este programa.".into());};if cad.working_version.is_none() || record.native_completion_sha256.is_none() || !crate::kicad_project::valid_hash(hash){return Err("La comprobación no pertenece a la captura aprobada.".into());}
        let native=crate::cadpilot_live::read_native_completion(directory,cad,record.native_completion_sha256.as_ref().unwrap())?.ok_or("Falta el recibo original de captura.")?;crate::native_working_version::validate_artifacts(directory,&native,hash)?;
    }
    if let Some(hash)=&record.refusal_sha256{
        let OperationInput::LiveDrawing(drawing)=input else{return Err("Prueba de rechazo incompatible; se conserva la pausa.".into());};
        if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || !crate::kicad_project::valid_hash(hash) || crate::kicad_project::digest(&crate::kicad_project::read_regular(&directory.join("drawing-completion.json"),256*1024)?)!=*hash || !crate::cadcraft_live::refusal_proof(drawing,directory)?{return Err("La prueba de rechazo cambió; se conserva la pausa.".into());}
    }
    Ok(Some(record))
}
pub(crate) fn retain_native_completion(directory:&Path,input:&crate::cadpilot_live::Input,result:&serde_json::Value)->Result<String,String>{
    let operation=OperationInput::LiveCad(input.clone());let mut record=read(directory,&operation)?.ok_or("Falta el registro original; no se adoptó un resultado ajeno.")?;
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || record.refusal_sha256.is_some(){return Err("La llamada no está admitida para conservar otro resultado.".into());}
    let hash=crate::cadpilot_live::write_native_completion(directory,input,result)?;
    if record.native_completion_sha256.as_ref().is_some_and(|saved|saved!=&hash){return Err("El recibo original ya tiene otro contenido; se conserva sin reemplazarlo.".into());}
    if record.native_completion_sha256.is_none(){record.native_completion_sha256=Some(hash.clone());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"La respuesta sigue local, pero no se confirmó su vínculo al envío. No repitas la edición.".to_string())?;}
    Ok(hash)
}
pub(crate) fn retain_agent_completion(directory:&Path,input:&crate::agent_editor::Input,report:&serde_json::Value)->Result<String,String>{
    let mut record=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original del editor.")?;
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || record.refusal_sha256.is_some(){return Err("El editor no tiene una llamada admitida para otro resultado.".into());}
    let hash=crate::agent_editor::write_native_completion(directory,input,report)?;
    if record.agent_completion_sha256.as_ref().is_some_and(|old|old!=&hash){return Err("El recibo del editor ya tiene otro contenido; no se reemplazó.".into());}
    if record.agent_completion_sha256.is_none(){record.agent_completion_sha256=Some(hash.clone());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"El recibo sigue local, pero falta vincularlo al envío. No repitas la edición.".to_string())?;}Ok(hash)
}
pub(crate) fn retain_agent_buffers(directory:&Path,input:&crate::agent_editor::Input,buffers:&serde_json::Value)->Result<String,String>{
    let mut record=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original del editor.")?;
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || record.refusal_sha256.is_some(){return Err("El checkpoint no corresponde a una llamada admitida.".into());}
    let core=record.agent_completion_sha256.as_deref().ok_or("Falta el recibo nativo original del checkpoint.")?;
    let hash=crate::agent_editor::write_buffer_checkpoint(directory,input,core,buffers)?;
    if record.agent_buffers_sha256.as_ref().is_some_and(|old|old!=&hash){return Err("El envío ya tiene otro checkpoint; se conserva sin reemplazarlo.".into());}
    if record.agent_buffers_sha256.is_none(){record.agent_buffers_sha256=Some(hash.clone());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"El checkpoint sigue local, pero falta vincularlo al envío. No repitas la edición.".to_string())?;}Ok(hash)
}
pub(crate) fn retain_agent_project_capture(directory:&Path,input:&crate::agent_editor::Input,link:&crate::arduino_version::Link,capture:&crate::arduino_version::Capture)->Result<(),String>{
    let mut record=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original del proyecto.")?;
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || record.refusal_sha256.is_some(){return Err("La copia no corresponde a una edición admitida.".into());}
    crate::arduino_version::matches_capture(link,capture)?;
    crate::agent_editor::checked_working_version(directory,input,record.agent_completion_sha256.as_deref().ok_or("Falta el recibo original.")?,record.agent_buffers_sha256.as_deref().ok_or("Falta el checkpoint original.")?,link,None)?;
    if record.agent_project_capture.as_ref().is_some_and(|old|old!=link){return Err("El lote ya tiene otra copia vinculada; no se reemplazó.".into());}
    if record.agent_project_capture.is_none(){record.agent_project_capture=Some(link.clone());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"La copia se conservó; falta vincularla al lote. No repitas la edición.")?;}Ok(())
}
pub(crate) fn retain_agent_project_check(directory:&Path,input:&crate::agent_editor::Input,hash:&str)->Result<(),String>{
    let mut record=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original del proyecto.")?;
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain){return Err("La comprobación no corresponde a una edición admitida.".into());}
    crate::agent_editor::checked_working_version(directory,input,record.agent_completion_sha256.as_deref().ok_or("Falta el recibo original.")?,record.agent_buffers_sha256.as_deref().ok_or("Falta el checkpoint original.")?,record.agent_project_capture.as_ref().ok_or("Falta la captura de proyecto.")?,Some(hash))?;
    if record.agent_project_check_sha256.as_ref().is_some_and(|old|old!=hash){return Err("La copia ya tiene otra comprobación vinculada; se conservó sin reemplazarla.".into());}
    if record.agent_project_check_sha256.is_none(){record.agent_project_check_sha256=Some(hash.into());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"La comprobación se conservó; falta vincularla al lote.")?;}Ok(())
}
pub(crate) fn retain_working_version(directory:&Path,input:&crate::cadpilot_live::Input,hash:&str)->Result<String,String>{
    let operation=OperationInput::LiveCad(input.clone());let mut record=read(directory,&operation)?.ok_or("Falta el envío original de captura.")?;
    if input.working_version.is_none() || !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || !crate::kicad_project::valid_hash(hash){return Err("La comprobación no corresponde a una captura admitida.".into());}
    let native=crate::cadpilot_live::read_native_completion(directory,input,record.native_completion_sha256.as_deref().ok_or("Falta el recibo original de captura.")?)?.ok_or("Falta la captura original.")?;crate::native_working_version::validate_artifacts(directory,&native,hash)?;
    if record.working_version_sha256.as_ref().is_some_and(|saved|saved!=hash){return Err("La copia ya tiene otra comprobación; no se reemplazó.".into());}
    if record.working_version_sha256.is_none(){record.working_version_sha256=Some(hash.into());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"La comprobación sigue local, pero falta vincularla al envío; no repitas la edición.".to_string())?;}Ok(hash.into())
}
pub(crate) fn retain_drawing_copy_check(directory:&Path,input:&crate::cadcraft_live::Input,hash:&str)->Result<(),String>{
    let mut record=read(directory,&OperationInput::LiveDrawing(input.clone()))?.ok_or("Falta el envío CADCraft original.")?;
    if record.drawing_copy_check_sha256.as_deref()==Some(hash){return Ok(());}
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain){return Err("La copia no pertenece a un lote admitido.".into());}
    let capture=record.drawing_copy_capture_sha256.as_deref().ok_or("Falta la captura CADCraft vinculada.")?;
    crate::cadcraft_live::validate_working_version(directory,input,capture,hash)?;
    if record.drawing_copy_check_sha256.as_ref().is_some_and(|old|old!=hash){return Err("La copia ya tiene otra comprobación; se conserva sin reemplazarla.".into());}
    if record.drawing_copy_check_sha256.is_none(){record.drawing_copy_check_sha256=Some(hash.into());atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"La comprobación sigue local; falta vincularla al lote. No repitas la edición.".to_string())?;}Ok(())
}
pub(crate) fn recover_refused(directory:&Path,input:&OperationInput)->Result<(),String>{
    let mut record=read(directory,input)?.ok_or("Falta el registro de la llamada; se conserva la pausa.")?;
    if record.refusal_sha256.is_some(){return Ok(());}
    let OperationInput::LiveDrawing(drawing)=input else{return Err("Este adaptador no tiene una prueba de rechazo compatible.".into());};
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain) || !crate::cadcraft_live::refusal_proof(drawing,directory)?{return Err("Falta la prueba original de que no inició la edición; se conserva la pausa.".into());}
    record.refusal_sha256=Some(crate::kicad_project::digest(&crate::kicad_project::read_regular(&directory.join("drawing-completion.json"),256*1024)?));
    atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"El rechazo sigue local, pero no se confirmó su registro. No repitas la operación.".into())
}
pub(crate) fn recover_checked(directory:&Path,checked:&OperationInput)->Result<(),String>{
    let mut record=read(directory,checked)?.ok_or("Falta el registro de la llamada; no se adoptó el resultado.")?;
    if matches!(checked,OperationInput::LivePcb(_)) && record.outcome!=Outcome::Acknowledged && record.pcb_completion_sha256.is_none(){return Err("Falta el vínculo original PCB; no se adoptó un recibo sin confirmar.".into());}
    if matches!(checked,OperationInput::LiveAgent(input) if input.native_completion_version==Some(1)) && record.agent_completion_sha256.is_none(){return Err("Falta el vínculo original del editor; no se adoptó otro resultado.".into());}
    if matches!(checked,OperationInput::LiveDrawing(input) if input.native_working_version.is_some()) && record.drawing_copy_check_sha256.is_none(){return Err("La edición se conserva, pero su nueva versión sigue pendiente de comprobar.".into());}
    if record.drawing_completion_required && record.drawing_completion_sha256.is_none(){return Err("Falta el vínculo original CADCraft; no se adoptó otro resultado.".into());}
    if record.refusal_sha256.is_some(){return Err("La edición no inició; no se adoptó otro resultado como completado.".into());}
    let hash=checked.receipt_sha256().ok_or("Falta el recibo del resultado comprobado.")?;
    if record.outcome==Outcome::Acknowledged{return Ok(());}
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain){return Err("La llamada no fue admitida; no se adoptó un resultado ajeno.".into());}
    record.receipt_sha256=Some(hash.into());record.outcome=Outcome::Acknowledged;
    atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"El resultado sigue local, pero no se confirmó la recuperación. No repitas la operación.".into())
}
pub(crate) struct Dispatch {
    path: PathBuf,
    record: Mutex<Record>,
    gate: Arc<Mutex<()>>,
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}
impl Dispatch {
    pub fn start(
        directory: &Path,
        input: &OperationInput,
        gate: Arc<Mutex<()>>,
        paused: Arc<AtomicBool>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Arc<Self>, String> {
        let record = Record {
            version: 1,
            job_id: input.job_id(),
            input_sha256: digest(input)?,
            outcome: Outcome::Pending,
            receipt_sha256: None,
            refusal_sha256:None,
            native_completion_sha256:None,
            working_version_sha256:None,
            drawing_completion_required:matches!(input,OperationInput::LiveDrawing(_)),
            drawing_completion_sha256:None,
            drawing_copy_capture_sha256:None,
            drawing_copy_check_sha256:None,
            agent_completion_sha256:None,
            agent_buffers_sha256:None,
            agent_project_capture:None,
            agent_project_check_sha256:None,
            pcb_completion_sha256:None,
        };
        let path = directory.join("live-dispatch.json");
        atomic_json(&path,&record,false).map_err(|_|"La operación ya tiene un registro o no se pudo reservar. No se enviaron nuevas llamadas.")?;
        Ok(Arc::new(Self {
            path,
            record: Mutex::new(record),
            gate,
            paused,
            cancelled,
        }))
    }
    pub fn enter(&self) -> Result<MutexGuard<'_, ()>, String> {
        let permit = self.gate.lock().map_err(|_| REFUSED)?;
        if self.paused.load(Ordering::SeqCst) || self.cancelled.load(Ordering::SeqCst) {
            return Err(REFUSED.into());
        }
        let mut record = self.record.lock().map_err(|_| REFUSED)?;
        if record.outcome != Outcome::Pending {
            return Err("Esta operación ya fue admitida; no se repetirá.".into());
        }
        record.outcome = Outcome::Dispatched;
        atomic_json(&self.path, &*record, true).map_err(|_| {
            "No se confirmó el registro; no se envió una nueva llamada.".to_string()
        })?;
        Ok(permit)
    }
    pub fn was_dispatched(&self) -> bool {
        self.record
            .lock()
            .map_or(true, |r| r.outcome != Outcome::Pending)
    }
    pub(crate) fn retain_native_completion(&self,input:&crate::cadpilot_live::Input,result:&serde_json::Value)->Result<(),String>{
        let mut record=self.record.lock().map_err(|_|"No se pudo conservar el resultado nativo.")?;
        record.native_completion_sha256=Some(retain_native_completion(self.path.parent().ok_or("Falta el registro nativo.")?,input,result)?);Ok(())
    }
    pub(crate) fn retain_working_version(&self,input:&crate::cadpilot_live::Input,hash:&str)->Result<(),String>{
        let mut record=self.record.lock().map_err(|_|"No se pudo vincular la comprobación de copia.")?;record.working_version_sha256=Some(retain_working_version(self.path.parent().ok_or("Falta el registro nativo.")?,input,hash)?);Ok(())
    }
    pub(crate) fn retain_pcb_completion(&self,input:&crate::kicad_live::Input,result:&crate::kicad_live::Receipt)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar el resultado PCB.")?;let directory=self.path.parent().ok_or("Falta el registro PCB.")?;
        let mut saved=read(directory,&OperationInput::LivePcb(input.clone()))?.ok_or("Falta el envío original PCB.")?;
        if !matches!(saved.outcome,Outcome::Dispatched|Outcome::Uncertain) || saved.refusal_sha256.is_some(){return Err("El recibo PCB no pertenece a una llamada admitida.".into());}
        let hash=crate::kicad_project::digest(&crate::kicad_project::read_regular(&directory.join("pcb-completion.json"),65536)?);
        let original=crate::kicad_live::confirmed_completion(directory,input,&hash)?;
        if serde_json::to_value(&original).map_err(|_|"Recibo PCB no válido.")?!=serde_json::to_value(result).map_err(|_|"Respuesta PCB no válida.")? || saved.pcb_completion_sha256.as_ref().is_some_and(|old|old!=&hash){return Err("El recibo PCB no coincide con la respuesta original; se conserva sin reemplazarlo.".into());}
        if saved.pcb_completion_sha256.is_none(){saved.pcb_completion_sha256=Some(hash);atomic_json(&self.path,&saved,true).map_err(|_|"El recibo PCB sigue local, pero falta vincularlo al envío. No repitas la edición.".to_string())?;}*current=saved;Ok(())
    }
    pub(crate) fn retain_agent_completion(&self,input:&crate::agent_editor::Input,result:&serde_json::Value)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar el resultado del editor.")?;
        current.agent_completion_sha256=Some(retain_agent_completion(self.path.parent().ok_or("Falta el registro del editor.")?,input,result)?);Ok(())
    }
    pub(crate) fn retain_agent_buffers(&self,input:&crate::agent_editor::Input,result:&serde_json::Value)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar el checkpoint del editor.")?;
        current.agent_buffers_sha256=Some(retain_agent_buffers(self.path.parent().ok_or("Falta el envío del editor.")?,input,result)?);Ok(())
    }
    pub(crate) fn retain_agent_project_capture(&self,input:&crate::agent_editor::Input,link:&crate::arduino_version::Link,capture:&crate::arduino_version::Capture)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar la copia del proyecto.")?;let directory=self.path.parent().ok_or("Falta el envío original.")?;
        retain_agent_project_capture(directory,input,link,capture)?;*current=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original.")?;Ok(())
    }
    pub(crate) fn retain_agent_project_check(&self,input:&crate::agent_editor::Input,hash:&str)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar la comprobación del proyecto.")?;let directory=self.path.parent().ok_or("Falta el envío original.")?;
        retain_agent_project_check(directory,input,hash)?;*current=read(directory,&OperationInput::LiveAgent(input.clone()))?.ok_or("Falta el envío original.")?;Ok(())
    }
    pub(crate) fn retain_drawing_completion(&self,input:&crate::cadcraft_live::Input,result:&serde_json::Value)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar el resultado CADCraft.")?;let directory=self.path.parent().ok_or("Falta el registro nativo.")?;let operation=OperationInput::LiveDrawing(input.clone());let mut saved=read(directory,&operation)?.ok_or("Falta el envío original CADCraft.")?;
        if !matches!(saved.outcome,Outcome::Dispatched|Outcome::Uncertain) || saved.refusal_sha256.is_some(){return Err("La llamada CADCraft no fue admitida para otro resultado.".into());}
        let bytes=crate::kicad_project::read_regular(&directory.join("drawing-completion.json"),256*1024)?;let hash=crate::kicad_project::digest(&bytes);let native:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|"Recibo CADCraft no válido.")?;
        if &native!=result || saved.drawing_completion_sha256.as_ref().is_some_and(|old|old!=&hash){return Err("El resultado CADCraft cambió; se conserva sin reemplazarlo.".into());}
        crate::cadcraft_live::confirmed_completion(directory,input,&hash)?;
        if saved.drawing_completion_sha256.is_none(){saved.drawing_completion_sha256=Some(hash.clone());atomic_json(&self.path,&saved,true).map_err(|_|"El resultado CADCraft sigue local, pero no se confirmó su vínculo. No repitas la edición.")?;}
        current.drawing_completion_sha256=Some(hash);Ok(())
    }
    pub(crate) fn retain_drawing_copy_capture(&self,input:&crate::cadcraft_live::Input,result:&serde_json::Value)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar la captura CADCraft.")?;
        let directory=self.path.parent().ok_or("Falta el registro CADCraft.")?;
        let mut saved=read(directory,&OperationInput::LiveDrawing(input.clone()))?.ok_or("Falta el envío original CADCraft.")?;
        if !matches!(saved.outcome,Outcome::Dispatched|Outcome::Uncertain) || saved.drawing_completion_sha256.is_none(){return Err("La captura no pertenece a una edición confirmada.".into());}
        let bytes=crate::kicad_project::read_regular(&directory.join("DrawingVersion/drawing-completion.json"),256*1024)?;
        let native:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|"Captura CADCraft no válida.")?;let hash=crate::kicad_project::digest(&bytes);
        if &native!=result || saved.drawing_copy_capture_sha256.as_ref().is_some_and(|old|old!=&hash){return Err("La captura CADCraft cambió; no se reemplazó.".into());}
        crate::cadcraft_live::confirmed_working_capture(directory,input,&hash)?;
        if saved.drawing_copy_capture_sha256.is_none(){saved.drawing_copy_capture_sha256=Some(hash);atomic_json(&self.path,&saved,true).map_err(|_|"La captura sigue local, pero falta vincularla al lote. No repitas la edición.".to_string())?;}
        *current=saved;Ok(())
    }
    pub(crate) fn retain_drawing_copy_check(&self,input:&crate::cadcraft_live::Input,hash:&str)->Result<(),String>{
        let mut current=self.record.lock().map_err(|_|"No se pudo conservar la comprobación CADCraft.")?;
        let directory=self.path.parent().ok_or("Falta el registro CADCraft.")?;retain_drawing_copy_check(directory,input,hash)?;
        *current=read(directory,&OperationInput::LiveDrawing(input.clone()))?.ok_or("Falta el envío original CADCraft.")?;Ok(())
    }
    pub fn complete(&self, checked: &OperationInput) -> Result<(), String> {
        if matches!(checked,OperationInput::LiveAgent(input) if crate::agent_editor::working_plan(input).is_some()) && read(self.path.parent().ok_or("Falta el registro del editor.")?,checked)?.is_none_or(|r|r.agent_project_check_sha256.is_none()){return Err("La copia del proyecto todavía no está comprobada.".into());}
        if matches!(checked,OperationInput::LiveAgent(input) if input.buffer_checkpoint.is_some()) && read(self.path.parent().ok_or("Falta el registro del editor.")?,checked)?.is_none_or(|record|record.agent_buffers_sha256.is_none()){return Err("La edición se conserva; falta su checkpoint de buffers.".into());}
        if matches!(checked,OperationInput::LivePcb(input) if input.native_completion_version==Some(1)) && read(self.path.parent().ok_or("Falta el registro PCB.")?,checked)?.is_none_or(|record|record.pcb_completion_sha256.is_none()){return Err("Falta el vínculo PCB original; se conserva la pausa.".into());}
        if matches!(checked,OperationInput::LiveAgent(input) if input.native_completion_version==Some(1)) && read(self.path.parent().ok_or("Falta el registro del editor.")?,checked)?.is_none_or(|record|record.agent_completion_sha256.is_none()){return Err("Falta el vínculo original del editor; se conserva la pausa.".into());}
        if matches!(checked,OperationInput::LiveDrawing(input) if input.native_working_version.is_some()) && read(self.path.parent().ok_or("Falta el registro CADCraft.")?,checked)?.is_none_or(|record|record.drawing_copy_check_sha256.is_none()){return Err("La edición se conserva; su nueva versión todavía no está comprobada.".into());}
        if read(self.path.parent().ok_or("Falta el registro nativo.")?,checked)?.is_some_and(|record|record.drawing_completion_required && record.drawing_completion_sha256.is_none()){return Err("Falta el vínculo original CADCraft; se conserva la pausa.".into());}
        if read(self.path.parent().ok_or("Falta el registro nativo.")?,checked)?.is_some_and(|record|record.refusal_sha256.is_some()){return Err("La edición no inició; no se marcará como completada.".into());}
        let mut record = self
            .record
            .lock()
            .map_err(|_| "No se pudo conservar el resultado nativo.")?;
        if record.outcome != Outcome::Dispatched || record.input_sha256 != digest(checked)? {
            return Err("El resultado no corresponde a la llamada admitida.".into());
        }
        let hash = checked
            .receipt_sha256()
            .ok_or("Falta el recibo del resultado comprobado.")?;
        record.receipt_sha256 = Some(hash.into());
        record.outcome = Outcome::Acknowledged;
        atomic_json(&self.path,&*record,true).map_err(|_|"El resultado sigue local, pero no se confirmó su registro. No repitas la operación.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drawing_completion_is_input_bound_and_survives_a_locked_anchor_without_replay(){
        use std::{fs,os::windows::fs::OpenOptionsExt};
        let root=std::env::temp_dir().join(format!("hazlo-drawing-anchor-{}",Uuid::new_v4()));fs::create_dir(&root).unwrap();let(input,proof,_)=crate::cadcraft_live::tests::completed_fixture();let original=OperationInput::LiveDrawing(input.clone());let fence=Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();let bytes=serde_json::to_vec(&proof).unwrap();fs::write(root.join("drawing-completion.json"),&bytes).unwrap();
        let original_record=fs::read(root.join("live-dispatch.json")).unwrap();let mut downgraded:serde_json::Value=serde_json::from_slice(&original_record).unwrap();downgraded.as_object_mut().unwrap().remove("drawing_completion_required");fs::write(root.join("live-dispatch.json"),serde_json::to_vec(&downgraded).unwrap()).unwrap();assert!(read(&root,&original).is_err());fs::write(root.join("live-dispatch.json"),&original_record).unwrap();assert!(fence.retain_drawing_completion(&input,&proof).is_err());drop(fence.enter().unwrap());assert!(crate::cadcraft_live::recover_result(&input,&root,&root.join("not-launched"),Arc::new(AtomicBool::new(false))).is_err());assert!(!root.join("result.json").exists());assert!(!root.join("not-launched").exists());
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(root.join("live-dispatch.json")).unwrap();assert!(fence.retain_drawing_completion(&input,&proof).is_err());assert!(read(&root,&original).unwrap().unwrap().drawing_completion_sha256.is_none());assert_eq!(fs::read(root.join("drawing-completion.json")).unwrap(),bytes);drop(lock);
        fence.retain_drawing_completion(&input,&proof).unwrap();fence.retain_drawing_completion(&input,&proof).unwrap();assert!(fence.enter().is_err());let summary=read(&root,&original).unwrap().unwrap().summary(&original);assert_eq!(summary.acknowledged,1);assert_eq!(summary.uncertain,0);assert!(summary.needs_review);assert!(!summary.finished);
        let mut changed=input.clone();changed.agreement.as_mut().unwrap().outcome="Changed conditions".into();assert!(read(&root,&OperationInput::LiveDrawing(changed)).is_err());let mut changed=proof.clone();changed["after"]["drawing"]["entities"][0]["geometry"]["radius"]=serde_json::json!(999);fs::write(root.join("drawing-completion.json"),serde_json::to_vec(&changed).unwrap()).unwrap();assert!(read(&root,&original).is_err());assert!(fence.retain_drawing_completion(&input,&changed).is_err());fs::write(root.join("drawing-completion.json"),&bytes).unwrap();
        assert!(crate::cadcraft_live::recover_recorded_result(&root,&input).unwrap());let mut checked=original.clone();checked.retain_receipt(&root).unwrap();recover_checked(&root,&checked).unwrap();let final_bytes=fs::read(root.join("result.json")).unwrap();fs::remove_file(root.join("result.json")).unwrap();assert!(crate::cadcraft_live::recover_recorded_result(&root,&input).unwrap());assert_eq!(fs::read(root.join("result.json")).unwrap(),final_bytes);fs::write(root.join("result.json"),b"preserve changed final report").unwrap();assert!(crate::cadcraft_live::recover_recorded_result(&root,&input).is_err());assert_eq!(fs::read(root.join("result.json")).unwrap(),b"preserve changed final report");drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn live_agreements_are_input_bound_and_legacy_receipts_do_not_gain_invented_conditions(){
        let cad=serde_json::from_value(serde_json::json!({"schema_version":1,"live_cad_version":1,"job_id":Uuid::new_v4(),"document_name":"Owned","document_token":Uuid::new_v4(),"state_sha256":"a".repeat(64),"description":"Apply the owned change","operations":[{"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}}],"before":[],"agreement":crate::native_tools::tests::agreement_fixture()})).unwrap();
        let cases=[OperationInput::LiveCad(cad),OperationInput::LiveAgent(crate::agent_editor::tests::job_fixture()),OperationInput::LivePcb(crate::kicad_live::tests::move_fixture()),OperationInput::LiveDrawing(crate::cadcraft_live::tests::fixture_input())];
        for original in cases{
            original.validate().unwrap();let root=std::env::temp_dir().join(format!("hazlo-live-agreement-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
            let fence=Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();drop(fence);
            let mut changed=serde_json::to_value(&original).unwrap();changed["agreement"]["outcome"]=serde_json::json!("Different outcome after approval");let changed:OperationInput=serde_json::from_value(changed).unwrap();assert!(read(&root,&changed).is_err());assert!(read(&root,&original).unwrap().is_some());std::fs::remove_file(root.join("live-dispatch.json")).unwrap();
            let mut legacy=serde_json::to_value(&original).unwrap();legacy.as_object_mut().unwrap().remove("agreement");let input:OperationInput=serde_json::from_value(legacy.clone()).unwrap();input.validate().unwrap();assert_eq!(serde_json::to_value(&input).unwrap(),legacy);assert!(input.agreement().is_none());
            let fence=Dispatch::start(&root,&input,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();drop(fence);assert!(read(&root,&input).unwrap().is_some());assert!(read(&root,&original).is_err(),"Conditions were retrofitted into an old receipt");assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn original_refusal_proof_resolves_uncertainty_without_erasing_admission_or_accepting_results(){
        let root=std::env::temp_dir().join(format!("hazlo-native-refusal-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let (input,completed)=crate::cadcraft_live::tests::recorded_fixture();let original=OperationInput::LiveDrawing(input.clone());
        let fence=Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();let proof=serde_json::json!({"version":1,"input_sha256":completed["input_sha256"],"outcome":"not_dispatched"});let path=root.join("drawing-completion.json");std::fs::write(&path,serde_json::to_vec(&proof).unwrap()).unwrap();assert!(recover_refused(&root,&original).is_err());drop(fence.enter().unwrap());
        recover_refused(&root,&original).unwrap();recover_refused(&root,&original).unwrap();let record=read(&root,&original).unwrap().unwrap();assert_eq!(record.outcome,Outcome::Dispatched);assert!(record.summary(&original).outcome_checked);assert!(!record.summary(&original).needs_review);assert_eq!(record.summary(&original).not_dispatched,1);assert_eq!(record.summary(&original).uncertain,0);
        std::fs::write(root.join("result.json"),serde_json::to_vec(&crate::cadcraft_live::tests::receipt(&input)).unwrap()).unwrap();let mut checked=original.clone();checked.retain_receipt(&root).unwrap();assert!(recover_checked(&root,&checked).is_err());assert!(fence.complete(&checked).is_err());std::fs::write(&path,serde_json::to_vec(&completed).unwrap()).unwrap();assert!(read(&root,&original).is_err());
        drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn recovered_acknowledgment_survives_locks_and_rejects_changed_evidence(){
        use std::os::windows::fs::OpenOptionsExt;
        let root=std::env::temp_dir().join(format!("hazlo-recovered-ack-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input=crate::agent_editor::tests::job_fixture();let original=OperationInput::LiveAgent(input.clone());let fence=Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap()).unwrap();let mut checked=original.clone();checked.retain_receipt(&root).unwrap();assert!(recover_checked(&root,&checked).is_err());drop(fence.enter().unwrap());drop(fence);
        let journal=root.join("live-dispatch.json");let before=std::fs::read(&journal).unwrap();let lock=std::fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&journal).unwrap();
        assert!(recover_checked(&root,&checked).is_err());assert_eq!(std::fs::read(&journal).unwrap(),before);assert!(read(&root,&original).unwrap().unwrap().summary(&original).needs_review);drop(lock);
        recover_checked(&root,&checked).unwrap();let after=std::fs::read(&journal).unwrap();recover_checked(&root,&checked).unwrap();assert_eq!(std::fs::read(&journal).unwrap(),after);assert!(!read(&root,&original).unwrap().unwrap().summary(&original).needs_review);
        std::fs::write(root.join("result.json"),b"changed native report").unwrap();assert!(read(&root,&original).is_err());assert!(recover_checked(&root,&checked).is_err());assert_eq!(std::fs::read(&journal).unwrap(),after);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn durable_fence_distinguishes_unsent_and_admitted_calls_without_replay() {
        let root = std::env::temp_dir().join(format!("hazlo-live-fence-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input = OperationInput::LiveAgent(crate::agent_editor::tests::job_fixture());
        let gate = Arc::new(Mutex::new(()));
        let paused = Arc::new(AtomicBool::new(true));
        let cancel = Arc::new(AtomicBool::new(false));
        let dispatch = Dispatch::start(&root, &input, gate.clone(), paused, cancel).unwrap();
        assert!(dispatch.enter().is_err());
        assert!(!dispatch.was_dispatched());
        assert_eq!(
            read(&root, &input).unwrap().unwrap().outcome,
            Outcome::Pending
        );
        assert!(
            !read(&root, &input)
                .unwrap()
                .unwrap()
                .summary(&input)
                .needs_review
        );
        assert!(Dispatch::start(
            &root,
            &input,
            gate.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        let other = root.join("admitted");
        std::fs::create_dir(&other).unwrap();
        let dispatch = Dispatch::start(
            &other,
            &input,
            gate.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let permit = dispatch.enter().unwrap();
        assert!(gate.try_lock().is_err());
        drop(permit);
        assert!(gate.try_lock().is_ok());
        assert!(dispatch.enter().is_err());
        assert!(
            read(&other, &input)
                .unwrap()
                .unwrap()
                .summary(&input)
                .needs_review
        );
        let mut altered = input.clone();
        if let OperationInput::LiveAgent(i) = &mut altered {
            i.source.push_str("\n// changed intent");
        }
        assert!(read(&other, &altered).is_err());
        assert!(!std::fs::read_to_string(other.join("live-dispatch.json"))
            .unwrap()
            .contains("unsaved note"));
        drop(dispatch);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checked_live_record_recovers_a_receipt_if_the_app_stops_before_finalization() {
        use crate::project::{Phase, Project, RunSource};
        let root = std::env::temp_dir().join(format!("hazlo-live-commit-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let project = Project::create(&root).unwrap();
        let input = crate::agent_editor::tests::job_fixture();
        let id = input.job_id;
        let mut snapshot = project
            .snapshot(OperationInput::LiveAgent(input.clone()), Phase::Running)
            .unwrap();
        snapshot.audit.as_mut().unwrap().source = RunSource::Mcp;
        snapshot.audit.as_mut().unwrap().native_attempted = Some(true);
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(id).unwrap();
        let bytes =
            serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap();
        std::fs::write(stage.path.join("result.json"), &bytes).unwrap();
        let dispatch = Dispatch::start(
            &stage.path,
            &snapshot.input,
            Arc::new(Mutex::new(())),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        drop(dispatch.enter().unwrap());
        let mut checked = snapshot.input.clone();
        checked.retain_receipt(&stage.path).unwrap();
        project.validate_outputs(&stage.path, &checked).unwrap();
        dispatch.complete(&checked).unwrap();
        assert!(snapshot.input.receipt_sha256().is_none());
        drop(dispatch);
        drop(stage);
        drop(project);
        let project = Project::open(&root).unwrap();
        let recovered = project.recover().unwrap();
        assert_eq!(recovered[0].phase, Phase::Completed);
        assert!(recovered[0].input.receipt_sha256().is_some());
        assert_eq!(
            std::fs::read(project.artifact_directory(id).join("result.json")).unwrap(),
            bytes
        );
        assert!(!project.native_recovery(None).unwrap()[0].needs_review);
        drop(project);
        std::fs::remove_dir_all(root).unwrap();
    }
}
