use crate::{
    agent, arduino, cadcraft, cadpilot, freecad, kicad_project, kicad_run, konnect_run,
    ollama::{OllamaClient, Proposal},
    orca, orca_run,
    project::{
        Artifact, ArtifactFormat, OperationInput, Phase, Project, RunSource, Snapshot, Stage,
    },
    Dimensions,
};
use serde::Serialize;
use std::{
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub snapshot: Snapshot,
    pub approval_id: Option<Uuid>,
    pub sequence: u64,
    pub error: Option<String>,
    pub busy: bool,
}

impl JobView {
    fn advance(&mut self) {
        self.sequence += 1;
        self.snapshot.advance();
        if let Some(audit) = &mut self.snapshot.audit {
            audit.error = self
                .error
                .as_ref()
                .map(|error| error.chars().take(4096).collect());
        }
    }
}

#[derive(Serialize)]
pub struct WorkflowView {
    pub project: Option<(Uuid, PathBuf)>,
    pub job: Option<JobView>,
    pub inference: Option<InferenceView>,
    pub history: Vec<Snapshot>,
}
#[derive(Clone,PartialEq,Eq)]
pub struct ConversationScope {
    pub project:(Uuid,PathBuf),
    pub task:Option<crate::tasks::Binding>,
    pub provider:Option<String>,
    pub references:Vec<crate::tasks::ReplySource>,
    pub instruction_sources:Vec<crate::tasks::InstructionSource>,
    pub parent_sha256:String,
    pub position:usize,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProfile {
    LocalExperimental,
    Cloud,
}

impl ModelProfile {
    pub fn identity(self) -> (&'static str, &'static str) {
        match self {
            Self::LocalExperimental => (
                "qwen3:4b",
                "359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7",
            ),
            Self::Cloud => (
                "kimi-k2.7-code:cloud",
                "eda07a6592375dcbde7cf167b6d6b368cdd28e244f9d71559fb59919aca882fa",
            ),
        }
    }
    pub fn client(self) -> Result<OllamaClient, String> {
        let (model, digest) = self.identity();
        match self {
            Self::Cloud => OllamaClient::cloud(11434, model, digest),
            Self::LocalExperimental => OllamaClient::new(11434, model, digest),
        }
    }
    fn source(self) -> RunSource {
        let (model, digest) = self.identity();
        RunSource::Ollama {
            model: model.into(),
            digest: digest.into(),
            cloud: matches!(self, Self::Cloud),
            runtime_version: "0.35.1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InferenceView {
    pub id: Uuid,
    pub profile: ModelProfile,
    pub model: String,
    pub digest: String,
    pub sequence: u64,
    pub busy: bool,
    pub cancelled: bool,
    pub text: String,
    pub message: Option<String>,
    pub error: Option<String>,
}
struct Inference {
    view: InferenceView,
    cancelled: Arc<AtomicBool>,
}
pub struct InferenceDispatch {
    id: Uuid,
    project_id: Uuid,
    pub profile: ModelProfile,
    pub cancelled: Arc<AtomicBool>,
}

struct Job {
    dispatch_gate: Arc<Mutex<()>>,
    dispatch_paused: Arc<AtomicBool>,
    live_agent: Option<crate::agent_editor::Connection>,
    live_pcb: Option<crate::kicad_live::Binding>,
    live_drawing: Option<crate::cadcraft_live::Connection>,
    native_tools: Option<crate::native_tools::Connection>,
    view: JobView,
    cancelled: Arc<AtomicBool>,
}
struct State {
    project: Option<Arc<Project>>,
    job: Option<Job>,
    inference: Option<Inference>,
    tool_pauses: std::collections::BTreeMap<(Uuid,PathBuf,Uuid),u64>,
}

fn inference_idle(state: &State) -> Result<(), String> {
    if state.inference.as_ref().is_some_and(|item| item.view.busy) {
        return Err("Espere a que termine o cancele la solicitud a Ollama.".into());
    }
    Ok(())
}
type ToolScope=((Uuid,PathBuf),crate::tasks::Binding);
// Tool handlers are synchronous on one blocking worker; keep this binding through proposal admission.
thread_local!{static TOOL_SCOPE:std::cell::RefCell<Option<ToolScope>>=const{std::cell::RefCell::new(None)};}
struct RestoreToolScope(Option<ToolScope>);
impl Drop for RestoreToolScope{fn drop(&mut self){TOOL_SCOPE.with(|scope|{scope.replace(self.0.take());});}}
pub(crate) fn with_tool_scope<T>(project:(Uuid,PathBuf),task:Option<crate::tasks::Binding>,call:impl FnOnce()->T)->T{
    let previous=TOOL_SCOPE.with(|scope|{let previous=scope.borrow().clone();if previous.is_none(){scope.replace(task.map(|task|(project,task)));}previous});let _restore=RestoreToolScope(previous);call()
}
fn check_tool_scope(state:&State)->Result<(),String>{
    TOOL_SCOPE.with(|scope|{
        let scope=scope.borrow();let Some((expected,binding))=scope.as_ref() else{return Ok(());};
        let project=state.project.as_ref().filter(|p|(p.id,p.root.clone())==*expected).ok_or("El contexto de la herramienta cambió; no se prepararon nuevos cambios.")?;
        if state.tool_pauses.get(&(expected.0,expected.1.clone(),binding.id)).is_some_and(|revision|binding.revision<=*revision){return Err("La herramienta está pausada; no se prepararon nuevos cambios.".into());}
        if crate::tasks::Store::new(project).and_then(|store|store.binding()).map_err(|_|"No se pudo comprobar el contexto de la herramienta.")?.as_ref()!=Some(binding){return Err("La herramienta pertenece a una revisión anterior; no se prepararon nuevos cambios.".into());}Ok(())
    })
}

pub struct Workflow {
    state: Mutex<State>,
    executable: Mutex<PathBuf>,
    arduino: Mutex<Option<ArduinoRuntime>>,
    agent: Mutex<Option<agent::Connection>>,
    agent_source: Mutex<Option<AgentSource>>,
    cadcraft: Mutex<Option<PathBuf>>,
    kicad: Mutex<Option<PathBuf>>,
    konnect: Mutex<Option<PathBuf>>,
    orca: Mutex<Option<PathBuf>>,
    profiles: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArduinoRuntime {
    pub executable: PathBuf,
    pub data: PathBuf,
}

#[derive(Clone, Serialize)]
pub struct AgentSource {
    pub project_id: Uuid,
    #[serde(skip)]
    project_root:PathBuf,
    #[serde(skip)]
    task_id:Option<Uuid>,
    pub source_name: String,
    pub source: String,
}
impl AgentSource {
    pub fn sha256(&self) -> String {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(self.source.as_bytes()))
    }
}

pub struct Dispatch {
    dispatch_gate: Arc<Mutex<()>>,
    dispatch_paused: Arc<AtomicBool>,
    live_agent: Option<crate::agent_editor::Connection>,
    live_pcb: Option<crate::kicad_live::Binding>,
    live_drawing: Option<crate::cadcraft_live::Connection>,
    native_tools: Option<crate::native_tools::Connection>,
    project: Arc<Project>,
    snapshot: Snapshot,
    cancelled: Arc<AtomicBool>,
    agent: Option<agent::Connection>,
}

impl Workflow {
    pub(crate) fn native_version_plan(&self)->Result<crate::native_working_version::Plan,String>{crate::native_working_version::plan(&self.executable.lock().map_err(|_|"No se pudo comprobar FreeCADCmd.")?)}
    pub fn correct_chat(&self,scope:&ConversationScope,input:&crate::tasks::ChatCorrectionInput,draft:&str)->Result<(),String>{
        self.interrupt_chat(scope,Some(input),input.chat_id,draft,None)
    }
    pub fn answer_chat(&self,scope:&ConversationScope,input:&crate::tasks::ChatCorrectionInput,body:&[u8],draft:&str)->Result<(),String>{self.interrupt_chat(scope,Some(input),input.chat_id,draft,Some(body))}
    pub fn stop_chat(&self,scope:&ConversationScope,chat_id:Uuid,draft:&str)->Result<(),String>{self.interrupt_chat(scope,None,chat_id,draft,None)}
    fn interrupt_chat(&self,scope:&ConversationScope,input:Option<&crate::tasks::ChatCorrectionInput>,chat_id:Uuid,draft:&str,answer:Option<&[u8]>)->Result<(),String>{
        let mut state=self.state.lock().map_err(|_|"No se pudo comprobar la tarea.")?;inference_idle(&state)?;
        let task_id=scope.task.as_ref().map(|task|task.id).filter(|id|input.is_none_or(|input|input.id==*id)).ok_or("La conversación cambió; no se detuvo otra tarea.")?;
        let project=state.project.clone().filter(|p|(p.id,p.root.clone())==scope.project).ok_or("La conversación cambió; no se detuvo otra tarea.")?;
        // Fence captured producers even when the durable pause cannot be saved.
        let revision=scope.task.as_ref().unwrap().revision;
        state.tool_pauses.entry((scope.project.0,scope.project.1.clone(),task_id)).and_modify(|saved|*saved=(*saved).max(revision)).or_insert(revision);
        let store=crate::tasks::Store::new(&project).map_err(|_|"No se pudo comprobar la corrección guardada.")?;
        let save=||match (input,answer){(Some(input),Some(body))=>store.answer_chat(input,body,draft,&scope.references,scope.provider.as_deref().unwrap_or(""),&scope.instruction_sources),(Some(input),None)=>store.correct_chat(input,draft,&scope.references,scope.provider.as_deref().unwrap_or("")),(None,_)=>store.stop_chat(task_id,chat_id,draft,&scope.references,scope.provider.as_deref().unwrap_or(""))};
        if store.active().ok().flatten().is_some_and(|task|task.id==task_id && match input{Some(input)=>task.chat_correction.as_ref().is_some_and(|saved|saved.id==input.correction_id),None=>task.chat_stop.as_ref().is_some_and(|stop|stop.chat_id==chat_id)}){return save().map_err(|_|"La pausa o corrección guardada no coincide; se conservó la tarea.".into());}
        let gate=state.job.as_ref().filter(|job|job.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref()).is_some_and(|t|t.id==task_id)).map(|job|job.dispatch_gate.clone());
        let permit=gate.as_ref().map(|gate|gate.lock().map_err(|_|"No se pudo confirmar la pausa nativa.")).transpose()?;
        if gate.is_some(){let job=state.job.as_mut().unwrap();job.dispatch_paused.store(true,Ordering::SeqCst);job.view.approval_id=None;job.view.error=Some("Los pasos pendientes están pausados. Las llamadas enviadas pueden terminar; detener la conversación no las deshace.".into());job.view.advance();}
        let result=save().map_err(|_|"Se pidió detener la conversación, pero no se confirmó su pausa o corrección guardada. Revisa la tarea antes de continuar.".to_string());
        drop(permit);Self::reconcile_pending_approval(&mut state)?;result
    }
    pub fn conversation_scope(&self)->Result<Option<ConversationScope>,String>{
        let state=self.state.lock().map_err(|_|"No se pudo comprobar la conversación.")?;
        state.project.as_ref().map(|p|{
            let task=crate::tasks::Store::new(p).and_then(|s|s.active()).map_err(|_|"No se pudo comprobar la conversación.")?;
            let (parent_sha256,position)=crate::tasks::history_parent(task.as_ref().and_then(|t|t.history.as_deref())).map_err(|_|"No se pudo comprobar el historial local.")?;
            Ok(ConversationScope{project:(p.id,p.root.clone()),task:task.as_ref().map(|t|t.binding()),provider:task.as_ref().map(|t|t.provider.clone()),instruction_sources:task.as_ref().map(|t|t.instruction_sources()).unwrap_or_default(),references:task.as_ref().map(|t|t.references.iter().filter(|r|r.shared_with.contains(&t.provider)).map(|r|r.reply_source()).collect()).unwrap_or_default(),parent_sha256,position})
        }).transpose()
    }
    pub fn record_completed_reply(&self,scope:&ConversationScope,id:Uuid,provider:&str,content:&str)->Result<bool,String>{
        let state=self.state.lock().map_err(|_|"No se pudo conservar la proveniencia de respuesta.")?;
        let Some(project)=state.project.as_ref().filter(|p|(p.id,p.root.clone())==scope.project) else{return Ok(false);};
        let Some(task)=scope.task.as_ref() else{return Ok(false);};
        if scope.provider.as_deref()!=Some(provider) || content.is_empty(){return Ok(false);}
        let receipt=crate::tasks::ReplyReceipt{context_policy_version:1,id,task:task.clone(),provider:provider.into(),content_sha256:crate::kicad_project::digest(content.as_bytes()),parent_sha256:scope.parent_sha256.clone(),position:scope.position,references:scope.references.clone(),instruction_sources:scope.instruction_sources.clone(),revoked:false};
        crate::tasks::Store::new(project).and_then(|s|s.record_reply(receipt)).map_err(|_|"La respuesta sigue local, pero no se pudo registrar su proveniencia. No se restauró contexto retirado.".into())
    }
    fn source_scope(state:&State)->Result<Option<(Uuid,PathBuf,Option<Uuid>)>,String>{
        state.project.as_ref().map(|project|{
            let task=crate::tasks::Store::new(project).and_then(|s|s.active()).map_err(|_|"No se pudo comprobar la conversación del sketch; el original se conserva.")?;
            Ok((project.id,project.root.clone(),task.map(|t|t.id)))
        }).transpose()
    }
    fn reconcile_pending_approval(state:&mut State)->Result<(),String>{
        let Some(project)=state.project.clone() else{return Ok(());};
        let Some(job)=state.job.as_mut().filter(|j|!j.view.busy && j.view.snapshot.phase==Phase::AwaitingApproval) else{return Ok(());};
        let current=crate::tasks::Store::new(&project).and_then(|s|s.binding()).map_err(|_|"No se pudo comprobar la tarea; la propuesta se conserva sin ejecutar.")?;
        if job.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref())==current.as_ref(){return Ok(());}
        const NOTICE:&str="La tarea o sus permisos cambiaron. Esta propuesta se conserva sin ejecutar; revisa las entradas actuales y prepara una nueva aprobación.";
        // Fence approval before storage I/O; an uncertain metadata write must never restore its token.
        job.view.approval_id=None;job.dispatch_paused.store(true,Ordering::SeqCst);job.view.error=Some(NOTICE.into());
        let saved=project.history().map_err(|_|"No se pudo releer la propuesta conservada; la aprobación anterior está bloqueada.")?.into_iter().find(|s|s.input.job_id()==job.view.snapshot.input.job_id()).ok_or("No se encontró la propuesta conservada; la aprobación anterior está bloqueada.")?;
        let saved_input=serde_json::to_value(&saved.input).map_err(|_|"No se pudo comprobar la propuesta conservada.")?;
        let expected_input=serde_json::to_value(&job.view.snapshot.input).map_err(|_|"No se pudo comprobar la propuesta conservada.")?;
        if saved_input!=expected_input || saved.audit.as_ref().and_then(|a|a.task.as_ref())!=job.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref()) {return Err("La propuesta guardada cambió; se conservó y no se renovó su aprobación.".into());}
        if saved.audit.as_ref().and_then(|a|a.native_attempted)==Some(true){return Err("La propuesta tiene una posible ejecución nativa; conserva el trabajo y revisa su recuperación sin repetirla.".into());}
        let mut view=job.view.clone();view.snapshot=saved;
        if view.snapshot.phase==Phase::Cancelled && view.snapshot.audit.as_ref().is_some_and(|a|a.approval_invalidated){view.sequence+=1;job.view=view;return Ok(());}
        if view.snapshot.phase!=Phase::AwaitingApproval{return Err("La propuesta guardada tiene otro estado; se conservó sin repetir ninguna operación.".into());}
        view.snapshot.phase=Phase::Cancelled;
        if let Some(audit)=&mut view.snapshot.audit{audit.approval_invalidated=true;}
        view.advance();
        project.save(&view.snapshot).map_err(|_|"La propuesta se conserva y la aprobación anterior está bloqueada. No se pudo guardar su estado; vuelve a comprobarlo antes de continuar.")?;
        job.view=view;Ok(())
    }
    pub(crate) fn reconcile_native_batch(&self,expected:(Uuid,PathBuf),job_id:Uuid,connection:&crate::native_tools::Connection,cancel:Arc<AtomicBool>)->Result<serde_json::Value,String>{
        let project={let state=self.state.lock().map_err(|e|e.to_string())?;inference_idle(&state)?;
            if state.job.as_ref().is_some_and(|j|j.view.busy || j.view.snapshot.phase==Phase::AwaitingApproval){return Err("Termina o cancela la propuesta actual antes de revisar el lote anterior.".into());}
            state.project.clone().filter(|p|p.id==expected.0 && p.root==expected.1).ok_or("El proyecto cambió.")?};
        let snapshot=project.history().map_err(|e|e.to_string())?.into_iter().find(|s|s.input.job_id()==job_id).ok_or("No se encontró el lote en este proyecto.")?;
        if !self.task_allows_snapshot(&snapshot)?{return Err("Vuelve a la conversación del lote para revisar sus comprobaciones.".into());}
        let OperationInput::NativeTools(input)=&snapshot.input else{return Err("Este adaptador necesita su revisión nativa específica.".into());};
        let stage=project.store("staging").join(job_id.to_string());let directory=if stage.try_exists().map_err(|_|"No se pudo comprobar el trabajo local.")?{stage}else{project.artifact_directory(job_id)};
        let _pins=crate::project::pin_lineage(&directory).map_err(|_|"No se pudo fijar el trabajo local para revisión.")?;
        crate::native_journal::reconcile(&directory,input,connection,cancel)
    }
    pub fn native_recovery(&self)->Result<Vec<crate::native_journal::Summary>,String>{
        let state=self.state.lock().map_err(|e|e.to_string())?;let Some(project)=&state.project else{return Ok(Vec::new());};
        project.native_recovery(None).map_err(|e|e.to_string())
    }
    pub(crate) fn recover_native_result(&self,expected:(Uuid,PathBuf),job_id:Uuid,cancel:Arc<AtomicBool>)->Result<serde_json::Value,String>{
        let (project,agent_connection)={let state=self.state.lock().map_err(|_|"No se pudo comprobar el trabajo local.")?;inference_idle(&state)?;
            if state.job.as_ref().is_some_and(|job|job.view.busy || job.view.snapshot.phase==Phase::AwaitingApproval){return Err("Termina o cancela la propuesta actual antes de recuperar el resultado anterior.".into());}
            let project=state.project.clone().filter(|p|p.id==expected.0 && p.root==expected.1).ok_or("El proyecto cambió; no se consultó otro trabajo.")?;
            (project,state.job.as_ref().filter(|job|job.view.snapshot.input.job_id()==job_id).and_then(|job|job.live_agent.clone()))};
        let snapshot=project.history().map_err(|_|"No se pudo leer el registro local.")?.into_iter().find(|s|s.input.job_id()==job_id).ok_or("No se encontró la operación en este proyecto.")?;
        if !self.task_allows_snapshot(&snapshot)?{return Err("Vuelve a la conversación de la operación para recuperar su resultado.".into());}
        if !matches!(&snapshot.input,OperationInput::NativeTools(_)|OperationInput::LiveCad(_)|OperationInput::LiveAgent(_)|OperationInput::LivePcb(_)|OperationInput::LiveDrawing(_)){return Err("Este programa todavía necesita su revisión nativa específica. No se repitió ninguna operación.".into());}
        let stage=project.store("staging").join(job_id.to_string());let directory=if stage.try_exists().map_err(|_|"No se pudo comprobar el trabajo local.")?{stage}else{project.artifact_directory(job_id)};
        let _pins=crate::project::pin_lineage(&directory).map_err(|_|"No se pudo fijar el resultado local para recuperación.")?;
        let mut checked=snapshot.input.clone();
        if let OperationInput::NativeTools(input)=&snapshot.input{
            checked=OperationInput::NativeTools(crate::native_tools::recover_recorded_result(&directory,input)?);
            let mut saved=snapshot.clone();saved.input=checked.clone();project.save(&saved).map_err(|_|"El resultado sigue local; no se pudo registrar su recibo recuperado.")?;
        }else{
        if let OperationInput::LiveCad(input)=&snapshot.input{crate::cadpilot_live::recover_recorded_result(&directory,input)?;}
        if let OperationInput::LiveDrawing(input)=&snapshot.input{crate::cadcraft_live::recover_recorded_result(&directory,input)?;}
        let record=crate::live_dispatch::read(&directory,&snapshot.input)?.ok_or("Falta el registro de la llamada; se conserva la pausa.")?;
        if record.outcome==crate::native_journal::Outcome::Pending{return Err("La llamada no fue admitida; prepara una nueva propuesta con el estado actual.".into());}
        if record.refusal_sha256.is_some() || matches!(&snapshot.input,OperationInput::LiveDrawing(input) if crate::cadcraft_live::refusal_proof(input,&directory).unwrap_or(false)){
            crate::live_dispatch::recover_refused(&directory,&snapshot.input)?;
            return Ok(serde_json::json!({"job_id":job_id,"recovered":true,"published":false,"native_operation_rejected":true,"edit_repeated":false,"guidance":"The original native receipt proves that the edit did not start. Preserve current work and inspect fresh state before a new proposal; this is not task completion."}));
        }
        if record.outcome!=crate::native_journal::Outcome::Acknowledged{
            let recovered=match &snapshot.input{
                OperationInput::LiveCad(input)=>crate::cadpilot_live::recover_result(input,&directory,&cancel)?,
                OperationInput::LiveAgent(input)=>{
                    let connection=if let Some(connection)=agent_connection{connection}else{crate::agent_editor::Connection::load(&crate::agent_editor::default_directory()?.join(format!("{}.json",input.before.instance_id))).map_err(|_|"La instancia original del editor ya no está conectada. Se conserva el trabajo y la pausa.")?};
                    crate::agent_editor::recover_result(input,&connection,&directory,&cancel)?
                },
                OperationInput::LivePcb(input)=>crate::kicad_live::recover_result(input,&directory)?,
                OperationInput::LiveDrawing(input)=>{
                    crate::cadcraft_live::recover_result(input,&directory,&self.profiles.join(format!("drawing-recovery-{}",Uuid::new_v4())),cancel.clone())?
                },
                _=>return Err("Este programa todavía necesita su evidencia nativa de finalización. Se conserva el trabajo y la pausa.".into()),
            };
            if !recovered{return Ok(serde_json::json!({"job_id":job_id,"recovered":false,"native_operation_running":true,"edit_repeated":false}));}
        }
        if let OperationInput::LiveCad(input)=&snapshot.input{if input.working_version.is_some(){let record=crate::live_dispatch::read(&directory,&snapshot.input)?.ok_or("Falta el envío original.")?;if record.working_version_sha256.is_none(){let result:serde_json::Value=crate::project::read_json(&directory.join("result.json")).map_err(|_|"Falta la captura nativa conservada.")?;let executable=self.executable.lock().map_err(|_|"No se pudo comprobar FreeCADCmd.")?.clone();let hash=crate::native_working_version::verify(&executable,input.working_version.as_ref().unwrap(),&directory,&self.profiles.join(format!("native-version-recovery-{}",Uuid::new_v4())),&result,cancel.clone())?;crate::live_dispatch::retain_working_version(&directory,input,&hash)?;}}}
        checked.retain_receipt(&directory).map_err(|_|"El resultado sigue local, pero no se pudo conservar su recibo.")?;
        project.validate_outputs(&directory,&checked).map_err(|_|"El resultado nativo no pasó sus comprobaciones; se conserva la pausa.")?;
        crate::live_dispatch::recover_checked(&directory,&checked)?;
        }
        drop(_pins);
        let recovered=project.recover().map_err(|_|"El resultado se conservó, pero su publicación local sigue pendiente. No repitas la operación.")?;
        let saved=recovered.into_iter().find(|s|s.input.job_id()==job_id).ok_or("El resultado sigue local; vuelve a comprobar su publicación.")?;let published=saved.phase==Phase::Completed;
        let mut state=self.state.lock().map_err(|_|"El resultado se conservó; vuelve a leer el estado local.")?;
        if state.project.as_ref().is_some_and(|p|p.id==project.id && p.root==project.root){
            if let Some(job)=state.job.as_mut().filter(|job|job.view.snapshot.input.job_id()==job_id && !job.view.busy){
                job.view.snapshot=saved;job.view.advance();
            }
        }
        Ok(serde_json::json!({"job_id":job_id,"recovered":true,"published":published,"edit_repeated":false,"guidance":"The original native result was recovered, not the current document state. Read the verified result locally or with its separate evidence consent, then inspect fresh native state before proposing further changes."}))
    }
    pub fn retain_tool_evidence(&self,expected:&crate::tasks::Binding,tool:&str,result:&Result<serde_json::Value,String>)->Result<Uuid,String>{
        let mut state=self.state.lock().map_err(|_|"No se pudo conservar evidencia local.")?;
        let project=state.project.clone().ok_or("No hay proyecto activo.")?;
        let store=crate::tasks::Store::new(&project).map_err(|_|"No se pudo conservar evidencia local.")?;
        if store.binding().map_err(|_|"No se pudo comprobar la tarea.")?.as_ref()!=Some(expected){return Err("La tarea cambió; no se compartió la respuesta nativa.".into());}
        let reference_name=if tool=="read_task_history"{"Conversación guardada.json"}else if tool=="inspect_native_tools"{"Catálogo nativo.json"}else if tool=="read_verified_result" && result.as_ref().is_ok_and(|v|v["format"]=="public_document_report"){"Documento público.json"}else{"Observación nativa.json"};
        let value=match result {Ok(value)=>serde_json::json!({"tool":tool,"result":value}),Err(error)=>serde_json::json!({"tool":tool,"error":error})};
        let bytes=serde_json::to_vec(&value).map_err(|_|"No se pudo conservar evidencia local.")?;
        let result={let sources=if tool=="read_verified_result"{let id=result.as_ref().ok().and_then(|value|value["job_id"].as_str()).and_then(|id|Uuid::parse_str(id).ok());project.history().map_err(|_|"No se pudo comprobar la proveniencia del resultado.")?.into_iter().find(|job|Some(job.input.job_id())==id).and_then(|job|job.audit).map(|audit|audit.instruction_sources).unwrap_or_default()}else{Vec::new()};if tool=="read_task_history"{store.retain_history_snapshot(expected,bytes)}else if sources.is_empty(){store.retain_evidence(expected,reference_name.into(),bytes)}else{store.retain_evidence_with_instructions(expected,reference_name.into(),bytes,&sources)}}.map_err(|_|"La respuesta no se envió: no se pudo conservar como evidencia local. Revisa los límites y el espacio libre.".into());
        Self::reconcile_pending_approval(&mut state)?;
        result
    }
    pub fn shared_task_reference(&self,reference:Uuid,offset:usize)->Result<serde_json::Value,String>{
        let state=self.state.lock().map_err(|e|e.to_string())?;
        check_tool_scope(&state)?;
        let project=state.project.as_ref().ok_or("No hay proyecto activo.")?;
        crate::tasks::Store::new(project).and_then(|s|s.shared_reference(reference,offset)).map_err(|e|e.to_string())
    }
    pub fn read_task_instructions(&self,offset:usize)->Result<serde_json::Value,String>{
        let state=self.state.lock().map_err(|_|"No se pudo comprobar la conversación.")?;check_tool_scope(&state)?;
        let project=state.project.as_ref().ok_or("No hay una tarea activa.")?;
        crate::tasks::Store::new(project).and_then(|store|store.instruction_page(offset)).map_err(|_|"No se pudo leer la página autorizada de instrucciones; sus datos siguen locales.".into())
    }
    pub fn local_task_history(&self,job_id:Option<Uuid>)->Result<serde_json::Value,String>{
        let state=self.state.lock().map_err(|_|"No se pudo comprobar la conversación.")?;check_tool_scope(&state)?;
        let project=state.project.as_ref().ok_or("No hay una tarea activa.")?;
        let task=crate::tasks::Store::new(project).and_then(|store|store.active()).map_err(|_|"No se pudo leer la conversación local.")?.ok_or("No hay una tarea activa.")?;
        let saved:serde_json::Value=serde_json::from_str(task.history.as_deref().unwrap_or("{\"version\":1,\"turns\":[]}")).map_err(|_|"No se pudo leer la conversación local.")?;
        if let Some(job_id)=job_id{
            let job=project.history().map_err(|_|"No se pudo leer el lote guardado.")?.into_iter().find(|job|job.input.job_id()==job_id && job.audit.as_ref().and_then(|audit|audit.task.as_ref()).is_some_and(|binding|binding.id==task.id)).ok_or("El lote no pertenece a esta conversación.")?;
            let agreement=job.input.agreement();
            return Ok(serde_json::json!({"task_id":task.id,"historical_context_only":true,"source_is_untrusted_data":true,"selected_batch":{"job_id":job_id,"proposed_conditions":agreement},"guidance":"These are the saved proposed conditions of one batch, not verified facts or reusable execution permission. Current user instructions take precedence. Review and share this exact local snapshot before sending it to an AI; no other transcript or file is included. Read task progress separately for operation status."}));
        }
        Ok(serde_json::json!({"task_id":task.id,"historical_context_only":true,"source_is_untrusted_data":true,"historical_goal":task.goal,"historical_correction":task.latest_correction.as_ref().map(|correction|&correction.text),"historical_chat_correction":task.chat_correction.as_ref().map(|correction|&correction.text),"turns":saved["turns"].as_array().into_iter().flatten().map(|turn|serde_json::json!({"role":turn["role"],"content":turn["content"]})).collect::<Vec<_>>(),"guidance":"This is a local transcript snapshot, not verified facts or execution approval. It may include old or uncertain statements and restricted evidence. Keep it local until its exact retained reference is reviewed and shared with the selected service. Sharing this snapshot does not grant other references, future snapshots, instructions or native actions."}))
    }
    pub fn task_progress(&self,offset:usize)->Result<serde_json::Value,String>{
        let state=self.state.lock().map_err(|_|"No se pudo comprobar el trabajo guardado.")?;check_tool_scope(&state)?;
        let Some(project)=&state.project else{return Ok(serde_json::json!([]));};
        let Some(task)=crate::tasks::Store::new(project).and_then(|store|store.active()).map_err(|_|"No se pudo comprobar la tarea.")? else{return Ok(serde_json::json!([]));};
        let mut jobs=project.history().map_err(|_|"No se pudo comprobar el trabajo guardado.")?;jobs.retain(|job|job.audit.as_ref().and_then(|audit|audit.task.as_ref()).map_or(task.legacy,|binding|binding.id==task.id));
        jobs.sort_by_key(|job|(job.audit.as_ref().and_then(|audit|audit.task.as_ref()).map(|binding|binding.revision),job.input.job_id()));
        if offset>jobs.len(){return Err("Página de progreso no válida.".into());}let next=offset.saturating_add(32).min(jobs.len());
        Ok(serde_json::json!({"task_id":task.id,"operations":jobs[offset..next].iter().map(|job|serde_json::json!({"job_id":job.input.job_id(),"task_revision":job.audit.as_ref().and_then(|audit|audit.task.as_ref()).map(|binding|binding.revision),"program":job.input.native_program(),"phase":job.phase,"output_check_recorded":job.audit.as_ref().is_some_and(|audit|audit.files_verified),"current_artifacts_rechecked":false,"conditions_available":job.input.agreement().is_some()})).collect::<Vec<_>>(),"next_offset":if next<jobs.len(){Some(next)}else{None},"read_tool":"read_task_progress","guidance":"These are saved operation records. An operation's completed phase or prior output check does not prove the task complete, current document state, saved-version reopen, or physical function. Use original receipts and fresh native checks; never replay uncertain edits."}))
    }
    pub fn task_state(&self) -> Result<Option<crate::tasks::State>, String> {
        let state=self.state.lock().map_err(|e|e.to_string())?;
        state.project.as_ref().map(|p|crate::tasks::Store::new(p).and_then(|s|s.state()).map_err(|e|e.to_string())).transpose()
    }
    pub fn task_binding(&self) -> Result<Option<crate::tasks::Binding>, String> {
        let state=self.state.lock().map_err(|e|e.to_string())?;
        state.project.as_ref().map(|p|crate::tasks::Store::new(p).and_then(|s|s.binding()).map_err(|e|e.to_string())).transpose().map(Option::flatten)
    }
    pub fn task_recipient(&self,provider:&str)->Result<crate::tasks::Binding,String>{
        let state=self.state.lock().map_err(|_|"No se pudo comprobar la conversación.")?;
        let project=state.project.as_ref().ok_or("Abre una conversación en Hazlo antes de conectar su IA.")?;
        let task=crate::tasks::Store::new(project).and_then(|s|s.active()).map_err(|_|"No se pudo comprobar la conversación.")?.ok_or("Abre una conversación en Hazlo antes de conectar su IA.")?;
        if task.provider!=provider {return Err("La IA conectada no coincide con el destinatario aprobado de esta conversación.".into());}
        Ok(task.binding())
    }
    pub fn task_allows_snapshot(&self,snapshot:&Snapshot)->Result<bool,String>{
        let state=self.state.lock().map_err(|e|e.to_string())?;
        let Some(project)=&state.project else{return Ok(false);};
        let task=crate::tasks::Store::new(project).and_then(|s|s.active()).map_err(|e|e.to_string())?;
        Ok(task.is_none_or(|task|snapshot.audit.as_ref().and_then(|a|a.task.as_ref()).map_or(task.legacy,|binding|binding.id==task.id)))
    }
    pub fn task_restricted(&self) -> Result<bool,String> {
        let state=self.state.lock().map_err(|e|e.to_string())?;
        Ok(state.project.as_ref().map(|p|crate::tasks::Store::new(p).and_then(|s|s.active())).transpose().map_err(|e|e.to_string())?.flatten().is_some_and(|t|t.restricted()))
    }
    pub fn task_request(&self,expected:(Uuid,PathBuf),request:crate::tasks::Request)->Result<serde_json::Value,String>{
        let mut state=self.state.lock().map_err(|e|e.to_string())?;inference_idle(&state)?;
        let correction=match &request {crate::tasks::Request::Message{id,revision,correction_of:Some(job_id),..}=>Some((*job_id,crate::tasks::Binding{id:*id,revision:*revision})),_=>None};
        if correction.is_none() && state.job.as_ref().is_some_and(|j|j.view.busy){return Err("Termina o detén la operación antes de cambiar la tarea.".into());}
        let switches=matches!(request,crate::tasks::Request::New|crate::tasks::Request::Open{..});
        let project=state.project.clone().filter(|p|p.id==expected.0 && p.root==expected.1).ok_or("El proyecto cambió; la tarea se conservó.")?;
        let store=crate::tasks::Store::new(&project).map_err(|e|e.to_string())?;
        let gate=if let Some((job_id,binding))=&correction {
            if store.binding().map_err(|e|e.to_string())?.as_ref()!=Some(binding){return Err("La tarea cambió; vuelve a leerla antes de corregir.".into());}
            let job=state.job.as_ref().filter(|j|j.view.snapshot.input.job_id()==*job_id && j.view.snapshot.input.native_program().is_some() && j.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref()).is_some_and(|t|t.id==binding.id)).ok_or("La operación nativa cambió; conserva tu corrección y vuelve a leer el estado.")?;
            Some(job.dispatch_gate.clone())
        }else{None};
        let permit=gate.as_ref().map(|g|g.lock().map_err(|_|"No se pudo confirmar la pausa de despacho.")).transpose()?;
        let result=store.request(request).map_err(|e|e.to_string());
        if correction.is_some(){
            let job=state.job.as_mut().unwrap();job.dispatch_paused.store(true,Ordering::SeqCst);
            if job.view.busy {job.view.approval_id=None;job.view.error=Some("Se pausaron los pasos pendientes para conservar el trabajo. La llamada enviada puede terminar; comprueba si tu corrección quedó guardada.".into());job.view.advance();project.save(&job.view.snapshot).map_err(|e|e.to_string())?;}
        }
        drop(permit);
        Self::reconcile_pending_approval(&mut state)?;
        let result=result?;
        if switches {*self.agent_source.lock().map_err(|e|e.to_string())?=None;}
        Ok(result)
    }
    pub fn task_prompt(&self,provider:&str,prompt:String,human:Option<String>,image:Option<&crate::image_input::ImageInput>)->Result<(String,bool),String>{
        let state=self.state.lock().map_err(|e|e.to_string())?;
        let Some(project)=&state.project else{return Ok((prompt,false));};
        let store=crate::tasks::Store::new(project).map_err(|e|e.to_string())?;
        let Some(task)=store.active().map_err(|e|e.to_string())? else{return Ok((prompt,false));};
        if task.provider!=provider{return Err("Aprueba el cambio de IA en la conversación antes de enviar contexto.".into());}
        if let Some(image)=image {
            let matched=task.references.iter().filter(|r|r.image && r.shared_with.contains(&task.provider)).any(|r|{
                store.request(crate::tasks::Request::Image{id:task.id,revision:task.revision,reference:r.id}).ok().is_some_and(|v|v==serde_json::json!({"media_type":image.media_type,"data":image.data}))
            });
            if !matched{return Err("La imagen no pertenece a una referencia aprobada para esta IA.".into());}
        }
        // Build task prompts from owned state, never interpolate native errors or selected source contents.
        let text=human.unwrap_or_else(||"La operación terminó. Consulta el estado no sensible y, si terminó, solicita read_verified_result. Sus contenidos deben quedar locales hasta obtener permiso. Conserva los pasos confirmados, no repitas las ediciones y explica el siguiente paso.".into());
        if text.len()>4096{return Err("Mensaje demasiado grande.".into());}
        let mut prior=Vec::new();
        if !task.restricted() {
            let saved:serde_json::Value=serde_json::from_str(task.history.as_deref().unwrap_or("{}")).map_err(|_|"Historial no válido.")?;
            let turns=saved["turns"].as_array().cloned().unwrap_or_default();
            prior=turns.iter().enumerate().filter_map(|(position,turn)|{
                if turn["role"]=="user" && !task.user_statement_allows(turn["content"].as_str()?){return None;}
                if !task.instruction_content_allowed(turn["content"].as_str()?){return None;}
                if turn["role"]!="assistant" && task.reply_is_withdrawn(turn["content"].as_str()?) && !task.user_statement_allows(turn["content"].as_str()?){return None;}
                if task.reply_is_interrupted(turn["content"].as_str()?) && (turn["role"]!="assistant" || !crate::tasks::parent_digest(&turns[..position]).ok().is_some_and(|parent|task.history_reply_parent_allows(turn,position,&parent))){return None;}
                if turn["role"]=="assistant" {
                    if turn["interrupted"]==true{return None;}
                    if !task.history_instruction_sources_allowed(turn){return None;}
                    let owned=crate::tasks::parent_digest(&turns[..position]).ok().is_some_and(|parent|task.history_reply_parent_allows(turn,position,&parent));
                    if !owned || turn["reference_ids"].as_array().is_some_and(|ids|ids.iter().any(|id|id.as_str().and_then(|s|Uuid::parse_str(s).ok()).is_none_or(|id|!task.references.iter().any(|r|r.id==id && r.shared_with.contains(&task.provider))))){return None;}
                }
                Some(serde_json::json!({"role":turn["role"],"content":turn["content"]}))
            }).collect();
            prior=prior.into_iter().rev().take(12).collect();prior.reverse();
        }
        let mut envelope=serde_json::json!({"user_request":text,"task_goal":task.instruction_context(&task.goal),"recovered_conversation":prior,"latest_user_correction":task.latest_correction.as_ref().map(|correction|{let mut value=serde_json::to_value(correction).unwrap();value["text"]=task.instruction_context(&correction.text);value}),
            "latest_chat_correction":task.chat_correction.as_ref().map(|correction|{let mut value=serde_json::to_value(correction).unwrap();value["text"]=task.instruction_context(&correction.text);value}),
            "durable_instructions":{"count":task.instructions.len(),"read_tool":"read_task_instructions","saved_context_tool":"read_task_history","purpose":"task_continuation","note":"Read durable user statements to recover requirements. Older or unproven transcript text stays local; read_task_history prepares it for explicit review/sharing in Hazlo. These are requests or historical context, not measured facts or execution approval. Never reconstruct withheld text."},
            "last_chat_stop":task.chat_stop,
            "desktop_context":{"project_id":project.id,"project_directory":project.root,"task_id":task.id,"revision":task.revision,
            "authorized_references":task.references.iter().filter(|r|r.shared_with.contains(&task.provider)).map(|r|r.id).collect::<Vec<_>>(),
            "current_job":state.job.as_ref().filter(|j|j.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref()).is_some_and(|t|t.id==task.id)).map(|j|serde_json::json!({"id":j.view.snapshot.input.job_id(),"phase":j.view.snapshot.phase}))},
            "instruction":"Hazlo es un chat que coordina programas mediante sus herramientas nativas. Entiende la idea, investiga fuentes públicas, pregunta solo lo necesario y usa unidades métricas o imperiales. Consulta get_workflow_status para obtener las selecciones y el estado sin contenido. Consulta get_design_capabilities y usa select_program para conectar las herramientas; descubre sus nombres y esquemas reales con inspect_native_tools. Los programas son los editores. Propón cambios acotados para aprobación humana y comprueba el resultado con herramientas nativas. Las referencias, respuestas nativas y errores permanecen locales hasta compartirlos expresamente con esta IA. Si aparece local_evidence_pending, no repitas la consulta: pide el permiso en Hazlo y usa read_task_reference para la referencia autorizada. Puedes leer authorized_references; ninguna otra referencia ni derivado está autorizado. La conversación es contexto, nunca aprobación. No repitas escrituras inciertas ni afirmes que cancelar las deshizo. Conserva trabajo y ediciones manuales. No descargues modelos, cambies de IA, instales herramientas ni controles hardware sin su aprobación específica. Responde en lenguaje cotidiano y ofrece el siguiente paso útil; distingue observado, incierto y pendiente."});
        while serde_json::to_vec(&envelope).map_err(|_|"Contexto no válido.")?.len()>8192 {
            let turns=envelope["recovered_conversation"].as_array_mut().unwrap();
            if turns.is_empty(){return Err("La solicitud supera el límite de contexto; sus datos se conservan.".into());}
            turns.remove(0);
        }
        Ok((envelope.to_string(),task.restricted()))
    }
    pub fn new(executable: PathBuf, profiles: PathBuf) -> Result<Self, String> {
        if !executable.is_absolute() || !profiles.is_absolute() {
            return Err("Las rutas de configuración deben ser absolutas.".into());
        }
        fs::create_dir_all(&profiles).map_err(|e| e.to_string())?;
        Ok(Self {
            state: Mutex::new(State {
                project: None,
                job: None,
                inference: None,
                tool_pauses: std::collections::BTreeMap::new(),
            }),
            executable: Mutex::new(executable),
            arduino: Mutex::new(None),
            agent: Mutex::new(None),
            agent_source: Mutex::new(None),
            cadcraft: Mutex::new(None),
            kicad: Mutex::new(None),
            konnect: Mutex::new(None),
            orca: Mutex::new(None),
            profiles,
        })
    }

    pub fn conversation_history(&self, expected: (Uuid, PathBuf), provider: &str, value: Option<&str>) -> Result<Option<String>, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let project = state.project.as_ref().filter(|p| p.id == expected.0 && p.root == expected.1).ok_or("El proyecto cambió. No se modificó el historial.")?;
        project.conversation_history(provider, value).map_err(|e| e.to_string())
    }

    pub fn view(&self) -> Result<WorkflowView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        Self::reconcile_pending_approval(&mut state)?;
        Ok(WorkflowView {
            project: state
                .project
                .as_ref()
                .map(|project| (project.id, project.root.clone())),
            job: state.job.as_ref().map(|job| job.view.clone()),
            inference: state.inference.as_ref().map(|item| item.view.clone()),
            history: state
                .project
                .as_ref()
                .map(|project| project.history().map_err(|e| e.to_string()))
                .transpose()?
                .unwrap_or_default(),
        })
    }

    pub fn set_executable(&self, executable: PathBuf) -> Result<(), String> {
        if !executable.is_absolute()
            || !executable.is_file()
            || !executable
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("FreeCADCmd.exe"))
        {
            return Err("Seleccione FreeCADCmd.exe del paquete FreeCAD 1.1.4.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| job.view.busy) {
            return Err("Espere a que termine la operación actual.".into());
        }
        if state.job.as_ref().is_some_and(|job| {
            job.view.snapshot.phase == Phase::AwaitingApproval
                && matches!(job.view.snapshot.input, OperationInput::Cadpilot(_))
        }) {
            return Err(
                "Cancele la propuesta CADPilot antes de cambiar FreeCAD y repetir la medición."
                    .into(),
            );
        }
        let project = state.project.clone();
        if let Some(job) = state.job.as_mut() {
            if job.view.snapshot.phase == Phase::AwaitingApproval {
                let mut view = job.view.clone();
                view.advance();
                project
                    .as_ref()
                    .ok_or("Seleccione un proyecto.")?
                    .save(&view.snapshot)
                    .map_err(|e| e.to_string())?;
                view.approval_id = Some(Uuid::new_v4());
                job.view = view;
            }
        }
        *self.executable.lock().map_err(|e| e.to_string())? = executable;
        Ok(())
    }

    pub fn artifact(&self, job: Uuid, format: ArtifactFormat) -> Result<Artifact, String> {
        self.state
            .lock()
            .map_err(|e| e.to_string())?
            .project
            .as_ref()
            .ok_or("Seleccione un proyecto.")?
            .artifact(job, format)
            .map_err(|e| e.to_string())
    }

    pub fn arduino_runtime(&self) -> Result<Option<ArduinoRuntime>, String> {
        Ok(self.arduino.lock().map_err(|e| e.to_string())?.clone())
    }

    pub fn agent_connection(&self) -> Result<Option<agent::Connection>, String> {
        Ok(self.agent.lock().map_err(|e| e.to_string())?.clone())
    }
    pub fn agent_source(&self) -> Result<Option<AgentSource>, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let scope=Self::source_scope(&state)?;
        Ok(self
            .agent_source
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .filter(|source| {
                scope.as_ref().is_some_and(|(id,root,task)|*id==source.project_id && *root==source.project_root && *task==source.task_id)
            }))
    }
    pub fn set_agent(&self, connection: agent::Connection) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| job.view.busy) {
            return Err("Espere a que termine la operación actual.".into());
        }
        let project = state.project.clone();
        if let Some(job) = state.job.as_mut() {
            if job.view.snapshot.phase == Phase::AwaitingApproval
                && matches!(job.view.snapshot.input, OperationInput::Agent(_))
            {
                let mut view = job.view.clone();
                view.approval_id = Some(Uuid::new_v4());
                view.advance();
                project
                    .as_ref()
                    .ok_or("Seleccione un proyecto.")?
                    .save(&view.snapshot)
                    .map_err(|e| e.to_string())?;
                job.view = view;
            }
        }
        *self.agent.lock().map_err(|e| e.to_string())? = Some(connection);
        Ok(())
    }
    pub fn set_agent_source(&self, name: String, source: String) -> Result<(), String> {
        agent::EditInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            agent_version: agent::VERSION.into(),
            source_name: name.clone(),
            original_source: source.clone(),
            source: source.clone(),
        }
        .validate()?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|job| job.view.busy || job.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta antes de seleccionar otro sketch.".into());
        }
        let (project_id,project_root,task_id)=Self::source_scope(&state)?.ok_or("Seleccione un proyecto.")?;
        *self.agent_source.lock().map_err(|e| e.to_string())? = Some(AgentSource {
            project_id,
            project_root,
            task_id,
            source_name: name,
            source,
        });
        Ok(())
    }
    pub fn propose_agent(&self, source: String) -> Result<JobView, String> {
        self.propose_agent_bound(source, None)
    }
    pub fn propose_tool_agent(
        &self,
        project: Uuid,
        source_name: String,
        source_sha256: String,
        base_job: Option<Uuid>,
        source: String,
    ) -> Result<JobView, String> {
        self.propose_agent_bound(
            source,
            Some((project, source_name, source_sha256, base_job)),
        )
    }
    fn propose_agent_bound(
        &self,
        source: String,
        inspected: Option<(Uuid, String, String, Option<Uuid>)>,
    ) -> Result<JobView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let scope=Self::source_scope(&state)?;
        let original = if let Some((id, _, _, Some(job))) = &inspected {
            let project = state
                .project
                .as_ref()
                .filter(|project| project.id == *id)
                .ok_or("El proyecto cambió. Conecta el agente de nuevo.")?;
            let history = project.history().map_err(|e| e.to_string())?;
            let run = history
                .iter()
                .find(|run| run.input.job_id() == *job && run.phase == Phase::Completed)
                .ok_or("Selecciona un sketch publicado y verificado en este proyecto.")?;
            let task=crate::tasks::Store::new(project).and_then(|s|s.active()).map_err(|_|"No se pudo comprobar la conversación del resultado.")?;
            if task.as_ref().is_some_and(|t|run.audit.as_ref().and_then(|a|a.task.as_ref()).map_or(!t.legacy,|binding|binding.id!=t.id)){return Err("El sketch verificado pertenece a otra conversación; vuelve a ella antes de usarlo.".into());}
            let name = match &run.input {
                OperationInput::Agent(input) => input.source_name.clone(),
                OperationInput::Arduino(_) => "Sketch.ino".into(),
                _ => return Err("La operación no produjo un sketch editable.".into()),
            };
            let source = project
                .artifact(*job, ArtifactFormat::Sketch)
                .and_then(|artifact| artifact.read_text())
                .map_err(|e| e.to_string())?;
            AgentSource {
                project_id: project.id,
                project_root:project.root.clone(),
                task_id:task.map(|t|t.id),
                source_name: name,
                source,
            }
        } else {
            self.agent_source
                .lock()
                .map_err(|e| e.to_string())?
                .clone()
                .filter(|original| {
                    scope.as_ref().is_some_and(|(id,root,task)|*id==original.project_id && *root==original.project_root && *task==original.task_id)
                })
                .ok_or("Seleccione un sketch original en este proyecto.")?
        };
        if inspected.as_ref().is_some_and(|(project, name, sha, _)| {
            *project != original.project_id
                || *name != original.source_name
                || *sha != original.sha256()
        }) {
            return Err(
                "El sketch seleccionado cambió. Inspecciónalo de nuevo antes de proponer.".into(),
            );
        }
        let input = agent::EditInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            agent_version: agent::VERSION.into(),
            source_name: original.source_name,
            original_source: original.source,
            source,
        };
        input.validate()?;
        let origin = if inspected.is_some() {
            RunSource::Mcp
        } else {
            RunSource::Manual
        };
        Self::propose_operation_locked(&mut state, input.into(), origin)
    }
    pub fn edit_agent(&self, id: Uuid, source: String) -> Result<JobView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        let job = state.job.as_mut().ok_or("No hay una propuesta de Agent.")?;
        if job.view.snapshot.phase != Phase::AwaitingApproval
            || job.view.snapshot.input.job_id() != id
        {
            return Err("La propuesta de Agent ya no está disponible.".into());
        }
        let mut snapshot = job.view.snapshot.clone();
        let OperationInput::Agent(input) = &mut snapshot.input else {
            return Err("Seleccione la operación de Agent.".into());
        };
        input.source = source;
        input.validate()?;
        snapshot.advance();
        snapshot.audit.as_mut().unwrap().user_edited = true;
        project.save(&snapshot).map_err(|e| e.to_string())?;
        job.view.snapshot = snapshot;
        job.view.approval_id = Some(Uuid::new_v4());
        job.view.sequence += 1;
        Ok(job.view.clone())
    }

    pub fn set_arduino(&self, executable: PathBuf, data: PathBuf) -> Result<(), String> {
        if !executable.is_absolute()
            || !executable.is_file()
            || !data.is_absolute()
            || !data.is_dir()
            || !executable
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("arduino-cli.exe"))
        {
            return Err(
                "Seleccione arduino-cli.exe y la carpeta data de su toolchain privado.".into(),
            );
        }
        let marker_path = data.join("hazlomexa-toolchain.json");
        let file = fs::File::open(&marker_path).map_err(|_| "Seleccione una carpeta data preparada para Hazlo Mexa, no su toolchain global de Arduino.".to_string())?;
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4096 {
            return Err("La identificación del toolchain no es válida.".into());
        }
        let marker: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| "La identificación del toolchain no es válida.".to_string())?;
        if marker
            != serde_json::json!({"schema_version":1,"owner":"HazloMexa","cli_version":arduino::CLI_VERSION,"core_version":arduino::CORE_VERSION})
        {
            return Err("La versión del toolchain privado no es compatible.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| job.view.busy) {
            return Err("Espere a que termine la operación actual.".into());
        }
        let project = state.project.clone();
        if let Some(job) = state.job.as_mut() {
            if job.view.snapshot.phase == Phase::AwaitingApproval
                && matches!(&job.view.snapshot.input, OperationInput::Arduino(_))
            {
                let mut view = job.view.clone();
                view.advance();
                project
                    .as_ref()
                    .ok_or("Seleccione un proyecto.")?
                    .save(&view.snapshot)
                    .map_err(|e| e.to_string())?;
                view.approval_id = Some(Uuid::new_v4());
                job.view = view;
            }
        }
        *self.arduino.lock().map_err(|e| e.to_string())? =
            Some(ArduinoRuntime { executable, data });
        Ok(())
    }

    pub fn propose_compile(&self, fqbn: String, source: String) -> Result<JobView, String> {
        let input = arduino::CompileInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            fqbn,
            source,
        };
        input.validate()?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let proposal = Self::propose_operation_locked(&mut state, input.into(), RunSource::Manual)?;
        state.inference = None;
        Ok(proposal)
    }
    pub fn propose_tool_solid(
        &self,
        project_id: Uuid,
        dimensions: Dimensions,
    ) -> Result<JobView, String> {
        self.propose_tool_input(project_id, crate::SolidInput::new(dimensions)?.into())
    }
    pub fn propose_tool_compile(
        &self,
        project_id: Uuid,
        fqbn: String,
        source: String,
    ) -> Result<JobView, String> {
        self.propose_tool_input(
            project_id,
            arduino::CompileInput {
                schema_version: 1,
                job_id: Uuid::new_v4(),
                fqbn,
                source,
            }
            .into(),
        )
    }
    fn propose_tool_input(
        &self,
        project_id: Uuid,
        input: OperationInput,
    ) -> Result<JobView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().is_none_or(|p| p.id != project_id) {
            return Err("El proyecto cambió. Conecta el agente de nuevo.".into());
        }
        let proposal = Self::propose_operation_locked(&mut state, input, RunSource::Mcp)?;
        state.inference = None;
        Ok(proposal)
    }

    pub fn cadcraft_runtime(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.cadcraft.lock().map_err(|e| e.to_string())?.clone())
    }
    pub fn kicad_runtime(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.kicad.lock().map_err(|e| e.to_string())?.clone())
    }
    pub fn konnect_runtime(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.konnect.lock().map_err(|e| e.to_string())?.clone())
    }
    pub(crate) fn read_live_pcb(
        &self,
        board: &std::path::Path,
        connection: &crate::kicad_live::Connection,
        cancel: Arc<AtomicBool>,
    ) -> Result<crate::kicad_live::Observation, String> {
        connection.validate()?;
        crate::project::valid_location(board).map_err(|_| "Selecciona una PCB local válida.")?;
        if board.extension().and_then(|x| x.to_str()) != Some("kicad_pcb") {
            return Err("Selecciona una PCB KiCad guardada que esté abierta en el editor.".into());
        }
        let parent = board.parent().ok_or("Falta la carpeta de la PCB.")?;
        let _source_pins = crate::project::pin_lineage(parent).map_err(|e| e.to_string())?;
        drop(kicad_project::open_regular(
            board,
            kicad_project::FILE_LIMIT,
        )?);
        let project = self.view()?.project.ok_or("Selecciona un proyecto.")?;
        let profile = self.profiles.join(format!("live-pcb-read-{}", Uuid::new_v4()));
        fs::create_dir(&profile).map_err(|_| "No se pudo preparar la lectura nativa.")?;
        let observation = crate::kicad_live::inspect(board, connection, &profile, cancel.clone())?;
        if cancel.load(Ordering::SeqCst) || self.view()?.project != Some(project) {
            return Err("El proyecto o la conexión cambiaron durante la lectura; vuelve a conectar.".into());
        }
        Ok(observation)
    }

    pub fn set_konnect(&self, executable: PathBuf) -> Result<(), String> {
        konnect_run::verify_runtime(&executable)?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|j| j.view.busy || j.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta antes de cambiar Konnect.".into());
        }
        *self.konnect.lock().map_err(|e| e.to_string())? = Some(executable);
        Ok(())
    }
    pub fn propose_konnect_pcb(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        pcb_move: crate::konnect::FootprintMove,
    ) -> Result<JobView, String> {
        self.propose_konnect_pcb_bound(root, entry, units, pcb_move, None, RunSource::Manual)
    }
    pub fn propose_discovered_konnect_pcb(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        pcb_move: crate::konnect::FootprintMove,
        project_id: Uuid,
        sha256: String,
    ) -> Result<JobView, String> {
        self.propose_konnect_pcb_bound(
            root,
            entry,
            units,
            pcb_move,
            Some((project_id, sha256)),
            RunSource::Manual,
        )
    }
    pub fn propose_tool_konnect_pcb(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        pcb_move: crate::konnect::FootprintMove,
        project_id: Uuid,
        sha256: String,
    ) -> Result<JobView, String> {
        self.propose_konnect_pcb_bound(
            root,
            entry,
            units,
            pcb_move,
            Some((project_id, sha256)),
            RunSource::Mcp,
        )
    }
    fn propose_konnect_pcb_bound(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        pcb_move: crate::konnect::FootprintMove,
        discovery: Option<(Uuid, String)>,
        origin: RunSource,
    ) -> Result<JobView, String> {
        pcb_move.validate()?;
        // KiCad stores positions at nanometer precision; persist that exact decimal request.
        let mut pcb_move = pcb_move;
        pcb_move.x_mm =
            serde_json::from_str(&format!("{:.6}", pcb_move.x_mm)).map_err(|e| e.to_string())?;
        pcb_move.y_mm =
            serde_json::from_str(&format!("{:.6}", pcb_move.y_mm)).map_err(|e| e.to_string())?;
        let runtime = self
            .konnect_runtime()?
            .ok_or("Seleccione Konnect antes de importar.")?;
        let cli = self
            .kicad_runtime()?
            .ok_or("Seleccione KiCad CLI antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if discovery.as_ref().is_some_and(|(id, _)| *id != project.id) {
            return Err("El proyecto cambió. Selecciona la PCB de nuevo.".into());
        }
        drop(state);
        let id = Uuid::new_v4();
        let source = kicad_project::capture(
            &root,
            &entry,
            &project.captured_source(id),
            id,
            units,
            Arc::new(AtomicBool::new(false)),
        )?;
        let input = crate::konnect_pcb::Input {
            schema_version: 1,
            job_id: id,
            konnect_version: konnect_run::VERSION.into(),
            source,
            pcb_move,
        };
        input.validate()?;
        if let Some((_, sha256)) = discovery {
            let captured = kicad_project::read_regular(
                &project
                    .captured_source(id)
                    .join(kicad_project::relative(&input.source.entry)?),
                kicad_project::FILE_LIMIT,
            )?;
            if kicad_project::digest(&captured) != sha256 {
                return Err(
                    "La PCB cambió. Inspecciónala de nuevo antes de preparar el movimiento.".into(),
                );
            }
        }
        crate::konnect::validate_current_position(
            &kicad_project::read_regular(
                &project
                    .captured_source(id)
                    .join(kicad_project::relative(&input.source.entry)?),
                kicad_project::FILE_LIMIT,
            )?,
            &input.pcb_move,
        )?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().is_none_or(|p| p.id != project.id)
            || self.konnect_runtime()?.as_ref() != Some(&runtime)
            || self.kicad_runtime()?.as_ref() != Some(&cli)
        {
            return Err("El proyecto o runtime cambió durante la importación.".into());
        }
        Self::propose_operation_locked(&mut state, OperationInput::KonnectPcb(input), origin)
    }
    pub fn propose_konnect(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        edit: crate::konnect::ValueEdit,
    ) -> Result<JobView, String> {
        self.propose_konnect_bound(root, entry, units, edit, None, RunSource::Manual)
    }
    pub fn propose_discovered_konnect(
        &self,
        root: PathBuf,
        entry: PathBuf,
        edit: crate::konnect::ValueEdit,
        project: Uuid,
        sha256: String,
    ) -> Result<JobView, String> {
        self.propose_konnect_bound(
            root,
            entry,
            crate::kicad::Units::Mm,
            edit,
            Some((project, sha256)),
            RunSource::Manual,
        )
    }
    pub fn propose_tool_konnect(
        &self,
        root: PathBuf,
        entry: PathBuf,
        edit: crate::konnect::ValueEdit,
        project: Uuid,
        sha256: String,
    ) -> Result<JobView, String> {
        self.propose_konnect_bound(
            root,
            entry,
            crate::kicad::Units::Mm,
            edit,
            Some((project, sha256)),
            RunSource::Mcp,
        )
    }
    pub fn propose_tool_konnect_result(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        reference: String,
        value: String,
    ) -> Result<JobView, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let project = state
            .project
            .as_ref()
            .filter(|p| p.id == project_id)
            .ok_or("El proyecto cambió. Conecta el agente de nuevo.")?;
        let history = project.history().map_err(|e| e.to_string())?;
        let run = history
            .iter()
            .find(|run| run.input.job_id() == base_job && run.phase == Phase::Completed)
            .ok_or("Lee un resultado de esquema publicado y verificado primero.")?;
        let OperationInput::Konnect(input) = &run.input else {
            return Err("La operación no produjo un esquema Konnect editable.".into());
        };
        let report = project
            .artifact(base_job, ArtifactFormat::KonnectReport)
            .map_err(|e| e.to_string())?;
        let content = report.read_text().map_err(|e| e.to_string())?;
        if kicad_project::digest(content.as_bytes()) != result_sha256 {
            return Err("El resultado cambió. Lee de nuevo el esquema verificado.".into());
        }
        let result: konnect_run::KonnectResult =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let directory = project.artifact_directory(base_job);
        let mut pins = Vec::new();
        // Keep every verified edited dependency unchanged while capturing the next copy.
        for file in result
            .files
            .iter()
            .filter(|file| file.path.starts_with("Edited/"))
        {
            let path = directory.join(kicad_project::relative(&file.path)?);
            pins.push(kicad_project::open_regular(
                &path,
                kicad_project::FILE_LIMIT,
            )?);
            if kicad_project::digest(&kicad_project::read_regular(
                &path,
                kicad_project::FILE_LIMIT,
            )?) != file.sha256
            {
                return Err(
                    "El esquema o sus dependencias cambiaron. Lee de nuevo el resultado.".into(),
                );
            }
        }
        let source = project
            .artifact(base_job, ArtifactFormat::KonnectEdited)
            .map_err(|e| e.to_string())?;
        let (sha256, components) = crate::konnect::inspect_file(&source.path)?;
        let expected_value = components
            .iter()
            .find(|component| component.reference == reference)
            .ok_or("El componente no pertenece al esquema verificado.")?
            .value
            .clone();
        let units = input.source.units;
        drop(state);
        self.propose_konnect_bound(
            directory.join("Edited"),
            source.path.clone(),
            units,
            crate::konnect::ValueEdit {
                reference,
                expected_value,
                value,
            },
            Some((project_id, sha256)),
            RunSource::Mcp,
        )
    }
    fn propose_konnect_bound(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        edit: crate::konnect::ValueEdit,
        discovery: Option<(Uuid, String)>,
        origin: RunSource,
    ) -> Result<JobView, String> {
        edit.validate()?;
        let runtime = self
            .konnect_runtime()?
            .ok_or("Seleccione Konnect antes de importar.")?;
        let cli = self
            .kicad_runtime()?
            .ok_or("Seleccione KiCad CLI antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|j| {
            j.view.busy
                || matches!(
                    j.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if discovery.as_ref().is_some_and(|(id, _)| *id != project.id) {
            return Err("El proyecto cambió. Selecciona el esquema de nuevo.".into());
        }
        drop(state);
        let id = Uuid::new_v4();
        let source = kicad_project::capture(
            &root,
            &entry,
            &project.captured_source(id),
            id,
            units,
            Arc::new(AtomicBool::new(false)),
        )?;
        let input = konnect_run::KonnectInput {
            schema_version: 2,
            job_id: id,
            konnect_version: konnect_run::VERSION.into(),
            source,
            edit,
        };
        input.validate()?;
        if let Some((_, expected_sha)) = discovery {
            let (actual_sha, _) = crate::konnect::inspect_file(
                &project
                    .captured_source(id)
                    .join(kicad_project::relative(&input.source.entry)?),
            )?;
            if actual_sha != expected_sha {
                return Err(
                    "El esquema cambió. Selecciónalo de nuevo antes de preparar el cambio.".into(),
                );
            }
        }
        crate::konnect::validate_current_value(
            &kicad_project::read_regular(
                &project
                    .captured_source(id)
                    .join(kicad_project::relative(&input.source.entry)?),
                kicad_project::FILE_LIMIT,
            )?,
            &input.edit,
        )?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().is_none_or(|p| p.id != project.id)
            || self.konnect_runtime()?.as_ref() != Some(&runtime)
            || self.kicad_runtime()?.as_ref() != Some(&cli)
        {
            return Err("El proyecto o runtime cambió durante la importación.".into());
        }
        Self::propose_operation_locked(&mut state, OperationInput::Konnect(input), origin)
    }
    pub fn propose_konnect_value(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        reference: String,
        value: String,
    ) -> Result<JobView, String> {
        let expected_value = crate::konnect::current_value(
            &kicad_project::read_regular(&entry, kicad_project::FILE_LIMIT)?,
            &reference,
        )?;
        self.propose_konnect(
            root,
            entry,
            units,
            crate::konnect::ValueEdit {
                reference,
                expected_value,
                value,
            },
        )
    }
    pub fn orca_runtime(&self) -> Result<Option<PathBuf>, String> {
        Ok(self.orca.lock().map_err(|e| e.to_string())?.clone())
    }
    pub fn set_orca(&self, executable: PathBuf) -> Result<(), String> {
        orca::verify_runtime(&executable)?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|j| j.view.busy || j.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta antes de cambiar OrcaSlicer.".into());
        }
        *self.orca.lock().map_err(|e| e.to_string())? = Some(executable);
        Ok(())
    }
    pub fn propose_slice(
        &self,
        profile_root: PathBuf,
        model: PathBuf,
        profiles: [PathBuf; 3],
        request: orca::Request,
    ) -> Result<JobView, String> {
        self.propose_slice_bound(
            profile_root,
            model,
            profiles,
            request,
            None,
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub(crate) fn discover_slice(
        &self,
        profile_root: &std::path::Path,
        model: &std::path::Path,
        profiles: [PathBuf; 3],
        cancel: Arc<AtomicBool>,
    ) -> Result<orca::Discovery, String> {
        let project = self.view()?.project.ok_or("Selecciona un proyecto.")?;
        let output = self
            .profiles
            .join(format!("slice-discovery-{}", Uuid::new_v4()));
        let discovered = orca::discover(profile_root, model, profiles, &output, cancel)?;
        if self.view()?.project != Some(project) {
            return Err(
                "El proyecto cambió durante la lectura; selecciona las fuentes de nuevo.".into(),
            );
        }
        Ok(discovered)
    }
    pub(crate) fn propose_discovered_slice(
        &self,
        profile_root: PathBuf,
        model: PathBuf,
        profiles: [PathBuf; 3],
        request: orca::Request,
        binding: (Uuid, PathBuf, orca::Discovery),
        cancel: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        self.propose_slice_bound(
            profile_root,
            model,
            profiles,
            request,
            Some(binding),
            cancel,
        )
    }
    fn propose_slice_bound(
        &self,
        profile_root: PathBuf,
        model: PathBuf,
        profiles: [PathBuf; 3],
        request: orca::Request,
        binding: Option<(Uuid, PathBuf, orca::Discovery)>,
        cancel: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        orca::stop(&cancel)?;
        let runtime = self
            .orca_runtime()?
            .ok_or("Seleccione OrcaSlicer antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|j| {
            j.view.busy
                || matches!(
                    j.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if binding
            .as_ref()
            .is_some_and(|(id, root, _)| project.id != *id || project.root != *root)
        {
            return Err("El proyecto cambió; selecciona el modelo de nuevo.".into());
        }
        drop(state);
        let id = Uuid::new_v4();
        let input = orca::capture(
            &profile_root,
            &model,
            profiles,
            &project.captured_source(id),
            id,
            request,
            cancel.clone(),
        )?;
        orca::stop(&cancel)?;
        if binding
            .as_ref()
            .is_some_and(|(_, _, discovery)| !discovery.matches_capture(&input))
        {
            return Err("El modelo o sus perfiles cambiaron. Selecciónalos e inspecciónalos de nuevo antes de proponer.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .project
            .as_ref()
            .is_none_or(|p| p.id != project.id || p.root != project.root)
            || self.orca_runtime()?.as_ref() != Some(&runtime)
        {
            return Err("El proyecto cambió durante la importación.".into());
        }
        let origin = if binding.is_some() {
            RunSource::Mcp
        } else {
            RunSource::Manual
        };
        Self::propose_operation_locked(&mut state, OperationInput::Orca(input), origin)
    }
    pub fn propose_cadpilot(
        &self,
        source: PathBuf,
        addon: PathBuf,
        request: cadpilot::Request,
    ) -> Result<JobView, String> {
        self.propose_cadpilot_bound(
            source,
            addon,
            request,
            None,
            RunSource::Manual,
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub(crate) fn discover_cadpilot(
        &self,
        source: &std::path::Path,
        addon: &std::path::Path,
        cancelled: Arc<AtomicBool>,
    ) -> Result<cadpilot::Discovery, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancela o termina la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Selecciona un proyecto.")?;
        let executable = self.executable.lock().map_err(|e| e.to_string())?.clone();
        drop(state);
        let id = Uuid::new_v4();
        let directory = self.profiles.join(format!("cadpilot-discovery-{id}"));
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let input = cadpilot::capture(
            source,
            addon,
            &directory.join("source"),
            id,
            cadpilot::Request {
                operation: cadpilot::Operation::Inspect,
                object: "Document".into(),
                units: orca::Units::Mm,
                parameters: Default::default(),
            },
            cancelled.clone(),
        )?;
        let stage = directory.join("stage");
        fs::create_dir(&stage).map_err(|e| e.to_string())?;
        let result = cadpilot::run(
            &executable,
            &directory.join("source"),
            &stage,
            &directory.join("native"),
            &input,
            cancelled.clone(),
        )?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        if cancelled.load(Ordering::SeqCst)
            || state
                .project
                .as_ref()
                .is_none_or(|p| p.id != project.id || p.root != project.root)
            || *self.executable.lock().map_err(|e| e.to_string())? != executable
        {
            return Err("El proyecto o FreeCAD cambió, o la inspección se detuvo. Selecciona el documento de nuevo.".into());
        }
        Ok(cadpilot::Discovery {
            source_name: input.source_name,
            source_sha256: result.source_sha256,
            preview: cadpilot::Preview {
                catalog: result.catalog,
                measurements: result.before,
            },
        })
    }
    pub(crate) fn propose_selected_cadpilot(
        &self,
        source: PathBuf,
        addon: PathBuf,
        request: cadpilot::Request,
        binding: (Uuid, PathBuf, String),
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        self.propose_cadpilot_bound(
            source,
            addon,
            request,
            Some(binding),
            RunSource::Mcp,
            cancelled,
        )
    }
    pub fn propose_tool_slice(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        bed: orca::Bed,
        temperature_c: f64,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Importación cancelada.".into());
        }
        let runtime = self
            .orca_runtime()?
            .ok_or("Seleccione OrcaSlicer antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|j| {
            j.view.busy
                || matches!(
                    j.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state
            .project
            .as_ref()
            .filter(|p| p.id == project_id)
            .cloned()
            .ok_or("El proyecto cambió. Lee el resultado de nuevo.")?;
        let history = project.history().map_err(|e| e.to_string())?;
        let run = history
            .iter()
            .find(|s| s.input.job_id() == base_job && s.phase == Phase::Completed)
            .ok_or("Lee un laminado completado y verificado antes de preparar el cambio.")?;
        let OperationInput::Orca(previous) = &run.input else {
            return Err("El resultado no es un laminado OrcaSlicer.".into());
        };
        let report = project
            .artifact(base_job, ArtifactFormat::SliceReport)
            .and_then(|a| a.read_text())
            .map_err(|e| e.to_string())?;
        if kicad_project::digest(report.as_bytes()) != result_sha256 {
            return Err("El laminado cambió. Lee el resultado verificado de nuevo.".into());
        }
        let model = project
            .artifact(base_job, ArtifactFormat::OriginalMesh)
            .map_err(|e| e.to_string())?;
        drop(state);
        let id = Uuid::new_v4();
        let input = orca::recapture(
            model.path.parent().ok_or("Falta la carpeta del modelo.")?,
            previous,
            &project.captured_source(id),
            id,
            bed,
            temperature_c,
            cancelled.clone(),
        )?;
        if cancelled.load(Ordering::SeqCst) {
            return Err("Importación cancelada.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().is_none_or(|p| p.id != project_id)
            || self.orca_runtime()?.as_ref() != Some(&runtime)
        {
            return Err("El proyecto o runtime cambió durante la captura.".into());
        }
        Self::propose_operation_locked(&mut state, OperationInput::Orca(input), RunSource::Mcp)
    }
    pub fn propose_tool_cadpilot(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        request: cadpilot::Request,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        self.propose_cadpilot_result(
            project_id,
            base_job,
            result_sha256,
            request,
            RunSource::Mcp,
            cancelled,
        )
    }
    pub fn propose_cadpilot_followup(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        request: cadpilot::Request,
    ) -> Result<JobView, String> {
        self.propose_cadpilot_result(
            project_id,
            base_job,
            result_sha256,
            request,
            RunSource::Manual,
            Arc::new(AtomicBool::new(false)),
        )
    }
    fn propose_cadpilot_result(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        request: cadpilot::Request,
        origin: RunSource,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        request.validate()?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let project = state
            .project
            .as_ref()
            .filter(|project| project.id == project_id)
            .ok_or("El proyecto cambió. Conecta el agente de nuevo.")?;
        let history = project.history().map_err(|e| e.to_string())?;
        let run = history
            .iter()
            .find(|run| run.input.job_id() == base_job && run.phase == Phase::Completed)
            .ok_or("Inspecciona y verifica el documento en Hazlo antes de proponer un cambio.")?;
        let OperationInput::Cadpilot(input) = &run.input else {
            return Err("Selecciona un resultado CADPilot medido y verificado.".into());
        };
        let report = project
            .artifact(base_job, ArtifactFormat::CadpilotReport)
            .and_then(|artifact| artifact.read_text())
            .map_err(|e| e.to_string())?;
        if kicad_project::digest(report.as_bytes()) != result_sha256 {
            return Err("La medición cambió. Lee de nuevo el resultado verificado.".into());
        }
        let format = if input.request.operation == cadpilot::Operation::Inspect {
            ArtifactFormat::OriginalCad
        } else {
            ArtifactFormat::Fcstd
        };
        let source = project
            .artifact(base_job, format)
            .map_err(|e| e.to_string())?;
        let source_hash = kicad_project::digest(&kicad_project::read_regular(
            &source.path,
            32 * 1024 * 1024,
        )?);
        let addon = project.captured_source(base_job).join("addon");
        let project_root = project.root.clone();
        drop(state);
        self.propose_cadpilot_bound(
            source.path.clone(),
            addon,
            request,
            Some((project_id, project_root, source_hash)),
            origin,
            cancelled,
        )
    }
    fn propose_cadpilot_bound(
        &self,
        source: PathBuf,
        addon: PathBuf,
        request: cadpilot::Request,
        binding: Option<(Uuid, PathBuf, String)>,
        origin: RunSource,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Inspección cancelada.".into());
        }
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|j| {
            j.view.busy
                || matches!(
                    j.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if binding
            .as_ref()
            .is_some_and(|(id, root, _)| *id != project.id || *root != project.root)
        {
            return Err("El proyecto cambió. Conecta el agente de nuevo.".into());
        }
        let executable = self.executable.lock().map_err(|e| e.to_string())?.clone();
        drop(state);
        let id = Uuid::new_v4();
        let mut input = cadpilot::capture(
            &source,
            &addon,
            &project.captured_source(id),
            id,
            request,
            cancelled.clone(),
        )?;
        if binding.as_ref().is_some_and(|(_, _, hash)| {
            input
                .files
                .iter()
                .find(|file| file.path == "Original.FCStd")
                .is_none_or(|file| &file.sha256 != hash)
        }) {
            return Err("El documento medido cambió. Inspecciónalo de nuevo.".into());
        }
        let mut inspecting = input.clone();
        inspecting.request.operation = cadpilot::Operation::Inspect;
        inspecting.request.parameters.clear();
        let preview_directory = project
            .root
            .join(".hazlomexa/runs")
            .join(format!("{id}.preview"));
        fs::create_dir(&preview_directory).map_err(|e| e.to_string())?;
        let preview = cadpilot::run(
            &executable,
            &project.captured_source(id),
            &preview_directory,
            &self.profiles.join(format!("{id}.preview")),
            &inspecting,
            cancelled.clone(),
        )?;
        let name = &input.request.object;
        match input.request.operation {
            cadpilot::Operation::EditBox
                if preview.catalog.get(name).map(String::as_str) != Some("Part::Box") =>
            {
                return Err("Seleccione una caja nativa del catálogo medido.".into())
            }
            cadpilot::Operation::AddBox | cadpilot::Operation::AddCylinder
                if preview.catalog.contains_key(name) =>
            {
                return Err("El nombre del nuevo objeto ya existe.".into())
            }
            _ => (),
        }
        if matches!(
            input.request.operation,
            cadpilot::Operation::AddBox | cadpilot::Operation::AddCylinder
        ) && preview.catalog.len() >= 32
        {
            return Err("La copia ya tiene el máximo de 32 objetos para este flujo.".into());
        }
        input.preview = Some(cadpilot::Preview {
            catalog: preview.catalog,
            measurements: preview.before,
        });
        if cancelled.load(Ordering::SeqCst) {
            return Err("Inspección cancelada; no se preparó una propuesta.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .project
            .as_ref()
            .is_none_or(|p| p.id != project.id || p.root != project.root)
            || *self.executable.lock().map_err(|e| e.to_string())? != executable
        {
            return Err(
                "El proyecto o FreeCAD cambió durante la inspección; prepare otra propuesta."
                    .into(),
            );
        }
        Self::propose_operation_locked(&mut state, OperationInput::Cadpilot(input), origin)
    }
    pub fn set_kicad(&self, executable: PathBuf) -> Result<(), String> {
        kicad_run::verify_runtime(&executable)?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|job| job.view.busy || job.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta antes de cambiar KiCad.".into());
        }
        *self.kicad.lock().map_err(|e| e.to_string())? = Some(executable);
        Ok(())
    }
    pub fn propose_kicad(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
    ) -> Result<JobView, String> {
        self.propose_kicad_bound(
            root,
            entry,
            units,
            None,
            RunSource::Manual,
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub fn propose_tool_kicad(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        units: crate::kicad::Units,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let project = state
            .project
            .as_ref()
            .filter(|p| p.id == project_id)
            .ok_or("El proyecto cambió. Lee el resultado de nuevo.")?;
        let project_root = project.root.clone();
        let history = project.history().map_err(|e| e.to_string())?;
        let run = history
            .iter()
            .find(|s| s.input.job_id() == base_job && s.phase == Phase::Completed)
            .ok_or("Lee un esquema o PCB completado y verificado antes de preparar la revisión.")?;
        let (source, report_format, file_format) = match &run.input {
            OperationInput::Kicad(input) => (
                input,
                ArtifactFormat::KicadReport,
                ArtifactFormat::KicadSource,
            ),
            OperationInput::Konnect(input) => (
                &input.source,
                ArtifactFormat::KonnectReport,
                ArtifactFormat::KonnectEdited,
            ),
            OperationInput::KonnectPcb(input) => (
                &input.source,
                ArtifactFormat::KonnectReport,
                ArtifactFormat::KonnectEdited,
            ),
            _ => return Err("El resultado no es un esquema o PCB KiCad editable.".into()),
        };
        let report = project
            .artifact(base_job, report_format)
            .and_then(|a| a.read_text())
            .map_err(|e| e.to_string())?;
        if kicad_project::digest(report.as_bytes()) != result_sha256 {
            return Err("El informe cambió. Lee el resultado verificado de nuevo.".into());
        }
        let entry = project
            .artifact(base_job, file_format)
            .map_err(|e| e.to_string())?;
        let mut root = entry.path.clone();
        for _ in kicad_project::relative(&source.entry)?.components() {
            root.pop();
        }
        let expected: Vec<_> = source
            .files
            .iter()
            .map(|file| {
                let bytes = kicad_project::read_regular(
                    &root.join(kicad_project::relative(&file.path)?),
                    kicad_project::FILE_LIMIT,
                )?;
                Ok(kicad_project::CapturedFile {
                    path: file.path.clone(),
                    sha256: kicad_project::digest(&bytes),
                    bytes: bytes.len() as u64,
                })
            })
            .collect::<Result<_, String>>()?;
        drop(state);
        self.propose_kicad_bound(
            root,
            entry.path.clone(),
            units,
            Some((project_id, project_root, expected)),
            RunSource::Mcp,
            cancelled,
        )
    }
    pub(crate) fn discover_kicad(
        &self,
        root: &std::path::Path,
        entry: &std::path::Path,
        cancelled: Arc<AtomicBool>,
    ) -> Result<kicad_project::KicadInput, String> {
        let project = self.view()?.project.ok_or("Selecciona un proyecto.")?;
        let input = kicad_project::inspect(root, entry, cancelled)?;
        if self.view()?.project != Some(project) {
            return Err(
                "El proyecto cambió durante la lectura. Selecciona la fuente de nuevo.".into(),
            );
        }
        Ok(input)
    }
    pub(crate) fn propose_selected_kicad(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        binding: (Uuid, PathBuf, Vec<kicad_project::CapturedFile>),
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        self.propose_kicad_bound(root, entry, units, Some(binding), RunSource::Mcp, cancelled)
    }
    fn propose_kicad_bound(
        &self,
        root: PathBuf,
        entry: PathBuf,
        units: crate::kicad::Units,
        binding: Option<(Uuid, PathBuf, Vec<kicad_project::CapturedFile>)>,
        origin: RunSource,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Importación cancelada.".into());
        }
        let runtime = self
            .kicad_runtime()?
            .ok_or("Seleccione KiCad CLI antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if binding
            .as_ref()
            .is_some_and(|(id, folder, _)| *id != project.id || *folder != project.root)
        {
            return Err("El proyecto cambió. Lee el resultado de nuevo.".into());
        }
        drop(state);
        let id = Uuid::new_v4();
        let input = kicad_project::capture(
            &root,
            &entry,
            &project.captured_source(id),
            id,
            units,
            cancelled.clone(),
        )?;
        if cancelled.load(Ordering::SeqCst) {
            return Err("Importación cancelada.".into());
        }
        if binding
            .as_ref()
            .is_some_and(|(_, _, expected)| *expected != input.files)
        {
            return Err(
                "Las fuentes cambiaron durante la captura. Lee el resultado de nuevo.".into(),
            );
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .project
            .as_ref()
            .is_none_or(|v| v.id != project.id || v.root != project.root)
            || self.kicad_runtime()?.as_ref() != Some(&runtime)
        {
            return Err("El proyecto cambió durante la importación.".into());
        }
        Self::propose_operation_locked(&mut state, OperationInput::Kicad(input), origin)
    }
    pub fn kicad_summary(&self, job: Uuid) -> Result<crate::kicad::CheckSummary, String> {
        let project = self
            .state
            .lock()
            .map_err(|e| e.to_string())?
            .project
            .clone()
            .ok_or("Seleccione un proyecto.")?;
        project.kicad_summary(job).map_err(|e| e.to_string())
    }

    pub fn set_cadcraft(&self, executable: PathBuf) -> Result<(), String> {
        cadcraft::verify_runtime(&executable)?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|job| job.view.busy || job.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta antes de cambiar CADCraft.".into());
        }
        *self.cadcraft.lock().map_err(|e| e.to_string())? = Some(executable);
        Ok(())
    }

    pub fn propose_drawing(
        &self,
        source: PathBuf,
        request: cadcraft::Request,
    ) -> Result<JobView, String> {
        self.propose_drawing_bound(
            source,
            request,
            None,
            RunSource::Manual,
            Arc::new(AtomicBool::new(false)),
        )
    }
    pub fn propose_tool_drawing(
        &self,
        project_id: Uuid,
        base_job: Uuid,
        result_sha256: String,
        request: cadcraft::Request,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        let state = self.state.lock().map_err(|e| e.to_string())?;
        let project = state
            .project
            .as_ref()
            .filter(|p| p.id == project_id)
            .ok_or("El proyecto cambió. Conecta el agente de nuevo.")?;
        let history = project.history().map_err(|e| e.to_string())?;
        let run = history
            .iter()
            .find(|run| run.input.job_id() == base_job && run.phase == Phase::Completed)
            .ok_or("Lee un dibujo completado y verificado antes de preparar el cambio.")?;
        let OperationInput::Drawing(input) = &run.input else {
            return Err("El resultado no es un dibujo CADCraft.".into());
        };
        if request.units != input.units {
            return Err("Conserva las unidades del dibujo verificado; convierte tus medidas antes de proponer.".into());
        }
        let report = project
            .artifact(base_job, ArtifactFormat::DrawingReport)
            .and_then(|artifact| artifact.read_text())
            .map_err(|e| e.to_string())?;
        if kicad_project::digest(report.as_bytes()) != result_sha256 {
            return Err("La huella del informe no coincide. Lee read_verified_result y usa su sha256 principal, no el output_sha256 del dibujo.".into());
        }
        if input.output_format == cadcraft::Format::Png {
            return Err(
                "Una imagen PNG no es un dibujo editable. Selecciona un resultado DXF o DWG."
                    .into(),
            );
        }
        let source = project
            .artifact(base_job, ArtifactFormat::Drawing)
            .map_err(|e| e.to_string())?;
        let hash = kicad_project::digest(&cadcraft::read_file(&source.path)?);
        let project_root = project.root.clone();
        drop(state);
        self.propose_drawing_bound(
            source.path.clone(),
            request,
            Some((project_id, project_root, hash)),
            RunSource::Mcp,
            cancelled,
        )
    }
    pub fn discover_drawing(
        &self,
        source: &std::path::Path,
        cancel: Arc<AtomicBool>,
    ) -> Result<cadcraft::Inspection, String> {
        self.discover_drawing_context(source, false, cancel)
            .map(|(inspection, _)| inspection)
    }
    pub(crate) fn inspect_drawing_tools(
        &self,
        source: &std::path::Path,
        cancel: Arc<AtomicBool>,
    ) -> Result<serde_json::Value, String> {
        let (inspection, native) = self.discover_drawing_context(source, true, cancel)?;
        Ok(
            serde_json::json!({"inspection":inspection,"native":native.ok_or("Falta el contexto nativo.")?}),
        )
    }
    fn discover_drawing_context(
        &self,
        source: &std::path::Path,
        include_tools: bool,
        cancel: Arc<AtomicBool>,
    ) -> Result<(cadcraft::Inspection, Option<serde_json::Value>), String> {
        let executable = self
            .cadcraft_runtime()?
            .ok_or("Seleccione CADCraft CLI antes de inspeccionar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state
            .project
            .as_ref()
            .ok_or("Seleccione un proyecto.")?
            .clone();
        drop(state);
        let profile = self
            .profiles
            .join(format!("drawing-discovery-{}", Uuid::new_v4()));
        fs::create_dir(&profile).map_err(|e| e.to_string())?;
        let saved = profile.join(source.file_name().ok_or("Falta el nombre del dibujo.")?);
        let inspected = cadcraft::discover(
            &executable,
            source,
            &saved,
            &profile.join("native"),
            cancel.clone(),
        )?;
        let native = if include_tools {
            Some(cadcraft::native_context(
                &executable,
                &saved,
                &profile.join("tools"),
                cancel,
            )?)
        } else {
            None
        };
        let state = self.state.lock().map_err(|e| e.to_string())?;
        if state
            .project
            .as_ref()
            .is_none_or(|current| current.id != project.id || current.root != project.root)
            || self.cadcraft_runtime()?.as_ref() != Some(&executable)
        {
            return Err("El proyecto o CADCraft cambió. Inspecciona de nuevo el dibujo.".into());
        }
        Ok((inspected, native))
    }

    pub(crate) fn propose_discovered_drawing(
        &self,
        source: PathBuf,
        request: cadcraft::Request,
        binding: (Uuid, PathBuf, String),
        origin: RunSource,
        cancel: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        self.propose_drawing_bound(source, request, Some(binding), origin, cancel)
    }

    fn propose_drawing_bound(
        &self,
        source: PathBuf,
        request: cadcraft::Request,
        binding: Option<(Uuid, PathBuf, String)>,
        origin: RunSource,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JobView, String> {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Inspección cancelada.".into());
        }
        let executable = self
            .cadcraft_runtime()?
            .ok_or("Seleccione CADCraft CLI antes de importar.")?;
        let state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        if binding
            .as_ref()
            .is_some_and(|(id, folder, _)| *id != project.id || *folder != project.root)
        {
            return Err("El proyecto cambió. Lee el dibujo de nuevo.".into());
        }
        drop(state);
        let id = Uuid::new_v4();
        let format = match source
            .extension()
            .and_then(|v| v.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("dxf") => cadcraft::Format::Dxf,
            Some("dwg") => cadcraft::Format::Dwg,
            _ => return Err("Seleccione un dibujo DXF o DWG.".into()),
        };
        let input = cadcraft::prepare(
            &executable,
            &source,
            &project.drawing_source(id, format),
            &self.profiles.join(format!("drawing-import-{id}")),
            request,
            id,
            cancelled.clone(),
        )?;
        if cancelled.load(Ordering::SeqCst) {
            return Err("Inspección cancelada.".into());
        }
        if binding
            .as_ref()
            .is_some_and(|(_, _, hash)| *hash != input.source_sha256)
        {
            return Err("El dibujo cambió durante la captura. Lee el resultado de nuevo.".into());
        }
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .project
            .as_ref()
            .is_none_or(|current| current.id != project.id || current.root != project.root)
            || self.cadcraft_runtime()?.as_ref() != Some(&executable)
        {
            return Err(
                "El proyecto cambió durante la importación. Prepare otra propuesta.".into(),
            );
        }
        Self::propose_operation_locked(&mut state, OperationInput::Drawing(input), origin)
    }

    pub fn edit_compile(
        &self,
        job_id: Uuid,
        fqbn: String,
        source: String,
    ) -> Result<JobView, String> {
        let input = arduino::CompileInput {
            schema_version: 1,
            job_id,
            fqbn,
            source,
        };
        input.validate()?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        let job = state
            .job
            .as_mut()
            .ok_or("No hay una operación pendiente.")?;
        if job.view.snapshot.input.job_id() != job_id
            || job.view.snapshot.phase != Phase::AwaitingApproval
            || !matches!(&job.view.snapshot.input, OperationInput::Arduino(_))
        {
            return Err("La propuesta de Arduino ya no está disponible para editar.".into());
        }
        let mut snapshot = job.view.snapshot.clone();
        snapshot.input = input.into();
        snapshot.advance();
        if let Some(audit) = &mut snapshot.audit {
            audit.user_edited = true;
        }
        project.save(&snapshot).map_err(|e| e.to_string())?;
        job.view.snapshot = snapshot;
        job.view.approval_id = Some(Uuid::new_v4());
        job.view.sequence += 1;
        Ok(job.view.clone())
    }

    pub fn gui_executable(&self) -> Result<PathBuf, String> {
        let command = self.executable.lock().map_err(|e| e.to_string())?;
        // The portable package's GUI is under bin; installed layouts may use a sibling executable.
        let portable_gui = command.with_file_name("bin").join("FreeCAD.exe");
        let gui = if portable_gui.is_file() {
            portable_gui
        } else {
            command.with_file_name("FreeCAD.exe")
        };
        if !gui.is_file() {
            return Err("Los archivos están guardados, pero falta FreeCAD.exe junto a FreeCADCmd.exe. Seleccione el paquete completo de FreeCAD 1.1.4.".into());
        }
        Ok(gui)
    }

    pub fn select_project(&self, project: Project) -> Result<Vec<Snapshot>, String> {
        let recovered = project.recover().map_err(|e| e.to_string())?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if let Some(job) = &state.job {
            job.cancelled.store(true, Ordering::SeqCst);
        }
        if let Some(inference) = state.inference.take() {
            inference.cancelled.store(true, Ordering::SeqCst);
        }
        state.project = Some(Arc::new(project));
        state.job = None;
        Ok(recovered)
    }

    pub fn reopen_current(&self, root: &std::path::Path) -> Result<Option<Vec<Snapshot>>, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let Some(project) = state
            .project
            .as_ref()
            .filter(|project| project.root == root)
        else {
            return Ok(None);
        };
        if state.job.as_ref().is_some_and(|job| job.view.busy) {
            return Err(
                "Espere a que termine la operación actual antes de reabrir el proyecto.".into(),
            );
        }
        let recovered = project.recover().map_err(|e| e.to_string())?;
        if let Some(job) = &state.job {
            job.cancelled.store(true, Ordering::SeqCst);
        }
        if let Some(inference) = state.inference.take() {
            inference.cancelled.store(true, Ordering::SeqCst);
        }
        state.job = None;
        Ok(Some(recovered))
    }

    pub fn propose(&self, dimensions: Dimensions) -> Result<JobView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let proposal = Self::propose_locked(&mut state, dimensions, RunSource::Manual)?;
        state.inference = None;
        Ok(proposal)
    }

    fn propose_locked(
        state: &mut State,
        dimensions: Dimensions,
        source: RunSource,
    ) -> Result<JobView, String> {
        Self::propose_operation_locked(state, crate::SolidInput::new(dimensions)?.into(), source)
    }

    fn propose_operation_locked(
        state: &mut State,
        input: OperationInput,
        source: RunSource,
    ) -> Result<JobView, String> {
        check_tool_scope(state)?;
        input.validate()?;
        Self::reconcile_pending_approval(state)?;
        if state.job.as_ref().is_some_and(|job| {
            job.view.busy
                || matches!(
                    job.view.snapshot.phase,
                    Phase::Running | Phase::AwaitingApproval
                )
        }) {
            return Err("Cancele o termine la operación actual primero.".into());
        }
        let project = state.project.as_ref().ok_or("Seleccione un proyecto.")?;
        if matches!(&input,OperationInput::NativeTools(_)|OperationInput::LiveCad(_)|OperationInput::LiveAgent(_)|OperationInput::LivePcb(_)|OperationInput::LiveDrawing(_)) && project.native_recovery(None).map_err(|e|e.to_string())?.iter().any(|j|j.needs_review) {return Err("Hay un lote nativo incompleto. Conserva los pasos confirmados y relee el programa con una consulta declarada de solo lectura; no se crearán nuevos lotes ni ediciones en vivo hasta reconciliar su estado.".into());}
        let mut snapshot = project
            .snapshot(input, Phase::AwaitingApproval)
            .map_err(|e| e.to_string())?;
        snapshot.audit.as_mut().unwrap().source = source;
        if matches!(snapshot.audit.as_ref().unwrap().source, RunSource::Mcp) {
            snapshot.schema_version = if matches!(snapshot.input,OperationInput::PublicDocument(_)) {
                18
            } else if matches!(snapshot.input, OperationInput::NativeTools(_)) {
                17
            } else if matches!(snapshot.input, OperationInput::LiveCad(_)) {
                13
            } else if matches!(snapshot.input, OperationInput::LiveDrawing(_)) {
                16
            } else if matches!(snapshot.input, OperationInput::LiveAgent(_)) {
                15
            } else if matches!(snapshot.input, OperationInput::LivePcb(_)) {
                14
            } else if matches!(snapshot.input, OperationInput::KonnectPcb(_)) {
                12
            } else {
                10
            };
        }
        project.save(&snapshot).map_err(|e| e.to_string())?;
        let view = JobView {
            snapshot,
            approval_id: Some(Uuid::new_v4()),
            sequence: 1,
            error: None,
            busy: false,
        };
        state.job = Some(Job {
            dispatch_gate: Arc::new(Mutex::new(())),
            dispatch_paused: Arc::new(AtomicBool::new(false)),
            live_agent: None,
            live_pcb: None,
            live_drawing: None,
            native_tools: None,
            view: view.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        Ok(view)
    }

    pub fn edit(&self, job_id: Uuid, dimensions: Dimensions) -> Result<JobView, String> {
        let dimensions = dimensions.validate()?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        let job = state
            .job
            .as_mut()
            .ok_or("No hay una operación pendiente.")?;
        if job.view.snapshot.input.job_id() != job_id
            || job.view.snapshot.phase != Phase::AwaitingApproval
        {
            return Err("La propuesta ya no está disponible para editar.".into());
        }
        let mut snapshot = job.view.snapshot.clone();
        match &mut snapshot.input {
            OperationInput::Freecad(input) => input.dimensions_mm = dimensions,
            _ => return Err("Seleccione la operación de FreeCAD para editar medidas.".into()),
        }
        snapshot.advance();
        if let Some(audit) = &mut snapshot.audit {
            audit.user_edited = true;
        }
        project.save(&snapshot).map_err(|e| e.to_string())?;
        job.view.snapshot = snapshot;
        job.view.approval_id = Some(Uuid::new_v4());
        job.view.sequence += 1;
        Ok(job.view.clone())
    }

    pub fn propose_live_cad(&self, project: (Uuid, PathBuf), input: crate::cadpilot_live::Input) -> Result<JobView, String> {
        input.agreement.as_ref().ok_or("Incluye las condiciones del lote antes de pedir aprobación.")?.validate()?;
        input.validate_for_dispatch()?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if input.receipt_sha256.is_some() { return Err("La propuesta ya contiene un resultado anterior.".into()); }
        if state.project.as_ref().map(|p| (p.id,p.root.clone())) != Some(project) {
            return Err("El proyecto cambió. Lee el documento de nuevo.".into());
        }
        Self::propose_operation_locked(&mut state, OperationInput::LiveCad(input), RunSource::Mcp)
    }

    pub fn propose_live_drawing(&self, project:(Uuid,PathBuf), input:crate::cadcraft_live::Input,
        connection:crate::cadcraft_live::Connection) -> Result<JobView,String> {
        input.agreement.as_ref().ok_or("Incluye las condiciones del lote antes de pedir aprobación.")?.validate()?;
        input.validate()?;
        connection.validate_for(&input.before)?;
        if input.receipt_sha256.is_some() { return Err("La propuesta ya contiene un resultado anterior.".into()); }
        let mut state=self.state.lock().map_err(|e|e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().map(|p|(p.id,p.root.clone()))!=Some(project) {
            return Err("El proyecto cambió. Relee CADCraft.".into());
        }
        let view=Self::propose_operation_locked(&mut state,OperationInput::LiveDrawing(input),RunSource::Mcp)?;
        state.job.as_mut().unwrap().live_drawing=Some(connection);
        Ok(view)
    }

    pub fn propose_live_agent(
        &self, project:(Uuid,PathBuf), input:crate::agent_editor::Input,
        connection:crate::agent_editor::Connection,
    ) -> Result<JobView,String> {
        input.agreement.as_ref().ok_or("Incluye las condiciones del lote antes de pedir aprobación.")?.validate()?;
        input.validate()?;
        connection.validate_for(&input.before).map_err(|error|error.message())?;
        if input.receipt_sha256.is_some() { return Err("La propuesta ya contiene un resultado anterior.".into()); }
        let mut state=self.state.lock().map_err(|e|e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().map(|p|(p.id,p.root.clone()))!=Some(project) {
            return Err("El proyecto cambió. Lee el sketch abierto de nuevo.".into());
        }
        let view=Self::propose_operation_locked(&mut state,OperationInput::LiveAgent(input),RunSource::Mcp)?;
        state.job.as_mut().unwrap().live_agent=Some(connection);
        Ok(view)
    }

    pub fn propose_live_pcb(
        &self, project:(Uuid,PathBuf), input:crate::kicad_live::Input,
        board:PathBuf, connection:crate::kicad_live::Connection,
    ) -> Result<JobView,String> {
        input.agreement.as_ref().ok_or("Incluye las condiciones del lote antes de pedir aprobación.")?.validate()?;
        input.validate()?;
        connection.validate()?;
        crate::project::valid_location(&board).map_err(|_|"Selecciona una PCB local válida.")?;
        if board.file_name().and_then(|s|s.to_str())!=Some(input.board_name.as_str()) || input.receipt_sha256.is_some() {
            return Err("La propuesta requiere la PCB abierta sin un resultado anterior.".into());
        }
        let mut state=self.state.lock().map_err(|e|e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().map(|p|(p.id,p.root.clone()))!=Some(project) {
            return Err("El proyecto cambió. Lee la PCB de nuevo.".into());
        }
        let view=Self::propose_operation_locked(&mut state,OperationInput::LivePcb(input),RunSource::Mcp)?;
        state.job.as_mut().unwrap().live_pcb=Some(crate::kicad_live::Binding {board,connection});
        Ok(view)
    }

    pub(crate) fn propose_native_tools(&self, project:(Uuid,PathBuf), input:crate::native_tools::Input, connection:crate::native_tools::Connection)->Result<JobView,String> {
        input.agreement.as_ref().ok_or("Explica primero el resultado, los supuestos y cómo se comprobará este lote; todavía no se creó una aprobación.")?.validate()?;
        let mut state=self.state.lock().map_err(|e|e.to_string())?;
        inference_idle(&state)?;
        if state.project.as_ref().is_none_or(|p|p.id!=project.0 || p.root!=project.1) {return Err("El proyecto cambió. Descubre las herramientas de nuevo.".into());}
        let view=Self::propose_operation_locked(&mut state,OperationInput::NativeTools(input),RunSource::Mcp)?;
        state.job.as_mut().unwrap().native_tools=Some(connection);
        Ok(view)
    }
    pub(crate) fn propose_public_document(&self,url:String,purpose:String)->Result<JobView,String>{
        let mut state=self.state.lock().map_err(|e|e.to_string())?;inference_idle(&state)?;
        Self::propose_public_document_locked(&mut state,url,purpose)
    }
    pub(crate) fn propose_public_redirect(&self,job_id:Uuid,purpose:String)->Result<JobView,String>{
        let mut state=self.state.lock().map_err(|e|e.to_string())?;inference_idle(&state)?;
        let project=state.project.as_ref().ok_or("No hay tarea activa.")?;
        let task=crate::tasks::Store::new(project).and_then(|s|s.binding()).map_err(|e|e.to_string())?.ok_or("No hay tarea activa.")?;
        let snapshot=project.history().map_err(|e|e.to_string())?.into_iter().find(|s|s.input.job_id()==job_id && s.audit.as_ref().and_then(|a|a.task.as_ref()).is_some_and(|t|t.id==task.id)).ok_or("La redirección no pertenece a esta tarea.")?;
        let OperationInput::PublicDocument(input)=&snapshot.input else{return Err("El resultado no es una consulta pública.".into());};
        let url=crate::public_document::redirect_target(&project.store("staging").join(job_id.to_string()),input)?;
        Self::propose_public_document_locked(&mut state,url,purpose)
    }
    fn propose_public_document_locked(state:&mut State,url:String,purpose:String)->Result<JobView,String>{
        let url=crate::public_document::address(&url)?.to_string();
        let project=state.project.as_ref().ok_or("Abre una tarea antes de investigar.")?;
        let task=crate::tasks::Store::new(project).and_then(|s|s.active()).map_err(|e|e.to_string())?.ok_or("Crea una conversación antes de preparar la consulta pública.")?;
        let attempts=project.history().map_err(|e|e.to_string())?.into_iter().filter(|s|s.audit.as_ref().and_then(|a|a.task.as_ref()).is_some_and(|b|b.id==task.id && b.revision>=task.last_user_revision)).filter_map(|s|match s.input {OperationInput::PublicDocument(input)=>Some((input,s.phase)),_=>None}).collect::<Vec<_>>();
        let mut contacted=Vec::new();
        for (input,phase) in attempts {if phase==Phase::Completed || project.store("staging").join(input.job_id.to_string()).try_exists().map_err(|e|e.to_string())? {contacted.push(input.url);}}
        if contacted.iter().any(|previous|previous==&url){return Err("Esta dirección ya se consultó en esta investigación. Conserva la respuesta local; no repitas la petición. Si cambió el sitio o necesitas otra lectura, pide una continuación explícita en el chat.".into());}
        if contacted.len()>=8 {return Err("Esta investigación llegó a ocho documentos. Conserva los hallazgos y ofrece una continuación acotada en el chat antes de pedir más consultas.".into());}
        Self::propose_operation_locked(state,OperationInput::PublicDocument(crate::public_document::Input{schema_version:1,public_document_version:1,job_id:Uuid::new_v4(),url,purpose,receipt_sha256:None,redirect_sha256:None}),RunSource::Mcp)
    }

    pub fn begin(&self, approval_id: Uuid) -> Result<Dispatch, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        Self::reconcile_pending_approval(&mut state)?;
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        let current_task=crate::tasks::Store::new(&project).and_then(|s|s.binding()).map_err(|e|e.to_string())?;
        let job = state
            .job
            .as_mut()
            .ok_or("No hay una operación pendiente.")?;
        if job.view.approval_id != Some(approval_id)
            || job.view.snapshot.phase != Phase::AwaitingApproval
            || job.view.snapshot.project_id != project.id
            || job.cancelled.load(Ordering::SeqCst)
            || job.dispatch_paused.load(Ordering::SeqCst)
        {
            return Err("La aprobación venció o ya fue utilizada.".into());
        }
        if job.view.snapshot.audit.as_ref().and_then(|a|a.task.as_ref())!=current_task.as_ref(){return Err("La tarea o sus entradas cambiaron; prepara una nueva propuesta.".into());}
        let agent = if matches!(job.view.snapshot.input, OperationInput::Agent(_)) {
            let connection = self
                .agent_connection()?
                .ok_or("Conecte Arduino Agent antes de aprobar.")?;
            connection.verify_token()?;
            Some(connection)
        } else {
            None
        };
        let live_agent=if matches!(job.view.snapshot.input,OperationInput::LiveAgent(_)) {
            Some(job.live_agent.clone().ok_or("La conexión nativa venció. Lee el sketch abierto de nuevo.")?)
        } else {None};
        let live_pcb=if matches!(job.view.snapshot.input,OperationInput::LivePcb(_)) {
            Some(job.live_pcb.clone().ok_or("La conexión nativa venció. Vuelve a leer la PCB.")?)
        } else {None};
        let live_drawing=if matches!(job.view.snapshot.input,OperationInput::LiveDrawing(_)) {
            Some(job.live_drawing.clone().ok_or("La conexión CADCraft venció. Relee el dibujo.")?)
        } else {None};
        let native_tools=if matches!(job.view.snapshot.input,OperationInput::NativeTools(_)) {Some(job.native_tools.clone().ok_or("La conexión nativa venció. Vuelve a conectarla.")?)}else{None};
        let mut snapshot = job.view.snapshot.clone();
        snapshot.phase = Phase::Running;
        snapshot.advance();
        project.save(&snapshot).map_err(|e| e.to_string())?;
        job.view.snapshot = snapshot.clone();
        job.view.approval_id = None;
        job.view.busy = true;
        job.view.sequence += 1;
        Ok(Dispatch {
            dispatch_gate: job.dispatch_gate.clone(),
            dispatch_paused: job.dispatch_paused.clone(),
            native_tools,
            live_agent,
            live_pcb,
            live_drawing,
            project,
            snapshot,
            cancelled: job.cancelled.clone(),
            agent,
        })
    }

    pub fn cancel(&self, job_id: Uuid) -> Result<JobView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        Self::cancel_locked(&mut state, job_id)
    }

    pub(crate) fn cancel_unapproved(&self, job_id: Uuid) -> Result<Option<JobView>, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if !state.job.as_ref().is_some_and(|job| {
            job.view.snapshot.input.job_id() == job_id
                && job.view.snapshot.phase == Phase::AwaitingApproval
        }) {
            return Ok(None);
        }
        Self::cancel_locked(&mut state, job_id).map(Some)
    }

    fn cancel_locked(state: &mut State, job_id: Uuid) -> Result<JobView, String> {
        let project = state.project.clone().ok_or("Seleccione un proyecto.")?;
        let job = state.job.as_mut().ok_or("No hay una operación activa.")?;
        if job.view.snapshot.input.job_id() != job_id {
            return Err("La operación ya no está activa.".into());
        }
        if matches!(
            job.view.snapshot.phase,
            Phase::Completed | Phase::Cancelled | Phase::Failed | Phase::Interrupted
        ) {
            return Ok(job.view.clone());
        }
        let _permit=job.dispatch_gate.lock().map_err(|_|"No se pudo confirmar la pausa de despacho.")?;
        job.cancelled.store(true, Ordering::SeqCst);
        job.view.snapshot.phase = Phase::Cancelled;
        job.view.approval_id = None;
        job.view.advance();
        project
            .save(&job.view.snapshot)
            .map_err(|e| e.to_string())?;
        Ok(job.view.clone())
    }

    pub fn current(&self) -> Result<Option<JobView>, String> {
        let mut state=self.state.lock().map_err(|e|e.to_string())?;
        Self::reconcile_pending_approval(&mut state)?;
        Ok(state.job.as_ref().map(|job|job.view.clone()))
    }

    pub fn start_inference(
        &self,
        profile: ModelProfile,
        prompt: &str,
    ) -> Result<InferenceDispatch, String> {
        crate::ollama::generation_request(profile.identity().0, prompt)?;
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        inference_idle(&state)?;
        if state
            .job
            .as_ref()
            .is_some_and(|job| job.view.busy || job.view.snapshot.phase == Phase::AwaitingApproval)
        {
            return Err("Cancele o termine la propuesta actual primero.".into());
        }
        let project_id = state.project.as_ref().ok_or("Seleccione un proyecto.")?.id;
        let id = Uuid::new_v4();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (model, digest) = profile.identity();
        state.inference = Some(Inference {
            cancelled: cancelled.clone(),
            view: InferenceView {
                id,
                profile,
                model: model.into(),
                digest: digest.into(),
                sequence: 1,
                busy: true,
                cancelled: false,
                text: String::new(),
                message: None,
                error: None,
            },
        });
        Ok(InferenceDispatch {
            id,
            project_id,
            profile,
            cancelled,
        })
    }

    pub fn inference_delta(
        &self,
        dispatch: &InferenceDispatch,
        text: &str,
    ) -> Result<Option<InferenceView>, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let Some(item) = state
            .inference
            .as_mut()
            .filter(|item| item.view.id == dispatch.id && item.view.busy && !item.view.cancelled)
        else {
            return Ok(None);
        };
        if text.len() > (16 * 1024usize).saturating_sub(item.view.text.len()) {
            item.cancelled.store(true, Ordering::SeqCst);
            return Err("La propuesta supera el límite permitido.".into());
        }
        item.view.text.push_str(text);
        item.view.sequence += 1;
        Ok(Some(item.view.clone()))
    }

    pub fn cancel_inference(&self, id: Uuid) -> Result<InferenceView, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let item = state
            .inference
            .as_mut()
            .filter(|item| item.view.id == id)
            .ok_or("La solicitud ya no está activa.")?;
        if item.view.busy {
            item.cancelled.store(true, Ordering::SeqCst);
            item.view.cancelled = true;
            item.view.sequence += 1;
        }
        Ok(item.view.clone())
    }

    pub fn finish_inference(
        &self,
        dispatch: InferenceDispatch,
        result: Result<Proposal, String>,
    ) -> Result<Option<InferenceView>, String> {
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        if state.project.as_ref().map(|project| project.id) != Some(dispatch.project_id)
            || !state
                .inference
                .as_ref()
                .is_some_and(|item| item.view.id == dispatch.id && item.view.busy)
        {
            return Ok(None);
        }
        let result = if dispatch.cancelled.load(Ordering::SeqCst) {
            Err("Solicitud cancelada.".into())
        } else {
            match result {
                Ok(Proposal::Proposal { dimensions_mm }) => {
                    Self::propose_locked(&mut state, dimensions_mm, dispatch.profile.source())
                        .map(|_| None)
                }
                Ok(Proposal::Clarification { message }) => Ok(Some(message)),
                Err(error) => Err(error),
            }
        };
        let item = state.inference.as_mut().unwrap();
        item.view.busy = false;
        item.view.sequence += 1;
        match result {
            Ok(message) => item.view.message = message,
            Err(error) => item.view.error = Some(error),
        }
        Ok(Some(item.view.clone()))
    }

    pub fn execute(&self, mut dispatch: Dispatch) -> Result<JobView, String> {
        let result = (|| {
            if dispatch.cancelled.load(Ordering::SeqCst) {
                return Err("Operación cancelada.".into());
            }
            let stage = dispatch
                .project
                .reserve_stage(dispatch.snapshot.input.job_id())
                .map_err(|e| e.to_string())?;
            let profile = self
                .profiles
                .join(dispatch.snapshot.input.job_id().to_string());
            let live_dispatch=if matches!(&dispatch.snapshot.input,OperationInput::LiveCad(_)|OperationInput::LiveAgent(_)|OperationInput::LivePcb(_)|OperationInput::LiveDrawing(_)) {
                Some(crate::live_dispatch::Dispatch::start(&stage.path,&dispatch.snapshot.input,dispatch.dispatch_gate.clone(),dispatch.dispatch_paused.clone(),dispatch.cancelled.clone())?)
            }else{None};
            if dispatch.snapshot.audit.as_ref().is_some_and(|a|a.native_attempted.is_some()) {
                dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);
                // Durable adapter-entry fact, not proof that bytes reached the native program.
                dispatch.project.save(&dispatch.snapshot).map_err(|e|e.to_string())?;
            }
            match &dispatch.snapshot.input {
                OperationInput::Freecad(input) => {
                    freecad::generate(
                        &self.executable.lock().map_err(|e| e.to_string())?.clone(),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::NativeTools(input) => {
                    crate::native_tools::run_gated(input,dispatch.native_tools.as_ref().ok_or("La conexión nativa venció.")?,&stage.path,dispatch.cancelled.clone(),&dispatch.dispatch_gate,&dispatch.dispatch_paused)?;
                }
                OperationInput::PublicDocument(input)=>{
                    crate::public_document::run(input,&stage.path,dispatch.cancelled.clone())?;
                }
                OperationInput::LiveDrawing(input) => {
                    crate::cadcraft_live::run_gated(input,dispatch.live_drawing.as_ref().ok_or("La conexión CADCraft venció.")?,
                        &stage.path,&profile,dispatch.cancelled.clone(),live_dispatch.as_ref().unwrap())?;
                }
                OperationInput::LiveAgent(input) => {
                    crate::agent_editor::run_gated(input,dispatch.live_agent.as_ref().ok_or("La conexión al sketch venció.")?,
                        &stage.path,&dispatch.cancelled,live_dispatch.as_ref().unwrap())?;
                }
                OperationInput::LivePcb(input) => {
                    crate::kicad_live::run_gated(input,dispatch.live_pcb.as_ref().ok_or("La conexión PCB venció.")?,
                        &stage.path,&profile,dispatch.cancelled.clone(),live_dispatch.as_ref().unwrap())?;
                }
                OperationInput::LiveCad(input) => {
                    let executable=self.executable.lock().map_err(|_|"No se pudo comprobar FreeCADCmd.")?.clone();
                    let _verifier_pin=input.working_version.as_ref().map(|plan|crate::native_working_version::pin_verifier(&executable,plan)).transpose()?;
                    crate::cadpilot_live::run_gated(input, &stage.path, &dispatch.cancelled,live_dispatch.as_ref().unwrap())?;
                    if let Some(plan)=&input.working_version{let result:serde_json::Value=crate::project::read_json(&stage.path.join("result.json")).map_err(|_|"Falta el recibo de captura de la nueva versión.")?;let hash=crate::native_working_version::verify(&executable,plan,&stage.path,&profile,&result,dispatch.cancelled.clone())?;live_dispatch.as_ref().unwrap().retain_working_version(input,&hash)?;}
                }
                OperationInput::Cadpilot(input) => {
                    cadpilot::run(
                        &self.executable.lock().map_err(|e| e.to_string())?.clone(),
                        &dispatch.project.captured_source(input.job_id),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Arduino(input) => {
                    let runtime = self.arduino_runtime()?.ok_or(
                        "Seleccione Arduino CLI y su carpeta de herramientas antes de compilar.",
                    )?;
                    arduino::compile(
                        &runtime.executable,
                        &runtime.data,
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Agent(input) => {
                    agent::edit(
                        dispatch
                            .agent
                            .as_ref()
                            .ok_or("Conecte Arduino Agent antes de editar.")?,
                        &stage.path,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Drawing(input) => {
                    let runtime = self
                        .cadcraft_runtime()?
                        .ok_or("Seleccione CADCraft CLI antes de aprobar.")?;
                    cadcraft::convert(
                        &runtime,
                        &dispatch
                            .project
                            .drawing_source(input.job_id, input.source_format),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Kicad(input) => {
                    let runtime = self
                        .kicad_runtime()?
                        .ok_or("Seleccione KiCad CLI antes de aprobar.")?;
                    kicad_run::run(
                        &runtime,
                        &dispatch.project.captured_source(input.job_id),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::KonnectPcb(input) => {
                    let runtime = self
                        .konnect_runtime()?
                        .ok_or("Seleccione Konnect antes de aprobar.")?;
                    let cli = self
                        .kicad_runtime()?
                        .ok_or("Seleccione KiCad CLI antes de aprobar.")?;
                    crate::konnect_pcb::run(
                        &runtime,
                        &cli,
                        &dispatch.project.captured_source(input.job_id),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Konnect(input) => {
                    let runtime = self
                        .konnect_runtime()?
                        .ok_or("Seleccione Konnect antes de aprobar.")?;
                    let cli = self
                        .kicad_runtime()?
                        .ok_or("Seleccione KiCad CLI antes de aprobar.")?;
                    konnect_run::run(
                        &runtime,
                        &cli,
                        &dispatch.project.captured_source(input.job_id),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
                OperationInput::Orca(input) => {
                    let runtime = self
                        .orca_runtime()?
                        .ok_or("Seleccione OrcaSlicer antes de aprobar.")?;
                    orca_run::run(
                        &runtime,
                        &dispatch.project.captured_source(input.job_id),
                        &stage.path,
                        &profile,
                        input,
                        dispatch.cancelled.clone(),
                    )?;
                }
            }
            if let Some(live_dispatch)=live_dispatch {
                let mut checked=dispatch.snapshot.input.clone();checked.retain_receipt(&stage.path).map_err(|_|"No se pudo comprobar el recibo nativo.")?;
                dispatch.project.validate_outputs(&stage.path,&checked).map_err(|_|"El resultado nativo sigue pendiente de comprobación.")?;
                live_dispatch.complete(&checked)?;
            }
            Ok(stage)
        })();
        self.finish(dispatch, result)
    }

    fn finish(&self, mut dispatch: Dispatch, result: Result<Stage, String>) -> Result<JobView, String> {
        let result=(||{
            if let OperationInput::PublicDocument(input)=&mut dispatch.snapshot.input {
                let directory=dispatch.project.store("staging").join(input.job_id.to_string());
                if let Some(receipt)=crate::public_document::redirect_receipt(&directory,input)? {input.redirect_sha256=Some(receipt);dispatch.project.save(&dispatch.snapshot).map_err(|e|e.to_string())?;}
            }
            result
        })();
        let result = result.and_then(|stage| {
            let mut checked=dispatch.snapshot.input.clone();
            if checked.retain_receipt(&stage.path).map_err(|_|"El resultado puede estar terminado, pero no se pudo conservar su recibo. No repitas la operación.".to_string())? {
                dispatch.project.validate_outputs(&stage.path,&checked).map_err(|_|"El resultado local no pasó sus comprobaciones. Se conserva para revisión; no repitas las llamadas.".to_string())?;
                dispatch.snapshot.input=checked;
                dispatch.project.save(&dispatch.snapshot).map_err(|e|e.to_string())?;
            }
            Ok(stage)
        });
        let mut state = self.state.lock().map_err(|e| e.to_string())?;
        let active = state
            .project
            .as_ref()
            .is_some_and(|project| project.id == dispatch.project.id)
            && state.job.as_ref().is_some_and(|job| {
                job.view.snapshot.input.job_id() == dispatch.snapshot.input.job_id()
            });
        if !active {
            let mut snapshot = dispatch.snapshot;
            snapshot.phase = Phase::Cancelled;
            snapshot.advance();
            let warning = matches!(snapshot.input, OperationInput::LiveCad(_) | OperationInput::LivePcb(_) | OperationInput::LiveAgent(_) | OperationInput::LiveDrawing(_) | OperationInput::NativeTools(_))
                .then_some("El proyecto cambió y no se publicó el informe. La operación nativa puede haber terminado; vuelve a leer el programa antes de continuar. Cancelar no deshace una edición nativa.");
            if let (Some(message), Some(audit)) = (warning, snapshot.audit.as_mut()) { audit.error = Some(message.into()); }
            dispatch
                .project
                .save(&snapshot)
                .map_err(|e| e.to_string())?;
            return Err(warning.unwrap_or("El proyecto activo cambió; los archivos no se publicaron.").into());
        }
        let job = state.job.as_mut().unwrap();
        if dispatch.snapshot.input.receipt_sha256().is_some() || matches!(&dispatch.snapshot.input,OperationInput::PublicDocument(input) if input.redirect_sha256.is_some()) {job.view.snapshot.input=dispatch.snapshot.input.clone();}
        if let Some((current,run))=job.view.snapshot.audit.as_mut().zip(dispatch.snapshot.audit.as_ref()){current.native_attempted=run.native_attempted;}
        if dispatch.dispatch_paused.load(Ordering::SeqCst) {
            // A correction during final readback must not discard a completed local receipt.
            job.view.snapshot.input=dispatch.snapshot.input.clone();
            job.view.snapshot.phase=Phase::Cancelled;
            job.view.error=Some("Se pausaron los pasos pendientes por tu corrección. Se conservaron las respuestas de las llamadas enviadas; revisa su estado antes de nuevos cambios. No se repiten ni se deshacen automáticamente.".into());
            if let Ok(stage)=result {
                if let Some(audit)=&mut job.view.snapshot.audit {audit.files_verified=true;}
                match dispatch.project.publish(stage,&dispatch.snapshot) {
                    Ok(_)=>{job.view.snapshot.phase=Phase::Completed;job.view.error=None;}
                    Err(_)=>{job.view.snapshot.phase=Phase::Interrupted;job.view.error=Some("El informe terminado sigue local y su publicación está pendiente. Tu corrección se conserva; no repitas las llamadas.".into());}
                }
            }
        } else if dispatch.cancelled.load(Ordering::SeqCst) && result.is_ok() && dispatch.snapshot.input.receipt_sha256().is_some() {
            // Stop never discards a result that already passed the adapter's native checks.
            match dispatch.project.publish(result.unwrap(),&dispatch.snapshot) {
                Ok(_)=>{job.view.snapshot.phase=Phase::Completed;job.view.error=None;if let Some(audit)=&mut job.view.snapshot.audit {audit.files_verified=true;}},
                Err(_)=>{job.view.snapshot.phase=Phase::Interrupted;job.view.error=Some("El resultado comprobado sigue local y su publicación está pendiente. No repitas las llamadas.".into());},
            }
        } else if dispatch.cancelled.load(Ordering::SeqCst) {
            job.view.snapshot.phase = Phase::Cancelled;
            if matches!(dispatch.snapshot.input, OperationInput::LiveCad(_)) {
                job.view.error = Some("La espera se canceló. FreeCAD puede haber completado la operación nativa; vuelve a leer el documento antes de continuar. Cancelar no deshace una edición nativa.".into());
            }
            if matches!(dispatch.snapshot.input,OperationInput::LiveAgent(_)) {
                job.view.error=Some("La espera se canceló. Arduino Agent puede haber completado el cambio; vuelve a leer el sketch. Cancelar no deshace una edición nativa.".into());
            }
            if matches!(dispatch.snapshot.input,OperationInput::LiveDrawing(_)) {
                job.view.error=Some("La espera se canceló. CADCraft puede haber completado el cambio; relee el dibujo. Cancelar no deshace una edición nativa.".into());
            }
            if matches!(dispatch.snapshot.input,OperationInput::LivePcb(_)) {
                job.view.error=Some("La espera se canceló. KiCad puede haber completado el cambio; vuelve a leer la PCB. Cancelar no deshace una edición nativa.".into());
            }
            if matches!(dispatch.snapshot.input, OperationInput::NativeTools(_)) {
                job.view.error=Some("La espera se canceló. El programa puede haber aceptado la llamada: relee su estado antes de continuar y no repitas la edición automáticamente.".into());
            }
            if matches!(dispatch.snapshot.input, OperationInput::Agent(_)) {
                job.view.error = Some("La espera se canceló. Agent puede terminar de modificar su copia temporal; no se publicaron esos cambios ni se modificó el original.".into());
            }
        } else {
            match result {
                Ok(stage) => {
                    if matches!(dispatch.snapshot.input, OperationInput::LiveCad(_) | OperationInput::LivePcb(_) | OperationInput::LiveAgent(_) | OperationInput::LiveDrawing(_) | OperationInput::NativeTools(_) | OperationInput::PublicDocument(_)) {
                        job.view.snapshot.input = dispatch.snapshot.input.clone();
                    }
                    if let Some(audit) = &mut job.view.snapshot.audit {
                        audit.files_verified = true;
                    }
                    match dispatch.project.publish(stage, &dispatch.snapshot) {
                        Ok(_) => job.view.snapshot.phase = Phase::Completed,
                        Err(error) => {
                            job.view.snapshot.phase = Phase::Failed;
                            job.view.error = Some(format!("No se pudieron publicar los archivos. Cierre las aplicaciones que los estén usando y revise los permisos y el espacio libre del proyecto. La entrada y los archivos de trabajo se conservan: {error}"));
                            if matches!(dispatch.snapshot.input, OperationInput::LiveCad(_) | OperationInput::LivePcb(_) | OperationInput::LiveAgent(_) | OperationInput::LiveDrawing(_) | OperationInput::NativeTools(_)) {
                                job.view.error.as_mut().unwrap().push_str(" El programa puede haber completado la operación nativa; vuelve a leerlo antes de repetirla.");
                            }
                        }
                    }
                }
                Err(error) => {
                    job.view.snapshot.phase = Phase::Failed;
                    job.view.error = Some(error);
                }
            }
        }
        job.view.advance();
        job.view.busy = false;
        if let Err(error) = dispatch.project.save(&job.view.snapshot) {
            let message = format!("No se pudo guardar el estado de la operación. Reabra el proyecto para recuperar los archivos publicados: {error}");
            job.view.error = Some(message.clone());
            return Err(message);
        }
        Ok(job.view.clone())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn durable_user_context_withholds_revoked_goals_and_recovers_long_statements_by_page(){
        use crate::tasks::Request;
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();let task=workflow.task_state().unwrap().unwrap().active;let text="Keep the original and use inches. ".repeat(120);
        workflow.task_request(project.clone(),Request::Message{id:task.id,revision:task.revision,text:text.clone(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":text}]}).to_string(),correction_of:None}).unwrap();let task=workflow.task_state().unwrap().unwrap().active;
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some(text.clone()),None).unwrap();assert!(prompt.len()<=8192);let sent:serde_json::Value=serde_json::from_str(&prompt).unwrap();assert_eq!(sent["task_goal"]["instruction_id"],task.instructions[0].id.to_string());assert_eq!(workflow.read_task_instructions(0).unwrap()["entries"][0]["text"],text);
        workflow.task_request(project.clone(),Request::ShareInstructions{id:task.id,revision:task.revision,statements:vec![task.instructions[0].id],allow:false}).unwrap();let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Review the saved operation status".into()),None).unwrap();assert!(!prompt.contains("Keep the original and use inches"));assert!(workflow.read_task_instructions(0).unwrap()["entries"][0]["text"].is_null());
        let task=workflow.task_state().unwrap().unwrap().active;workflow.task_request(project,Request::RemoveInstructions{id:task.id,revision:task.revision,statements:vec![task.instructions[0].id]}).unwrap();drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn durable_progress_is_task_scoped_paged_metadata_without_raw_errors_or_completion_claims(){
        let (root,workflow)=setup();let selected=workflow.view().unwrap().project.unwrap();workflow.task_state().unwrap();let first=workflow.propose(dimensions()).unwrap();workflow.cancel(first.snapshot.input.job_id()).unwrap();workflow.task_request(selected,crate::tasks::Request::New).unwrap();let task=workflow.task_state().unwrap().unwrap().active;
        let project=workflow.state.lock().unwrap().project.clone().unwrap();for _ in 0..35{let mut snapshot=project.snapshot(crate::SolidInput::new(dimensions()).unwrap(),Phase::Failed).unwrap();snapshot.audit.as_mut().unwrap().task=Some(task.binding());snapshot.audit.as_mut().unwrap().error=Some("PRIVATE_PROGRESS_SENTINEL".into());project.save(&snapshot).unwrap();}drop(project);
        let first_page=workflow.task_progress(0).unwrap();assert_eq!(first_page["operations"].as_array().unwrap().len(),32);assert_eq!(first_page["next_offset"],32);let next=workflow.task_progress(32).unwrap();assert_eq!(next["operations"].as_array().unwrap().len(),3);assert!(next["next_offset"].is_null());let raw=first_page.to_string()+&next.to_string();assert!(!raw.contains("PRIVATE_PROGRESS_SENTINEL") && !raw.contains(&first.snapshot.input.job_id().to_string()));assert!(first_page["operations"].as_array().unwrap().iter().all(|entry|entry["current_artifacts_rechecked"]==false && entry["output_check_recorded"]==false));assert!(workflow.task_progress(36).is_err());
        drop(workflow);let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();assert_eq!(workflow.task_progress(32).unwrap()["operations"].as_array().unwrap().len(),3);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn redirect_preparation_reuses_local_identity_without_exposing_or_requesting_the_target(){
        use crate::{tasks::Request,host_tools::{HostTools,McpTools,Program}};
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let task=workflow.task_state().unwrap().unwrap().active;
        let selected=workflow.view().unwrap().project.unwrap();
        let pending=workflow.propose_public_document("https://example.org/start".into(),"Read the official manual".into()).unwrap();let id=pending.snapshot.input.job_id();
        let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
        let stage=dispatch.project.reserve_stage(id).unwrap();
        let target="https://example.org/manual?q=PRIVATE_REDIRECT_SENTINEL";
        let bytes=serde_json::to_vec(&serde_json::json!({"job_id":id,"source_url":"https://example.org/start","target_url":target})).unwrap();
        std::fs::write(stage.path.join("redirect.json"),&bytes).unwrap();
        workflow.finish(dispatch,Err("El sitio pide otra dirección. Hazlo no la siguió: prepara esa dirección para una nueva revisión.".into())).unwrap();
        let host=Arc::new(HostTools::new(workflow.clone()));let tools=McpTools::for_chat(host.clone(),selected.clone(),Some(Program::Planning),"codex").unwrap();
        let status=tools.call_in_project("get_workflow_status".into(),serde_json::json!({}),Arc::new(AtomicBool::new(false))).await.unwrap();
        assert_eq!(status["job"]["public_document_status"]["kind"],"redirect_needs_approval");assert!(!status.to_string().contains("PRIVATE_REDIRECT_SENTINEL"));
        let prepared=tools.call_in_project("propose_public_document_read".into(),serde_json::json!({"redirect_from_job_id":id,"purpose":"Read the public manual at its new address"}),Arc::new(AtomicBool::new(false))).await.unwrap();
        assert!(!prepared.to_string().contains("PRIVATE_REDIRECT_SENTINEL"));
        let pending=workflow.current().unwrap().unwrap();let OperationInput::PublicDocument(input)=&pending.snapshot.input else{panic!("Wrong input");};assert_eq!(input.url,target);
        assert!(!workflow.state.lock().unwrap().project.as_ref().unwrap().store("staging").join(input.job_id.to_string()).exists());
        workflow.cancel(input.job_id).unwrap();
        std::fs::write(stage.path.join("redirect.json"),b"forged").unwrap();assert!(workflow.propose_public_redirect(id,"Retry redirect".into()).is_err());
        workflow.task_request(selected,Request::New).unwrap();assert!(workflow.propose_public_redirect(id,"Other task".into()).is_err());
        assert_ne!(workflow.task_state().unwrap().unwrap().active.id,task.id);
        drop(stage);drop(tools);drop(host);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn public_documents_bind_approval_recover_locally_and_require_response_sharing(){
        use crate::{tasks::Request,host_tools::{HostTools,McpTools,Program}};
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let selected=workflow.view().unwrap().project.unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(selected.clone(),Request::Message{id:task.id,revision:task.revision,text:"Investigate public documentation".into(),history:"{\"version\":1,\"turns\":[]}".into(),correction_of:None}).unwrap();
        let pending=workflow.propose_public_document("https://example.invalid/manual".into(),"Read an official manual".into()).unwrap();
        let old_approval=pending.approval_id.unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(selected.clone(),Request::Message{id:task.id,revision:task.revision,text:"Preserve local evidence".into(),history:"{\"version\":1,\"turns\":[]}".into(),correction_of:None}).unwrap();
        assert!(workflow.begin(old_approval).is_err());
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(selected.clone(),Request::AddReference{id:task.id,revision:task.revision,name:"capture.bin".into(),data:base64::Engine::encode(&base64::engine::general_purpose::STANDARD,b"RAW_PRIVATE_CAPTURE_SENTINEL"),preview:None}).unwrap();
        let pending={
            let host=Arc::new(HostTools::new(workflow.clone()));let tools=McpTools::for_chat(host,selected.clone(),Some(Program::Planning),"codex").unwrap();
            let prepared=tools.call_in_project("propose_public_document_read".into(),serde_json::json!({"url":"https://example.invalid/manual","purpose":"Read an official manual"}),Arc::new(AtomicBool::new(false))).await.unwrap();
            assert_eq!(prepared["human_approval_required"],true);workflow.current().unwrap().unwrap()
        };
        let project=workflow.state.lock().unwrap().project.clone().unwrap();
        assert!(!project.store("staging").join(pending.snapshot.input.job_id().to_string()).exists());
        let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
        assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        let mut snapshot=dispatch.snapshot.clone();let id=snapshot.input.job_id();
        let stage=project.reserve_stage(id).unwrap();
        let content="PUBLIC_DOCUMENT_SENTINEL <script>never execute</script> ".repeat(2500);
        let OperationInput::PublicDocument(input)=&mut snapshot.input else{panic!("Wrong input");};
        let report=serde_json::json!({"public_document_version":1,"job_id":id,"url":input.url,"purpose":input.purpose,"media_type":"text/plain","bytes":content.len(),"sha256":crate::kicad_project::digest(content.as_bytes()),"content":content,"source_is_untrusted_data":true});
        let bytes=serde_json::to_vec(&report).unwrap();std::fs::write(stage.path.join("result.json"),&bytes).unwrap();input.receipt_sha256=Some(crate::kicad_project::digest(&bytes));project.save(&snapshot).unwrap();
        // Recovery must publish the complete receipt, never fetch this deliberately nonexistent site.
        drop(stage);drop(dispatch);drop(project);drop(workflow);
        let workflow=Arc::new(Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap());workflow.select_project(Project::open(&root).unwrap()).unwrap();
        assert_eq!(workflow.view().unwrap().history.iter().find(|s|s.input.job_id()==id).unwrap().phase,Phase::Completed);
        let host=Arc::new(HostTools::new(workflow.clone()));let tools=McpTools::for_chat(host.clone(),selected.clone(),Some(Program::Planning),"codex").unwrap();
        let held=tools.call_in_project("read_verified_result".into(),serde_json::json!({"job_id":id}),Arc::new(AtomicBool::new(false))).await.unwrap();
        assert_eq!(held["local_evidence_pending"],true);assert!(!held.to_string().contains("PUBLIC_DOCUMENT_SENTINEL"));
        let task=workflow.task_state().unwrap().unwrap().active;let reference=task.references.last().unwrap().id;
        assert_eq!(task.references.last().unwrap().name,"Documento público.json");
        assert!(task.references.last().unwrap().bytes>65536);assert!(workflow.shared_task_reference(reference,0).is_err());
        assert!(workflow.propose_public_document("https://example.invalid/manual".into(),"Read again".into()).is_err());
        workflow.task_request(selected.clone(),Request::Share{id:task.id,revision:task.revision,reference,allow:true}).unwrap();
        let page=workflow.shared_task_reference(reference,0).unwrap();assert!(page["content"].as_str().unwrap().contains("PUBLIC_DOCUMENT_SENTINEL"));assert!(!page.to_string().contains("RAW_PRIVATE_CAPTURE_SENTINEL"));
        assert!(workflow.propose_public_document("https://example.invalid/manual".into(),"Read again".into()).is_err());
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(selected.clone(),Request::Message{id:task.id,revision:task.revision,text:"The site changed; allow a fresh check".into(),history:"{\"version\":1,\"turns\":[]}".into(),correction_of:None}).unwrap();
        let fresh=workflow.propose_public_document("https://example.invalid/manual".into(),"A newly requested check".into()).unwrap();workflow.cancel(fresh.snapshot.input.job_id()).unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(selected,Request::Provider{id:task.id,revision:task.revision,provider:"claude".into()}).unwrap();assert!(workflow.shared_task_reference(reference,0).is_err());
        drop(tools);drop(host);drop(workflow);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore="Explicit public HTTPS request to official Rust documentation; no credentials or device data"]
    fn real_public_document_fetch_through_owned_approval(){
        let (root,workflow)=setup();workflow.task_state().unwrap();
        let job=workflow.propose_public_document("https://doc.rust-lang.org/std/net/struct.Ipv4Addr.html".into(),"Owned public documentation acceptance".into()).unwrap();
        let dispatch=workflow.begin(job.approval_id.unwrap()).unwrap();let result=workflow.execute(dispatch).unwrap();
        assert_eq!(result.snapshot.phase,Phase::Completed,"{:?}",result.error);
        let artifact=workflow.artifact(result.snapshot.input.job_id(),ArtifactFormat::PublicDocumentReport).unwrap();let text=artifact.read_text_bounded(crate::public_document::REPORT_LIMIT).unwrap();
        assert!(text.contains("Ipv4Addr"));assert!(text.contains("source_is_untrusted_data"));
        drop(artifact);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Explicit disposable MCP fixture; tests correction during a real in-flight call"]
    fn native_correction_preserves_inflight_effect_and_survives_restart(){
        use crate::{tasks::Request,native_tools::{Connection,McpConfig,Input,Call,Guard},host_tools::Program};
        for scenario in 0..3 {
        let final_readback=scenario==1;
        let (root,workflow)=setup();let workflow=Arc::new(workflow);
        let project=workflow.view().unwrap().project.unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(project.clone(),Request::AddReference{id:task.id,revision:task.revision,name:"capture.bin".into(),data:base64::Engine::encode(&base64::engine::general_purpose::STANDARD,b"RAW_CORRECTION_SENTINEL"),preview:None}).unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        let marker=root.join("effect.marker");
        let readback_marker=root.join("readback.marker");
        let executable=PathBuf::from(std::env::var_os("HAZLOMEXA_NATIVE_FIXTURE").expect("Set owned fixture"));
        let cancel=||Arc::new(AtomicBool::new(false));
        let (connection,catalog)=Connection::mcp(Program::Drawing,McpConfig{executable,args:vec![],env:std::collections::BTreeMap::from([("NATIVE_EFFECT_MARKER".into(),marker.to_string_lossy().into_owned()),("NATIVE_EFFECT_DELAY_MS".into(),"10000".into()),("NATIVE_READBACK_MARKER".into(),readback_marker.to_string_lossy().into_owned())]),url:None,authorization:None},root.clone(),cancel()).unwrap();
        let call=|tool:&str,args:serde_json::Value|Call{tool:tool.into(),arguments:args.as_object().unwrap().clone()};
        let query=call("observe_native_state",serde_json::json!({}));
        let before=connection.read(&catalog.catalog_sha256,&query,cancel()).unwrap();
        let input=Input{schema_version:1,native_tools_version:1,agreement:Some(crate::native_tools::tests::agreement_fixture()),job_id:Uuid::new_v4(),connection_id:catalog.connection_id,program:Program::Drawing,catalog_sha256:catalog.catalog_sha256.clone(),description:"Owned correction fixture".into(),calls:if final_readback{vec![call("program_owned_parameter_edit",serde_json::json!({"value":7}))]}else{vec![call("wait_after_effect",serde_json::json!({"value":7})),call("program_owned_parameter_edit",serde_json::json!({"value":99}))]},readback:if final_readback{call("observe_native_state",serde_json::json!({"value":1}))}else{query.clone()},guard:Some(Guard{call:query.clone(),result_sha256:before["readback_sha256"].as_str().unwrap().into(),checks:vec![]}),receipt_sha256:None};
        let id=input.job_id;
        let proposed=workflow.propose_native_tools(project.clone(),input.clone(),connection.clone()).unwrap();
        let dispatch=workflow.begin(proposed.approval_id.unwrap()).unwrap();
        let worker=workflow.clone();let handle=std::thread::spawn(move||worker.execute(dispatch));
        let started=std::time::Instant::now();
        let pause_marker=if final_readback{&readback_marker}else{&marker};
        while !pause_marker.exists() && started.elapsed()<std::time::Duration::from_secs(10){std::thread::sleep(std::time::Duration::from_millis(10));}
        assert!(pause_marker.exists(),"Owned fixture did not reach its in-flight effect/readback");
        let correction="Keep the valid first change; make the next part smaller";
        let request=|revision,job_id|Request::Message{id:task.id,revision,text:correction.into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":correction}]}).to_string(),correction_of:Some(job_id)};
        assert!(workflow.task_request(project.clone(),request(task.revision-1,id)).is_err());
        assert!(workflow.task_request(project.clone(),request(task.revision,Uuid::new_v4())).is_err());
        assert!(!workflow.state.lock().unwrap().job.as_ref().unwrap().cancelled.load(Ordering::SeqCst));
        workflow.task_request(project.clone(),request(task.revision,id)).unwrap();
        assert!(!workflow.state.lock().unwrap().job.as_ref().unwrap().cancelled.load(Ordering::SeqCst));
        if scenario==2 {workflow.cancel(id).unwrap();}
        let stopped=handle.join().unwrap().unwrap();
        assert_eq!(stopped.snapshot.phase,if final_readback{Phase::Completed}else{Phase::Cancelled});assert!(!stopped.busy);
        let observed=connection.read(&catalog.catalog_sha256,&query,cancel()).unwrap();
        assert_eq!(observed["readback"]["structuredContent"]["native_value"],7);
        assert_eq!(observed["readback"]["structuredContent"]["native_writes"],1);
        let actual:serde_json::Value=serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        assert_eq!(actual["native_value"],7);assert_eq!(actual["native_writes"],1);
        let native_project=workflow.state.lock().unwrap().project.clone().unwrap();
        let directory=if final_readback{native_project.artifact_directory(id)}else{native_project.store("staging").join(id.to_string())};
        let journal=crate::native_journal::read(&directory,&input).unwrap().unwrap();
        assert_eq!(journal.steps[1].outcome,if scenario==2{crate::native_journal::Outcome::Uncertain}else{crate::native_journal::Outcome::Acknowledged});
        if final_readback {
            assert!(journal.finished);
            let OperationInput::NativeTools(completed)=&stopped.snapshot.input else{panic!("Wrong input");};
            assert!(completed.receipt_sha256.is_some());
            crate::native_tools::validate_artifacts(&directory,completed).unwrap();
        }else{
            assert_eq!(journal.steps[2].outcome,crate::native_journal::Outcome::Pending);
            assert_eq!(journal.steps[3].outcome,crate::native_journal::Outcome::Pending);
        }
        let (prompt,restricted)=workflow.task_prompt("codex",String::new(),None,None).unwrap();
        assert!(restricted && prompt.contains(correction) && !prompt.contains("RAW_CORRECTION_SENTINEL"));
        let revision=workflow.task_state().unwrap().unwrap().active.revision;
        connection.close();drop(native_project);drop(workflow);
        let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();
        let restored=workflow.task_state().unwrap().unwrap().active;
        assert_eq!(restored.revision,revision);assert_eq!(restored.latest_correction.unwrap().text,correction);
        let history=workflow.task_request(workflow.view().unwrap().project.unwrap(),Request::History{id:restored.id,revision:restored.revision,value:None}).unwrap();
        assert!(history.as_str().unwrap().contains(correction));
        let summary=workflow.native_recovery().unwrap();assert_eq!(summary[0].needs_review,!final_readback);assert_eq!(summary[0].uncertain,usize::from(scenario==2));assert_eq!(summary[0].not_dispatched,if final_readback{0}else{2});
        if final_readback {
            assert_eq!(workflow.view().unwrap().history.iter().find(|s|s.input.job_id()==id).unwrap().phase,Phase::Completed);
        }else{
        let mut next=input;next.job_id=Uuid::new_v4();
        assert!(Workflow::propose_operation_locked(&mut workflow.state.lock().unwrap(),OperationInput::NativeTools(next),RunSource::Mcp).is_err());
        }
        drop(workflow);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        Uuid::parse_str(root.file_name().unwrap().to_str().unwrap().strip_prefix("hazlomexa-workflow-").unwrap()).unwrap();
        // Only disposable teardown retries: closing the MCP is not native completion evidence.
        let teardown=std::time::Instant::now();
        loop {match std::fs::remove_dir_all(&root) {
            Ok(())=>break,
            Err(e) if matches!(e.raw_os_error(),Some(5|32|145)) && teardown.elapsed()<std::time::Duration::from_secs(5)=>std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(e)=>panic!("Owned fixture teardown failed: {e}"),
        }}
        }
    }
    #[test]
    fn incomplete_native_dispatch_blocks_new_mutations_after_restart(){
        use crate::{native_tools::{Input,Call,Guard},native_journal::Batch,host_tools::Program};
        let (root,workflow)=setup();workflow.task_state().unwrap();
        let project=workflow.state.lock().unwrap().project.clone().unwrap();
        let read=Call{tool:"state".into(),arguments:Default::default()};
        let input=Input{schema_version:1,native_tools_version:1,agreement:None,job_id:Uuid::new_v4(),connection_id:Uuid::new_v4(),program:Program::Freecad,catalog_sha256:"a".repeat(64),description:"Owned recovery fixture".into(),calls:vec![Call{tool:"edit".into(),arguments:Default::default()}],readback:read.clone(),guard:Some(Guard{checks:Vec::new(),call:read,result_sha256:"b".repeat(64)}),receipt_sha256:None};
        let mut snapshot=project.snapshot(OperationInput::NativeTools(input.clone()),Phase::Running).unwrap();snapshot.audit.as_mut().unwrap().source=RunSource::Mcp;project.save(&snapshot).unwrap();
        let stage=project.reserve_stage(input.job_id).unwrap();let mut batch=Batch::start(&stage.path,&input).unwrap();batch.dispatch(0).unwrap();batch.observe(0,&Ok(serde_json::json!({"state":"before"}))).unwrap();batch.dispatch(1).unwrap();
        drop(batch);drop(stage);drop(project);drop(workflow);
        let reopened=Project::open(&root).unwrap();
        let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(reopened).unwrap();
        assert!(workflow.native_recovery().unwrap()[0].needs_review);
        workflow.task_request(workflow.view().unwrap().project.unwrap(),crate::tasks::Request::New).unwrap();
        let mut next=input.clone();next.job_id=Uuid::new_v4();
        let error=Workflow::propose_operation_locked(&mut workflow.state.lock().unwrap(),OperationInput::NativeTools(next),RunSource::Mcp).err().unwrap();
        assert!(error.contains("incompleto"));
        let mut read_only_batch=input;read_only_batch.job_id=Uuid::new_v4();read_only_batch.calls.clear();read_only_batch.guard=None;
        assert!(Workflow::propose_operation_locked(&mut workflow.state.lock().unwrap(),OperationInput::NativeTools(read_only_batch),RunSource::Mcp).is_err());
        drop(workflow);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn evidence_intake_retires_only_stale_unstarted_approvals_and_keeps_the_draft(){
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;
        fs::write(root.join("manual-work.txt"),b"valid manual edits").unwrap();
        let pending=workflow.propose(dimensions()).unwrap();let id=pending.snapshot.input.job_id();let input=serde_json::to_value(&pending.snapshot.input).unwrap();
        let reference=workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Ok(serde_json::json!({"private":"LOCAL_ONLY_SENTINEL"}))).unwrap();
        assert!(workflow.shared_task_reference(reference,0).is_err());assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        let retired=workflow.current().unwrap().unwrap();assert_eq!(retired.snapshot.phase,Phase::Cancelled);assert!(retired.approval_id.is_none());
        assert!(retired.snapshot.audit.as_ref().unwrap().approval_invalidated);
        assert_eq!(serde_json::to_value(&retired.snapshot.input).unwrap(),input);assert!(retired.error.unwrap().contains("sin ejecutar"));
        assert!(!root.join(".hazlomexa/staging").join(id.to_string()).exists());
        let fresh=workflow.propose(dimensions()).unwrap();assert_ne!(fresh.snapshot.input.job_id(),id);assert!(fresh.approval_id.is_some());
        let history=workflow.view().unwrap().history;assert_eq!(history.len(),2);assert_eq!(serde_json::to_value(&history.iter().find(|s|s.input.job_id()==id).unwrap().input).unwrap(),input);
        assert_eq!(fs::read(root.join("manual-work.txt")).unwrap(),b"valid manual edits");
        drop(workflow);let project=Project::open(&root).unwrap();let recovered=project.recover().unwrap();
        assert_eq!(recovered.iter().find(|s|s.input.job_id()==id).unwrap().phase,Phase::Cancelled);
        drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn sketch_selection_follows_task_identity_after_failed_creation_and_recovery(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let original=workflow.task_state().unwrap().unwrap().active;let project=workflow.view().unwrap().project.unwrap();
        let source="void setup() {} void loop() {}";workflow.set_agent_source("Original.ino".into(),source.into()).unwrap();
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(root.join(".hazlomexa/tasks/index.json")).unwrap();
        assert!(workflow.task_request(project.clone(),crate::tasks::Request::New).is_err());assert!(workflow.agent_source().is_err());drop(lock);
        assert!(workflow.agent_source().unwrap().is_none());assert_ne!(workflow.task_state().unwrap().unwrap().active.id,original.id);
        assert!(workflow.propose_agent(source.into()).unwrap_err().contains("original en este proyecto"));
        // Simulate reading a committed switch after its original reply was lost; the original remains scoped.
        {let state=workflow.state.lock().unwrap();crate::tasks::Store::new(state.project.as_ref().unwrap()).unwrap().request(crate::tasks::Request::Open{id:original.id}).unwrap();}
        assert_eq!(workflow.agent_source().unwrap().unwrap().source,source);
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn sketch_selection_cannot_cross_a_copied_project_with_the_same_uuid(){
        let (root,workflow)=setup();let source="void setup() {} void loop() {}";workflow.set_agent_source("Original.ino".into(),source.into()).unwrap();
        let copied=root.join("copied");fs::create_dir(&copied).unwrap();drop(Project::create(&copied).unwrap());fs::copy(root.join("hazlomx.json"),copied.join("hazlomx.json")).unwrap();
        let original_id=workflow.view().unwrap().project.unwrap().0;workflow.select_project(Project::open(&copied).unwrap()).unwrap();assert_eq!(workflow.view().unwrap().project.unwrap().0,original_id);
        assert!(workflow.agent_source().unwrap().is_none());assert!(workflow.propose_agent(source.into()).unwrap_err().contains("original en este proyecto"));
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn foreign_sketch_followup_is_refused_before_artifact_access(){
        let (root,workflow)=setup();let old=workflow.task_state().unwrap().unwrap().active;let project=workflow.view().unwrap().project.unwrap();let source="void setup() {} void loop() {}";
        let id=Uuid::new_v4();
        {let state=workflow.state.lock().unwrap();let p=state.project.as_ref().unwrap();let snapshot=p.snapshot(OperationInput::Agent(agent::EditInput{schema_version:1,job_id:id,agent_version:agent::VERSION.into(),source_name:"Owned.ino".into(),original_source:source.into(),source:source.into()}),Phase::Completed).unwrap();assert_eq!(snapshot.audit.as_ref().unwrap().task.as_ref().unwrap().id,old.id);p.save(&snapshot).unwrap();}
        workflow.task_request(project.clone(),crate::tasks::Request::New).unwrap();
        let error=workflow.propose_tool_agent(project.0,"Owned.ino".into(),crate::kicad_project::digest(source.as_bytes()),Some(id),source.into()).unwrap_err();
        assert!(error.contains("otra conversación"));assert!(!root.join(".hazlomexa/artifacts").join(id.to_string()).exists());
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locked_title_index_does_not_fail_a_committed_task_message(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let project=workflow.view().unwrap().project.unwrap();
        let pending=workflow.propose(dimensions()).unwrap();
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(root.join(".hazlomexa/tasks/index.json")).unwrap();
        let history=r#"{"version":1,"turns":[{"role":"user","content":"Preserve my manual edits"}]}"#;
        assert!(workflow.task_request(project,crate::tasks::Request::Message{id:task.id,revision:task.revision,text:"Preserve my manual edits".into(),history:history.into(),correction_of:None}).is_ok());
        // Task data is authoritative; updating a title no longer requires an index write.
        let state=workflow.task_state().unwrap().unwrap().active;assert_eq!(state.revision,task.revision+1);assert_eq!(state.goal,"Preserve my manual edits");
        assert_eq!(workflow.task_state().unwrap().unwrap().tasks[0].title,"Preserve my manual edits");
        assert!(workflow.begin(pending.approval_id.unwrap()).is_err());assert_eq!(workflow.current().unwrap().unwrap().snapshot.phase,Phase::Cancelled);
        drop(lock);
        let state=workflow.task_state().unwrap().unwrap().active;assert_eq!(state.revision,task.revision+1);
        assert!(workflow.propose(dimensions()).is_ok());drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locked_retirement_blocks_old_approval_and_recovers_without_repeating_intake(){
        use std::os::windows::fs::OpenOptionsExt;
        for already_committed in [false,true]{
            let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let pending=workflow.propose(dimensions()).unwrap();let id=pending.snapshot.input.job_id();
            let path=root.join(".hazlomexa/runs").join(format!("{id}.json"));
            let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap();
            assert!(workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Err("LOCAL_ERROR_SENTINEL".into())).is_err());
            assert!(workflow.state.lock().unwrap().job.as_ref().unwrap().view.approval_id.is_none());assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
            let retained=workflow.task_state().unwrap().unwrap().active;assert_eq!(retained.references.len(),1);assert_eq!(retained.revision,task.revision+1);assert!(retained.references[0].shared_with.is_empty());
            drop(lock);
            let commit_lock=if already_committed {
                let state=workflow.state.lock().unwrap();let job=state.job.as_ref().unwrap();let mut saved=job.view.clone();saved.snapshot.phase=Phase::Cancelled;saved.snapshot.audit.as_mut().unwrap().approval_invalidated=true;saved.advance();state.project.as_ref().unwrap().save(&saved.snapshot).unwrap();
                Some(fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap())
            }else{None};
            let recovered=workflow.view().unwrap();assert_eq!(recovered.job.unwrap().snapshot.phase,Phase::Cancelled);assert_eq!(recovered.history.len(),1);
            drop(commit_lock);assert_eq!(workflow.task_state().unwrap().unwrap().active.references.len(),1);assert!(workflow.propose(dimensions()).is_ok());
            drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn rejected_intake_preserves_a_current_approval_and_running_work_is_not_retired(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let pending=workflow.propose(dimensions()).unwrap();
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(root.join(".hazlomexa/tasks").join(format!("{}.json",task.id))).unwrap();
        assert!(workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Ok(serde_json::json!({"value":1}))).is_err());
        assert_eq!(workflow.current().unwrap().unwrap().approval_id,pending.approval_id);assert_eq!(workflow.task_binding().unwrap(),Some(task.binding()));drop(lock);
        let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
        workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Ok(serde_json::json!({"value":2}))).unwrap();
        let current=workflow.current().unwrap().unwrap();assert!(current.busy);assert_eq!(current.snapshot.phase,Phase::Running);assert!(!dispatch.dispatch_paused.load(Ordering::SeqCst));assert!(!dispatch.cancelled.load(Ordering::SeqCst));
        drop(dispatch);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_review_never_replaces_a_saved_possible_native_execution(){
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let pending=workflow.propose(dimensions()).unwrap();
        let path=root.join(".hazlomexa/runs").join(format!("{}.json",pending.snapshot.input.job_id()));
        let mut native=pending.snapshot.clone();native.phase=Phase::Running;native.audit.as_mut().unwrap().native_attempted=Some(true);native.advance();
        workflow.state.lock().unwrap().project.as_ref().unwrap().save(&native).unwrap();let before=fs::read(&path).unwrap();
        let mut forged=native.clone();forged.phase=Phase::Cancelled;forged.audit.as_mut().unwrap().approval_invalidated=true;
        assert!(workflow.state.lock().unwrap().project.as_ref().unwrap().save(&forged).is_err());
        assert!(workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Ok(serde_json::json!({"value":1}))).is_err());
        assert!(workflow.begin(pending.approval_id.unwrap()).is_err());assert!(workflow.propose(dimensions()).is_err());
        assert_eq!(fs::read(&path).unwrap(),before);assert!(workflow.state.lock().unwrap().job.as_ref().unwrap().view.approval_id.is_none());
        assert_eq!(workflow.task_state().unwrap().unwrap().active.references.len(),1);
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_review_preserves_a_different_saved_draft_instead_of_overwriting_it(){
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let pending=workflow.propose(dimensions()).unwrap();
        let mut edited=pending.snapshot.clone();let OperationInput::Freecad(input)=&mut edited.input else{panic!("Expected a solid draft");};input.dimensions_mm.width=47.;edited.audit.as_mut().unwrap().user_edited=true;edited.advance();
        workflow.state.lock().unwrap().project.as_ref().unwrap().save(&edited).unwrap();
        let path=root.join(".hazlomexa/runs").join(format!("{}.json",edited.input.job_id()));let before=fs::read(&path).unwrap();
        assert!(workflow.retain_tool_evidence(&task.binding(),"read_native_tool",&Ok(serde_json::json!({"value":1}))).is_err());
        assert!(workflow.begin(pending.approval_id.unwrap()).is_err());assert_eq!(fs::read(&path).unwrap(),before);
        drop(workflow);let project=Project::open(&root).unwrap();let recovered=project.recover().unwrap();
        assert_eq!(recovered[0].input.solid().unwrap().dimensions_mm.width,47.);assert!(recovered[0].audit.as_ref().unwrap().user_edited);
        drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn completed_reply_proof_binds_owned_scope_and_projects_only_authorized_text(){
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();let t=workflow.task_state().unwrap().unwrap().active;
        let history=serde_json::json!({"version":1,"turns":[{"role":"user","content":"Keep my design"}]}).to_string();
        workflow.task_request(project.clone(),crate::tasks::Request::History{id:t.id,revision:t.revision,value:Some(history)}).unwrap();
        let scope=workflow.conversation_scope().unwrap().unwrap();let id=Uuid::new_v4();
        assert!(!workflow.record_completed_reply(&scope,id,"claude","authorized answer").unwrap());
        let mut wrong=scope.clone();wrong.project.1=root.join("another-folder");assert!(!workflow.record_completed_reply(&wrong,id,"codex","authorized answer").unwrap());
        assert!(workflow.record_completed_reply(&scope,id,"codex","authorized answer").unwrap());
        assert!(workflow.record_completed_reply(&scope,id,"codex","authorized answer").unwrap());
        let t=workflow.task_state().unwrap().unwrap().active;assert_eq!(t.reply_receipts.len(),1);
        workflow.task_request(project.clone(),crate::tasks::Request::History{id:t.id,revision:t.revision,value:Some(serde_json::json!({"version":1,"turns":[{"role":"user","content":"Keep my design"},{"role":"assistant","content":"authorized answer","reply_id":id,"private_data":"KEEP_EXTRA_FIELD_LOCAL"}]}).to_string())}).unwrap();
        // An older withdrawn generation may have identical text; only this owned frame is eligible.
        let path=root.join(".hazlomexa/tasks").join(format!("{}.json",t.id));let mut saved:crate::tasks::Task=crate::project::read_json(&path).unwrap();saved.withdrawn_replies.insert(crate::kicad_project::digest(b"authorized answer"),saved.revision);crate::project::atomic_json(&path,&saved,true).unwrap();
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue".into()),None).unwrap();assert!(prompt.contains("authorized answer"));assert!(!prompt.contains("reply_id") && !prompt.contains("KEEP_EXTRA_FIELD_LOCAL") && !prompt.contains("parent_sha256"));
        let t=workflow.task_state().unwrap().unwrap().active;let saved=workflow.task_request(project,crate::tasks::Request::History{id:t.id,revision:t.revision,value:None}).unwrap();assert!(saved.as_str().unwrap().contains(&id.to_string()));
        let mut stale=scope.clone();stale.position=0;assert!(!workflow.record_completed_reply(&stale,Uuid::new_v4(),"codex","not sent again").unwrap());
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stream_corrections_fence_proposals_preserve_drafts_and_reject_old_tool_callers(){
        let (root,workflow)=setup();let workflow=Arc::new(workflow);workflow.task_state().unwrap();let scope=workflow.conversation_scope().unwrap().unwrap();let pending=workflow.propose(dimensions()).unwrap();let binding=scope.task.clone().unwrap();
        let (ready,waiting)=std::sync::mpsc::channel();let (resume,go)=std::sync::mpsc::channel();let old_scope=scope.clone();let worker=workflow.clone();let delayed=std::thread::spawn(move||with_tool_scope(old_scope.project,old_scope.task,||{ready.send(()).unwrap();go.recv().unwrap();worker.propose(dimensions())}));waiting.recv().unwrap();
        let request=crate::tasks::ChatCorrectionInput{id:binding.id,revision:binding.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Preserve the idea; use inches".into()};
        workflow.correct_chat(&scope,&request,"UNFINISHED_LOCAL_SENTINEL").unwrap();assert!(workflow.begin(pending.approval_id.unwrap()).is_err());let task=workflow.task_state().unwrap().unwrap().active;assert_eq!(task.chat_correction.unwrap().text,request.text);
        resume.send(()).unwrap();assert!(delayed.join().unwrap().is_err(),"An old tool call created a proposal under the corrected revision");
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue".into()),None).unwrap();assert!(prompt.contains(&request.text));assert!(!prompt.contains("UNFINISHED_LOCAL_SENTINEL"));
        workflow.task_request(scope.project.clone(),crate::tasks::Request::Message{id:request.id,revision:task.revision,text:"Keep my manual label".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"UNFINISHED_LOCAL_SENTINEL"},{"role":"user","content":"Keep my manual label"}]}).to_string(),correction_of:None}).unwrap();let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue".into()),None).unwrap();assert!(!prompt.contains("UNFINISHED_LOCAL_SENTINEL") && prompt.contains("Keep my manual label"));
        with_tool_scope(scope.project.clone(),Some(binding),||{assert!(workflow.propose(dimensions()).is_err());with_tool_scope(scope.project.clone(),None,||assert!(workflow.propose(dimensions()).is_err()));});
        let fresh=workflow.propose(dimensions()).unwrap();workflow.correct_chat(&scope,&request,"late body").unwrap();assert!(workflow.begin(fresh.approval_id.unwrap()).is_ok(),"A duplicate correction paused newer work");
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn owned_question_answers_fence_old_producers_and_only_expose_authorized_reference_reads(){
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let scope=workflow.conversation_scope().unwrap().unwrap();let pending=workflow.propose(dimensions()).unwrap();
        let request=crate::tasks::ChatCorrectionInput{id:task.id,revision:task.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:String::new()};
        let body=serde_json::to_vec(&serde_json::json!(["hazlo_question_answers_v1",request.correction_id,[["intent","MODEL_QUESTION_SENTINEL","PRIVATE_ANSWER_SENTINEL"]]])).unwrap();
        workflow.answer_chat(&scope,&request,&body,"UNFINISHED_LOCAL_SENTINEL").unwrap();assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        with_tool_scope(scope.project.clone(),scope.task.clone(),||assert!(workflow.propose(dimensions()).is_err()));
        let task=workflow.task_state().unwrap().unwrap().active;let reference=task.references[0].id;assert!(task.revision>request.revision);assert_eq!(task.chat_correction.as_ref().unwrap().id,request.correction_id);
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some(task.chat_correction.as_ref().unwrap().text.clone()),None).unwrap();assert!(prompt.contains(&reference.to_string()));for sentinel in ["MODEL_QUESTION_SENTINEL","PRIVATE_ANSWER_SENTINEL","UNFINISHED_LOCAL_SENTINEL"]{assert!(!prompt.contains(sentinel));}
        let project=workflow.state.lock().unwrap().project.clone().unwrap();let store=crate::tasks::Store::new(&project).unwrap();assert!(store.shared_reference(reference,0).unwrap().to_string().contains("PRIVATE_ANSWER_SENTINEL"));drop(store);drop(project);
        workflow.task_request(scope.project.clone(),crate::tasks::Request::Provider{id:task.id,revision:task.revision,provider:"claude".into()}).unwrap();let project=workflow.state.lock().unwrap().project.clone().unwrap();assert!(crate::tasks::Store::new(&project).unwrap().shared_reference(reference,0).is_err());drop(project);
        let (prompt,_)=workflow.task_prompt("claude",String::new(),Some("Continue".into()),None).unwrap();assert!(!prompt.contains("PRIVATE_ANSWER_SENTINEL"));assert!(workflow.answer_chat(&scope,&request,&body,"old draft").is_err());
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn typed_live_conditions_survive_restart_and_task_progress_does_not_expose_them(){
        let(root,workflow)=setup();workflow.task_state().unwrap();let project=workflow.state.lock().unwrap().project.clone().unwrap();
        let cad=serde_json::from_value(serde_json::json!({"schema_version":1,"live_cad_version":1,"job_id":Uuid::new_v4(),"document_name":"PRIVATE_NATIVE_TARGET_SENTINEL","document_token":Uuid::new_v4(),"state_sha256":"a".repeat(64),"description":"Owned live change","operations":[{"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}}],"before":[],"agreement":crate::native_tools::tests::agreement_fixture()})).unwrap();
        let inputs=[OperationInput::LiveCad(cad),OperationInput::LiveAgent(crate::agent_editor::tests::job_fixture()),OperationInput::LivePcb(crate::kicad_live::tests::move_fixture()),OperationInput::LiveDrawing(crate::cadcraft_live::tests::fixture_input())];let mut ids=Vec::new();
        for mut input in inputs{
            let mut value=serde_json::to_value(&input).unwrap();value["agreement"]["outcome"]=serde_json::json!("PRIVATE_CONDITIONS_SENTINEL");input=serde_json::from_value(value).unwrap();let id=input.job_id();let mut snapshot=project.snapshot(input,Phase::Cancelled).unwrap();snapshot.audit.as_mut().unwrap().source=RunSource::Mcp;project.save(&snapshot).unwrap();ids.push(id);
            let local=workflow.local_task_history(Some(id)).unwrap();assert_eq!(local["selected_batch"]["proposed_conditions"]["outcome"],"PRIVATE_CONDITIONS_SENTINEL");assert!(local.get("turns").is_none());assert!(!local.to_string().contains("PRIVATE_NATIVE_TARGET_SENTINEL"));
        }
        let progress=workflow.task_progress(0).unwrap();assert_eq!(progress["operations"].as_array().unwrap().len(),4);assert!(progress["operations"].as_array().unwrap().iter().all(|op|op["conditions_available"]==true));assert!(!progress.to_string().contains("SENTINEL"));drop(project);drop(workflow);
        let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();for id in ids{assert_eq!(workflow.local_task_history(Some(id)).unwrap()["selected_batch"]["proposed_conditions"]["outcome"],"PRIVATE_CONDITIONS_SENTINEL");}let project=workflow.view().unwrap().project.unwrap();workflow.task_request(project,crate::tasks::Request::New).unwrap();assert!(workflow.task_progress(0).unwrap()["operations"].as_array().unwrap().is_empty());drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn durable_chat_stop_fences_old_callers_and_preserves_newer_work_on_repeated_stop(){
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let task=workflow.task_state().unwrap().unwrap().active;let scope=workflow.conversation_scope().unwrap().unwrap();let pending=workflow.propose(dimensions()).unwrap();let chat_id=Uuid::new_v4();
        let old_scope=scope.clone();let worker=workflow.clone();let (ready,waiting)=std::sync::mpsc::channel();let (resume,go)=std::sync::mpsc::channel();let delayed=std::thread::spawn(move||with_tool_scope(old_scope.project,old_scope.task,||{ready.send(()).unwrap();go.recv().unwrap();worker.propose(dimensions())}));waiting.recv().unwrap();workflow.stop_chat(&scope,chat_id,"STOPPED_TOOL_DRAFT_SENTINEL").unwrap();resume.send(()).unwrap();assert!(delayed.join().unwrap().is_err());assert!(workflow.begin(pending.approval_id.unwrap()).is_err());let stopped=workflow.task_state().unwrap().unwrap().active;assert_eq!(stopped.chat_stop.unwrap().chat_id,chat_id);assert_eq!(stopped.revision,task.revision+1);
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue after checking the prior work".into()),None).unwrap();assert!(!prompt.contains("STOPPED_TOOL_DRAFT_SENTINEL"));let fresh=workflow.propose(dimensions()).unwrap();workflow.stop_chat(&scope,chat_id,"changed late text").unwrap();assert!(workflow.begin(fresh.approval_id.unwrap()).is_ok());
        drop(workflow);let project=Project::open(&root).unwrap();assert!(project.recover().unwrap().iter().all(|snapshot|snapshot.phase!=Phase::Running));assert_eq!(crate::tasks::Store::new(&project).unwrap().active().unwrap().unwrap().chat_stop.unwrap().chat_id,chat_id);drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_stop_without_a_job_fences_delayed_tool_producers(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let task=workflow.task_state().unwrap().unwrap().active;let scope=workflow.conversation_scope().unwrap().unwrap();let path=root.join(".hazlomexa/tasks").join(format!("{}.json",task.id));let before=fs::read(&path).unwrap();
        let old_scope=scope.clone();let worker=workflow.clone();let (ready,waiting)=std::sync::mpsc::channel();let (resume,go)=std::sync::mpsc::channel();let delayed=std::thread::spawn(move||with_tool_scope(old_scope.project,old_scope.task,||{ready.send(()).unwrap();go.recv().unwrap();worker.propose(dimensions())}));waiting.recv().unwrap();
        let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap();assert!(workflow.stop_chat(&scope,Uuid::new_v4(),"local unfinished reply").is_err());resume.send(()).unwrap();assert!(delayed.join().unwrap().is_err());assert_eq!(fs::read(&path).unwrap(),before);drop(lock);
        let project=workflow.view().unwrap().project.unwrap();workflow.task_request(project,crate::tasks::Request::Message{id:task.id,revision:task.revision,text:"Continue with a fresh review".into(),history:r#"{"version":1,"turns":[{"role":"user","content":"Continue with a fresh review"}]}"#.into(),correction_of:None}).unwrap();let fresh=workflow.conversation_scope().unwrap().unwrap();assert!(with_tool_scope(fresh.project,fresh.task,||workflow.propose(dimensions())).is_ok());
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_stop_save_still_fences_the_pending_approval_and_successful_retry_saves_once(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let scope=workflow.conversation_scope().unwrap().unwrap();let pending=workflow.propose(dimensions()).unwrap();let path=root.join(".hazlomexa/tasks").join(format!("{}.json",task.id));let before=fs::read(&path).unwrap();let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap();let chat_id=Uuid::new_v4();assert!(workflow.stop_chat(&scope,chat_id,"owned local draft").is_err());assert!(workflow.begin(pending.approval_id.unwrap()).is_err());assert_eq!(fs::read(&path).unwrap(),before);drop(lock);workflow.stop_chat(&scope,chat_id,"owned local draft").unwrap();let stopped=fs::read(&path).unwrap();workflow.stop_chat(&scope,chat_id,"different late draft").unwrap();assert_eq!(fs::read(&path).unwrap(),stopped);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_stream_correction_still_fences_the_unstarted_approval_and_can_recover_forward(){
        use std::os::windows::fs::OpenOptionsExt;
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let scope=workflow.conversation_scope().unwrap().unwrap();let pending=workflow.propose(dimensions()).unwrap();let path=root.join(".hazlomexa/tasks").join(format!("{}.json",task.id));let before=fs::read(&path).unwrap();let lock=fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap();
        let request=crate::tasks::ChatCorrectionInput{id:task.id,revision:task.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Keep all manual changes".into()};assert!(workflow.correct_chat(&scope,&request,"local partial reply").is_err());assert_eq!(fs::read(&path).unwrap(),before);assert!(workflow.begin(pending.approval_id.unwrap()).is_err());drop(lock);
        workflow.correct_chat(&scope,&request,"local partial reply").unwrap();drop(workflow);let project=Project::open(&root).unwrap();assert_ne!(project.recover().unwrap()[0].phase,Phase::Running);assert_eq!(crate::tasks::Store::new(&project).unwrap().active().unwrap().unwrap().chat_correction.unwrap().id,request.correction_id);drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn prompt_history_projects_only_authorized_role_and_text_and_keeps_other_fields_local(){
        let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let project=workflow.view().unwrap().project.unwrap();
        let mut raw=serde_json::json!({"version":1,"turns":[{"role":"user","content":"Keep the intended dimensions","capture":"PRIVATE_UNKNOWN_USER_FIELD"},{"role":"assistant","content":"I will inspect native state","private_log":"PRIVATE_UNKNOWN_ASSISTANT_FIELD","image":{"data":"PRIVATE_UNKNOWN_IMAGE_FIELD"}}]});
        workflow.task_request(project.clone(),crate::tasks::Request::Message{id:task.id,revision:task.revision,text:"Keep the intended dimensions".into(),history:serde_json::json!({"version":1,"turns":[raw["turns"][0]]}).to_string(),correction_of:None}).unwrap();let scope=workflow.conversation_scope().unwrap().unwrap();let reply=Uuid::new_v4();assert!(workflow.record_completed_reply(&scope,reply,"codex","I will inspect native state").unwrap());raw["turns"][1]["reply_id"]=serde_json::json!(reply);
        let task=workflow.task_state().unwrap().unwrap().active;workflow.task_request(project.clone(),crate::tasks::Request::History{id:task.id,revision:task.revision,value:Some(raw.to_string())}).unwrap();
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue the task".into()),None).unwrap();let sent:serde_json::Value=serde_json::from_str(&prompt).unwrap();
        assert_eq!(sent["recovered_conversation"],serde_json::json!([{"role":"user","content":"Keep the intended dimensions"},{"role":"assistant","content":"I will inspect native state"}]));
        assert!(!prompt.contains("PRIVATE_UNKNOWN_"));
        let retained=workflow.task_request(project,crate::tasks::Request::History{id:task.id,revision:task.revision,value:None}).unwrap();assert!(retained.as_str().unwrap().contains("PRIVATE_UNKNOWN_IMAGE_FIELD"));
        // Restored metadata is still local data, not an authorization to resend a withdrawn reply.
        let path=root.join(".hazlomexa/tasks").join(format!("{}.json",task.id));
        let mut restored:crate::tasks::Task=crate::project::read_json(&path).unwrap();
        restored.withdrawn_replies.insert(crate::kicad_project::digest(b"WITHDRAWN_HISTORY_SENTINEL"),restored.revision);
        restored.history=Some(serde_json::json!({"version":1,"turns":[{"role":"user","content":"Keep my current intent"},{"role":"assistant","content":"WITHDRAWN_HISTORY_SENTINEL"},{"role":"assistant","content":"FOREIGN_LINEAGE_SENTINEL","reference_ids":[Uuid::new_v4()]}]}).to_string());
        crate::project::atomic_json(&path,&restored,true).unwrap();let local=fs::read(&path).unwrap();
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue using authorized facts".into()),None).unwrap();
        assert!(!prompt.contains("WITHDRAWN_HISTORY_SENTINEL") && !prompt.contains("FOREIGN_LINEAGE_SENTINEL") && !prompt.contains("Keep my current intent"));assert_eq!(fs::read(&path).unwrap(),local);
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn saved_batch_conditions_survive_restart_and_are_available_locally_without_the_transcript(){
        let (root,workflow)=setup();let selected=workflow.view().unwrap().project.unwrap();let task=workflow.task_state().unwrap().unwrap().active;workflow.task_request(selected.clone(),crate::tasks::Request::Message{id:task.id,revision:task.revision,text:"Preserve the original and make a provisional draft".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"PRIVATE_TRANSCRIPT_SENTINEL"}]}).to_string(),correction_of:None}).unwrap();let(fixture,mut input,_)=crate::native_tools::tests::recorded_fixture("all_acknowledged");input.agreement=Some(crate::native_tools::tests::agreement_fixture());let id=input.job_id;let project=workflow.state.lock().unwrap().project.clone().unwrap();let mut snapshot=project.snapshot(OperationInput::NativeTools(input),Phase::Cancelled).unwrap();snapshot.audit.as_mut().unwrap().source=RunSource::Mcp;project.save(&snapshot).unwrap();drop(project);let local=workflow.local_task_history(Some(id)).unwrap();assert_eq!(local["selected_batch"]["proposed_conditions"]["kind"],"provisional_draft");assert!(!local.to_string().contains("PRIVATE_TRANSCRIPT_SENTINEL") && !local.to_string().contains("PRIVATE_NATIVE_SENTINEL"));assert!(local.get("turns").is_none());assert_eq!(workflow.task_progress(0).unwrap()["operations"][0]["conditions_available"],true);drop(workflow);
        let reopened=Workflow::new(root.join("unused.exe"),root.join("profiles")).unwrap();reopened.select_project(Project::open(&root).unwrap()).unwrap();assert_eq!(reopened.local_task_history(Some(id)).unwrap(),local);let selected=reopened.view().unwrap().project.unwrap();reopened.task_request(selected,crate::tasks::Request::New).unwrap();assert!(reopened.local_task_history(Some(id)).is_err());drop(reopened);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()) && fixture.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();fs::remove_dir_all(fixture).unwrap();
    }
    #[test]
    fn unproven_transcript_text_and_legacy_intent_stay_local_until_explicit_snapshot_sharing(){
        use crate::tasks::Request;
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();let task=workflow.task_state().unwrap().unwrap().active;
        let history=serde_json::json!({"version":1,"turns":[{"role":"user","content":"UNPROVEN_USER_SENTINEL","private":"HIDDEN_METADATA_SENTINEL"},{"role":"assistant","content":"UNPROVEN_REPLY_SENTINEL","instruction_ids":[],"reference_ids":[]}]});
        workflow.task_request(project.clone(),Request::History{id:task.id,revision:task.revision,value:Some(history.to_string())}).unwrap();
        let path=root.join(".hazlomexa/tasks").join(format!("{}.json",task.id));let mut task:crate::tasks::Task=crate::project::read_json(&path).unwrap();task.goal="LEGACY_GOAL_SENTINEL".into();crate::project::atomic_json(&path,&task,true).unwrap();let original=fs::read(&path).unwrap();
        let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue public research".into()),None).unwrap();assert!(!prompt.contains("UNPROVEN_") && !prompt.contains("LEGACY_GOAL_SENTINEL") && !prompt.contains("HIDDEN_METADATA_SENTINEL"));assert!(prompt.contains("read_task_history"));assert_eq!(fs::read(&path).unwrap(),original);
        let local=workflow.local_task_history(None).unwrap();assert!(local.to_string().contains("UNPROVEN_USER_SENTINEL") && local.to_string().contains("UNPROVEN_REPLY_SENTINEL"));assert_eq!(local["historical_goal"],"LEGACY_GOAL_SENTINEL");assert!(!local.to_string().contains("HIDDEN_METADATA_SENTINEL"));assert_eq!(local["historical_context_only"],true);assert_eq!(fs::read(&path).unwrap(),original);
        // Changing an old reply's role cannot make it an owned human instruction.
        workflow.task_request(project.clone(),Request::History{id:task.id,revision:task.revision,value:Some(serde_json::json!({"version":1,"turns":[{"role":"user","content":"UNPROVEN_REPLY_SENTINEL"}]}).to_string())}).unwrap();let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue".into()),None).unwrap();assert!(!prompt.contains("UNPROVEN_REPLY_SENTINEL"));
        // A new explicit human submission may intentionally authorize those exact bytes.
        workflow.task_request(project,Request::Message{id:task.id,revision:task.revision,text:"UNPROVEN_REPLY_SENTINEL".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"UNPROVEN_REPLY_SENTINEL"}]}).to_string(),correction_of:None}).unwrap();let (prompt,_)=workflow.task_prompt("codex",String::new(),Some("Continue".into()),None).unwrap();assert!(prompt.contains("UNPROVEN_REPLY_SENTINEL"));
        drop(workflow);let p=Project::open(&root).unwrap();let saved=crate::tasks::Store::new(&p).unwrap().active().unwrap().unwrap();assert_eq!(saved.goal,"LEGACY_GOAL_SENTINEL");assert_eq!(saved.instructions.len(),1);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tasks_bind_approval_and_keep_restricted_content_out_of_prompts(){
        use crate::tasks::Request;
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let project=workflow.view().unwrap().project.unwrap();
        let task=workflow.task_state().unwrap().unwrap().active;
        let proposed=workflow.propose(dimensions()).unwrap();assert_eq!(proposed.snapshot.audit.as_ref().unwrap().task.as_ref().unwrap().id,task.id);
        let approval=proposed.approval_id.unwrap();
        workflow.task_request(project.clone(),Request::Message{id:task.id,revision:task.revision,text:"Preserve my edits".into(),history:"{\"version\":1,\"turns\":[]}".into(),correction_of:None}).unwrap();
        assert!(workflow.begin(approval).is_err());
        let (prompt,_)=workflow.task_prompt("codex","selected source or automatic error: NO_CONSENT_SENTINEL".into(),Some("Research public information".into()),None).unwrap();
        assert!(!prompt.contains("NO_CONSENT_SENTINEL"));
        let (prompt,_)=workflow.task_prompt("codex","native error: NO_CONSENT_SENTINEL".into(),None,None).unwrap();
        assert!(!prompt.contains("NO_CONSENT_SENTINEL"));
        let task=workflow.task_state().unwrap().unwrap().active;
        workflow.task_request(project.clone(),Request::AddReference{id:task.id,revision:task.revision,name:"capture.bin".into(),data:base64::Engine::encode(&base64::engine::general_purpose::STANDARD,b"private sentinel"),preview:None}).unwrap();
        let (prompt,isolated)=workflow.task_prompt("codex","native error contains private sentinel".into(),None,None).unwrap();
        assert!(isolated && !prompt.contains("private sentinel"));
        let (prompt,_)=workflow.task_prompt("codex","history contains private sentinel".into(),Some("Research public documents".into()),None).unwrap();
        assert!(!prompt.contains("private sentinel") && prompt.contains("Research public documents") && prompt.contains("Preserve my edits"));
        assert!(workflow.task_prompt("claude","hello".into(),None,None).is_err());
        workflow.task_request(project,Request::New).unwrap();assert!(!workflow.task_allows_snapshot(&proposed.snapshot).unwrap());
        let host=Arc::new(crate::host_tools::HostTools::new(workflow.clone()));
        let tools=crate::host_tools::McpTools::new(host.clone()).unwrap();
        assert!(host.call("propose_cad_document_change",serde_json::json!({"base_job_id":proposed.snapshot.input.job_id()})).unwrap_err().contains("otra conversación"));
        workflow.task_request(workflow.view().unwrap().project.unwrap(),Request::New).unwrap();
        assert!(tools.check_scope().is_err());
        drop(tools);drop(host);drop(workflow);std::fs::remove_dir_all(root).unwrap();
    }
    use super::*;
    #[test]
    fn conversation_history_is_bound_to_current_project_and_folder() {
        let (root, workflow)=setup();
        let project=workflow.view().unwrap().project.unwrap();
        let raw=r#"{"version":1,"turns":[{"role":"user","content":"Keep my constraints"}]}"#;
        workflow.conversation_history(project.clone(),"codex",Some(raw)).unwrap();
        assert!(workflow.conversation_history((Uuid::new_v4(),root.clone()),"codex",Some(raw)).is_err());
        assert!(workflow.conversation_history((project.0,root.join("other")),"codex",Some(raw)).is_err());
        assert_eq!(workflow.conversation_history(project,"codex",None).unwrap().as_deref(),Some(raw));
    }
    #[test]
    fn native_agent_jobs_keep_credentials_private_and_bind_approval_report_and_recovery() {
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();
        let input=crate::agent_editor::tests::job_fixture();
        let connection=crate::agent_editor::tests::connection_fixture(&input);
        assert!(workflow.propose_live_agent((Uuid::new_v4(),root.clone()),input.clone(),connection.clone()).is_err());
        let pending=workflow.propose_live_agent(project,input.clone(),connection).unwrap();
        assert_eq!(pending.snapshot.schema_version,15);
        let serialized=serde_json::to_string(&pending.snapshot).unwrap();
        assert!(!serialized.contains(&"b".repeat(64)) && !serialized.contains("pipe") && !serialized.contains("\"token\":"));
        assert_eq!(fs::read_dir(root.join(".hazlomexa/staging")).unwrap().count(),0);
        assert!(workflow.begin(Uuid::new_v4()).is_err());let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();assert!(dispatch.live_agent.is_some());
        assert!(workflow.begin(approval).is_err());let stage=dispatch.project.reserve_stage(input.job_id).unwrap();
        let bytes=serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap();
        fs::write(stage.path.join("result.json"),&bytes).unwrap();
        let completed=workflow.finish(dispatch,Ok(stage)).unwrap();assert_eq!(completed.snapshot.phase,Phase::Completed,"{:?}",completed.error);
        assert_eq!(workflow.artifact(input.job_id,ArtifactFormat::LiveAgentReport).unwrap().read_text().unwrap().as_bytes(),bytes);
        let report=root.join(".hazlomexa/artifacts").join(input.job_id.to_string()).join("result.json");
        fs::write(&report,b"changed report").unwrap();assert!(workflow.artifact(input.job_id,ArtifactFormat::LiveAgentReport).is_err());
        fs::write(&report,bytes).unwrap();drop(workflow);
        let project=Project::open(&root).unwrap();let history=project.recover().unwrap();
        assert_eq!(history[0].schema_version,15);assert_eq!(history[0].phase,Phase::Completed);drop(project);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_agent_cancelled_dispatch_never_claims_undo_or_publishes_a_result() {
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();
        let input=crate::agent_editor::tests::job_fixture();let connection=crate::agent_editor::tests::connection_fixture(&input);
        let pending=workflow.propose_live_agent(project,input.clone(),connection).unwrap();let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();workflow.cancel(input.job_id).unwrap();
        let result=workflow.finish(dispatch,Err("Stopped awaiting native response".into())).unwrap();
        assert_eq!(result.snapshot.phase,Phase::Cancelled);assert!(result.error.unwrap().contains("Cancelar no deshace"));
        assert!(workflow.begin(approval).is_err());assert!(workflow.artifact(input.job_id,ArtifactFormat::LiveAgentReport).is_err());
        drop(workflow);let project=Project::open(&root).unwrap();assert_eq!(project.recover().unwrap()[0].phase,Phase::Cancelled);drop(project);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chat_recovers_recorded_catalog_batches_without_an_editor_or_replaying_calls(){
        let (fixture,input,_)=crate::native_tools::tests::recorded_fixture("all_acknowledged");let (root,workflow)=setup();let task=workflow.task_state().unwrap().unwrap().active;let expected=workflow.view().unwrap().project.unwrap();let project=workflow.state.lock().unwrap().project.clone().unwrap();let id=input.job_id;let stage=project.reserve_stage(id).unwrap();for entry in fs::read_dir(&fixture).unwrap(){let entry=entry.unwrap();fs::copy(entry.path(),stage.path.join(entry.file_name())).unwrap();}let mut snapshot=project.snapshot(OperationInput::NativeTools(input),Phase::Interrupted).unwrap();snapshot.audit.as_mut().unwrap().source=RunSource::Mcp;snapshot.audit.as_mut().unwrap().native_attempted=Some(true);project.save(&snapshot).unwrap();fs::write(root.join("manual.native"),b"keep manual work").unwrap();drop(stage);drop(project);
        let task_before=workflow.task_state().unwrap().unwrap().active;let result=workflow.recover_native_result(expected.clone(),id,Arc::new(AtomicBool::new(false))).unwrap();assert_eq!(result["recovered"],true);assert_eq!(result["published"],true);assert_eq!(result["edit_repeated"],false);assert!(!result.to_string().contains("PRIVATE_NATIVE_SENTINEL"));assert_eq!(workflow.task_state().unwrap().unwrap().active.revision,task_before.revision);assert_eq!(workflow.task_state().unwrap().unwrap().active.id,task.id);let bytes=workflow.artifact(id,ArtifactFormat::NativeToolsReport).unwrap().read_text().unwrap();assert!(bytes.contains("PRIVATE_NATIVE_SENTINEL"));assert_eq!(workflow.recover_native_result(expected.clone(),id,Arc::new(AtomicBool::new(false))).unwrap()["published"],true);assert_eq!(workflow.artifact(id,ArtifactFormat::NativeToolsReport).unwrap().read_text().unwrap(),bytes);assert_eq!(fs::read(root.join("manual.native")).unwrap(),b"keep manual work");assert!(!workflow.native_recovery().unwrap()[0].needs_review);
        workflow.task_request(expected.clone(),crate::tasks::Request::New).unwrap();assert!(workflow.recover_native_result(expected,id,Arc::new(AtomicBool::new(false))).is_err());drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()) && fixture.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();fs::remove_dir_all(fixture).unwrap();
    }
    #[test]
    fn recovered_native_receipts_publish_without_replay_and_preserve_occupied_outputs(){
        for blocked in [false,true]{
            let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();workflow.task_state().unwrap();
            let task=workflow.task_state().unwrap().unwrap().active;workflow.task_request(project.clone(),crate::tasks::Request::Message{id:task.id,revision:task.revision,text:"Keep this requirement local after withdrawal".into(),history:r#"{"version":1,"turns":[]}"#.into(),correction_of:None}).unwrap();let original_sources=workflow.task_state().unwrap().unwrap().active.instruction_sources();
            let input=crate::cadpilot_live::Input{working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:Uuid::new_v4(),document_name:"Owned".into(),document_token:Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Keep the approved change".into(),operations:vec![serde_json::json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}})],before:serde_json::json!([]),receipt_sha256:None};let id=input.job_id;
            let pending=workflow.propose_live_cad(project.clone(),input.clone()).unwrap();let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();let stage=dispatch.project.reserve_stage(id).unwrap();
            let fence=crate::live_dispatch::Dispatch::start(&stage.path,&dispatch.snapshot.input,dispatch.dispatch_gate.clone(),dispatch.dispatch_paused.clone(),dispatch.cancelled.clone()).unwrap();drop(fence.enter().unwrap());
            let report=serde_json::json!({"live_cad_version":1,"job_id":id,"document_token":input.document_token,"before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":"b".repeat(64),"objects":[]},"operations":[{"action":"edit_object","success":true}],"undo_available":true});fs::write(stage.path.join("result.json"),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
            let mut checked=dispatch.snapshot.input.clone();checked.retain_receipt(&stage.path).unwrap();dispatch.project.validate_outputs(&stage.path,&checked).unwrap();crate::live_dispatch::recover_checked(&stage.path,&checked).unwrap();
            let destination=dispatch.project.artifact_directory(id);if blocked{fs::create_dir(&destination).unwrap();fs::write(destination.join("manual.native"),b"preserve manual output").unwrap();}
            drop(stage);drop(fence);workflow.finish(dispatch,Err("lost native reply".into())).unwrap();
            let mut foreign=project.clone();foreign.1=root.join("copied-project");assert!(workflow.recover_native_result(foreign,id,Arc::new(AtomicBool::new(false))).is_err());
            let recovered=workflow.recover_native_result(project.clone(),id,Arc::new(AtomicBool::new(false))).unwrap();assert_eq!(recovered["recovered"],true);assert_eq!(recovered["published"],!blocked);assert!(!workflow.native_recovery().unwrap()[0].needs_review);
            if blocked{assert_eq!(fs::read(destination.join("manual.native")).unwrap(),b"preserve manual output");fs::remove_file(destination.join("manual.native")).unwrap();fs::remove_dir(&destination).unwrap();assert_eq!(workflow.recover_native_result(project.clone(),id,Arc::new(AtomicBool::new(false))).unwrap()["published"],true);}
            let t=workflow.task_state().unwrap().unwrap().active;workflow.task_request(project.clone(),crate::tasks::Request::RemoveInstructions{id:t.id,revision:t.revision,statements:vec![original_sources[0].id]}).unwrap();let t=workflow.task_state().unwrap().unwrap().active;let evidence=workflow.retain_tool_evidence(&t.binding(),"read_verified_result",&Ok(serde_json::json!({"job_id":id,"content":"original local native report"}))).unwrap();let t=workflow.task_state().unwrap().unwrap().active;let retained=t.references.iter().find(|reference|reference.id==evidence).unwrap();assert_eq!(retained.instruction_sources,original_sources);assert!(retained.shared_with.is_empty());
            workflow.task_request(project.clone(),crate::tasks::Request::Message{id:t.id,revision:t.revision,text:"Preserve my latest intent".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"Preserve my latest intent"}]}).to_string(),correction_of:None}).unwrap();let revised=workflow.task_state().unwrap().unwrap().active;
            assert_eq!(workflow.recover_native_result(project.clone(),id,Arc::new(AtomicBool::new(false))).unwrap()["published"],true);let retained=workflow.task_state().unwrap().unwrap().active;assert_eq!(retained.revision,revised.revision);assert_eq!(retained.goal,revised.goal);
            assert!(workflow.begin(pending.approval_id.unwrap()).is_err());let t=workflow.task_state().unwrap().unwrap().active;workflow.task_request(project.clone(),crate::tasks::Request::New).unwrap();assert_ne!(workflow.task_state().unwrap().unwrap().active.id,t.id);assert!(workflow.recover_native_result(project,id,Arc::new(AtomicBool::new(false))).is_err());
            drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn chat_recovers_an_admitted_agent_result_without_source_egress_or_edit_replay(){
        use tokio::{io::{AsyncReadExt,AsyncWriteExt},net::windows::named_pipe::ServerOptions};
        let (root,workflow)=setup();let workflow=Arc::new(workflow);let project=workflow.view().unwrap().project.unwrap();workflow.task_state().unwrap();
        let input=crate::agent_editor::tests::job_fixture();let id=input.job_id;let pipe=format!(r"\\.\pipe\hazlomexa-arduino-{}",input.before.instance_id);let mut server=ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&pipe).unwrap();
        let pending=workflow.propose_live_agent(project.clone(),input.clone(),crate::agent_editor::tests::connection_fixture(&input)).unwrap();let mut dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);dispatch.project.save(&dispatch.snapshot).unwrap();
        let stage=dispatch.project.reserve_stage(id).unwrap();let fence=crate::live_dispatch::Dispatch::start(&stage.path,&dispatch.snapshot.input,dispatch.dispatch_gate.clone(),dispatch.dispatch_paused.clone(),dispatch.cancelled.clone()).unwrap();drop(fence.enter().unwrap());drop(fence);drop(stage);workflow.finish(dispatch,Err("lost outer reply".into())).unwrap();assert!(workflow.native_recovery().unwrap()[0].needs_review);
        let expected=input.clone();let worker=tokio::spawn(async move{
            server.connect().await.unwrap();let mut bytes=Vec::new();loop{let byte=server.read_u8().await.unwrap();if byte==b'\n'{break;}bytes.push(byte);assert!(bytes.len()<512*1024);}
            let message:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(message["request"]["action"],"recover");assert_eq!(message["request"]["original"]["job_id"],expected.job_id.to_string());assert_eq!(message["request"]["original"]["source"],expected.source);
            let mut report=crate::agent_editor::tests::report_fixture(&expected);report["after"]["source"]=serde_json::json!(expected.source);server.write_all(&serde_json::to_vec(&serde_json::json!({"ok":true,"result":report})).unwrap()).await.unwrap();
        });
        let host=Arc::new(crate::host_tools::HostTools::new(workflow.clone()));let tools=crate::host_tools::McpTools::for_chat(host.clone(),project.clone(),Some(crate::host_tools::Program::Planning),"codex").unwrap();
        let recovered=tools.call_in_project("recover_native_result".into(),serde_json::json!({"job_id":id}),Arc::new(AtomicBool::new(false))).await.unwrap();worker.await.unwrap();assert_eq!(recovered["recovered"],true);assert_eq!(recovered["published"],true);assert!(!recovered.to_string().contains(&input.source));assert!(!workflow.native_recovery().unwrap()[0].needs_review);assert_eq!(workflow.current().unwrap().unwrap().snapshot.phase,Phase::Completed);
        let again=tools.call_in_project("recover_native_result".into(),serde_json::json!({"job_id":id}),Arc::new(AtomicBool::new(false))).await.unwrap();assert_eq!(again["published"],true);assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        let report=workflow.artifact(id,ArtifactFormat::LiveAgentReport).unwrap().read_text().unwrap();assert!(!report.contains("\"source\""));
        drop(tools);drop(host);drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checked_live_result_survives_stop_and_publication_failure_without_replay() {
        for blocked in [false,true] {
            let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();
            let input=crate::agent_editor::tests::job_fixture();let id=input.job_id;
            let pending=workflow.propose_live_agent(project,input.clone(),crate::agent_editor::tests::connection_fixture(&input)).unwrap();
            let mut dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
            dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);dispatch.project.save(&dispatch.snapshot).unwrap();
            let stage=dispatch.project.reserve_stage(id).unwrap();let bytes=serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap();fs::write(stage.path.join("result.json"),&bytes).unwrap();
            let destination=dispatch.project.artifact_directory(id);
            if blocked {fs::create_dir(&destination).unwrap();fs::write(destination.join("preserve.native"),b"valid unrelated output").unwrap();}
            workflow.cancel(id).unwrap();
            let done=workflow.finish(dispatch,Ok(stage)).unwrap();
            assert_eq!(done.snapshot.phase,if blocked{Phase::Interrupted}else{Phase::Completed});
            assert!(done.snapshot.input.receipt_sha256().is_some());assert!(done.snapshot.audit.as_ref().unwrap().native_attempted.unwrap());
            assert!(!workflow.native_recovery().unwrap()[0].needs_review);
            assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
            if blocked {
                assert!(destination.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()));
                assert_eq!(fs::read(destination.join("preserve.native")).unwrap(),b"valid unrelated output");
                fs::remove_file(destination.join("preserve.native")).unwrap();fs::remove_dir(&destination).unwrap();
            }
            drop(workflow);let project=Project::open(&root).unwrap();assert_eq!(project.recover().unwrap()[0].phase,Phase::Completed);
            assert_eq!(fs::read(destination.join("result.json")).unwrap(),bytes);
            drop(project);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn incomplete_live_adapter_blocks_new_mutations_after_restart() {
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();
        let input=crate::agent_editor::tests::job_fixture();let id=input.job_id;
        let pending=workflow.propose_live_agent(project.clone(),input.clone(),crate::agent_editor::tests::connection_fixture(&input)).unwrap();
        let mut dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
        dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);dispatch.project.save(&dispatch.snapshot).unwrap();
        let stage=dispatch.project.reserve_stage(id).unwrap();fs::write(stage.path.join("partial.local"),b"valid partial work").unwrap();
        workflow.cancel(id).unwrap();workflow.finish(dispatch,Err("Lost native reply".into())).unwrap();
        assert!(workflow.native_recovery().unwrap()[0].needs_review);
        let next=crate::agent_editor::tests::job_fixture();assert!(workflow.propose_live_agent(project.clone(),next.clone(),crate::agent_editor::tests::connection_fixture(&next)).unwrap_err().contains("lote nativo incompleto"));
        let summary=workflow.native_recovery().unwrap();assert_eq!(summary[0].program,Some(crate::host_tools::Program::Agent));assert_eq!(summary[0].kind,"live_adapter");
        drop(stage);drop(workflow);
        let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();workflow.task_state().unwrap();
        workflow.task_request(project.clone(),crate::tasks::Request::New).unwrap();
        assert!(workflow.native_recovery().unwrap()[0].needs_review);
        let next=crate::agent_editor::tests::job_fixture();assert!(workflow.propose_live_agent(project,next.clone(),crate::agent_editor::tests::connection_fixture(&next)).unwrap_err().contains("lote nativo incompleto"));
        assert_eq!(fs::read(root.join(".hazlomexa/staging").join(id.to_string()).join("partial.local")).unwrap(),b"valid partial work");
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn altered_live_report_does_not_get_a_completion_receipt_or_release_the_write_hold() {
        let (root,workflow)=setup();let selected=workflow.view().unwrap().project.unwrap();
        let input=crate::agent_editor::tests::job_fixture();let id=input.job_id;
        let pending=workflow.propose_live_agent(selected,input.clone(),crate::agent_editor::tests::connection_fixture(&input)).unwrap();
        let mut dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);dispatch.project.save(&dispatch.snapshot).unwrap();
        let stage=dispatch.project.reserve_stage(id).unwrap();let mut report=crate::agent_editor::tests::report_fixture(&input);report["job_id"]=serde_json::json!(Uuid::new_v4());
        let bytes=serde_json::to_vec_pretty(&report).unwrap();fs::write(stage.path.join("result.json"),&bytes).unwrap();
        let done=workflow.finish(dispatch,Ok(stage)).unwrap();assert_eq!(done.snapshot.phase,Phase::Failed);
        assert!(done.snapshot.input.receipt_sha256().is_none());assert!(!done.snapshot.audit.as_ref().unwrap().files_verified);
        assert!(workflow.native_recovery().unwrap()[0].needs_review);
        assert_eq!(fs::read(root.join(".hazlomexa/staging").join(id.to_string()).join("result.json")).unwrap(),bytes);
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changing_projects_preserves_the_native_uncertainty_warning_in_old_history() {
        let (root,workflow)=setup();let project=workflow.view().unwrap().project.unwrap();
        let input=crate::agent_editor::tests::job_fixture();let connection=crate::agent_editor::tests::connection_fixture(&input);
        let pending=workflow.propose_live_agent(project,input.clone(),connection).unwrap();
        let dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();
        let next=root.join("next-project");fs::create_dir(&next).unwrap();
        workflow.select_project(Project::create(&next).unwrap()).unwrap();
        assert!(workflow.finish(dispatch,Err("Stopped old native request".into())).unwrap_err().contains("puede haber terminado"));
        assert!(workflow.view().unwrap().history.is_empty());drop(workflow);
        let project=Project::open(&root).unwrap();let history=project.recover().unwrap();
        assert_eq!(history[0].phase,Phase::Cancelled);
        assert!(history[0].audit.as_ref().unwrap().error.as_ref().unwrap().contains("Cancelar no deshace"));drop(project);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_drawing_completion_recovers_after_restart_without_a_live_connection(){
        let (root,workflow)=setup();let selected=workflow.view().unwrap().project.unwrap();workflow.task_state().unwrap();let (input,proof)=crate::cadcraft_live::tests::recorded_fixture();let id=input.job_id;
        let pending=Workflow::propose_operation_locked(&mut workflow.state.lock().unwrap(),OperationInput::LiveDrawing(input),RunSource::Mcp).unwrap();let mut snapshot=pending.snapshot.clone();snapshot.phase=Phase::Running;snapshot.audit.as_mut().unwrap().native_attempted=Some(true);let project=workflow.state.lock().unwrap().project.clone().unwrap();project.save(&snapshot).unwrap();let stage=project.reserve_stage(id).unwrap();let fence=crate::live_dispatch::Dispatch::start(&stage.path,&snapshot.input,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();drop(fence.enter().unwrap());fs::write(stage.path.join("drawing-completion.json"),serde_json::to_vec(&proof).unwrap()).unwrap();if let OperationInput::LiveDrawing(input)=&snapshot.input{fence.retain_drawing_completion(input,&proof).unwrap();}drop(fence);drop(stage);drop(project);drop(workflow);
        let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();assert!(!workflow.native_recovery().unwrap()[0].needs_review);let recovered=workflow.recover_native_result(selected.clone(),id,Arc::new(AtomicBool::new(false))).unwrap();assert_eq!(recovered["published"],true);assert_eq!(recovered["edit_repeated"],false);assert!(!workflow.native_recovery().unwrap()[0].needs_review);assert!(workflow.artifact(id,ArtifactFormat::LiveDrawingReport).unwrap().read_text().unwrap().contains("\"saved\": false"));assert_eq!(workflow.recover_native_result(selected,id,Arc::new(AtomicBool::new(false))).unwrap()["published"],true);assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retained_pcb_completion_recovers_after_restart_without_editor_access_or_replay(){
        for export in [false,true]{
            let (root,workflow)=setup();let selected=workflow.view().unwrap().project.unwrap();workflow.task_state().unwrap();let mut input=crate::kicad_live::tests::move_fixture();input.before.components[0].value="PRIVATE_PCB_COMPLETION_SENTINEL".into();let exported=b"(kicad_pcb (version 20260101))";
            if export{input.export_copy=true;input.movement=None;input.before.state_sha256=crate::kicad_project::digest(exported);}
            let id=input.job_id;let pending=workflow.propose_live_pcb(selected.clone(),input.clone(),root.join("Owned.kicad_pcb"),crate::kicad_live::Connection{endpoint:format!("ipc://{}",root.join("not-running.sock").display()),token:String::new()}).unwrap();let mut dispatch=workflow.begin(pending.approval_id.unwrap()).unwrap();dispatch.snapshot.audit.as_mut().unwrap().native_attempted=Some(true);dispatch.project.save(&dispatch.snapshot).unwrap();
            let stage=dispatch.project.reserve_stage(id).unwrap();let fence=crate::live_dispatch::Dispatch::start(&stage.path,&dispatch.snapshot.input,dispatch.dispatch_gate.clone(),dispatch.dispatch_paused.clone(),dispatch.cancelled.clone()).unwrap();drop(fence.enter().unwrap());drop(fence);
            let proof=serde_json::json!({"version":1,"input_sha256":crate::kicad_project::digest(&serde_json::to_vec(&input).unwrap()),"result":crate::kicad_live::tests::receipt_fixture(&input)});fs::write(stage.path.join("pcb-completion.json"),serde_json::to_vec(&proof).unwrap()).unwrap();if export{fs::write(stage.path.join("Board.kicad_pcb"),exported).unwrap();}drop(stage);workflow.finish(dispatch,Err("lost worker reply".into())).unwrap();assert!(workflow.native_recovery().unwrap()[0].needs_review);drop(workflow);
            let workflow=Workflow::new(root.join("FreeCADCmd.exe"),root.join("profiles")).unwrap();workflow.select_project(Project::open(&root).unwrap()).unwrap();let result=workflow.recover_native_result(selected.clone(),id,Arc::new(AtomicBool::new(false))).unwrap();assert_eq!(result["recovered"],true);assert_eq!(result["published"],true);assert!(!result.to_string().contains("PRIVATE_PCB_COMPLETION_SENTINEL"));assert!(!workflow.native_recovery().unwrap()[0].needs_review);
            let report=workflow.artifact(id,ArtifactFormat::LivePcbReport).unwrap().read_text().unwrap();assert!(report.contains("PRIVATE_PCB_COMPLETION_SENTINEL"));assert_eq!(workflow.recover_native_result(selected,id,Arc::new(AtomicBool::new(false))).unwrap()["published"],true);assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
            drop(workflow);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn native_pcb_jobs_bind_private_connections_to_one_use_approval_and_receipt_recovery() {
        for export_copy in [false,true] {
        let (root,workflow)=setup();
        let project=workflow.view().unwrap().project.unwrap();
        let mut input=crate::kicad_live::tests::move_fixture();
        let exported=b"(kicad_pcb (version 20260101))";
        if export_copy {
            input.export_copy=true;input.movement=None;input.before.state_sha256=crate::kicad_project::digest(exported);
        }
        let connection=crate::kicad_live::Connection {endpoint:format!("ipc://{}",root.join("owned.sock").display()),token:"owned-private-session".into()};
        let board=root.join("Owned.kicad_pcb");
        assert!(workflow.propose_live_pcb((Uuid::new_v4(),root.clone()),input.clone(),board.clone(),connection.clone()).is_err());
        let pending=workflow.propose_live_pcb(project,input.clone(),board,connection).unwrap();
        assert_eq!(pending.snapshot.schema_version,14);
        assert_eq!(pending.snapshot.phase,Phase::AwaitingApproval);
        let persisted=serde_json::to_string(&pending.snapshot).unwrap();
        assert!(!persisted.contains("owned-private-session") && !persisted.contains("ipc://") && !persisted.contains("board_path"));
        assert_eq!(fs::read_dir(root.join(".hazlomexa/staging")).unwrap().count(),0);
        assert!(workflow.begin(Uuid::new_v4()).is_err());
        let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();
        assert!(dispatch.live_pcb.is_some());
        assert!(workflow.begin(approval).is_err());
        let stage=dispatch.project.reserve_stage(input.job_id).unwrap();
        let bytes=serde_json::to_vec_pretty(&crate::kicad_live::tests::receipt_fixture(&input)).unwrap();
        fs::write(stage.path.join("result.json"),&bytes).unwrap();
        if export_copy {fs::write(stage.path.join("Board.kicad_pcb"),exported).unwrap();}
        let done=workflow.finish(dispatch,Ok(stage)).unwrap();
        assert_eq!(done.snapshot.phase,Phase::Completed,"{:?}",done.error);
        let OperationInput::LivePcb(completed)=&done.snapshot.input else {panic!("Wrong native input")};
        assert_eq!(completed.receipt_sha256.as_deref(),Some(crate::kicad_project::digest(&bytes).as_str()));
        assert_eq!(workflow.artifact(input.job_id,ArtifactFormat::LivePcbReport).unwrap().read_text().unwrap().as_bytes(),bytes);
        if export_copy {
            let artifact=workflow.artifact(input.job_id,ArtifactFormat::KicadSource).unwrap();
            assert_eq!(fs::read(&artifact.path).unwrap(),exported);let exported_path=artifact.path.clone();drop(artifact);
            fs::write(&exported_path,b"tampered").unwrap();
            assert!(workflow.artifact(input.job_id,ArtifactFormat::KicadSource).is_err());
            assert!(workflow.artifact(input.job_id,ArtifactFormat::LivePcbReport).is_err());
            fs::write(&exported_path,exported).unwrap();
        }
        let path=root.join(".hazlomexa/artifacts").join(input.job_id.to_string()).join("result.json");
        fs::write(&path,b"changed receipt").unwrap();
        assert!(workflow.artifact(input.job_id,ArtifactFormat::LivePcbReport).is_err());
        fs::write(&path,&bytes).unwrap();
        drop(workflow);
        let project=Project::open(&root).unwrap();
        let recovered=project.recover().unwrap();
        assert_eq!(recovered[0].schema_version,14);
        assert_eq!(recovered[0].phase,Phase::Completed);
        drop(project);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn native_pcb_cancellation_never_reuses_approval_or_claims_rollback() {
        let (root,workflow)=setup();
        let project=workflow.view().unwrap().project.unwrap();
        let input=crate::kicad_live::tests::move_fixture();
        let pending=workflow.propose_live_pcb(project,input.clone(),root.join("Owned.kicad_pcb"),
            crate::kicad_live::Connection {endpoint:format!("ipc://{}",root.join("owned.sock").display()),token:String::new()}).unwrap();
        let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();
        workflow.cancel(input.job_id).unwrap();
        let done=workflow.finish(dispatch,Err("Stopped owned worker".into())).unwrap();
        assert_eq!(done.snapshot.phase,Phase::Cancelled);
        assert!(done.error.unwrap().contains("Cancelar no deshace"));
        assert!(workflow.begin(approval).is_err());
        assert!(workflow.artifact(input.job_id,ArtifactFormat::LivePcbReport).is_err());
        drop(workflow);
        let project=Project::open(&root).unwrap();
        assert_eq!(project.recover().unwrap()[0].phase,Phase::Cancelled);
        drop(project);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_primitive_creation_refuses_hidden_default_dimensions_before_approval() {
        let (root,workflow)=setup();
        let project=workflow.view().unwrap().project.unwrap();
        let mut input=crate::cadpilot_live::Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:Uuid::new_v4(),
            document_name:"NewPart".into(),document_token:Uuid::new_v4(),state_sha256:"a".repeat(64),
            description:"Create a box; its size is not known yet".into(),
            operations:vec![serde_json::json!({"action":"create_object","obj_type":"Part::Box","obj_name":"Box"})],
            before:serde_json::json!([]),receipt_sha256:None};
        assert!(workflow.propose_live_cad(project.clone(),input.clone()).is_err(),"Missing dimensions reached human approval");
        input.operations[0]["obj_properties"]=serde_json::json!({"Length":"0.5 in","Width":"2 cm","Height":"10 mm","x":"0 mm","y":"0 mm","z":"0 mm"});
        assert!(workflow.propose_live_cad(project,input).is_err(),"Invalid coordinates reached human approval");
        assert!(workflow.current().unwrap().is_none());
        drop(workflow);
        let reopened=Project::open(&root).unwrap();
        assert!(reopened.recover().unwrap().is_empty());
        drop(reopened);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_changes_require_one_time_approval_and_persist_a_verified_receipt_hash() {
        let (root,workflow)=setup();
        let project=workflow.view().unwrap().project.unwrap();
        let input=crate::cadpilot_live::Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:Uuid::new_v4(),document_name:"Enclosure".into(),document_token:Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Make it longer".into(),operations:vec![serde_json::json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"1.2 in"}})],before:serde_json::json!([]),receipt_sha256:None};
        assert!(workflow.propose_live_cad((Uuid::new_v4(),project.1.clone()),input.clone()).is_err());
        let pending=workflow.propose_live_cad(project,input.clone()).unwrap();
        assert_eq!(pending.snapshot.schema_version,13);
        assert_eq!(pending.snapshot.phase,Phase::AwaitingApproval);
        assert_eq!(fs::read_dir(root.join(".hazlomexa/staging")).unwrap().count(),0);
        let approval=pending.approval_id.unwrap();
        assert!(workflow.begin(Uuid::new_v4()).is_err());
        let dispatch=workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let stage=dispatch.project.reserve_stage(input.job_id).unwrap();
        let report=serde_json::json!({"live_cad_version":1,"job_id":input.job_id,"document_token":input.document_token,"before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":"b".repeat(64),"objects":[]},"operations":[{"action":"edit_object","success":true}],"undo_available":true});
        let bytes=serde_json::to_vec_pretty(&report).unwrap();
        fs::write(stage.path.join("result.json"),&bytes).unwrap();
        let done=workflow.finish(dispatch,Ok(stage)).unwrap();
        assert_eq!(done.snapshot.phase,Phase::Completed,"{:?}",done.error);
        let OperationInput::LiveCad(completed)=&done.snapshot.input else {panic!("Wrong native input")};
        assert_eq!(completed.receipt_sha256.as_deref(),Some(crate::kicad_project::digest(&bytes).as_str()));
        assert_eq!(workflow.artifact(input.job_id,ArtifactFormat::CadpilotReport).unwrap().read_text().unwrap().as_bytes(),bytes);
        let path=root.join(".hazlomexa/artifacts").join(input.job_id.to_string()).join("result.json");
        fs::write(&path,serde_json::to_vec(&report).unwrap()).unwrap();
        assert!(workflow.artifact(input.job_id,ArtifactFormat::CadpilotReport).is_err(),"Changed receipt bytes were accepted");
        fs::write(&path,&bytes).unwrap();
        drop(workflow);
        let reopened=Project::open(&root).unwrap();
        let recovered=reopened.recover().unwrap();
        assert_eq!(recovered[0].schema_version,13);
        assert_eq!(recovered[0].phase,Phase::Completed);
        drop(reopened);
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        fs::remove_dir_all(root).unwrap();
    }
    #[allow(dead_code)]
    mod read_barrier {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/fixtures/publication_barrier.rs"
        ));
    }

    fn check_tool_readback(
        workflow: &Arc<Workflow>,
        id: Uuid,
        root: &std::path::Path,
        format: ArtifactFormat,
    ) {
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let result = host
            .call("read_verified_result", serde_json::json!({"job_id":id}))
            .unwrap();
        assert_eq!(result["job_id"], id.to_string());
        assert_eq!(result["scope"], "verified_published_result");
        assert_eq!(
            result["content"],
            workflow.artifact(id, format).unwrap().read_text().unwrap()
        );
        let _: serde_json::Value =
            serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
        fs::write(
            root.join(format!("{id}-tool-readback.json")),
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[ignore = "Actual official CADCraft Windows CLI; explicitly verifies shared approval/publication/recovery"]
    fn actual_cadcraft_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join(
            "target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe",
        );
        let source = workspace.join("docs/research/cadcraft-runtime-2026-10-11/metric.dxf");
        let original = cadcraft::read_file(&source).unwrap();
        let root =
            std::env::temp_dir().join(format!("hazlomexa-drawing-workflow-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project_root = root.join("project");
        fs::create_dir(&project_root).unwrap();
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_cadcraft(exe).unwrap();
        let request = cadcraft::Request {
            output_format: cadcraft::Format::Dxf,
            units: cadcraft::Units::Mm,
            circle: Some(cadcraft::Circle {
                center: [30.0, 0.0],
                radius: 2.0,
            }),
            allow_loss: false,
            native_tools: None,
        };
        let denied = workflow
            .propose_drawing(source.clone(), request.clone())
            .unwrap();
        let expired = denied.approval_id.unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(expired).is_err());
        let proposal = workflow
            .propose_drawing(source.clone(), request.clone())
            .unwrap();
        let approval = proposal.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(completed.snapshot.phase, Phase::Completed);
        check_tool_readback(
            &workflow,
            completed.snapshot.input.job_id(),
            &root,
            ArtifactFormat::DrawingReport,
        );
        for format in [
            ArtifactFormat::Drawing,
            ArtifactFormat::OriginalDrawing,
            ArtifactFormat::DrawingPreview,
            ArtifactFormat::DrawingReport,
        ] {
            assert!(workflow
                .artifact(completed.snapshot.input.job_id(), format)
                .is_ok());
        }
        let baseline = cadcraft::read_file(
            &workflow
                .artifact(completed.snapshot.input.job_id(), ArtifactFormat::Drawing)
                .unwrap()
                .path,
        )
        .unwrap();
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let read = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id": completed.snapshot.input.job_id()}),
            )
            .unwrap();
        assert_eq!(read["drawing"]["units"], "mm");
        assert_eq!(read["drawing"]["editable"], true);
        let arguments = serde_json::json!({"project_id": completed.snapshot.project_id,
            "base_job_id": completed.snapshot.input.job_id(), "result_sha256": read["sha256"],
            "request": {"output_format":"dxf","units":"mm","circle":{"center":[40.0,0.0],"radius":3.0},"allow_loss":false}});
        for field in ["result_sha256", "units", "path"] {
            let mut wrong = arguments.clone();
            match field {
                "result_sha256" => wrong[field] = serde_json::json!("0".repeat(64)),
                "units" => wrong["request"][field] = serde_json::json!("in"),
                _ => wrong[field] = serde_json::json!(source),
            }
            assert!(host.call("propose_drawing_change", wrong).is_err());
            assert!(workflow
                .view()
                .unwrap()
                .job
                .as_ref()
                .is_none_or(|j| j.snapshot.phase != Phase::AwaitingApproval));
        }
        let proposed = host.call("propose_drawing_change", arguments).unwrap();
        assert_eq!(proposed["human_approval_required"], true);
        assert!(proposed.get("approval_id").is_none());
        let pending = workflow.view().unwrap().job.unwrap();
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let OperationInput::Drawing(drawing) = &pending.snapshot.input else {
            panic!("Wrong integration");
        };
        assert_eq!(drawing.source_sha256, kicad_project::digest(&baseline));
        let baseline_report: cadcraft::DrawingResult =
            serde_json::from_str(read["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            drawing.original_summary,
            *baseline_report.output_summary.as_ref().unwrap()
        );
        let edited = workflow
            .execute(workflow.begin(pending.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            edited.snapshot.phase,
            Phase::Completed,
            "{:?}",
            edited.error
        );
        let read_edited = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":edited.snapshot.input.job_id()}),
            )
            .unwrap();
        let report: cadcraft::DrawingResult =
            serde_json::from_str(read_edited["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            report.output_summary.as_ref().unwrap().entity_count,
            baseline_report
                .output_summary
                .as_ref()
                .unwrap()
                .entity_count
                + 1
        );
        assert_eq!(
            report.circle_readback.as_ref().unwrap().final_matches,
            Some(1)
        );
        fs::write(root.join("tool-calls.json"), serde_json::to_vec_pretty(&serde_json::json!({"first_read":read,"proposal":proposed,"final_read":read_edited})).unwrap()).unwrap();
        drop(host);
        let occupied = workflow
            .propose_drawing(source.clone(), request.clone())
            .unwrap();
        let occupied_dir = project_root
            .join(".hazlomexa/artifacts")
            .join(occupied.snapshot.input.job_id().to_string());
        fs::create_dir(&occupied_dir).unwrap();
        fs::write(occupied_dir.join("sentinel"), b"keep").unwrap();
        let failed = workflow
            .execute(workflow.begin(occupied.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(fs::read(occupied_dir.join("sentinel")).unwrap(), b"keep");
        let cancelled = workflow.propose_drawing(source.clone(), request).unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        assert_eq!(cadcraft::read_file(&source).unwrap(), original);
        assert_eq!(
            cadcraft::read_file(
                &workflow
                    .artifact(completed.snapshot.input.job_id(), ArtifactFormat::Drawing)
                    .unwrap()
                    .path
            )
            .unwrap(),
            baseline
        );
        drop(workflow);
        let reopened = Project::open(&project_root).unwrap();
        let history = reopened.recover().unwrap();
        assert_eq!(history.len(), 5);
        assert!(history.iter().any(|item| item.phase == Phase::Completed));
        assert!(
            history
                .iter()
                .any(|item| item.phase == Phase::Failed
                    && item.audit.as_ref().unwrap().files_verified)
        );
        println!(
            "Verified CADCraft shared workflow: {}",
            project_root.display()
        );
    }

    #[test]
    #[ignore = "Actual official KiCad 10.0.7 Windows runtime: approval, publication and recovery"]
    fn actual_kicad_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        let root =
            std::env::temp_dir().join(format!("hazlomexa-kicad-workflow-{}", Uuid::new_v4()));
        let source = root.join("sources");
        let project_root = root.join("project");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir(&project_root).unwrap();
        for name in [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ] {
            fs::copy(
                workspace.join("target/kicad-probe/fixtures").join(name),
                source.join(name),
            )
            .unwrap();
        }
        let originals: Vec<_> = fs::read_dir(&source)
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_kicad(exe).unwrap();
        let entry = source.join("basic_test.kicad_sch");
        let denied = workflow
            .propose_kicad(source.clone(), entry.clone(), crate::kicad::Units::In)
            .unwrap();
        let expired = denied.approval_id.unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(expired).is_err());
        let proposal = workflow
            .propose_kicad(source.clone(), entry.clone(), crate::kicad::Units::In)
            .unwrap();
        let approval = proposal.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(completed.snapshot.phase, Phase::Completed);
        let job_id = completed.snapshot.input.job_id();
        check_tool_readback(&workflow, job_id, &root, ArtifactFormat::KicadReport);
        assert_eq!(workflow.kicad_summary(job_id).unwrap().errors, 2);
        for format in [
            ArtifactFormat::KicadSource,
            ArtifactFormat::KicadOutputs,
            ArtifactFormat::KicadReport,
        ] {
            assert!(workflow.artifact(job_id, format).is_ok());
        }
        let published_report = workflow
            .artifact(job_id, ArtifactFormat::KicadReport)
            .unwrap()
            .read_text()
            .unwrap();
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let read = host
            .call("read_verified_result", serde_json::json!({"job_id":job_id}))
            .unwrap();
        assert_eq!(read["checks"]["errors"], 2);
        let arguments = serde_json::json!({"project_id":completed.snapshot.project_id,"base_job_id":job_id,"result_sha256":read["sha256"],"units":"mils"});
        for field in ["result_sha256", "path"] {
            let mut wrong = arguments.clone();
            wrong[field] = if field == "path" {
                serde_json::json!(entry)
            } else {
                serde_json::json!("0".repeat(64))
            };
            assert!(host.call("propose_kicad_check", wrong).is_err());
            assert!(workflow
                .view()
                .unwrap()
                .job
                .as_ref()
                .is_none_or(|j| j.snapshot.phase != Phase::AwaitingApproval));
        }
        let proposed = host.call("propose_kicad_check", arguments).unwrap();
        assert_eq!(proposed["human_approval_required"], true);
        assert!(proposed.get("approval_id").is_none());
        let pending = workflow.view().unwrap().job.unwrap();
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let second = workflow
            .execute(workflow.begin(pending.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            second.snapshot.phase,
            Phase::Completed,
            "{:?}",
            second.error
        );
        let final_read = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":second.snapshot.input.job_id()}),
            )
            .unwrap();
        assert_eq!(final_read["checks"]["errors"], 2);
        assert_eq!(final_read["checks"]["units"], "mils");
        fs::write(
            root.join("tool-calls.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"first_read":read,"proposal":proposed,"final_read":final_read}),
            )
            .unwrap(),
        )
        .unwrap();
        drop(host);
        let occupied = workflow
            .propose_kicad(source.clone(), entry.clone(), crate::kicad::Units::Mm)
            .unwrap();
        let occupied_dir = project_root
            .join(".hazlomexa/artifacts")
            .join(occupied.snapshot.input.job_id().to_string());
        fs::create_dir(&occupied_dir).unwrap();
        fs::write(occupied_dir.join("sentinel"), b"keep").unwrap();
        let failed = workflow
            .execute(workflow.begin(occupied.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(fs::read(occupied_dir.join("sentinel")).unwrap(), b"keep");
        let cancelled = workflow
            .propose_kicad(source, entry, crate::kicad::Units::Mils)
            .unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        assert_eq!(
            workflow
                .artifact(job_id, ArtifactFormat::KicadReport)
                .unwrap()
                .read_text()
                .unwrap(),
            published_report
        );
        for (path, bytes) in originals {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        drop(workflow);
        let reopened = Project::open(&project_root).unwrap();
        let history = reopened.recover().unwrap();
        assert_eq!(history.len(), 5);
        assert!(history.iter().any(|v| v.phase == Phase::Completed));
        assert!(history
            .iter()
            .any(|v| v.phase == Phase::Failed && v.audit.as_ref().unwrap().files_verified));
        assert_eq!(reopened.kicad_summary(job_id).unwrap().errors, 2);
        println!("Verified KiCad shared workflow: {}", project_root.display());
    }

    #[test]
    #[ignore = "Actual identified CADCraft process cancellation; invoke alone or with --test-threads=1"]
    fn actual_cadcraft_owned_process_cancellation() {
        use crate::process::tests::{child_processes, is_alive};
        use std::time::{Duration, Instant};
        assert!(child_processes("cadcraft-cli.exe").is_empty());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join(
            "target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe",
        );
        let root =
            std::env::temp_dir().join(format!("hazlomexa-cadcraft-cancel-{}", Uuid::new_v4()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let source = root.join("Selected.dxf");
        let original =
            fs::read(workspace.join("docs/research/cadcraft-runtime-2026-10-11/metric.dxf"))
                .unwrap();
        fs::write(&source, &original).unwrap();
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_cadcraft(exe).unwrap();
        let proposal = workflow
            .propose_drawing(
                source.clone(),
                cadcraft::Request {
                    output_format: cadcraft::Format::Dxf,
                    units: cadcraft::Units::Mm,
                    circle: Some(cadcraft::Circle {
                        center: [30., 0.],
                        radius: 2.,
                    }),
                    allow_loss: false,
                    native_tools: None,
                },
            )
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        let executing = workflow.clone();
        let worker = std::thread::spawn(move || executing.execute(dispatch));
        let wait = Instant::now();
        let observed = loop {
            let pids = child_processes("cadcraft-cli.exe");
            if !pids.is_empty() {
                break pids;
            }
            assert!(
                !worker.is_finished(),
                "CADCraft completed before its owned process was observed"
            );
            assert!(wait.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(observed.iter().all(|pid| is_alive(*pid)));
        let cancel_started = Instant::now();
        workflow.cancel(id).unwrap();
        let outcome = worker.join().unwrap().unwrap();
        let cancellation_ms = cancel_started.elapsed().as_millis();
        assert_eq!(outcome.snapshot.phase, Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(cancellation_ms < 1000);
        assert!(observed.iter().all(|pid| !is_alive(*pid)));
        assert!(child_processes("cadcraft-cli.exe").is_empty());
        assert!(!project_root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        assert!(workflow
            .artifact(id, ArtifactFormat::DrawingReport)
            .is_err());
        assert_eq!(fs::read(&source).unwrap(), original);
        drop(workflow);
        let history = Project::open(&project_root).unwrap().recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].phase, Phase::Cancelled);
        let record = serde_json::json!({"passed":true,"root":root,"job_id":id,"observed_owned_pids":observed,"cancellation_ms":cancellation_ms,"phase":"cancelled","published":false,"original_preserved":true,"recovered_runs":1,"scope":"actual approved CADCraft dispatch cancellation during an observed live CLI; exact mutation phase and native UI cancellation latency unverified"});
        fs::write(
            root.join("cancel-evidence.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        println!("Verified CADCraft owned cancellation: {}", root.display());
    }

    #[test]
    #[ignore = "Actual identified KiCad process cancellation; invoke alone or with --test-threads=1"]
    fn actual_kicad_owned_process_cancellation() {
        use crate::process::tests::{child_processes, is_alive};
        use std::time::{Duration, Instant};
        assert!(child_processes("kicad-cli.exe").is_empty());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let root = std::env::temp_dir().join(format!("hazlomexa-kicad-cancel-{}", Uuid::new_v4()));
        let source = root.join("source");
        let project_root = root.join("project");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir(&project_root).unwrap();
        let mut originals = Vec::new();
        for name in [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ] {
            let bytes = fs::read(workspace.join("target/kicad-probe/fixtures").join(name)).unwrap();
            fs::write(source.join(name), &bytes).unwrap();
            originals.push((name, bytes));
        }
        let exe = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_kicad(exe).unwrap();
        let proposal = workflow
            .propose_kicad(
                source.clone(),
                source.join("basic_test.kicad_pcb"),
                crate::kicad::Units::Mm,
            )
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        let executing = workflow.clone();
        let worker = std::thread::spawn(move || executing.execute(dispatch));
        let wait = Instant::now();
        let observed = loop {
            let pids = child_processes("kicad-cli.exe");
            if !pids.is_empty() {
                break pids;
            }
            assert!(
                !worker.is_finished(),
                "KiCad finished before an owned CLI process was observed"
            );
            assert!(wait.elapsed() < Duration::from_secs(90));
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(observed.iter().any(|pid| is_alive(*pid)));
        let cancel_started = Instant::now();
        workflow.cancel(id).unwrap();
        let outcome = worker.join().unwrap().unwrap();
        let cancellation_ms = cancel_started.elapsed().as_millis();
        assert_eq!(outcome.snapshot.phase, Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(cancellation_ms < 5000);
        assert!(observed.iter().all(|pid| !is_alive(*pid)));
        assert!(child_processes("kicad-cli.exe").is_empty());
        assert!(!project_root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        assert!(workflow.kicad_summary(id).is_err());
        for (name, bytes) in originals {
            assert_eq!(fs::read(source.join(name)).unwrap(), bytes);
        }
        drop(workflow);
        let history = Project::open(&project_root).unwrap().recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].phase, Phase::Cancelled);
        let record = serde_json::json!({"passed":true,"root":root,"job_id":id,"observed_owned_pids":observed,"cancellation_ms":cancellation_ms,"phase":"cancelled","published":false,"native_ui_cancel_verified":false});
        fs::write(
            root.join("cancel-evidence.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        println!("Verified KiCad owned cancellation: {}", root.display());
    }

    #[test]
    #[ignore = "Actual official OrcaSlicer 2.4.2 approval, publication, cancellation and recovery"]
    fn actual_orca_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join("target/orca-probe/runtime/orca-slicer.exe");
        let profiles = workspace.join("target/orca-probe/runtime/resources/profiles/Prusa");
        let selected = [
            profiles.join("machine/Prusa MK3S 0.4 nozzle.json"),
            profiles.join("process/0.20mm Standard @MK3S 0.4.json"),
            profiles.join("filament/Prusa Generic PLA.json"),
        ];
        let root = std::env::temp_dir().join(format!("hazlomexa-orca-workflow-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project_root = root.join("project");
        fs::create_dir(&project_root).unwrap();
        let model = root.join("Original.stl");
        let original =
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl");
        fs::write(&model, original).unwrap();
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_orca(exe).unwrap();
        let request = orca::Request {
            units: orca::Units::Mm,
            bed: orca::Bed::Textured,
            bed_temperature_c: 45.,
        };
        let denied = workflow
            .propose_slice(
                profiles.clone(),
                model.clone(),
                selected.clone(),
                request.clone(),
            )
            .unwrap();
        let expired = denied.approval_id.unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(expired).is_err());
        let proposal = workflow
            .propose_slice(
                profiles.clone(),
                model.clone(),
                selected.clone(),
                request.clone(),
            )
            .unwrap();
        let approval = proposal.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        let id = completed.snapshot.input.job_id();
        check_tool_readback(&workflow, id, &root, ArtifactFormat::SliceReport);
        let published = workflow.artifact(id, ArtifactFormat::Sliced).unwrap().path;
        let baseline = fs::read(&published).unwrap();
        for format in [
            ArtifactFormat::OriginalMesh,
            ArtifactFormat::Sliced,
            ArtifactFormat::Gcode,
            ArtifactFormat::SliceReport,
        ] {
            assert!(workflow.artifact(id, format).is_ok());
        }
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let read = host
            .call("read_verified_result", serde_json::json!({"job_id":id}))
            .unwrap();
        assert_eq!(read["slicing"]["settings"]["bed_c"], 45.);
        let arguments = serde_json::json!({"project_id":completed.snapshot.project_id,"base_job_id":id,
            "result_sha256":read["sha256"],"bed":"textured","bed_temperature_c":50.});
        for field in ["result_sha256", "path", "bed_temperature_c"] {
            let mut wrong = arguments.clone();
            wrong[field] = match field {
                "result_sha256" => serde_json::json!("0".repeat(64)),
                "path" => serde_json::json!(model),
                _ => serde_json::json!(201.),
            };
            assert!(host.call("propose_slice_settings", wrong).is_err());
            assert!(workflow
                .view()
                .unwrap()
                .job
                .as_ref()
                .is_none_or(|j| j.snapshot.phase != Phase::AwaitingApproval));
        }
        let proposed = host.call("propose_slice_settings", arguments).unwrap();
        assert_eq!(proposed["human_approval_required"], true);
        assert!(proposed.get("approval_id").is_none());
        let pending = workflow.view().unwrap().job.unwrap();
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let OperationInput::Orca(previous) = &completed.snapshot.input else {
            panic!("Wrong source");
        };
        let OperationInput::Orca(next) = &pending.snapshot.input else {
            panic!("Wrong proposal");
        };
        assert_eq!(next.mesh, previous.mesh);
        assert_eq!(next.settings.machine, previous.settings.machine);
        assert_eq!(next.settings.process, previous.settings.process);
        assert_eq!(next.settings.filament, previous.settings.filament);
        let edited = workflow
            .execute(workflow.begin(pending.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            edited.snapshot.phase,
            Phase::Completed,
            "{:?}",
            edited.error
        );
        let final_read = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":edited.snapshot.input.job_id()}),
            )
            .unwrap();
        let report: crate::orca_run::SliceResult =
            serde_json::from_str(final_read["content"].as_str().unwrap()).unwrap();
        assert_eq!(report.summary.settings["bed_c"], 50.);
        fs::write(
            root.join("tool-calls.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"first_read":read,"proposal":proposed,"final_read":final_read}),
            )
            .unwrap(),
        )
        .unwrap();
        drop(host);
        let occupied = workflow
            .propose_slice(
                profiles.clone(),
                model.clone(),
                selected.clone(),
                request.clone(),
            )
            .unwrap();
        let destination = project_root
            .join(".hazlomexa/artifacts")
            .join(occupied.snapshot.input.job_id().to_string());
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("sentinel"), b"keep").unwrap();
        let failed = workflow
            .execute(workflow.begin(occupied.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(fs::read(destination.join("sentinel")).unwrap(), b"keep");
        let cancelled = workflow
            .propose_slice(profiles, model.clone(), selected, request)
            .unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        assert_eq!(fs::read(&published).unwrap(), baseline);
        assert_eq!(fs::read(model).unwrap(), original);
        drop(workflow);
        let reopened = Project::open(&project_root).unwrap();
        let history = reopened.recover().unwrap();
        assert_eq!(history.len(), 5);
        assert!(history.iter().any(|s| s.phase == Phase::Completed));
        assert!(history
            .iter()
            .any(|s| s.phase == Phase::Failed && s.audit.as_ref().unwrap().files_verified));
        assert!(reopened.artifact(id, ArtifactFormat::Sliced).is_ok());
        println!(
            "Verified OrcaSlicer shared workflow: {}",
            project_root.display()
        );
    }

    #[test]
    #[ignore = "Actual identified OrcaSlicer live process cancellation; run serially"]
    fn actual_orca_owned_process_cancellation() {
        use crate::process::tests::{child_processes, is_alive};
        use std::time::{Duration, Instant};
        assert!(child_processes("orca-slicer.exe").is_empty());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let root = std::env::temp_dir().join(format!("hazlomexa-orca-cancel-{}", Uuid::new_v4()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let model = root.join("Original.stl");
        let original =
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl");
        fs::write(&model, original).unwrap();
        let exe = workspace.join("target/orca-probe/runtime/orca-slicer.exe");
        let profiles = workspace.join("target/orca-probe/runtime/resources/profiles/Prusa");
        let selected = [
            profiles.join("machine/Prusa MK3S 0.4 nozzle.json"),
            profiles.join("process/0.20mm Standard @MK3S 0.4.json"),
            profiles.join("filament/Prusa Generic PLA.json"),
        ];
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        workflow.set_orca(exe).unwrap();
        let proposal = workflow
            .propose_slice(
                profiles,
                model.clone(),
                selected,
                orca::Request {
                    units: orca::Units::Mm,
                    bed: orca::Bed::Textured,
                    bed_temperature_c: 45.,
                },
            )
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        let executing = workflow.clone();
        let worker = std::thread::spawn(move || executing.execute(dispatch));
        let wait = Instant::now();
        let observed = loop {
            let pids = child_processes("orca-slicer.exe");
            if !pids.is_empty() {
                break pids;
            }
            assert!(
                !worker.is_finished(),
                "Orca finished before an owned process was observed"
            );
            assert!(wait.elapsed() < Duration::from_secs(90));
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(observed.iter().any(|pid| is_alive(*pid)));
        let start = Instant::now();
        workflow.cancel(id).unwrap();
        let outcome = worker.join().unwrap().unwrap();
        let cancellation_ms = start.elapsed().as_millis();
        assert_eq!(outcome.snapshot.phase, Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(cancellation_ms < 5000);
        assert!(observed.iter().all(|pid| !is_alive(*pid)));
        assert!(child_processes("orca-slicer.exe").is_empty());
        assert!(!project_root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        assert!(workflow.artifact(id, ArtifactFormat::SliceReport).is_err());
        assert_eq!(fs::read(model).unwrap(), original);
        drop(workflow);
        let history = Project::open(&project_root).unwrap().recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].phase, Phase::Cancelled);
        fs::write(root.join("cancel-evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({"passed":true,"root":root,"job_id":id,"observed_owned_pids":observed,"cancellation_ms":cancellation_ms,"published":false,"native_ui_cancel_verified":false})).unwrap()).unwrap();
        println!("Verified OrcaSlicer owned cancellation: {}", root.display());
    }

    #[test]
    #[ignore = "Actual CADPilot 0.7.7 selected-copy approval, publication and recovery"]
    fn actual_cadpilot_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("hazlomexa-cadpilot-workflow-{}", Uuid::new_v4()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let source = root.join("Selected.FCStd");
        let original = include_bytes!(
            "../../../../docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd"
        );
        fs::write(&source, original).unwrap();
        let addon=workspace.join("target/cadpilot-probe/source/cadpilot-58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot");
        let exe = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(
            "HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe",
        );
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        let requests = [
            cadpilot::Request {
                operation: cadpilot::Operation::Inspect,
                object: "Document".into(),
                units: orca::Units::Mm,
                parameters: Default::default(),
            },
            cadpilot::Request {
                operation: cadpilot::Operation::EditBox,
                object: "Target".into(),
                units: orca::Units::In,
                parameters: [
                    ("length".into(), 0.5),
                    ("width".into(), 1.),
                    ("height".into(), 0.3),
                ]
                .into(),
            },
            cadpilot::Request {
                operation: cadpilot::Operation::AddBox,
                object: "AddedBox".into(),
                units: orca::Units::Cm,
                parameters: [
                    ("length".into(), 1.),
                    ("width".into(), 2.),
                    ("height".into(), 3.),
                    ("x".into(), 5.),
                    ("y".into(), 0.),
                    ("z".into(), 0.),
                ]
                .into(),
            },
            cadpilot::Request {
                operation: cadpilot::Operation::AddCylinder,
                object: "AddedCylinder".into(),
                units: orca::Units::Mm,
                parameters: [
                    ("radius".into(), 2.),
                    ("height".into(), 8.),
                    ("x".into(), 40.),
                    ("y".into(), 0.),
                    ("z".into(), 0.),
                ]
                .into(),
            },
        ];
        let denied = workflow
            .propose_cadpilot(source.clone(), addon.clone(), requests[1].clone())
            .unwrap();
        let expired = denied.approval_id.unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(expired).is_err());
        let mut verified = Vec::new();
        for request in &requests {
            let proposal = workflow
                .propose_cadpilot(source.clone(), addon.clone(), request.clone())
                .unwrap();
            let OperationInput::Cadpilot(input) = &proposal.snapshot.input else {
                panic!("wrong adapter")
            };
            let preview = input.preview.as_ref().unwrap();
            assert_eq!(preview.catalog.get("Target").unwrap(), "Part::Box");
            assert_eq!(preview.measurements["Target"]["volume_mm3"], 6000.);
            assert!(workflow.set_executable(exe.clone()).is_err());
            let approval = proposal.approval_id.unwrap();
            let dispatch = workflow.begin(approval).unwrap();
            assert!(workflow.begin(approval).is_err());
            let completed = workflow.execute(dispatch).unwrap();
            assert_eq!(
                completed.snapshot.phase,
                Phase::Completed,
                "{:?}",
                completed.error
            );
            let id = completed.snapshot.input.job_id();
            check_tool_readback(&workflow, id, &root, ArtifactFormat::CadpilotReport);
            for format in [ArtifactFormat::OriginalCad, ArtifactFormat::CadpilotReport] {
                assert!(workflow.artifact(id, format).is_ok());
            }
            if request.operation == cadpilot::Operation::Inspect {
                assert!(workflow.artifact(id, ArtifactFormat::Fcstd).is_err());
            } else {
                for format in [ArtifactFormat::Fcstd, ArtifactFormat::Step] {
                    let path = workflow.artifact(id, format).unwrap().path;
                    verified.push((path.clone(), fs::read(path).unwrap()));
                }
            }
            assert_eq!(fs::read(&source).unwrap(), original);
        }
        let occupied = workflow
            .propose_cadpilot(source.clone(), addon.clone(), requests[1].clone())
            .unwrap();
        let destination = project_root
            .join(".hazlomexa/artifacts")
            .join(occupied.snapshot.input.job_id().to_string());
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("sentinel"), b"keep").unwrap();
        let failed = workflow
            .execute(workflow.begin(occupied.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(fs::read(destination.join("sentinel")).unwrap(), b"keep");
        let cancelled = workflow
            .propose_cadpilot(source.clone(), addon, requests[1].clone())
            .unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        for (path, bytes) in verified {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert_eq!(fs::read(source).unwrap(), original);
        drop(workflow);
        let reopened = Project::open(&project_root).unwrap();
        let history = reopened.recover().unwrap();
        assert_eq!(history.len(), 7);
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Completed)
                .count(),
            4
        );
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Cancelled)
                .count(),
            2
        );
        assert!(history
            .iter()
            .any(|s| s.phase == Phase::Failed && s.audit.as_ref().unwrap().files_verified));
        for snapshot in &history {
            if snapshot.phase == Phase::Completed {
                assert!(reopened
                    .artifact(snapshot.input.job_id(), ArtifactFormat::CadpilotReport)
                    .is_ok());
            }
        }
        println!(
            "Verified CADPilot shared workflow: {}",
            project_root.display()
        );
    }

    #[test]
    #[ignore = "Actual CADPilot live FreeCAD launcher/native-child cancellation; run serially"]
    fn actual_cadpilot_owned_process_cancellation() {
        use crate::process::tests::{child_processes, child_processes_of, is_alive};
        use std::time::{Duration, Instant};
        assert!(child_processes("FreeCADCmd.exe").is_empty());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("hazlomexa-cadpilot-cancel-{}", Uuid::new_v4()));
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let source = root.join("Selected.FCStd");
        let original = include_bytes!(
            "../../../../docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd"
        );
        fs::write(&source, original).unwrap();
        let addon = workspace.join("target/cadpilot-probe/source/cadpilot-58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot");
        let exe = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(
            "HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe",
        );
        let workflow = Arc::new(Workflow::new(exe.clone(), root.join("profiles")).unwrap());
        workflow
            .select_project(Project::create(&project_root).unwrap())
            .unwrap();
        let proposal = workflow
            .propose_cadpilot(
                source.clone(),
                addon,
                cadpilot::Request {
                    operation: cadpilot::Operation::EditBox,
                    object: "Target".into(),
                    units: orca::Units::In,
                    parameters: [
                        ("length".into(), 0.5),
                        ("width".into(), 1.),
                        ("height".into(), 0.3),
                    ]
                    .into(),
                },
            )
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let approval = proposal.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        let executing = workflow.clone();
        let worker = std::thread::spawn(move || executing.execute(dispatch));
        let wait = Instant::now();
        let (launchers, native) = loop {
            let launchers = child_processes("FreeCADCmd.exe");
            let native: Vec<_> = launchers
                .iter()
                .flat_map(|pid| child_processes_of(*pid, "FreeCADCmd.exe"))
                .collect();
            if !native.is_empty() {
                break (launchers, native);
            }
            assert!(
                !worker.is_finished(),
                "CADPilot finished before its native FreeCAD child was observed"
            );
            assert!(wait.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(5));
        };
        let observed: Vec<_> = launchers.iter().chain(&native).copied().collect();
        assert!(observed.iter().all(|pid| is_alive(*pid)));
        let start = Instant::now();
        workflow.cancel(id).unwrap();
        let outcome = worker.join().unwrap().unwrap();
        let cancellation_ms = start.elapsed().as_millis();
        assert_eq!(outcome.snapshot.phase, Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(cancellation_ms < 5000);
        assert!(observed.iter().all(|pid| !is_alive(*pid)));
        assert!(child_processes("FreeCADCmd.exe").is_empty());
        assert!(workflow.begin(approval).is_err());
        assert!(!project_root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        for format in [
            ArtifactFormat::OriginalCad,
            ArtifactFormat::Fcstd,
            ArtifactFormat::Step,
            ArtifactFormat::CadpilotReport,
        ] {
            assert!(workflow.artifact(id, format).is_err());
        }
        assert_eq!(fs::read(&source).unwrap(), original);
        drop(workflow);
        let history = Project::open(&project_root).unwrap().recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].phase, Phase::Cancelled);
        fs::write(root.join("cancel-evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "passed":true,"root":root,"job_id":id,"launcher_pids":launchers,"native_pids":native,
            "cancellation_ms":cancellation_ms,"published":false,"original_preserved":true,"recovered_runs":1,
            "scope":"actual approved CADPilot execution cancelled after observing both the FreeCAD launcher and native child; exact mutation phase, native UI input and full production deadline remain unverified"
        })).unwrap()).unwrap();
        println!("Verified CADPilot owned cancellation: {}", root.display());
    }

    fn setup() -> (PathBuf, Workflow) {
        let root = std::env::temp_dir().join(format!("hazlomexa-workflow-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let workflow = Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap();
        workflow
            .select_project(Project::create(&root).unwrap())
            .unwrap();
        (root, workflow)
    }

    #[test]
    fn cancelled_chat_cleanup_respects_human_approval() {
        let (root, workflow) = setup();
        let proposal = workflow.propose(dimensions()).unwrap();
        let id = proposal.snapshot.input.job_id();
        assert!(workflow
            .cancel_unapproved(Uuid::new_v4())
            .unwrap()
            .is_none());
        assert_eq!(
            workflow.current().unwrap().unwrap().snapshot.phase,
            Phase::AwaitingApproval
        );
        assert_eq!(
            workflow
                .cancel_unapproved(id)
                .unwrap()
                .unwrap()
                .snapshot
                .phase,
            Phase::Cancelled
        );
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());

        let proposal = workflow.propose(dimensions()).unwrap();
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        assert!(workflow
            .cancel_unapproved(proposal.snapshot.input.job_id())
            .unwrap()
            .is_none());
        assert!(!dispatch.cancelled.load(Ordering::SeqCst));
        assert_eq!(
            workflow.current().unwrap().unwrap().snapshot.phase,
            Phase::Running
        );
        workflow.cancel(dispatch.snapshot.input.job_id()).unwrap();
        drop(dispatch);
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    fn pcb_move_fixture() -> crate::konnect::FootprintMove {
        let bytes = include_bytes!(
            "../../../../docs/research/konnect-pcb-2026-10-12/Original/basic_test.kicad_pcb"
        );
        let fp = crate::konnect::footprints(bytes)
            .unwrap()
            .into_iter()
            .find(|fp| fp.reference == "J1")
            .unwrap();
        crate::konnect::FootprintMove {
            reference: fp.reference,
            uuid: fp.uuid,
            expected_x_mm: fp.x_mm,
            expected_y_mm: fp.y_mm,
            x_mm: 203.34,
            y_mm: 155.33,
        }
    }
    #[test]
    fn pcb_proposal_binds_position_and_recovery_version() {
        let (root, workflow) = setup();
        *workflow.konnect.lock().unwrap() = Some(root.join("konnect.exe"));
        *workflow.kicad.lock().unwrap() = Some(root.join("kicad-cli.exe"));
        let source = root.join("sources");
        fs::create_dir(&source).unwrap();
        let entry = source.join("basic_test.kicad_pcb");
        fs::write(
            &entry,
            include_bytes!(
                "../../../../docs/research/konnect-pcb-2026-10-12/Original/basic_test.kicad_pcb"
            ),
        )
        .unwrap();
        let project_id = workflow.view().unwrap().project.unwrap().0;
        assert!(workflow
            .propose_discovered_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                pcb_move_fixture(),
                project_id,
                "0".repeat(64)
            )
            .unwrap_err()
            .contains("PCB cambió"));
        assert!(workflow
            .propose_discovered_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                pcb_move_fixture(),
                Uuid::new_v4(),
                "0".repeat(64)
            )
            .unwrap_err()
            .contains("proyecto cambió"));
        let mut stale = pcb_move_fixture();
        stale.expected_x_mm += 1.0;
        assert!(workflow
            .propose_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                stale
            )
            .is_err());
        let proposal = workflow
            .propose_konnect_pcb(source, entry, crate::kicad::Units::In, pcb_move_fixture())
            .unwrap();
        assert_eq!(proposal.snapshot.schema_version, 11);
        let mut downgraded = proposal.snapshot.clone();
        downgraded.schema_version = 10;
        downgraded.audit.as_mut().unwrap().source = RunSource::Mcp;
        assert!(workflow
            .state
            .lock()
            .unwrap()
            .project
            .as_ref()
            .unwrap()
            .save(&downgraded)
            .is_err());
        workflow.cancel(proposal.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].schema_version, 11);
        assert_eq!(history[0].phase, Phase::Cancelled);
        assert!(matches!(history[0].input, OperationInput::KonnectPcb(_)));
    }
    #[test]
    #[ignore = "Actual Konnect PCB move, KiCad check/export, approval, publication and recovery"]
    fn actual_konnect_pcb_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let (root, workflow) = setup();
        let workflow = Arc::new(workflow);
        workflow
            .set_konnect(workspace.join("target/konnect-probe/runtime/konnect.exe"))
            .unwrap();
        workflow
            .set_kicad(workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe"))
            .unwrap();
        let source = root.join("sources");
        fs::create_dir(&source).unwrap();
        let names = [
            "basic_test.kicad_pcb",
            "basic_test.kicad_sch",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ];
        for name in names {
            fs::copy(
                workspace.join("target/kicad-probe/fixtures").join(name),
                source.join(name),
            )
            .unwrap();
        }
        let entry = source.join("basic_test.kicad_pcb");
        let denied = workflow
            .propose_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                pcb_move_fixture(),
            )
            .unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(denied.approval_id.unwrap()).is_err());
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let selection = host
            .select_pcb(
                workflow.view().unwrap().project.unwrap().0,
                source.clone(),
                entry.clone(),
            )
            .unwrap();
        let component = selection
            .components
            .iter()
            .find(|fp| fp.reference == "J1")
            .unwrap();
        assert_eq!(component.value, "Conn_01x01_Pin");
        assert_eq!(component.layer, "F.Cu");
        let pending = host
            .propose_pcb_move(
                selection.id,
                component.uuid,
                crate::kicad::Units::Mils,
                203.34 / 0.0254,
                155.33 / 0.0254,
            )
            .unwrap();
        assert_eq!(pending.snapshot.schema_version, 11);
        let id = pending.snapshot.input.job_id();
        let approval = pending.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let done = workflow.execute(dispatch).unwrap();
        assert_eq!(done.snapshot.phase, Phase::Completed, "{:?}", done.error);
        for format in [
            ArtifactFormat::KonnectOriginal,
            ArtifactFormat::KonnectEdited,
            ArtifactFormat::KonnectReport,
            ArtifactFormat::KicadReport,
            ArtifactFormat::KicadOutputs,
        ] {
            assert!(workflow.artifact(id, format).is_ok());
        }
        assert!(workflow
            .artifact(id, ArtifactFormat::KonnectNetlist)
            .is_err());
        let final_board = fs::read(
            workflow
                .artifact(id, ArtifactFormat::KonnectEdited)
                .unwrap()
                .path,
        )
        .unwrap();
        let final_component = crate::konnect::footprints(&final_board)
            .unwrap()
            .into_iter()
            .find(|fp| fp.reference == "J1")
            .unwrap();
        assert_eq!(
            (final_component.x_mm, final_component.y_mm),
            (203.34, 155.33)
        );
        let report = workflow
            .artifact(id, ArtifactFormat::KonnectReport)
            .unwrap()
            .read_text()
            .unwrap();
        let result: crate::konnect_pcb::ResultDocument = serde_json::from_str(&report).unwrap();
        assert_eq!(result.verification.summary.errors, 1);
        assert_eq!(result.verification.summary.units, crate::kicad::Units::Mils);
        let occupied = workflow
            .propose_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::Mm,
                pcb_move_fixture(),
            )
            .unwrap();
        let destination = root.join(format!(
            ".hazlomexa/artifacts/{}",
            occupied.snapshot.input.job_id()
        ));
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("preserve.txt"), b"preserve").unwrap();
        let dispatch = workflow.begin(occupied.approval_id.unwrap()).unwrap();
        let failed = workflow.execute(dispatch).unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(
            fs::read(destination.join("preserve.txt")).unwrap(),
            b"preserve"
        );
        let cancelled = workflow
            .propose_konnect_pcb(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::Mm,
                pcb_move_fixture(),
            )
            .unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        for name in names {
            assert_eq!(
                fs::read(source.join(name)).unwrap(),
                fs::read(workspace.join("target/kicad-probe/fixtures").join(name)).unwrap()
            );
        }
        drop(host);
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        assert_eq!(history.len(), 4);
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Completed)
                .count(),
            1
        );
        assert_eq!(
            fs::read(
                project
                    .artifact(id, ArtifactFormat::KonnectEdited)
                    .unwrap()
                    .path
            )
            .unwrap(),
            final_board
        );
        println!("Verified Konnect PCB shared workflow: {}", root.display());
    }

    #[test]
    #[ignore = "Actual Konnect 0.13.0/KiCad 10.0.7: shared approval, publication and recovery"]
    fn actual_konnect_workflow_publication_and_recovery() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let (root, workflow) = setup();
        let workflow = Arc::new(workflow);
        let konnect = workspace.join("target/konnect-probe/runtime/konnect.exe");
        let cli = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        workflow.set_konnect(konnect.clone()).unwrap();
        workflow.set_kicad(cli.clone()).unwrap();
        let source = root.join("sources");
        fs::create_dir(&source).unwrap();
        let entry = source.join("basic_test.kicad_sch");
        let original=include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_sch");
        fs::write(&entry, original).unwrap();
        let edit = crate::konnect::ValueEdit {
            reference: "R1".into(),
            expected_value: "10k".into(),
            value: "12.34k".into(),
        };
        let mut stale = edit.clone();
        let project_id = workflow.view().unwrap().project.unwrap().0;
        let (discovered_sha, _) = crate::konnect::inspect_file(&entry).unwrap();
        assert!(workflow
            .propose_discovered_konnect(
                source.clone(),
                entry.clone(),
                edit.clone(),
                Uuid::new_v4(),
                discovered_sha.clone()
            )
            .unwrap_err()
            .contains("proyecto cambió"));
        assert!(workflow
            .propose_discovered_konnect(
                source.clone(),
                entry.clone(),
                edit.clone(),
                project_id,
                "0".repeat(64)
            )
            .unwrap_err()
            .contains("esquema cambió"));
        stale.expected_value = "wrong".into();
        assert!(workflow
            .propose_konnect(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                stale
            )
            .is_err());
        let denied = workflow
            .propose_konnect(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                edit.clone(),
            )
            .unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(denied.approval_id.unwrap()).is_err());
        let pending = workflow
            .propose_discovered_konnect(
                source.clone(),
                entry.clone(),
                edit.clone(),
                project_id,
                discovered_sha,
            )
            .unwrap();
        assert_eq!(pending.snapshot.schema_version, 9);
        assert!(workflow.set_konnect(konnect).is_err());
        assert!(workflow.set_kicad(cli).is_err());
        let dispatch = workflow.begin(pending.approval_id.unwrap()).unwrap();
        assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
        let done = workflow.execute(dispatch).unwrap();
        assert_eq!(done.snapshot.phase, Phase::Completed, "{:?}", done.error);
        let id = done.snapshot.input.job_id();
        for format in [
            ArtifactFormat::KonnectOriginal,
            ArtifactFormat::KonnectEdited,
            ArtifactFormat::KonnectReport,
            ArtifactFormat::KonnectNetlist,
        ] {
            assert!(workflow.artifact(id, format).unwrap().path.is_file());
        }
        assert!(workflow.artifact(id, ArtifactFormat::Sketch).is_err());
        let published = fs::read(root.join(format!(
            ".hazlomexa/artifacts/{id}/Edited/basic_test.kicad_sch"
        )))
        .unwrap();
        crate::konnect::verify_value_edit(original, &published, &edit).unwrap();
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let read = host
            .call("read_verified_result", serde_json::json!({"job_id":id}))
            .unwrap();
        let proposed=host.call("propose_kicad_check",serde_json::json!({"project_id":project_id,"base_job_id":id,"result_sha256":read["sha256"],"units":"mm"})).unwrap();
        let pending = workflow.view().unwrap().job.unwrap();
        let checked = workflow
            .execute(workflow.begin(pending.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            checked.snapshot.phase,
            Phase::Completed,
            "{:?}",
            checked.error
        );
        let file = workflow
            .artifact(checked.snapshot.input.job_id(), ArtifactFormat::KicadSource)
            .unwrap();
        let (_, components) = crate::konnect::inspect_file(&file.path).unwrap();
        assert_eq!(
            components
                .iter()
                .find(|c| c.reference == "R1")
                .unwrap()
                .value,
            "12.34k"
        );
        let checked_read = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":checked.snapshot.input.job_id()}),
            )
            .unwrap();
        assert_eq!(checked_read["checks"]["check"], "erc");
        fs::write(root.join("kicad-tool-calls.json"),serde_json::to_vec_pretty(&serde_json::json!({"edited_sheet_read":read,"proposal":proposed,"checked_read":checked_read})).unwrap()).unwrap();
        drop(host);
        let occupied = workflow
            .propose_konnect(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::Mils,
                edit.clone(),
            )
            .unwrap();
        let occupied_id = occupied.snapshot.input.job_id();
        let destination = root.join(format!(".hazlomexa/artifacts/{occupied_id}"));
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("preserve.txt"), b"preserve").unwrap();
        let dispatch = workflow.begin(occupied.approval_id.unwrap()).unwrap();
        let failed = workflow.execute(dispatch).unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert_eq!(
            fs::read(destination.join("preserve.txt")).unwrap(),
            b"preserve"
        );
        let cancelled = workflow
            .propose_konnect(source.clone(), entry.clone(), crate::kicad::Units::Mm, edit)
            .unwrap();
        let dispatch = workflow.begin(cancelled.approval_id.unwrap()).unwrap();
        workflow.cancel(cancelled.snapshot.input.job_id()).unwrap();
        assert_eq!(
            workflow.execute(dispatch).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        assert_eq!(fs::read(&entry).unwrap(), original);
        assert_eq!(
            fs::read(root.join(format!(
                ".hazlomexa/artifacts/{id}/Edited/basic_test.kicad_sch"
            )))
            .unwrap(),
            published
        );
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        assert_eq!(history.len(), 5);
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Completed)
                .count(),
            2
        );
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Cancelled)
                .count(),
            2
        );
        assert_eq!(
            history
                .iter()
                .filter(|s| s.phase == Phase::Failed && s.audit.as_ref().unwrap().files_verified)
                .count(),
            1
        );
        assert!(project
            .artifact(id, ArtifactFormat::KonnectEdited)
            .unwrap()
            .path
            .is_file());
        println!("Verified Konnect shared workflow: {}", root.display());
    }

    #[test]
    #[ignore = "Actual approved Konnect workflow held file access, cancellation and recovery; run serially"]
    fn actual_konnect_blocked_workflow_cancellation_recovery() {
        blocked_konnect_workflow(true, false, false);
    }

    #[test]
    #[ignore = "Actual approved Konnect workflow full 60-second deadline and recovery; run serially"]
    fn actual_konnect_blocked_workflow_deadline_recovery() {
        blocked_konnect_workflow(false, false, false);
    }

    #[test]
    #[ignore = "Actual approved PCB workflow refuses held file access and recovers; run serially"]
    fn actual_konnect_pcb_held_workflow_refusal_recovery() {
        blocked_konnect_workflow(false, true, false);
    }

    #[test]
    #[ignore = "Actual approved PCB SDK held-read cancellation, shutdown and recovery; run serially"]
    fn actual_konnect_pcb_sdk_workflow_cancellation_recovery() {
        blocked_konnect_workflow(true, true, true);
    }

    #[test]
    #[ignore = "Actual approved PCB SDK full sixty-second deadline and recovery; run serially"]
    fn actual_konnect_pcb_sdk_workflow_deadline_recovery() {
        blocked_konnect_workflow(false, true, true);
    }

    fn blocked_konnect_workflow(cancel_on_access: bool, pcb: bool, native_board_read: bool) {
        use crate::process::tests::{child_processes, is_alive};
        use std::{
            sync::mpsc,
            time::{Duration, Instant},
        };
        assert!(child_processes("konnect.exe").is_empty());
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let (root, workflow) = setup();
        println!("Observed Konnect interruption fixture: {}", root.display());
        let workflow = Arc::new(workflow);
        workflow
            .set_konnect(workspace.join("target/konnect-probe/runtime/konnect.exe"))
            .unwrap();
        workflow
            .set_kicad(workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe"))
            .unwrap();
        let sources = root.join("sources");
        fs::create_dir(&sources).unwrap();
        let mut names = [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ];
        if pcb {
            names.swap(0, 1);
        }
        let originals: Vec<_> = names
            .iter()
            .map(|name| {
                let bytes =
                    fs::read(workspace.join("target/kicad-probe/fixtures").join(name)).unwrap();
                fs::write(sources.join(name), &bytes).unwrap();
                (*name, bytes)
            })
            .collect();
        let proposal = if pcb {
            workflow.propose_konnect_pcb(
                sources.clone(),
                sources.join(names[0]),
                crate::kicad::Units::In,
                pcb_move_fixture(),
            )
        } else {
            workflow.propose_konnect(
                sources.clone(),
                sources.join(names[0]),
                crate::kicad::Units::In,
                crate::konnect::ValueEdit {
                    reference: "R1".into(),
                    expected_value: "10k".into(),
                    value: "12.34k".into(),
                },
            )
        }
        .unwrap();
        let id = proposal.snapshot.input.job_id();
        let approval = proposal.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let source_input = match &proposal.snapshot.input {
            OperationInput::Konnect(input) => &input.source,
            OperationInput::KonnectPcb(input) => &input.source,
            _ => panic!("Expected schematic or PCB proposal"),
        };
        let mut expected = Vec::new();
        crate::kicad_project::portable_files(
            &dispatch.project.captured_source(id),
            source_input,
            &AtomicBool::new(false),
            |relative, bytes| {
                if relative == std::path::Path::new(names[0]) {
                    expected = bytes.to_vec();
                }
                Ok(())
            },
        )
        .unwrap();
        assert!(!expected.is_empty());
        let stage = root.join(".hazlomexa/staging").join(id.to_string());
        let edited = stage.join("Edited").join(names[0]);
        let profile = workflow.profiles.join(id.to_string());
        if native_board_read {
            fs::write(profile.with_extension("test-board-hold"), b"fixture").unwrap();
        }
        let executing = workflow.clone();
        let (sender, receiver) = mpsc::channel();
        let started = Instant::now();
        let worker = std::thread::spawn(move || {
            sender.send(executing.execute(dispatch)).unwrap();
        });
        // Observe attributes only so this fixture does not consume the held data access.
        while if native_board_read {
            !profile.with_extension("test-board-ready").is_file()
        } else {
            !fs::metadata(&edited).is_ok_and(|metadata| metadata.len() == expected.len() as u64)
        } {
            assert!(
                !worker.is_finished(),
                "Workflow finished before staging was observed"
            );
            assert!(started.elapsed() < Duration::from_secs(30));
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(2));
        let mut barrier = read_barrier::Barrier::request(&edited);
        if native_board_read {
            fs::write(profile.with_extension("test-board-resume"), b"resume").unwrap();
        }
        let access_requested = barrier.wait(20000) == 0;
        let observed = child_processes("konnect.exe");
        let observed_alive = !observed.is_empty() && observed.iter().all(|pid| is_alive(*pid));
        let cancel_started = Instant::now();
        let cancellation = cancel_on_access.then(|| workflow.cancel(id));
        let cancel_return_ms = cancel_on_access.then(|| cancel_started.elapsed().as_millis());
        let outcome = receiver.recv_timeout(Duration::from_secs(if pcb && !native_board_read {
            10
        } else {
            100
        }));
        let elapsed_ms = started.elapsed().as_millis();
        let held_response_ms = cancel_started.elapsed().as_millis();
        let cancellation_ms = cancel_on_access.then(|| cancel_started.elapsed().as_millis());
        let ceased_before_release =
            !observed.is_empty() && observed.iter().all(|pid| !is_alive(*pid));
        let no_owned_server_remaining_before_release = child_processes("konnect.exe").is_empty();
        barrier.release();
        worker.join().unwrap();
        assert!(
            access_requested,
            "Native access did not break the owned barrier"
        );
        assert!(
            pcb && !native_board_read || observed_alive,
            "Owned native server was not alive during held access"
        );
        let outcome = outcome.expect("Workflow missed the watchdog").unwrap();
        let phase = if cancel_on_access {
            Phase::Cancelled
        } else {
            Phase::Failed
        };
        assert_eq!(outcome.snapshot.phase, phase);
        assert!(!outcome.busy);
        if let Some(cancelled) = cancellation {
            assert_eq!(cancelled.unwrap().snapshot.phase, Phase::Cancelled);
            assert!(cancel_return_ms.unwrap() < 1000);
            assert!(cancellation_ms.unwrap() < 10000);
        } else if pcb && !native_board_read {
            assert!(outcome.error.as_ref().unwrap().contains("ocupado"));
            assert!(held_response_ms < 10000);
        } else {
            assert!(outcome
                .error
                .as_ref()
                .unwrap()
                .contains("no confirmó la edición a tiempo"));
            assert!((60000..90000).contains(&elapsed_ms));
        }
        assert!(
            if pcb && !native_board_read {
                no_owned_server_remaining_before_release
            } else {
                ceased_before_release
            },
            "Native server needed barrier release to cease"
        );
        assert!(child_processes("konnect.exe").is_empty());
        assert!(!root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        assert!(workflow
            .artifact(id, ArtifactFormat::KonnectReport)
            .is_err());
        assert!(workflow
            .artifact(id, ArtifactFormat::KonnectEdited)
            .is_err());
        assert!(!stage.join("result.json").exists());
        assert_eq!(fs::read(&edited).unwrap(), expected);
        for (name, bytes) in originals {
            assert_eq!(fs::read(sources.join(name)).unwrap(), bytes);
        }
        let saved_error = outcome.error.clone();
        drop(workflow);
        let history = Project::open(&root).unwrap().recover().unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].phase, phase);
        assert!(!history[0].audit.as_ref().unwrap().files_verified);
        let mode = if pcb && !native_board_read {
            "refusal"
        } else if cancel_on_access {
            "cancellation"
        } else {
            "deadline"
        };
        fs::write(root.join(format!("blocked-workflow-{mode}.json")), serde_json::to_vec_pretty(&serde_json::json!({
            "passed":true,"root":root,"job_id":id,"mode":mode,"phase":phase,"error":saved_error,
            "elapsed_ms":elapsed_ms,"held_response_ms":held_response_ms,"cancel_return_ms":cancel_return_ms,"cancellation_ms":cancellation_ms,
            "owned_pids":observed,"ceased_before_barrier_release":ceased_before_release,
            "no_owned_server_remaining_before_release":no_owned_server_remaining_before_release,
            "approval_consumed":true,"published":false,"source_and_working_copy_preserved":true,"recovered_runs":1,
            "integration":if pcb {"pcb"} else {"schematic"},
            "native_board_read_checkpoint":native_board_read,
            "scope":"actual approved workflow held staging-file access; exact SDK mutation phase and native UI not asserted"
        })).unwrap()).unwrap();
        println!(
            "Verified Konnect blocked workflow {mode}: {}",
            root.display()
        );
    }

    #[test]
    #[ignore = "Actual Konnect schematic/PCB SDK startup cancellation; run serially"]
    fn actual_konnect_owned_process_cancellation() {
        use crate::process::tests::{child_processes, is_alive};
        use std::time::{Duration, Instant};
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        for board in [false, true] {
            assert!(child_processes("konnect.exe").is_empty());
            let (root, workflow) = setup();
            let workflow = Arc::new(workflow);
            workflow
                .set_konnect(workspace.join("target/konnect-probe/runtime/konnect.exe"))
                .unwrap();
            workflow
                .set_kicad(workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe"))
                .unwrap();
            let sources = root.join("sources");
            fs::create_dir(&sources).unwrap();
            let names = [
                "basic_test.kicad_sch",
                "basic_test.kicad_pcb",
                "basic_test.kicad_pro",
                "custom_ds.kicad_wks",
            ];
            let originals: Vec<_> = names
                .iter()
                .map(|name| {
                    let bytes =
                        fs::read(workspace.join("target/kicad-probe/fixtures").join(name)).unwrap();
                    fs::write(sources.join(name), &bytes).unwrap();
                    (*name, bytes)
                })
                .collect();
            let proposal = if board {
                workflow
                    .propose_konnect_pcb(
                        sources.clone(),
                        sources.join(names[1]),
                        crate::kicad::Units::In,
                        pcb_move_fixture(),
                    )
                    .unwrap()
            } else {
                workflow
                    .propose_konnect(
                        sources.clone(),
                        sources.join(names[0]),
                        crate::kicad::Units::In,
                        crate::konnect::ValueEdit {
                            reference: "R1".into(),
                            expected_value: "10k".into(),
                            value: "12.34k".into(),
                        },
                    )
                    .unwrap()
            };
            let id = proposal.snapshot.input.job_id();
            let approval = proposal.approval_id.unwrap();
            let dispatch = workflow.begin(approval).unwrap();
            let executing = workflow.clone();
            let worker = std::thread::spawn(move || executing.execute(dispatch));
            let wait = Instant::now();
            let observed = loop {
                let pids = child_processes("konnect.exe");
                if !pids.is_empty() {
                    break pids;
                }
                assert!(
                    !worker.is_finished(),
                    "Konnect finished before its owned SDK server was observed"
                );
                assert!(wait.elapsed() < Duration::from_secs(30));
                std::thread::sleep(Duration::from_millis(2));
            };
            assert!(observed.iter().all(|pid| is_alive(*pid)));
            let start = Instant::now();
            workflow.cancel(id).unwrap();
            let outcome = worker.join().unwrap().unwrap();
            let cancellation_ms = start.elapsed().as_millis();
            assert_eq!(outcome.snapshot.phase, Phase::Cancelled);
            assert!(!outcome.busy);
            assert!(cancellation_ms < 5000);
            assert!(observed.iter().all(|pid| !is_alive(*pid)));
            assert!(child_processes("konnect.exe").is_empty());
            assert!(workflow.begin(approval).is_err());
            assert!(!root
                .join(".hazlomexa/artifacts")
                .join(id.to_string())
                .exists());
            assert!(workflow
                .artifact(id, ArtifactFormat::KonnectReport)
                .is_err());
            for (name, bytes) in originals {
                assert_eq!(fs::read(sources.join(name)).unwrap(), bytes);
            }
            drop(workflow);
            let history = Project::open(&root).unwrap().recover().unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].phase, Phase::Cancelled);
            fs::write(root.join("cancel-evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({
                "passed":true,"root":root,"job_id":id,"board":board,"observed_owned_pids":observed,
                "cancellation_ms":cancellation_ms,"published":false,"originals_preserved":true,"recovered_runs":1,
                "scope":"actual approved Konnect SDK startup cancelled after observing the live owned server; initialized tool-call cancellation, exact mutation phase, native UI input and full deadlines remain unverified"
            })).unwrap()).unwrap();
            println!("Verified Konnect owned cancellation: {}", root.display());
        }
    }

    #[test]
    fn konnect_proposal_validates_current_value_and_recovers_schema_nine() {
        let (root, workflow) = setup();
        // Proposal-only fixture: execution still verifies the real identified runtimes.
        *workflow.konnect.lock().unwrap() = Some(root.join("konnect.exe"));
        *workflow.kicad.lock().unwrap() = Some(root.join("kicad-cli.exe"));
        let source = root.join("sources");
        fs::create_dir(&source).unwrap();
        let entry = source.join("basic_test.kicad_sch");
        fs::write(&entry,include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_sch")).unwrap();
        let mut edit = crate::konnect::ValueEdit {
            reference: "R1".into(),
            expected_value: "wrong".into(),
            value: "12.34k".into(),
        };
        assert!(workflow
            .propose_konnect(
                source.clone(),
                entry.clone(),
                crate::kicad::Units::In,
                edit.clone()
            )
            .is_err());
        edit.expected_value = "10k".into();
        let proposal = workflow
            .propose_konnect(source, entry, crate::kicad::Units::In, edit)
            .unwrap();
        assert_eq!(proposal.snapshot.schema_version, 9);
        let restored: Snapshot =
            serde_json::from_slice(&serde_json::to_vec(&proposal.snapshot).unwrap()).unwrap();
        assert!(matches!(restored.input, OperationInput::Konnect(_)));
        workflow.cancel(proposal.snapshot.input.job_id()).unwrap();
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let recovered = project.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].phase, Phase::Cancelled);
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }
    fn dimensions() -> Dimensions {
        Dimensions {
            length: 10.0,
            width: 20.0,
            height: 30.0,
        }
    }

    #[test]
    fn agent_original_binding_edit_gates_and_project_changes() {
        let (root, workflow) = setup();
        assert!(workflow.propose_agent("code".into()).is_err());
        workflow
            .set_agent_source(
                "Original.ino".into(),
                "void setup() {} void loop() {}".into(),
            )
            .unwrap();
        let proposal = workflow
            .propose_agent("void setup() {} void loop() { delay(100); }".into())
            .unwrap();
        assert_eq!(proposal.snapshot.schema_version, 4);
        assert!(workflow
            .set_agent_source("Other.ino".into(), "code".into())
            .is_err());
        let edited = workflow
            .edit_agent(
                proposal.snapshot.input.job_id(),
                "void setup() {} void loop() { delay(500); }".into(),
            )
            .unwrap();
        let OperationInput::Agent(input) = &edited.snapshot.input else {
            panic!("Agent expected");
        };
        assert_eq!(input.original_source, "void setup() {} void loop() {}");
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());
        assert!(
            matches!(workflow.begin(edited.approval_id.unwrap()),Err(error) if error.contains("Conecte Arduino Agent"))
        );
        workflow.cancel(input.job_id).unwrap();
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        workflow
            .select_project(Project::create(&other).unwrap())
            .unwrap();
        assert!(workflow.agent_source().unwrap().is_none());
        assert!(workflow.propose_agent("code".into()).is_err());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires authenticated Arduino Agent 0.7.1 in the owned Windows guest"]
    fn actual_agent_workflow_approval_publication_failure_and_recovery() {
        assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (root, workflow) = setup();
        let workflow = Arc::new(workflow);
        let original = "void setup() {} void loop() {}";
        fs::write(root.join("UserOriginal.ino"), original).unwrap();
        let (name, source) = agent::selected_source(&root.join("UserOriginal.ino")).unwrap();
        workflow.set_agent_source(name, source).unwrap();
        workflow
            .set_agent(runtime.block_on(agent::connect()).unwrap())
            .unwrap();
        let proposed = workflow
            .propose_agent("void setup() {} void loop() { delay(100); }".into())
            .unwrap();
        let edited = workflow
            .edit_agent(
                proposed.snapshot.input.job_id(),
                "void setup() {} void loop() { delay(500); }".into(),
            )
            .unwrap();
        assert!(workflow.begin(proposed.approval_id.unwrap()).is_err());
        workflow
            .set_agent(runtime.block_on(agent::connect()).unwrap())
            .unwrap();
        assert!(workflow.begin(edited.approval_id.unwrap()).is_err());
        let current = workflow.current().unwrap().unwrap();
        let completed = workflow
            .execute(workflow.begin(current.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(completed.snapshot.phase, Phase::Completed);
        assert!(completed.snapshot.audit.as_ref().unwrap().files_verified);
        let id = completed.snapshot.input.job_id();
        let accepted = workflow
            .artifact(id, ArtifactFormat::Sketch)
            .unwrap()
            .read_text()
            .unwrap();
        assert_eq!(accepted, "void setup() {} void loop() { delay(500); }");
        assert_eq!(
            workflow
                .artifact(id, ArtifactFormat::OriginalSketch)
                .unwrap()
                .read_text()
                .unwrap(),
            original
        );
        let host = crate::host_tools::HostTools::new(workflow.clone());
        let read = host
            .call("read_verified_result", serde_json::json!({"job_id":id}))
            .unwrap();
        assert_eq!(read["content"], accepted);
        let next_source = "void setup() {} void loop() { delay(1000); }";
        let next = host
            .call(
                "propose_selected_sketch_edit",
                serde_json::json!({
                    "project_id":read["project_id"],"base_job_id":id,
                    "source_name":read["source_name"],"source_sha256":read["sha256"],
                    "source":next_source
                }),
            )
            .unwrap();
        assert_eq!(next["human_approval_required"], true);
        assert!(next.get("approval_id").is_none());
        let reviewed = workflow.current().unwrap().unwrap();
        assert_eq!(reviewed.snapshot.schema_version, 10);
        let OperationInput::Agent(next_input) = &reviewed.snapshot.input else {
            panic!("Agent follow-up expected");
        };
        assert_eq!(next_input.original_source, accepted);
        assert_eq!(next_input.source, next_source);
        let approval = reviewed.approval_id.unwrap();
        let execution = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let followup = workflow.execute(execution).unwrap();
        assert_eq!(followup.snapshot.phase, Phase::Completed);
        assert!(followup.snapshot.audit.as_ref().unwrap().files_verified);
        let followup_id = followup.snapshot.input.job_id();
        let followup_read = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":followup_id}),
            )
            .unwrap();
        assert_eq!(followup_read["content"], next_source);
        assert_eq!(
            workflow
                .artifact(followup_id, ArtifactFormat::OriginalSketch)
                .unwrap()
                .read_text()
                .unwrap(),
            accepted
        );
        assert_eq!(
            host.call("read_verified_result", serde_json::json!({"job_id":id}))
                .unwrap()["content"],
            accepted
        );
        assert_eq!(
            host.call("inspect_selected_sketch", serde_json::json!({}))
                .unwrap()["source"],
            original
        );
        let denied = workflow
            .propose_agent("void setup() {} void loop() { delay(250); }".into())
            .unwrap();
        workflow.cancel(denied.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(denied.approval_id.unwrap()).is_err());
        let occupied = workflow
            .propose_agent("void setup() {} void loop() { delay(750); }".into())
            .unwrap();
        let destination = root
            .join(".hazlomexa/artifacts")
            .join(occupied.snapshot.input.job_id().to_string());
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("keep.txt"), "occupied original").unwrap();
        let failed = workflow
            .execute(workflow.begin(occupied.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert!(workflow
            .artifact(failed.snapshot.input.job_id(), ArtifactFormat::Sketch)
            .is_err());
        assert_eq!(
            fs::read_to_string(destination.join("keep.txt")).unwrap(),
            "occupied original"
        );
        assert_eq!(
            fs::read_to_string(root.join("UserOriginal.ino")).unwrap(),
            original
        );
        drop(host);
        drop(workflow);
        let reopened = Project::open(&root).unwrap();
        let history = reopened.recover().unwrap();
        assert_eq!(history.len(), 4);
        assert!(history.iter().any(|run| run.phase == Phase::Cancelled));
        assert!(history.iter().any(|run| run.phase == Phase::Failed));
        assert_eq!(
            reopened
                .artifact(id, ArtifactFormat::Sketch)
                .unwrap()
                .read_text()
                .unwrap(),
            accepted
        );
        drop(reopened);
        let recovered =
            Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
        recovered
            .select_project(Project::open(&root).unwrap())
            .unwrap();
        let recovered_host = crate::host_tools::HostTools::new(recovered.clone());
        assert_eq!(
            recovered_host
                .call(
                    "read_verified_result",
                    serde_json::json!({"job_id":followup_id})
                )
                .unwrap()["content"],
            next_source
        );
        drop(recovered_host);
        drop(recovered);
        fs::write("C:\\AgentProbe\\output\\workflow-result.json",serde_json::to_vec_pretty(&serde_json::json!({"pass":true,"root":root,"completed":completed.snapshot,"followup":followup.snapshot,"first_tool_readback":read,"followup_tool_readback":followup_read,"followup_applied_by_authenticated_agent":true,"followup_original_is_previous_result":true,"picker_source_remains_original":true,"failed":failed.snapshot,"cancelled":denied.snapshot.input.job_id(),"reopened_without_regeneration":true,"original_preserved":true,"stale_approval_rejected":true,"reconnect_invalidated_approval":true,"occupied_destination_preserved":true})).unwrap()).unwrap();
    }

    #[test]
    #[ignore = "Requires owned Agent guest with the confirmed-write reply barrier"]
    fn actual_agent_cancel_with_confirmed_write_reply_held() {
        assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
        assert!(std::path::Path::new("C:\\AgentProbe\\output\\held-proxy.json").is_file());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (root, workflow) = setup();
        let workflow = Arc::new(workflow);
        let original = "void setup() {} void loop() {}";
        fs::write(root.join("UserOriginal.ino"), original).unwrap();
        workflow
            .set_agent_source("UserOriginal.ino".into(), original.into())
            .unwrap();
        workflow
            .set_agent(runtime.block_on(agent::connect()).unwrap())
            .unwrap();
        let proposed = workflow
            .propose_agent("void setup() {} void loop() { delay(500); }".into())
            .unwrap();
        let id = proposed.snapshot.input.job_id();
        fs::write(
            "C:\\AgentProbe\\output\\hold-job.json",
            serde_json::to_vec(&serde_json::json!({"job_id":id})).unwrap(),
        )
        .unwrap();
        let dispatch = workflow.begin(proposed.approval_id.unwrap()).unwrap();
        let executor = workflow.clone();
        let worker = std::thread::spawn(move || executor.execute(dispatch));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
        let held = std::path::Path::new("C:\\AgentProbe\\output\\held-reply.json");
        while !held.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let hold: serde_json::Value = serde_json::from_slice(&fs::read(held).unwrap()).unwrap();
        assert_eq!(hold["job_id"], id.to_string());
        assert_eq!(hold["upstream_write_verified"], true);
        let start = std::time::Instant::now();
        assert_eq!(
            workflow.cancel(id).unwrap().snapshot.phase,
            Phase::Cancelled
        );
        let cancel_ms = start.elapsed().as_secs_f64() * 1000.0;
        while !worker.is_finished() && start.elapsed() < std::time::Duration::from_secs(1) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(
            worker.is_finished(),
            "Owned client did not stop within one second"
        );
        let final_view = worker.join().unwrap().unwrap();
        assert_eq!(final_view.snapshot.phase, Phase::Cancelled);
        let stop_ms = start.elapsed().as_secs_f64() * 1000.0;
        fs::write("C:\\AgentProbe\\output\\release-reply.json", b"{}").unwrap();
        let released = std::path::Path::new("C:\\AgentProbe\\output\\held-released.json");
        while !released.is_file() && start.elapsed() < std::time::Duration::from_secs(2) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let late: serde_json::Value = serde_json::from_slice(&fs::read(released).unwrap()).unwrap();
        assert_eq!(late["write_requests"], 1);
        assert_eq!(
            fs::read_to_string(root.join("UserOriginal.ino")).unwrap(),
            original
        );
        assert!(!root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        assert!(!root
            .join(".hazlomexa/staging")
            .join(id.to_string())
            .join("result.json")
            .exists());
        drop(workflow);
        let reopened = Project::open(&root).unwrap();
        assert_eq!(reopened.recover().unwrap()[0].phase, Phase::Cancelled);
        drop(reopened);
        fs::write("C:\\AgentProbe\\output\\cancel-result.json",serde_json::to_vec_pretty(&serde_json::json!({"pass":true,"job_id":id,"root":root,"cancel_ms":cancel_ms,"client_stop_ms":stop_ms,"real_upstream_write_confirmed":true,"write_requests":1,"late_reply_published":false,"original_preserved":true,"recovered_cancelled":true,"hardware_mutations":false})).unwrap()).unwrap();
    }

    #[test]
    fn arduino_approval_edit_denial_and_mixed_history_preserve_legacy_runs() {
        let (root, workflow) = setup();
        fs::write(root.join("original.txt"), b"keep").unwrap();
        let cad = workflow.propose(dimensions()).unwrap();
        let operation = workflow.begin(cad.approval_id.unwrap()).unwrap();
        let stage = operation
            .project
            .reserve_stage(cad.snapshot.input.job_id())
            .unwrap();
        crate::project::tests::outputs(&stage, &operation.snapshot.input);
        workflow.finish(operation, Ok(stage)).unwrap();
        let source = "void setup() {} void loop() {}";
        let proposal = workflow
            .propose_compile(arduino::FQBN.into(), source.into())
            .unwrap();
        assert_eq!(proposal.snapshot.schema_version, 3);
        let id = proposal.snapshot.input.job_id();
        let edited = workflow
            .edit_compile(id, arduino::FQBN.into(), format!("{source}\n// edit"))
            .unwrap();
        assert_ne!(proposal.approval_id, edited.approval_id);
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());
        assert!(workflow.edit(id, dimensions()).is_err());
        assert!(workflow
            .edit_compile(id, "--upload".into(), source.into())
            .is_err());
        assert_eq!(
            workflow.current().unwrap().unwrap().approval_id,
            edited.approval_id
        );
        workflow.cancel(id).unwrap();
        assert!(workflow.begin(edited.approval_id.unwrap()).is_err());
        assert!(!root
            .join(".hazlomexa/artifacts")
            .join(id.to_string())
            .exists());
        let mut legacy = serde_json::to_value(&cad.snapshot).unwrap();
        legacy["schema_version"] = 1.into();
        legacy.as_object_mut().unwrap().remove("audit");
        assert!(serde_json::from_value::<Snapshot>(legacy)
            .unwrap()
            .input
            .solid()
            .is_ok());
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            history
                .iter()
                .find(|run| run.input.job_id() == id)
                .unwrap()
                .phase,
            Phase::Cancelled
        );
        assert_eq!(
            history
                .iter()
                .find(|run| run.input.job_id() == cad.snapshot.input.job_id())
                .unwrap()
                .phase,
            Phase::Completed
        );
        assert_eq!(fs::read(root.join("original.txt")).unwrap(), b"keep");
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn escaped_sketch_metadata_is_bounded_and_reopens_without_solid_dimensions() {
        let (root, workflow) = setup();
        let source = format!("//{}", "\u{0001}".repeat(65534));
        let proposal = workflow
            .propose_compile(arduino::FQBN.into(), source.clone())
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let bytes = fs::read(root.join(".hazlomexa/runs").join(format!("{id}.json"))).unwrap();
        assert!(bytes.len() > 65536 && bytes.len() < 1048576);
        let mut incompatible = proposal.snapshot.clone();
        incompatible.schema_version = 2;
        assert!(workflow
            .state
            .lock()
            .unwrap()
            .project
            .as_ref()
            .unwrap()
            .save(&incompatible)
            .is_err());
        let mut mixed = serde_json::to_value(&proposal.snapshot.input).unwrap();
        mixed["dimensions_mm"] = serde_json::to_value(dimensions()).unwrap();
        assert!(serde_json::from_value::<OperationInput>(mixed).is_err());
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        match &history[0].input {
            OperationInput::Arduino(input) => assert_eq!(input.source, source),
            _ => panic!("Wrong operation"),
        }
        assert_eq!(history[0].phase, Phase::Interrupted);
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn arduino_runtime_requires_owned_toolchain_marker_and_invalidates_approval() {
        let (root, workflow) = setup();
        let executable = root.join("arduino-cli.exe");
        fs::write(&executable, b"fixture; never executed").unwrap();
        let data = root.join("data");
        fs::create_dir(&data).unwrap();
        assert!(workflow
            .set_arduino(executable.clone(), data.clone())
            .is_err());
        fs::write(data.join("hazlomexa-toolchain.json"), b"{}").unwrap();
        assert!(workflow
            .set_arduino(executable.clone(), data.clone())
            .is_err());
        fs::write(data.join("hazlomexa-toolchain.json"), serde_json::to_vec(&serde_json::json!({"schema_version":1,"owner":"HazloMexa","cli_version":arduino::CLI_VERSION,"core_version":arduino::CORE_VERSION})).unwrap()).unwrap();
        let proposal = workflow
            .propose_compile(
                arduino::FQBN.into(),
                "void setup() {} void loop() {}".into(),
            )
            .unwrap();
        workflow
            .set_arduino(executable.clone(), data.clone())
            .unwrap();
        let configured = workflow.current().unwrap().unwrap();
        assert_ne!(configured.approval_id, proposal.approval_id);
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());
        let dispatch = workflow.begin(configured.approval_id.unwrap()).unwrap();
        assert!(workflow.set_arduino(executable, data).is_err());
        workflow.cancel(dispatch.snapshot.input.job_id()).unwrap();
        workflow.execute(dispatch).unwrap();
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires explicitly provisioned Arduino CLI and owned AVR core"]
    fn actual_arduino_workflow_publication_recovery_and_failure() {
        let executable = std::env::var_os("HAZLOMEXA_ARDUINO_CLI")
            .map(PathBuf::from)
            .expect("Explicit CLI required");
        let data = std::env::var_os("HAZLOMEXA_ARDUINO_DATA")
            .map(PathBuf::from)
            .expect("Explicit owned data required");
        let (root, workflow) = setup();
        fs::write(root.join("original.txt"), b"preserve Arduino original").unwrap();
        workflow.set_arduino(executable, data).unwrap();
        let proposal = workflow
            .propose_compile(
                arduino::FQBN.into(),
                "void setup() {} void loop() {}".into(),
            )
            .unwrap();
        let id = proposal.snapshot.input.job_id();
        let source = "void setup() { pinMode(LED_BUILTIN, OUTPUT); } void loop() { digitalWrite(LED_BUILTIN, HIGH); }";
        let edited = workflow
            .edit_compile(id, arduino::FQBN.into(), source.into())
            .unwrap();
        assert!(workflow.begin(proposal.approval_id.unwrap()).is_err());
        let dispatch = workflow.begin(edited.approval_id.unwrap()).unwrap();
        assert!(workflow.begin(edited.approval_id.unwrap()).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(completed.snapshot.phase, Phase::Completed);
        assert!(completed.snapshot.audit.as_ref().unwrap().files_verified);
        assert!(workflow.artifact(id, ArtifactFormat::Fcstd).is_err());
        let names = ["firmware.hex", "firmware.elf", "Sketch.ino"];
        let directory = root.join(".hazlomexa/artifacts").join(id.to_string());
        let baseline: Vec<_> = names
            .iter()
            .map(|name| fs::read(directory.join(name)).unwrap())
            .collect();
        assert_eq!(baseline[2], source.as_bytes());
        for format in [
            ArtifactFormat::Hex,
            ArtifactFormat::Elf,
            ArtifactFormat::Sketch,
        ] {
            assert!(workflow.artifact(id, format).unwrap().path.is_file());
        }
        let invalid = workflow
            .propose_compile(
                arduino::FQBN.into(),
                "void setup() { missing_symbol; } void loop() {}".into(),
            )
            .unwrap();
        let failed_id = invalid.snapshot.input.job_id();
        let failed = workflow
            .execute(workflow.begin(invalid.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(!failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert!(!root
            .join(".hazlomexa/artifacts")
            .join(failed_id.to_string())
            .exists());
        let denied = workflow
            .propose_compile(arduino::FQBN.into(), source.into())
            .unwrap();
        let denied_id = denied.snapshot.input.job_id();
        workflow.cancel(denied_id).unwrap();
        assert!(workflow.begin(denied.approval_id.unwrap()).is_err());
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let history = project.recover().unwrap();
        for (job, phase) in [
            (id, Phase::Completed),
            (failed_id, Phase::Failed),
            (denied_id, Phase::Cancelled),
        ] {
            assert_eq!(
                history
                    .iter()
                    .find(|run| run.input.job_id() == job)
                    .unwrap()
                    .phase,
                phase
            );
        }
        for (name, bytes) in names.iter().zip(baseline) {
            assert_eq!(fs::read(directory.join(name)).unwrap(), bytes);
        }
        assert_eq!(
            fs::read(root.join("original.txt")).unwrap(),
            b"preserve Arduino original"
        );
        let report = serde_json::json!({"pass":true,"root":root,"completed_job":id,"failed_job":failed_id,"denied_job":denied_id,"history_runs":history.len(),"artifacts_preserved":true,"hardware_mutations":false,"models_run":0});
        println!("{report}");
        fs::write(
            root.join("workflow-result.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn manual_fallback_clears_failed_inference_only_after_valid_proposal() {
        let (root, workflow) = setup();
        let request = workflow
            .start_inference(ModelProfile::LocalExperimental, "Un cubo de 10 mm")
            .unwrap();
        workflow
            .finish_inference(request, Err("Modelo no instalado.".into()))
            .unwrap();
        assert!(workflow
            .propose(Dimensions {
                height: 0.0,
                ..dimensions()
            })
            .is_err());
        assert!(workflow.view().unwrap().inference.unwrap().error.is_some());
        let proposal = workflow.propose(dimensions()).unwrap();
        assert!(workflow.view().unwrap().inference.is_none());
        assert_eq!(proposal.snapshot.audit.unwrap().source, RunSource::Manual);
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn history_keeps_previous_results_and_does_not_interrupt_an_active_run() {
        let (root, workflow) = setup();
        let first = workflow.propose(dimensions()).unwrap();
        let operation = workflow.begin(first.approval_id.unwrap()).unwrap();
        let stage = operation
            .project
            .reserve_stage(first.snapshot.input.job_id())
            .unwrap();
        crate::project::tests::outputs(&stage, &operation.snapshot.input);
        workflow.finish(operation, Ok(stage)).unwrap();
        let second = workflow.propose(dimensions()).unwrap();
        let operation = workflow.begin(second.approval_id.unwrap()).unwrap();
        let path = root
            .join(".hazlomexa/runs")
            .join(format!("{}.json", second.snapshot.input.job_id()));
        let before = fs::read(&path).unwrap();
        let view = workflow.view().unwrap();
        assert_eq!(view.history.len(), 2);
        assert!(view
            .history
            .iter()
            .any(|run| run.input.job_id() == first.snapshot.input.job_id()
                && run.phase == Phase::Completed));
        assert!(view
            .history
            .iter()
            .any(|run| run.input.job_id() == second.snapshot.input.job_id()
                && run.phase == Phase::Running));
        assert_eq!(view.job.unwrap().snapshot.phase, Phase::Running);
        assert_eq!(fs::read(&path).unwrap(), before);
        workflow.cancel(second.snapshot.input.job_id()).unwrap();
        workflow.finish(operation, Err("cancelled".into())).unwrap();
        assert_eq!(workflow.view().unwrap().history.len(), 2);
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gui_resolution_supports_portable_and_installed_layouts_and_missing_gui() {
        let (root, workflow) = setup();
        assert!(workflow
            .gui_executable()
            .unwrap_err()
            .contains("archivos están guardados"));
        fs::write(root.join("FreeCAD.exe"), b"fixture").unwrap();
        assert_eq!(workflow.gui_executable().unwrap(), root.join("FreeCAD.exe"));
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(root.join("bin/FreeCAD.exe"), b"fixture").unwrap();
        assert_eq!(
            workflow.gui_executable().unwrap(),
            root.join("bin/FreeCAD.exe")
        );
        assert!(workflow.current().unwrap().is_none());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn model_identity_edits_and_failure_history_survive_restart() {
        let (root, workflow) = setup();
        let fake_credential = "hazlomexa-test-secret-do-not-persist-9bb";
        let dispatch = workflow
            .start_inference(
                ModelProfile::Cloud,
                &format!("Un bloque de 10 por 20 por 30 mm; clave {fake_credential}"),
            )
            .unwrap();
        workflow
            .finish_inference(
                dispatch,
                Ok(Proposal::Proposal {
                    dimensions_mm: dimensions(),
                }),
            )
            .unwrap();
        let proposal = workflow.current().unwrap().unwrap();
        assert_eq!(proposal.snapshot.schema_version, 2);
        let first = proposal.snapshot.audit.as_ref().unwrap();
        assert_eq!(first.source, ModelProfile::Cloud.source());
        assert!(!first.user_edited);
        assert_eq!(first.sequence, proposal.sequence);
        let edited = workflow
            .edit(
                proposal.snapshot.input.job_id(),
                Dimensions {
                    length: 12.5,
                    ..dimensions()
                },
            )
            .unwrap();
        let audit = edited.snapshot.audit.as_ref().unwrap();
        assert!(audit.user_edited);
        assert_eq!(audit.created_unix_ms, first.created_unix_ms);
        assert!(audit.updated_unix_ms >= first.updated_unix_ms);
        assert_eq!(audit.sequence, edited.sequence);
        let operation = workflow.begin(edited.approval_id.unwrap()).unwrap();
        let failed = workflow
            .finish(operation, Err("FreeCAD no está disponible.".into()))
            .unwrap();
        assert_eq!(
            failed.snapshot.audit.as_ref().unwrap().error.as_deref(),
            failed.error.as_deref()
        );
        assert_eq!(
            failed.snapshot.audit.as_ref().unwrap().sequence,
            failed.sequence
        );
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let recovered = project.recover().unwrap();
        assert_eq!(recovered[0].phase, Phase::Failed);
        let stored = recovered[0].audit.as_ref().unwrap();
        assert_eq!(stored.source, ModelProfile::Cloud.source());
        assert!(stored.user_edited);
        assert_eq!(stored.error.as_deref(), Some("FreeCAD no está disponible."));
        assert_eq!(stored.sequence, failed.sequence);
        assert_eq!(
            stored.destination,
            format!(".hazlomexa/artifacts/{}", proposal.snapshot.input.job_id())
        );
        for entry in fs::read_dir(root.join(".hazlomexa/runs")).unwrap() {
            let bytes = fs::read(entry.unwrap().path()).unwrap();
            assert!(!String::from_utf8_lossy(&bytes).contains(fake_credential));
        }
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inference_cannot_dispatch_and_cancelled_or_old_results_cannot_propose() {
        let (root, workflow) = setup();
        let dispatch = workflow
            .start_inference(ModelProfile::Cloud, "Un cubo de 10 mm por lado")
            .unwrap();
        let id = dispatch.id;
        workflow.inference_delta(&dispatch, "{\"status\":").unwrap();
        assert!(workflow.propose(dimensions()).is_err());
        assert!(workflow
            .start_inference(ModelProfile::Cloud, "otro")
            .is_err());
        assert!(workflow.current().unwrap().is_none());
        workflow.cancel_inference(id).unwrap();
        assert!(workflow
            .inference_delta(&dispatch, "late")
            .unwrap()
            .is_none());
        let view = workflow
            .finish_inference(
                dispatch,
                Ok(Proposal::Proposal {
                    dimensions_mm: dimensions(),
                }),
            )
            .unwrap()
            .unwrap();
        assert!(!view.busy);
        assert!(view.error.is_some());
        assert!(workflow.current().unwrap().is_none());

        let old = workflow
            .start_inference(ModelProfile::Cloud, "cubo de 10 mm")
            .unwrap();
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        workflow
            .select_project(Project::create(&other).unwrap())
            .unwrap();
        let new = workflow
            .start_inference(ModelProfile::Cloud, "cubo de 20 mm")
            .unwrap();
        assert!(workflow
            .finish_inference(
                old,
                Ok(Proposal::Proposal {
                    dimensions_mm: dimensions()
                })
            )
            .unwrap()
            .is_none());
        assert!(workflow.current().unwrap().is_none());
        workflow
            .finish_inference(
                new,
                Ok(Proposal::Proposal {
                    dimensions_mm: dimensions(),
                }),
            )
            .unwrap();
        let proposal = workflow.current().unwrap().unwrap();
        assert_eq!(proposal.snapshot.phase, Phase::AwaitingApproval);
        assert!(proposal.approval_id.is_some());
        assert_eq!(
            fs::read_dir(other.join(".hazlomexa/staging"))
                .unwrap()
                .count(),
            0
        );
        assert!(workflow
            .start_inference(ModelProfile::Cloud, "otro")
            .is_err());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inference_clarification_and_invalid_dimensions_leave_no_job() {
        let (root, workflow) = setup();
        for result in [
            Ok(Proposal::Clarification {
                message: "¿Qué alto?".into(),
            }),
            Ok(Proposal::Proposal {
                dimensions_mm: Dimensions {
                    height: 0.0,
                    ..dimensions()
                },
            }),
            Err("connection lost".into()),
        ] {
            let dispatch = workflow
                .start_inference(ModelProfile::LocalExperimental, "largo 10 mm ancho 20 mm")
                .unwrap();
            let view = workflow
                .finish_inference(dispatch, result)
                .unwrap()
                .unwrap();
            assert!(!view.busy);
            assert!(view.message.is_some() || view.error.is_some());
            assert!(workflow.current().unwrap().is_none());
        }
        assert!(workflow
            .start_inference(ModelProfile::Cloud, &"x".repeat(4097))
            .is_err());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edits_invalidate_previous_approval_and_approval_is_single_use() {
        let (root, workflow) = setup();
        let first = workflow.propose(dimensions()).unwrap();
        let edited = workflow
            .edit(
                first.snapshot.input.job_id(),
                Dimensions {
                    length: 12.0,
                    ..dimensions()
                },
            )
            .unwrap();
        assert!(workflow.begin(first.approval_id.unwrap()).is_err());
        let dispatch = workflow.begin(edited.approval_id.unwrap()).unwrap();
        assert_eq!(
            dispatch
                .snapshot
                .input
                .solid()
                .unwrap()
                .dimensions_mm
                .length,
            12.0
        );
        assert!(workflow.begin(edited.approval_id.unwrap()).is_err());
        workflow.cancel(edited.snapshot.input.job_id()).unwrap();
        assert!(workflow.propose(dimensions()).is_err());
        assert_eq!(
            workflow
                .finish(dispatch, Err("late result".into()))
                .unwrap()
                .snapshot
                .phase,
            Phase::Cancelled
        );
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn denial_prevents_dispatch_and_project_switch_invalidates_approval() {
        let (root, workflow) = setup();
        let first = workflow.propose(dimensions()).unwrap();
        workflow.cancel(first.snapshot.input.job_id()).unwrap();
        assert!(workflow.begin(first.approval_id.unwrap()).is_err());
        let next = workflow.propose(dimensions()).unwrap();
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        workflow
            .select_project(Project::create(&other).unwrap())
            .unwrap();
        assert!(workflow.begin(next.approval_id.unwrap()).is_err());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_never_restores_a_live_approval() {
        let (root, workflow) = setup();
        let first = workflow.propose(dimensions()).unwrap();
        drop(workflow);
        let workflow = Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap();
        let recovered = workflow
            .select_project(Project::open(&root).unwrap())
            .unwrap();
        assert_eq!(recovered[0].phase, Phase::Interrupted);
        assert!(workflow.begin(first.approval_id.unwrap()).is_err());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopening_selected_project_reuses_writer_and_invalidates_approval() {
        let (root, workflow) = setup();
        let first = workflow.propose(dimensions()).unwrap();
        let recovered = workflow.reopen_current(&root).unwrap().unwrap();
        assert_eq!(recovered[0].phase, Phase::Interrupted);
        assert!(workflow.current().unwrap().is_none());
        assert!(workflow.begin(first.approval_id.unwrap()).is_err());
        let next = workflow.propose(dimensions()).unwrap();
        let dispatch = workflow.begin(next.approval_id.unwrap()).unwrap();
        assert!(workflow.reopen_current(&root).is_err());
        workflow.cancel(next.snapshot.input.job_id()).unwrap();
        workflow.finish(dispatch, Err("cancelled".into())).unwrap();
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires the owned 64 MiB guest NTFS disk and official FreeCAD 1.1.4"]
    fn actual_cad_publication_on_exhausted_disk_preserves_whole_outputs_and_recovery() {
        use std::io::Write;
        assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
        assert_eq!(
            fs::read(r"T:\HazloDiskProject\owned-fixture.txt").unwrap(),
            b"Hazlo owned 64 MiB NTFS fixture"
        );
        let executable = PathBuf::from(std::env::var_os("HAZLOMEXA_FREECAD_CMD").unwrap());
        assert_eq!(executable, PathBuf::from(r"C:\FreeCADRO\FreeCADCmd.exe"));
        let root = PathBuf::from(r"T:\HazloDiskPublication");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("original.txt"), b"preserve original on full disk").unwrap();
        let profiles = PathBuf::from(r"C:\HazloDiskPublicationProfiles");
        assert!(!profiles.exists());
        let workflow = Workflow::new(executable.clone(), profiles.clone()).unwrap();
        workflow
            .select_project(Project::create(&root).unwrap())
            .unwrap();
        let first = workflow.propose(dimensions()).unwrap();
        let baseline = workflow
            .execute(workflow.begin(first.approval_id.unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            baseline.snapshot.phase,
            Phase::Completed,
            "{:?}",
            baseline.error
        );
        let baseline_path = root
            .join(".hazlomexa/artifacts")
            .join(baseline.snapshot.input.job_id().to_string());
        let names = ["input.json", "result.json", "solid.FCStd", "solid.step"];
        let baseline_bytes: Vec<_> = names
            .iter()
            .map(|name| fs::read(baseline_path.join(name)).unwrap())
            .collect();
        let next = workflow.propose(dimensions()).unwrap();
        let dispatch = workflow.begin(next.approval_id.unwrap()).unwrap();
        let job = dispatch.snapshot.input.job_id();
        let stage = dispatch.project.reserve_stage(job).unwrap();
        let stage_path = stage.path.clone();
        freecad::generate(
            &executable,
            &stage.path,
            &profiles.join(job.to_string()),
            dispatch.snapshot.input.solid().unwrap(),
            dispatch.cancelled.clone(),
        )
        .unwrap();
        let generated_bytes: Vec<_> = names
            .iter()
            .map(|name| fs::read(stage.path.join(name)).unwrap())
            .collect();
        let run_path = root.join(".hazlomexa/runs").join(format!("{job}.json"));
        let previous = fs::read(&run_path).unwrap();
        let filler_path = root.join("owned-filler.bin");
        let mut filler = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&filler_path)
            .unwrap();
        let mut written = 0;
        let full_error = loop {
            assert!(written < 64 * 1024 * 1024, "Not the bounded guest disk");
            match filler.write_all(&[0x5a; 4096]) {
                Ok(()) => written += 4096,
                Err(error) => break error,
            }
        };
        assert!(
            matches!(full_error.raw_os_error(), Some(112 | 39)),
            "{full_error}"
        );
        let finish = workflow.finish(dispatch, Ok(stage));
        let current = workflow.current().unwrap().unwrap();
        assert!(!current.busy);
        assert!(matches!(
            current.snapshot.phase,
            Phase::Completed | Phase::Failed
        ));
        let published_path = root.join(".hazlomexa/artifacts").join(job.to_string());
        let published = published_path.exists();
        assert_eq!(published, current.snapshot.phase == Phase::Completed);
        let retained_path = if published {
            &published_path
        } else {
            &stage_path
        };
        for (name, bytes) in names.iter().zip(&generated_bytes) {
            assert_eq!(&fs::read(retained_path.join(name)).unwrap(), bytes);
        }
        for (name, bytes) in names.iter().zip(&baseline_bytes) {
            assert_eq!(&fs::read(baseline_path.join(name)).unwrap(), bytes);
        }
        if let Err(error) = &finish {
            assert!(error.contains("guardar el estado"), "{error}");
            assert_eq!(fs::read(&run_path).unwrap(), previous);
            assert!(current.error.is_some());
        } else if !published {
            assert!(current.error.as_ref().unwrap().contains("publicar"));
        }
        drop(filler);
        fs::remove_file(&filler_path).unwrap();
        drop(workflow);
        let reopened = Project::open(&root).unwrap();
        let recovered = reopened.recover().unwrap();
        assert_eq!(recovered.len(), 2);
        assert_eq!(
            recovered
                .iter()
                .find(|run| run.input.job_id() == baseline.snapshot.input.job_id())
                .unwrap()
                .phase,
            Phase::Completed
        );
        let outcome = recovered
            .iter()
            .find(|run| run.input.job_id() == job)
            .unwrap();
        if published {
            assert_eq!(outcome.phase, Phase::Completed);
        } else {
            assert!(matches!(outcome.phase, Phase::Failed | Phase::Interrupted));
        }
        assert_eq!(
            fs::read(root.join("original.txt")).unwrap(),
            b"preserve original on full disk"
        );
        assert_eq!(
            fs::read_dir(root.join(".hazlomexa/artifacts"))
                .unwrap()
                .count(),
            if published { 2 } else { 1 }
        );
        let result = serde_json::json!({"pass":true,"root":root,"job_id":job,"baseline_job_id":baseline.snapshot.input.job_id(),"filled_bytes":written,"disk_full_error":full_error.raw_os_error(),"finish_error":finish.err(),"published":published,"phase":current.snapshot.phase,"recovered_phase":outcome.phase,"actual_generated_files_preserved":true,"baseline_files_preserved":true,"original_preserved":true,"no_regeneration":true});
        fs::write(
            r"C:\HazloProbe\output\cad-disk-full-result.json",
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn publication_failure_preserves_verified_files_and_distinction_on_reopen() {
        let (root, workflow) = setup();
        let proposal = workflow.propose(dimensions()).unwrap();
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        let job = dispatch.snapshot.input.job_id();
        let stage = dispatch.project.reserve_stage(job).unwrap();
        crate::project::tests::outputs(&stage, &dispatch.snapshot.input);
        let destination = dispatch.project.artifact_directory(job);
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("original.txt"), b"keep existing output").unwrap();
        let failed = workflow.finish(dispatch, Ok(stage)).unwrap();
        assert_eq!(failed.snapshot.phase, Phase::Failed);
        assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
        assert!(failed.error.as_ref().unwrap().contains("publicar"));
        assert!(root
            .join(".hazlomexa/staging")
            .join(job.to_string())
            .join("solid.FCStd")
            .is_file());
        assert_eq!(
            fs::read(destination.join("original.txt")).unwrap(),
            b"keep existing output"
        );
        assert!(!destination.join("solid.FCStd").exists());
        drop(workflow);
        let project = Project::open(&root).unwrap();
        let recovered = project.recover().unwrap();
        assert_eq!(recovered[0].phase, Phase::Failed);
        assert!(recovered[0].audit.as_ref().unwrap().files_verified);
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_cancel_and_commit_have_one_authoritative_outcome() {
        for _ in 0..24 {
            let (root, workflow) = setup();
            let workflow = Arc::new(workflow);
            let proposal = workflow.propose(dimensions()).unwrap();
            let job = proposal.snapshot.input.job_id();
            let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
            let stage = dispatch.project.reserve_stage(job).unwrap();
            crate::project::tests::outputs(&stage, &dispatch.snapshot.input);
            let barrier = Arc::new(std::sync::Barrier::new(3));
            let commit_workflow = workflow.clone();
            let commit_barrier = barrier.clone();
            let commit = std::thread::spawn(move || {
                commit_barrier.wait();
                commit_workflow.finish(dispatch, Ok(stage)).unwrap()
            });
            let cancel_workflow = workflow.clone();
            let cancel_barrier = barrier.clone();
            let cancel = std::thread::spawn(move || {
                cancel_barrier.wait();
                cancel_workflow.cancel(job).unwrap()
            });
            barrier.wait();
            let finished = commit.join().unwrap();
            cancel.join().unwrap();
            let current = workflow.current().unwrap().unwrap();
            assert_eq!(current.snapshot.phase, finished.snapshot.phase);
            assert!(!current.busy);
            assert!(current.approval_id.is_none());
            let published = root.join(".hazlomexa/artifacts").join(job.to_string());
            match current.snapshot.phase {
                Phase::Completed => {
                    assert!(published.join("solid.FCStd").is_file());
                    assert!(published.join("solid.step").is_file());
                }
                Phase::Cancelled => assert!(!published.exists()),
                phase => panic!("Unexpected cancel/commit outcome: {phase:?}"),
            }
            drop(workflow);
            let project = Project::open(&root).unwrap();
            assert_eq!(project.recover().unwrap()[0].phase, current.snapshot.phase);
            drop(project);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[ignore = "requires HAZLOMEXA_FREECAD_CMD pointing to official FreeCAD 1.1.4"]
    fn actual_approved_workflow_publishes_and_recovers() {
        let executable = PathBuf::from(std::env::var_os("HAZLOMEXA_FREECAD_CMD").unwrap());
        let root = std::env::temp_dir().join(format!("Hazlo Mexa á's workflow {}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = Project::create(&root).unwrap();
        let project_id = project.id;
        let profile = if std::env::var_os("HAZLOMEXA_TEST_ASCII_PROFILES").is_some() {
            std::env::temp_dir().join(format!("hazlomexa-workflow-profile-{}", Uuid::new_v4()))
        } else {
            root.join("profiles")
        };
        let workflow = Workflow::new(executable, profile.clone()).unwrap();
        workflow.select_project(project).unwrap();
        let proposal = workflow.propose(dimensions()).unwrap();
        let job_id = proposal.snapshot.input.job_id();
        let artifacts = root.join(".hazlomexa/artifacts").join(job_id.to_string());
        assert!(!artifacts.exists());
        assert_eq!(
            fs::read_dir(root.join(".hazlomexa/staging"))
                .unwrap()
                .count(),
            0
        );
        let dispatch = workflow.begin(proposal.approval_id.unwrap()).unwrap();
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        assert!(!completed.busy);
        for name in ["solid.FCStd", "solid.step", "result.json"] {
            assert!(fs::metadata(artifacts.join(name)).unwrap().len() > 0);
        }
        assert_eq!(
            workflow.cancel(job_id).unwrap().snapshot.phase,
            Phase::Completed
        );
        drop(workflow);
        let project = Project::open(&root).unwrap();
        assert_eq!(project.id, project_id);
        let recovered = project.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].phase, Phase::Completed);
        assert_eq!(recovered[0].input.job_id(), job_id);
        drop(project);
        fs::remove_dir_all(root).unwrap();
        if profile.exists() {
            fs::remove_dir_all(profile).unwrap();
        }
    }
}
