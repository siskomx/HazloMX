//! Task-local history and immutable references. Sharing is granted by owned desktop IPC, not MCP.
use crate::{
    image_input::ImageInput,
    project::{atomic_json, pin_lineage, read_json, Project},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
    path::PathBuf,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

const FILE_LIMIT: usize = 32 * 1024 * 1024;
const TASK_LIMIT: u64 = 256 * 1024 * 1024;
const WITHDRAWN_REPLY:&str="Respuesta retirada junto con su permiso de referencia local.";
fn error(s: &str) -> io::Error {
    io::Error::other(s)
}
fn provider(s: &str) -> bool {
    matches!(s, "codex" | "claude" | "ollama_local" | "ollama_cloud")
}
fn history(raw: &str) -> io::Result<()> {
    if raw.len() > 512 * 1024 {
        return Err(error("La conversación supera 512 KiB."));
    }
    let value: serde_json::Value = serde_json::from_str(raw).map_err(io::Error::other)?;
    if value["version"] != 1
        || value["turns"].as_array().is_none_or(|turns| {
            turns.len() > 128
                || turns.iter().any(|t| {
                    !matches!(t["role"].as_str(), Some("user" | "assistant"))
                        || !t["content"].is_string()
                })
        })
    {
        return Err(error("Historial de tarea no válido."));
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: Uuid,
    pub revision: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub id: Uuid,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub image: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_sha256: Option<String>,
    pub shared_with: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<Uuid>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRemoval {
    pub id: Uuid,
    pub bytes: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    pub job_id: Uuid,
    pub revision: u64,
    pub text: String,
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCorrectionInput {pub id:Uuid,pub revision:u64,pub chat_id:Uuid,pub correction_id:Uuid,pub text:String}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCorrection {pub id:Uuid,pub chat_id:Uuid,pub revision:u64,pub text:String}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatStop {pub chat_id:Uuid,pub revision:u64}
#[derive(Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplySource {
    pub id:Uuid,
    pub sha256:String,
    pub preview_sha256:Option<String>,
}
impl Reference {
    pub(crate) fn reply_source(&self)->ReplySource{ReplySource{id:self.id,sha256:self.sha256.clone(),preview_sha256:self.preview_sha256.clone()}}
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyReceipt {
    pub id:Uuid,
    pub task:Binding,
    pub provider:String,
    pub content_sha256:String,
    pub parent_sha256:String,
    pub position:usize,
    pub references:Vec<ReplySource>,
    pub revoked:bool,
}
pub(crate) fn history_parent(raw:Option<&str>)->io::Result<(String,usize)>{
    let value:serde_json::Value=serde_json::from_str(raw.unwrap_or("{\"version\":1,\"turns\":[]}")).map_err(io::Error::other)?;
    let turns=value["turns"].as_array().ok_or_else(||error("Historial no válido."))?;
    Ok((parent_digest(turns)?,turns.len()))
}
pub(crate) fn parent_digest(turns:&[serde_json::Value])->io::Result<String>{
    let projection=turns.iter().map(|t|serde_json::json!({"role":t["role"],"content":t["content"]})).collect::<Vec<_>>();
    Ok(crate::kicad_project::digest(&serde_json::to_vec(&projection).map_err(io::Error::other)?))
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: Uuid,
    pub revision: u64,
    pub title: String,
    #[serde(default)]
    pub last_user_revision: u64,
    pub goal: String,
    pub provider: String,
    pub references: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_removals: Vec<PendingRemoval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_correction: Option<Correction>,
    #[serde(default)]
    pub restricted_results: bool,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub withdrawn_replies:std::collections::BTreeMap<String,u64>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub reply_receipts:Vec<ReplyReceipt>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub chat_correction:Option<ChatCorrection>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub chat_stop:Option<ChatStop>,
    #[serde(default,skip_serializing_if="std::collections::BTreeSet::is_empty")]
    pub interrupted_replies:std::collections::BTreeSet<String>,
    #[serde(default)]
    pub legacy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<String>,
}
impl Task {
    pub fn reply_receipt_allows(&self,id:Uuid,content:&str)->bool{
        let hash=crate::kicad_project::digest(content.as_bytes());
        self.reply_receipts.iter().any(|r|r.id==id && r.content_sha256==hash && self.receipt_authorized(r))
    }
    fn receipt_authorized(&self,r:&ReplyReceipt)->bool{
        !r.revoked && r.task.id==self.id && r.provider==self.provider && r.references.iter().all(|source|self.reply_source_allowed(source,&r.provider))
    }
    pub fn reply_source_allowed(&self,source:&ReplySource,provider:&str)->bool{self.references.iter().any(|r|r.id==source.id && r.sha256==source.sha256 && r.preview_sha256==source.preview_sha256 && r.shared_with.iter().any(|p|p==provider))}
    pub(crate) fn history_reply_allows(&self,turn:&serde_json::Value)->bool{
        turn["reply_id"].as_str().and_then(|id|Uuid::parse_str(id).ok()).zip(turn["content"].as_str()).is_some_and(|(id,content)|self.reply_receipt_allows(id,content))
    }
    pub(crate) fn history_reply_parent_allows(&self,turn:&serde_json::Value,position:usize,parent:&str)->bool{
        self.history_reply_allows(turn) && self.reply_receipts.iter().any(|r|Some(r.id.to_string())==turn["reply_id"].as_str().map(str::to_string) && r.position==position && r.parent_sha256==parent)
    }
    pub fn reply_is_withdrawn(&self,content:&str)->bool{
        self.withdrawn_replies.contains_key(&crate::kicad_project::digest(content.as_bytes()))
    }
    pub(crate) fn reply_is_interrupted(&self,content:&str)->bool{self.interrupted_replies.contains(&crate::kicad_project::digest(content.as_bytes()))}
    pub fn reply_was_withdrawn_after(&self,content:&str,revision:u64)->bool{
        self.withdrawn_replies.get(&crate::kicad_project::digest(content.as_bytes())).is_some_and(|withdrawn|*withdrawn>revision)
    }
    pub fn binding(&self) -> Binding {
        Binding {
            id: self.id,
            revision: self.revision,
        }
    }
    pub fn restricted(&self) -> bool {
        self.restricted_results
            || self
                .references
                .iter()
                .any(|r| !r.shared_with.contains(&self.provider))
    }
    fn validate(&self) -> io::Result<()> {
        if self.chat_stop.as_ref().is_some_and(|stop|stop.chat_id.is_nil() || stop.revision==0 || stop.revision>self.revision){return Err(error("Pausa de conversación no válida; se conservó la tarea."));}
        if self.chat_correction.as_ref().is_some_and(|c|c.id.is_nil() || c.chat_id.is_nil() || c.revision==0 || c.revision>self.revision || c.text.trim().is_empty() || c.text.len()>4096){return Err(error("Corrección de conversación no válida; se conservó la tarea."));}
        if self.latest_correction.as_ref().is_some_and(|c| {
            c.job_id.is_nil()
                || c.revision == 0
                || c.revision > self.revision
                || c.text.trim().is_empty()
                || c.text.len() > 4096
        }) {
            return Err(error(
                "Corrección de tarea no válida; sus datos se conservan.",
            ));
        }
        if self.id.is_nil()
            || self.revision == 0
            || self.last_user_revision > self.revision
            || !provider(&self.provider)
            || self.title.len() > 256
            || self.goal.len() > 4096
            || self.references.len() + self.pending_removals.len() > 64
            || self.withdrawn_replies.len()>128
            || self.withdrawn_replies.iter().any(|(h,revision)|!crate::kicad_project::valid_hash(h) || *revision==0 || *revision>self.revision)
            || serde_json::to_vec_pretty(self)
                .map_err(io::Error::other)?
                .len()
                > 1024 * 1024
        {
            return Err(error(
                "Tarea no válida o demasiado grande; sus datos se conservan.",
            ));
        }
        if let Some(raw) = &self.history {
            history(raw)?;
        }
        let mut receipt_ids=std::collections::BTreeSet::new();
        if self.reply_receipts.len()>128 || self.reply_receipts.iter().any(|r|r.id.is_nil() || !receipt_ids.insert(r.id) || r.task.id!=self.id || r.task.revision==0 || r.task.revision>self.revision || !provider(&r.provider) || !crate::kicad_project::valid_hash(&r.content_sha256) || !crate::kicad_project::valid_hash(&r.parent_sha256) || r.position>=128 || r.references.len()>64 || r.references.iter().any(|s|s.id.is_nil() || !crate::kicad_project::valid_hash(&s.sha256) || s.preview_sha256.as_ref().is_some_and(|h|!crate::kicad_project::valid_hash(h))) || r.references.iter().map(|s|s.id).collect::<std::collections::BTreeSet<_>>().len()!=r.references.len() || (!r.revoked && r.references.iter().any(|s|!self.references.iter().any(|reference|reference.id==s.id)))) {return Err(error("Proveniencia de respuesta no válida; se conservó la conversación."));}
        let mut reply_hashes:std::collections::BTreeSet<_>=self.withdrawn_replies.keys().cloned().collect();
        if self.interrupted_replies.len()>128 || self.interrupted_replies.iter().any(|hash|!crate::kicad_project::valid_hash(hash)){return Err(error("Registro de borradores no válido; se conservó la tarea."));}
        reply_hashes.extend(self.interrupted_replies.iter().cloned());
        reply_hashes.extend(self.reply_receipts.iter().map(|r|r.content_sha256.clone()));
        if let Some(raw)=&self.history{
            let saved:serde_json::Value=serde_json::from_str(raw).map_err(io::Error::other)?;
            for turn in saved["turns"].as_array().into_iter().flatten().filter(|t|t["role"]=="assistant"){
                let content=turn["content"].as_str().unwrap();
                if content!=WITHDRAWN_REPLY{reply_hashes.insert(crate::kicad_project::digest(content.as_bytes()));}
            }
        }
        // Reserve non-content withdrawal records at intake, so revocation cannot run out of this budget.
        if reply_hashes.len()>128{return Err(error("Se alcanzó el límite de respuestas guardadas de esta conversación. Conserva su historial e inicia otra para continuar; no se reemplazó el trabajo anterior."));}
        let mut ids = std::collections::BTreeSet::new();
        let mut total = 0;
        for r in &self.references {
            let parents: std::collections::BTreeSet<_> = r.parents.iter().copied().collect();
            if r.id.is_nil()
                || parents.len() != r.parents.len()
                || parents.iter().any(|id| !ids.contains(id))
                || !ids.insert(r.id)
                || !valid_name(&r.name)
                || r.bytes == 0
                || r.bytes > FILE_LIMIT as u64
                || !crate::kicad_project::valid_hash(&r.sha256)
                || r.preview_sha256
                    .as_ref()
                    .is_some_and(|h| !r.image || !crate::kicad_project::valid_hash(h))
                || r.shared_with.len() > 4
                || r.shared_with.iter().any(|p| !provider(p))
            {
                return Err(error("Referencia de tarea no válida."));
            }
            total += r.bytes;
        }
        for r in &self.pending_removals {
            if r.id.is_nil() || !ids.insert(r.id) || r.bytes == 0 || r.bytes > FILE_LIMIT as u64 {
                return Err(error(
                    "Retirada de referencia no válida; sus datos se conservan.",
                ));
            }
            total += r.bytes;
        }
        if total > TASK_LIMIT {
            return Err(error("Las referencias superan 256 MiB por tarea."));
        }
        Ok(())
    }
}
fn descendants(task: &Task, reference: Uuid) -> std::collections::BTreeSet<Uuid> {
    let mut affected = std::collections::BTreeSet::from([reference]);
    // References are validated in parent-before-child order; cycles and missing parents fail closed.
    for r in &task.references {
        if r.parents.iter().any(|id| affected.contains(id)) {
            affected.insert(r.id);
        }
    }
    affected
}
fn scrub_history(task: &mut Task, affected: &std::collections::BTreeSet<Uuid>) -> io::Result<()> {
    let revision=task.revision.checked_add(1).ok_or_else(||error("La revisión llegó a su límite; se conservó el historial."))?;
    // Without fresh reply provenance, another withdrawal must fence matching older cached generations too.
    for withdrawn in task.withdrawn_replies.values_mut(){*withdrawn=revision;}
    for receipt in &mut task.reply_receipts{
        if receipt.references.iter().any(|source|affected.contains(&source.id)){
            receipt.revoked=true;task.withdrawn_replies.insert(receipt.content_sha256.clone(),revision);
        }
    }
    let mut value: serde_json::Value =
        serde_json::from_str(task.history.as_deref().unwrap_or("{}")).map_err(io::Error::other)?;
    let mut authenticated=std::collections::BTreeSet::new();
    if let Some(turns)=value["turns"].as_array(){
        for (position,turn) in turns.iter().enumerate().filter(|(_,turn)|turn["role"]=="assistant"){
            let parent=parent_digest(&turns[..position])?;
            if let Some(receipt)=task.reply_receipts.iter().find(|r|!r.revoked && r.position==position && r.parent_sha256==parent && turn["reply_id"].as_str()==Some(r.id.to_string().as_str()) && turn["content"].as_str().is_some_and(|text|crate::kicad_project::digest(text.as_bytes())==r.content_sha256)){
                authenticated.insert((receipt.id,position));
            }
        }
    }
    if let Some(turns) = value["turns"].as_array_mut() {
        for turn in turns {
            if turn["reference_ids"].as_array().is_some_and(|ids| {
                ids.iter()
                    .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                    .any(|id| affected.contains(&id))
            }) {
                if turn["role"]=="assistant" {
                    if let Some(content)=turn["content"].as_str().filter(|content|*content!=WITHDRAWN_REPLY){
                        let hash=crate::kicad_project::digest(content.as_bytes());
                        task.withdrawn_replies.insert(hash,revision);
                    }
                }
                *turn = serde_json::json!({"role":"assistant","content":WITHDRAWN_REPLY});
            }
        }
    }
    if let Some(turns)=value["turns"].as_array(){
        for (position,turn) in turns.iter().enumerate(){
            if let Some(id)=turn["reply_id"].as_str().and_then(|id|Uuid::parse_str(id).ok()){
                if let Some(receipt)=task.reply_receipts.iter_mut().find(|r|r.id==id && authenticated.contains(&(id,position)) && !r.revoked && turn["content"].as_str().is_some_and(|text|crate::kicad_project::digest(text.as_bytes())==r.content_sha256)){
                    receipt.position=position;receipt.parent_sha256=parent_digest(&turns[..position])?;
                }
            }
        }
    }
    task.history = Some(serde_json::to_string(&value).map_err(io::Error::other)?);
    Ok(())
}
fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 240
        && !s
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
}
fn retain_lineage(task: &Task, raw: &str) -> io::Result<String> {
    history(raw)?;
    let mut value: serde_json::Value = serde_json::from_str(raw).map_err(io::Error::other)?;
    let prior: serde_json::Value =
        serde_json::from_str(task.history.as_deref().unwrap_or("{}")).map_err(io::Error::other)?;
    let incoming=value["turns"].as_array().unwrap().clone();
    for (position,turn) in value["turns"].as_array_mut().unwrap().iter_mut().enumerate(){
        if turn["role"] == "assistant" {
            let hash=crate::kicad_project::digest(turn["content"].as_str().unwrap().as_bytes());
            let parent=parent_digest(&incoming[..position])?;
            let reply_id=turn["reply_id"].as_str().and_then(|id|Uuid::parse_str(id).ok());
            let fresh=task.reply_receipts.iter().find(|r|Some(r.id)==reply_id && r.content_sha256==hash && r.position==position && r.parent_sha256==parent && task.receipt_authorized(r));
            if let Some(receipt)=fresh{turn["reply_id"]=serde_json::json!(receipt.id);turn["reference_ids"]=serde_json::json!(receipt.references.iter().map(|s|s.id).collect::<Vec<_>>());continue;}
            let known=prior["turns"][position].clone();
            if known["role"]=="assistant" && known["content"]==turn["content"] && task.history_reply_parent_allows(&known,position,&parent){turn["reply_id"]=known["reply_id"].clone();turn["reference_ids"]=known["reference_ids"].clone();continue;}
            turn.as_object_mut().unwrap().remove("reply_id");
            if task.reply_is_interrupted(turn["content"].as_str().unwrap()) || prior["turns"].as_array().into_iter().flatten().any(|saved|saved["role"]=="assistant" && saved["interrupted"]==true && saved["content"]==turn["content"]){turn["interrupted"]=serde_json::json!(true);}
            if task.withdrawn_replies.contains_key(&hash){*turn=serde_json::json!({"role":"assistant","content":WITHDRAWN_REPLY});continue;}
            let mut references=std::collections::BTreeSet::new();let mut found=false;
            for saved in prior["turns"].as_array().into_iter().flatten().filter(|saved|saved["role"]=="assistant" && saved["content"]==turn["content"]){
                found=true;
                if let Some(ids)=saved["reference_ids"].as_array(){for id in ids {if let Some(id)=id.as_str().and_then(|id|Uuid::parse_str(id).ok()){references.insert(id);}}}
            }
            if !found{references.extend(task.references.iter().filter(|r|r.shared_with.contains(&task.provider)).map(|r|r.id));}
            turn["reference_ids"]=serde_json::json!(references);
        }
    }
    serde_json::to_string(&value).map_err(io::Error::other)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    active: Uuid,
    ids: Vec<Uuid>,
    #[serde(default)]
    titles: std::collections::BTreeMap<Uuid, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Creation {
    version:u32,
    before:Option<Index>,
    after:Index,
    tasks:Vec<CreatedTask>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatedTask {
    id:Uuid,
    provider:String,
    legacy_history_sha256:Option<String>,
    task_sha256:String,
}
#[derive(Serialize)]
pub struct State {
    pub active: Task,
    pub tasks: Vec<Summary>,
}
#[derive(Serialize)]
pub struct Summary {
    pub id: Uuid,
    pub title: String,
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    State,
    New,
    Open {
        id: Uuid,
    },
    History {
        id: Uuid,
        revision: u64,
        value: Option<String>,
    },
    Message {
        id: Uuid,
        revision: u64,
        text: String,
        history: String,
        #[serde(default)]
        correction_of: Option<Uuid>,
    },
    Provider {
        id: Uuid,
        revision: u64,
        provider: String,
    },
    AddReference {
        id: Uuid,
        revision: u64,
        name: String,
        data: String,
        preview: Option<ImageInput>,
    },
    Share {
        id: Uuid,
        revision: u64,
        reference: Uuid,
        allow: bool,
    },
    Remove {
        id: Uuid,
        revision: u64,
        reference: Uuid,
    },
    Image {
        id: Uuid,
        revision: u64,
        reference: Uuid,
    },
    PreviewReference {
        id: Uuid,
        revision: u64,
        reference: Uuid,
        #[serde(default)]
        offset: usize,
    },
}
impl Request {
    pub fn changes_scope(&self) -> bool {
        !matches!(
            self,
            Self::State | Self::History { .. } | Self::Image { .. } | Self::PreviewReference { .. }
        )
    }
}
pub struct Store<'a> {
    project: &'a Project,
    root: PathBuf,
    _pins: Vec<File>,
}
fn remember_interrupted(task:&mut Task)->io::Result<()>{
    let history:serde_json::Value=serde_json::from_str(task.history.as_deref().unwrap_or("{}")).map_err(io::Error::other)?;
    for turn in history["turns"].as_array().into_iter().flatten().filter(|turn|turn["role"]=="assistant" && turn["interrupted"]==true){if let Some(content)=turn["content"].as_str().filter(|content|*content!=WITHDRAWN_REPLY){task.interrupted_replies.insert(crate::kicad_project::digest(content.as_bytes()));}}
    Ok(())
}
fn interrupt_reply(task:&mut Task,chat_id:Uuid){
    for receipt in task.reply_receipts.iter_mut().filter(|receipt|receipt.id==chat_id){receipt.revoked=true;task.interrupted_replies.insert(receipt.content_sha256.clone());}
}
fn interrupted_history(task:&Task,draft:&str,sources:&[ReplySource],provider:&str)->io::Result<serde_json::Value>{
    let mut value:serde_json::Value=serde_json::from_str(task.history.as_deref().unwrap_or("{\"version\":1,\"turns\":[]}")).map_err(io::Error::other)?;
    let turns=value["turns"].as_array_mut().ok_or_else(||error("Historial no válido; se conservó la tarea."))?;
    if !draft.is_empty(){let allowed=task.provider==provider && sources.iter().all(|s|task.reply_source_allowed(s,provider)) && !task.reply_is_withdrawn(draft);turns.push(serde_json::json!({"role":"assistant","content":if allowed{draft}else{WITHDRAWN_REPLY},"interrupted":true,"reference_ids":sources.iter().map(|s|s.id).collect::<Vec<_>>()}));}
    Ok(value)
}
impl<'a> Store<'a> {
    pub fn new(project: &'a Project) -> io::Result<Self> {
        let root = project.store("tasks");
        if !root.exists() {
            fs::create_dir(&root)?;
        }
        let pins = pin_lineage(&root)?;
        Ok(Self {
            project,
            root,
            _pins: pins,
        })
    }
    fn path(&self, id: Uuid) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
    fn index(&self) -> io::Result<Option<Index>> {
        let p = self.root.join("index.json");
        if !p.try_exists()? {
            return Ok(None);
        }
        let value: Index = read_json(&p)?;
        Self::validate_index(&value)?;
        Ok(Some(value))
    }
    fn validate_index(value:&Index)->io::Result<()> {
        let ids: std::collections::BTreeSet<_> = value.ids.iter().collect();
        if value.version != 1
            || value.ids.is_empty()
            || value.ids.len() > 128
            || ids.len() != value.ids.len()
            || !value.ids.contains(&value.active)
            || value.ids.iter().any(Uuid::is_nil)
        {
            return Err(error("Índice de tareas no válido; sus datos se conservan."));
        }
        Ok(())
    }
    fn read(&self, id: Uuid) -> io::Result<Task> {
        let mut task: Task = read_json(&self.path(id))?;
        task.validate()?;
        if task.id != id {
            return Err(error("La identidad de tarea cambió."));
        }
        // Legacy previews have no immutable receipt. Keep their pixels local until fresh consent.
        for r in &mut task.references {
            if r.image && r.preview_sha256.is_none() {
                r.shared_with.clear();
            }
        }
        Ok(task)
    }
    pub fn active(&self) -> io::Result<Option<Task>> {
        self.recover_creation()?;
        self.index()?.map(|i| self.read(i.active)).transpose()
    }
    pub(crate) fn record_reply(&self,receipt:ReplyReceipt)->io::Result<bool>{
        let mut task=self.active()?.ok_or_else(||error("No hay conversación activa."))?;
        if receipt.task.id!=task.id || receipt.task.revision>task.revision || receipt.task.revision<task.last_user_revision || !task.receipt_authorized(&receipt){return Ok(false);}
        if let Some(previous)=task.reply_receipts.iter().find(|r|r.id==receipt.id){
            return if serde_json::to_value(previous).map_err(io::Error::other)?==serde_json::to_value(&receipt).map_err(io::Error::other)?{Ok(!previous.revoked)}else{Err(error("La identidad de respuesta ya tiene otra proveniencia; no se reemplazó."))};
        }
        let (parent,position)=history_parent(task.history.as_deref())?;
        if receipt.parent_sha256!=parent || receipt.position!=position{return Ok(false);}
        task.reply_receipts.push(receipt);self.save(&task)?;Ok(true)
    }
    pub(crate) fn correct_chat(&self,input:&ChatCorrectionInput,draft:&str,sources:&[ReplySource],provider:&str)->io::Result<()> {
        if input.correction_id.is_nil() || input.chat_id.is_nil() || input.text.trim().is_empty() || input.text.len()>4096{return Err(error("Escribe una corrección de hasta 4 KiB."));}
        let mut task=self.active()?.filter(|t|t.id==input.id).ok_or_else(||error("La tarea cambió; se conservó la corrección."))?;
        if let Some(saved)=task.chat_correction.as_ref().filter(|c|c.id==input.correction_id){return if saved.chat_id==input.chat_id && saved.text==input.text{Ok(())}else{Err(error("La corrección ya tiene otro contenido; no se reemplazó."))};}
        if input.revision>task.revision || (task.revision!=input.revision && task.last_user_revision>input.revision){return Err(error("La tarea cambió; vuelve a leerla antes de corregir."));}
        interrupt_reply(&mut task,input.chat_id);
        let mut value=interrupted_history(&task,draft,sources,provider)?;
        value["turns"].as_array_mut().unwrap().push(serde_json::json!({"role":"user","content":input.text}));
        task.revision=task.revision.checked_add(1).ok_or_else(||error("La revisión llegó a su límite; se conservó la tarea."))?;task.last_user_revision=task.revision;
        task.chat_correction=Some(ChatCorrection{id:input.correction_id,chat_id:input.chat_id,revision:task.revision,text:input.text.clone()});
        if task.goal.is_empty(){task.goal=input.text.clone();task.title=task.goal.chars().take(60).collect();}
        task.history=Some(serde_json::to_string(&value).map_err(io::Error::other)?);remember_interrupted(&mut task)?;self.save(&task)
    }
    pub(crate) fn stop_chat(&self,id:Uuid,chat_id:Uuid,draft:&str,sources:&[ReplySource],provider:&str)->io::Result<()>{
        if chat_id.is_nil(){return Err(error("No se pudo identificar la conversación detenida."));}
        let mut task=self.active()?.filter(|task|task.id==id).ok_or_else(||error("La tarea cambió; no se detuvo otra conversación."))?;
        if task.chat_stop.as_ref().is_some_and(|stop|stop.chat_id==chat_id){return Ok(());}
        interrupt_reply(&mut task,chat_id);
        if task.chat_correction.as_ref().is_none_or(|correction|correction.chat_id!=chat_id){let value=interrupted_history(&task,draft,sources,provider)?;task.history=Some(serde_json::to_string(&value).map_err(io::Error::other)?);remember_interrupted(&mut task)?;}
        task.revision=task.revision.checked_add(1).ok_or_else(||error("La revisión llegó a su límite; se conservó la tarea."))?;task.last_user_revision=task.revision;
        task.chat_stop=Some(ChatStop{chat_id,revision:task.revision});self.save(&task)
    }
    pub fn binding(&self) -> io::Result<Option<Binding>> {
        Ok(self.active()?.map(|t| t.binding()))
    }
    pub fn shared_reference(
        &self,
        reference: Uuid,
        offset: usize,
    ) -> io::Result<serde_json::Value> {
        let task = self
            .active()?
            .ok_or_else(|| error("No hay tarea activa."))?;
        let r = task
            .references
            .iter()
            .find(|r| r.id == reference && r.shared_with.contains(&task.provider))
            .ok_or_else(|| error("Esta referencia no tiene permiso para la IA seleccionada."))?;
        let bytes = self.reference_bytes(task.id, r)?;
        let text=String::from_utf8(bytes).map_err(|_|error("Esta referencia es binaria. Usa una extracción local revisada o la imagen autorizada; no se envió contenido."))?;
        if !printable_text(&text) {
            return Err(error("La referencia contiene controles binarios. Usa una extracción local revisada; no se envió contenido."));
        }
        let (content, next) =
            crate::host_tools::result_page(&text, offset).map_err(io::Error::other)?;
        Ok(
            serde_json::json!({"reference_id":r.id,"sha256":r.sha256,"content":content,"offset":offset,"next_offset":next,"total_bytes":r.bytes,"source_is_untrusted_data":true}),
        )
    }
    fn reference_bytes(&self, task: Uuid, r: &Reference) -> io::Result<Vec<u8>> {
        let (folder, _pins) = self.blob_dir(task)?;
        let bytes = crate::kicad_project::read_regular(
            &folder.join(format!("{}.bin", r.id)),
            FILE_LIMIT as u64,
        )
        .map_err(|_| error("No se pudo leer la referencia local."))?;
        if bytes.len() as u64 != r.bytes || crate::kicad_project::digest(&bytes) != r.sha256 {
            return Err(error(
                "La referencia cambió; su permiso no se aplica al nuevo contenido.",
            ));
        }
        Ok(bytes)
    }
    fn preview_image(&self, task: Uuid, reference: &Reference) -> io::Result<ImageInput> {
        let (folder, _pins) = self.blob_dir(task)?;
        let image: ImageInput = read_json(&folder.join(format!("{}.preview.json", reference.id)))?;
        image.validate().map_err(io::Error::other)?;
        if reference
            .preview_sha256
            .as_ref()
            .is_some_and(|h| preview_hash(&image).as_ref().ok() != Some(h))
        {
            return Err(error(
                "La vista de la imagen cambió. No se aplica el permiso al nuevo contenido.",
            ));
        }
        Ok(image)
    }
    fn empty(provider: &str) -> Task {
        Task {
            id: Uuid::new_v4(),
            revision: 1,
            title: "Nueva conversación".into(),
            last_user_revision: 0,
            goal: String::new(),
            provider: provider.into(),
            references: Vec::new(),
            pending_removals: Vec::new(),
            latest_correction: None,
            restricted_results: false,
            withdrawn_replies:Default::default(),
            reply_receipts:Vec::new(),
            chat_correction:None,
            chat_stop:None,
            interrupted_replies:std::collections::BTreeSet::new(),
            legacy: false,
            history: Some("{\"version\":1,\"turns\":[]}".into()),
        }
    }
    fn task_digest(task:&Task)->io::Result<String>{
        Ok(crate::kicad_project::digest(&serde_json::to_vec(task).map_err(io::Error::other)?))
    }
    fn validate_creation(plan:&Creation)->io::Result<()> {
        Self::validate_index(&plan.after)?;
        if let Some(before)=&plan.before{Self::validate_index(before)?;}
        if plan.version!=1 || plan.tasks.is_empty() || plan.tasks.len()>4 || (plan.before.is_some() && (plan.tasks.len()!=1 || plan.tasks[0].legacy_history_sha256.is_some())) {return Err(error("Creación de conversación no compatible; sus datos se conservan."));}
        let mut expected=plan.before.clone().unwrap_or(Index{version:1,active:plan.tasks[0].id,ids:Vec::new(),titles:Default::default()});
        let mut providers=std::collections::BTreeSet::new();
        for task in &plan.tasks{
            if task.id.is_nil() || expected.ids.contains(&task.id) || !provider(&task.provider) || !providers.insert(&task.provider) || !crate::kicad_project::valid_hash(&task.task_sha256) || task.legacy_history_sha256.as_ref().is_some_and(|h|!crate::kicad_project::valid_hash(h)) || (plan.tasks.len()>1 && task.legacy_history_sha256.is_none()) {return Err(error("Identidad de creación no válida; no se reemplazó ninguna conversación."));}
            expected.ids.push(task.id);expected.titles.insert(task.id,if task.legacy_history_sha256.is_some(){format!("Conversación anterior · {}",task.provider)}else{"Nueva conversación".into()});
        }
        expected.active=plan.tasks[0].id;
        if plan.after!=expected {return Err(error("El índice previsto no coincide con la creación; sus datos se conservan."));}
        Ok(())
    }
    fn create(&self,before:Option<Index>,tasks:Vec<Task>)->io::Result<()> {
        let first=tasks.first().ok_or_else(||error("No hay una conversación preparada para crear."))?.id;
        let mut after=before.clone().unwrap_or(Index{version:1,active:first,ids:Vec::new(),titles:Default::default()});
        let mut created=Vec::new();
        for task in tasks{
            task.validate()?;after.ids.push(task.id);after.titles.insert(task.id,task.title.clone());
            created.push(CreatedTask{id:task.id,provider:task.provider.clone(),legacy_history_sha256:task.legacy.then(||crate::kicad_project::digest(task.history.as_deref().unwrap_or("").as_bytes())),task_sha256:Self::task_digest(&task)?});
        }
        after.active=created[0].id;let plan=Creation{version:1,before,after,tasks:created};Self::validate_creation(&plan)?;
        if serde_json::to_vec_pretty(&plan).map_err(io::Error::other)?.len()>1024*1024{return Err(error("La creación supera el límite de metadatos; las conversaciones se conservan."));}
        // Reserve exact identities and hashes before publishing any conversation file.
        atomic_json(&self.root.join("creation.json"),&plan,false)?;
        self.recover_creation()
    }
    fn recover_creation(&self)->io::Result<()> {
        let path=self.root.join("creation.json");if !path.try_exists()?{return Ok(());}
        let plan:Creation=read_json(&path)?;Self::validate_creation(&plan)?;
        let current=self.index()?;
        if current.as_ref()!=plan.before.as_ref() && current.as_ref()!=Some(&plan.after){return Err(error("El índice cambió durante la creación. Se conservaron los archivos y no se repitió la solicitud."));}
        for entry in &plan.tasks {
            let task=if self.path(entry.id).try_exists()?{self.read(entry.id)?}else{
                let mut task=Self::empty(&entry.provider);task.id=entry.id;
                if let Some(hash)=&entry.legacy_history_sha256 {
                    let raw=self.project.conversation_history(&entry.provider,None)?.ok_or_else(||error("Falta el historial original de la migración; no se creó otra copia."))?;
                    if crate::kicad_project::digest(raw.as_bytes())!=*hash{return Err(error("El historial original cambió durante la migración; se conserva sin reemplazarlo."));}
                    task.legacy=true;task.title=format!("Conversación anterior · {}",entry.provider);task.history=Some(raw);
                }
                task.validate()?;
                if Self::task_digest(&task)?!=entry.task_sha256{return Err(error("La conversación prevista cambió; no se regeneró con otros datos."));}
                atomic_json(&self.path(task.id),&task,false)?;task
            };
            if Self::task_digest(&task)?!=entry.task_sha256{return Err(error("La conversación guardada cambió; se preservó sin sobrescribirla."));}
        }
        if current.as_ref()!=Some(&plan.after){atomic_json(&self.root.join("index.json"),&plan.after,plan.before.is_some())?;}
        fs::remove_file(path)
    }
    pub fn ensure(&self) -> io::Result<()> {
        self.recover_creation()?;
        if self.index()?.is_some() {
            return Ok(());
        }
        for entry in fs::read_dir(&self.root)?{
            let entry=entry?;
            if entry.file_name().to_str().and_then(|name|name.strip_suffix(".json")).is_some_and(|name|Uuid::parse_str(name).is_ok()) {return Err(error("Falta el índice de conversaciones y hay datos anteriores sin un registro de creación. Se conservaron: recupera el índice original antes de iniciar otra migración."));}
        }
        let mut tasks = Vec::new();
        for provider in ["codex", "claude", "ollama_local", "ollama_cloud"] {
            if let Some(raw) = self.project.conversation_history(provider, None)? {
                let mut task = Self::empty(provider);
                task.history = Some(raw);
                task.legacy = true;
                task.title = format!("Conversación anterior · {provider}");
                task.validate()?;
                tasks.push(task);
            }
        }
        if tasks.is_empty() {
            let task = Self::empty("codex");
            tasks.push(task);
        }
        self.create(None,tasks)
    }
    pub fn state(&self) -> io::Result<State> {
        self.ensure()?;
        let index = self.index()?.unwrap();
        let mut active = self.read(index.active)?;
        self.cleanup_references(&mut active)?;
        active.history = None;
        let tasks = index
            .ids
            .iter()
            .map(|id| Summary {
                id: *id,
                title: if *id==active.id {active.title.clone()} else {self.read(*id).map(|t|t.title).unwrap_or_else(|_|"Conversación no disponible; datos conservados".into())},
            })
            .collect();
        Ok(State { active, tasks })
    }
    fn blob_dir(&self, id: Uuid) -> io::Result<(PathBuf, Vec<File>)> {
        let path = self.root.join(id.to_string());
        if !path.exists() {
            fs::create_dir(&path)?;
        }
        let pins = pin_lineage(&path)?;
        Ok((path, pins))
    }
    fn scoped(&self, id: Uuid, revision: u64) -> io::Result<Task> {
        let mut task = self
            .active()?
            .ok_or_else(|| error("No hay una tarea activa."))?;
        if task.binding() != (Binding { id, revision }) {
            return Err(error(
                "La tarea cambió. Vuelve a leerla antes de continuar.",
            ));
        }
        self.cleanup_references(&mut task)?;
        Ok(task)
    }
    fn cleanup_references(&self, task: &mut Task) -> io::Result<()> {
        if task.pending_removals.is_empty() {
            return Ok(());
        }
        let (folder, _pins) = self.blob_dir(task.id)?;
        let before = task.pending_removals.len();
        task.pending_removals.retain(|r| {
            let mut pending = false;
            for suffix in ["bin", "preview.json"] {
                let path = folder.join(format!("{}.{suffix}", r.id));
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => pending = true,
                }
            }
            pending
        });
        if before != task.pending_removals.len() {
            // A restart can safely repeat deletion of these exact UUID-owned files.
            self.save(task)?;
        }
        Ok(())
    }
    fn save(&self, task: &Task) -> io::Result<()> {
        task.validate()?;
        atomic_json(&self.path(task.id), task, true)
    }
    fn add_reference(
        &self,
        mut task: Task,
        name: String,
        bytes: Vec<u8>,
        preview: Option<ImageInput>,
        parents: Vec<Uuid>,
    ) -> io::Result<Uuid> {
        if !valid_name(&name) {
            return Err(error("Referencia no válida."));
        }
        if bytes.is_empty()
            || bytes.len() > FILE_LIMIT
            || task.references.len() + task.pending_removals.len() >= 64
            || task.references.iter().map(|r| r.bytes).sum::<u64>()
                + task.pending_removals.iter().map(|r| r.bytes).sum::<u64>()
                + bytes.len() as u64
                > TASK_LIMIT
        {
            return Err(error(
                "Límite: 32 MiB por archivo, 64 referencias y 256 MiB por tarea.",
            ));
        }
        if let Some(image) = &preview {
            image.validate().map_err(io::Error::other)?;
        }
        let reference = Reference {
            id: Uuid::new_v4(),
            name,
            bytes: bytes.len() as u64,
            sha256: crate::kicad_project::digest(&bytes),
            image: preview.is_some(),
            preview_sha256: preview.as_ref().map(preview_hash).transpose()?,
            shared_with: Vec::new(),
            parents,
        };
        let (folder, _pins) = self.blob_dir(task.id)?;
        let reference_id = reference.id;
        let blob = folder.join(format!("{reference_id}.bin"));
        let preview_path = folder.join(format!("{reference_id}.preview.json"));
        // Reserve cleanup before creating bytes. An interrupted intake has no sharing permission.
        task.pending_removals.push(PendingRemoval {
            id: reference_id,
            bytes: reference.bytes,
        });
        task.revision += 1;
        self.save(&task)?;
        task.pending_removals.retain(|r| r.id != reference_id);
        task.references.push(reference);
        task.validate()?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(FILE_SHARE_READ)
                .open(&blob)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            if let Some(image) = preview {
                atomic_json(&preview_path, &image, false)?;
            }
            self.save(&task)
        })();
        // Failure leaves either a committed reference or its reserved cleanup; never guess which.
        result?;
        Ok(reference_id)
    }
    pub(crate) fn retain_evidence(
        &self,
        binding: &Binding,
        name: String,
        bytes: Vec<u8>,
    ) -> io::Result<Uuid> {
        self.ensure()?;
        let task = self.scoped(binding.id, binding.revision)?;
        // Native tools may observe any current reference. Unknown dependency is never declassified.
        let parents = task.references.iter().map(|r| r.id).collect();
        self.add_reference(task, name, bytes, None, parents)
    }
    pub fn request(&self, request: Request) -> io::Result<serde_json::Value> {
        use serde_json::json;
        self.ensure()?;
        match request {
            Request::State => {}
            Request::New => {
                let index = self.index()?.unwrap();
                if index.ids.len() >= 128 {
                    return Err(error(
                        "Se alcanzó el límite de 128 tareas en este proyecto.",
                    ));
                }
                let task = Self::empty(&self.read(index.active)?.provider);
                self.create(Some(index),vec![task])?;
            }
            Request::Open { id } => {
                let mut index = self.index()?.unwrap();
                if !index.ids.contains(&id) {
                    return Err(error("La tarea no pertenece al proyecto."));
                }
                self.read(id)?;
                index.active = id;
                atomic_json(&self.root.join("index.json"), &index, true)?;
            }
            Request::History {
                id,
                revision,
                value,
            } => {
                let mut task = self.scoped(id, revision)?;
                if let Some(raw) = value {
                    task.history = Some(retain_lineage(&task, &raw)?);
                    remember_interrupted(&mut task)?;
                    self.save(&task)?;
                } else {
                    return Ok(json!(task.history));
                }
            }
            Request::Message {
                id,
                revision,
                text,
                history: raw,
                correction_of,
            } => {
                if text.trim().is_empty() || text.len() > 4096 {
                    return Err(error("Escribe un mensaje de hasta 4 KiB."));
                }
                history(&raw)?;
                let mut task = self.scoped(id, revision)?;
                task.revision += 1;
                if let Some(job_id) = correction_of {
                    task.latest_correction = Some(Correction {
                        job_id,
                        revision: task.revision,
                        text: text.clone(),
                    });
                }
                task.last_user_revision = task.revision;
                if task.goal.is_empty() {
                    task.goal = text;
                    task.title = task.goal.chars().take(60).collect();
                }
                task.history = Some(retain_lineage(&task, &raw)?);
                remember_interrupted(&mut task)?;
                self.save(&task)?;
            }
            Request::Provider {
                id,
                revision,
                provider: p,
            } => {
                if !provider(&p) {
                    return Err(error("IA no válida."));
                }
                let mut task = self.scoped(id, revision)?;
                task.provider = p;
                task.revision += 1;
                self.save(&task)?;
            }
            Request::AddReference {
                id,
                revision,
                name,
                data,
                preview,
            } => {
                if !valid_name(&name) || data.len() > FILE_LIMIT.div_ceil(3) * 4 {
                    return Err(error("Referencia no válida o mayor de 32 MiB."));
                }
                let bytes = STANDARD
                    .decode(data)
                    .map_err(|_| error("Referencia no válida."))?;
                let task = self.scoped(id, revision)?;
                self.add_reference(task, name, bytes, preview, Vec::new())?;
            }
            Request::Share {
                id,
                revision,
                reference,
                allow,
            } => {
                let mut task = self.scoped(id, revision)?;
                let r = task
                    .references
                    .iter_mut()
                    .find(|r| r.id == reference)
                    .ok_or_else(|| error("Referencia no disponible."))?;
                r.shared_with.retain(|p| p != &task.provider);
                if allow {
                    if r.image {
                        r.preview_sha256 = Some(preview_hash(&self.preview_image(task.id, r)?)?);
                    }
                    r.shared_with.push(task.provider.clone());
                } else {
                    let affected = descendants(&task, reference);
                    for r in &mut task.references {
                        if affected.contains(&r.id) {
                            r.shared_with.retain(|p| p != &task.provider);
                        }
                    }
                    scrub_history(&mut task, &affected)?;
                }
                task.revision += 1;
                self.save(&task)?;
            }
            Request::Remove {
                id,
                revision,
                reference,
            } => {
                let mut task = self.scoped(id, revision)?;
                task.references
                    .iter()
                    .find(|r| r.id == reference)
                    .ok_or_else(|| error("Referencia no disponible."))?;
                let affected = descendants(&task, reference);
                // Revoke access before deleting bytes; a failed deletion must never restore sharing.
                // shortcut: old native reports have no per-reference lineage; keep them local after removal.
                task.restricted_results |= task
                    .references
                    .iter()
                    .any(|r| affected.contains(&r.id) && !r.shared_with.is_empty());
                task.pending_removals.extend(
                    task.references
                        .iter()
                        .filter(|r| affected.contains(&r.id))
                        .map(|r| PendingRemoval {
                            id: r.id,
                            bytes: r.bytes,
                        }),
                );
                task.references.retain(|r| !affected.contains(&r.id));
                scrub_history(&mut task, &affected)?;
                task.revision += 1;
                self.save(&task)?;
            }
            Request::PreviewReference {
                id,
                revision,
                reference,
                offset,
            } => {
                let task = self.scoped(id, revision)?;
                let r = task
                    .references
                    .iter()
                    .find(|r| r.id == reference)
                    .ok_or_else(|| error("Referencia local no disponible."))?;
                let bytes = self.reference_bytes(id, r)?;
                if r.image {
                    if offset != 0 {
                        return Err(error("Página de imagen no válida."));
                    }
                    return Ok(
                        json!({"kind":"image","image":self.preview_image(id,r)?,"next_offset":null}),
                    );
                }
                return match String::from_utf8(bytes) {
                    Ok(text) if printable_text(&text) => {
                        let mut page = reference_page(r, &text, offset)?;
                        page["kind"] = json!("text");
                        Ok(page)
                    }
                    _ => Ok(
                        json!({"kind":"binary","content":"Archivo binario local. La vista no ejecuta decodificadores ni lo envía a una IA. Usa una extracción local revisada para inspeccionar su contenido.","next_offset":null}),
                    ),
                };
            }
            Request::Image {
                id,
                revision,
                reference,
            } => {
                let task = self.scoped(id, revision)?;
                let r = task
                    .references
                    .iter()
                    .find(|r| {
                        r.id == reference
                            && r.image
                            && r.preview_sha256.is_some()
                            && r.shared_with.contains(&task.provider)
                    })
                    .ok_or_else(|| {
                        error(
                            "Aprueba compartir esta imagen con la IA seleccionada antes de usarla.",
                        )
                    })?;
                self.reference_bytes(id, r)?;
                let image = self.preview_image(id, r)?;
                return serde_json::to_value(image).map_err(io::Error::other);
            }
        }
        serde_json::to_value(self.state()?).map_err(io::Error::other)
    }
}

fn preview_hash(image: &ImageInput) -> io::Result<String> {
    Ok(crate::kicad_project::digest(
        &serde_json::to_vec(image).map_err(io::Error::other)?,
    ))
}
fn reference_page(r: &Reference, text: &str, offset: usize) -> io::Result<serde_json::Value> {
    let total = text.chars().count();
    if offset > total {
        return Err(error("Página de referencia no válida."));
    }
    let content: String = text.chars().skip(offset).take(4096).collect();
    let next = offset + content.chars().count();
    Ok(
        serde_json::json!({"reference_id":r.id,"sha256":r.sha256,"content":content,"offset":offset,"next_offset":if next<total{Some(next)}else{None},"source_is_untrusted_data":true}),
    )
}
fn printable_text(text: &str) -> bool {
    !text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_turns_fence_existing_and_late_completion_proofs(){
        for mode in ["stop","correction"]{for timing in ["before","after"]{
            let (root,p)=setup();let store=Store::new(&p).unwrap();let task=store.state().unwrap().active;let old=receipt(&task,"OLD_COMPLETE_REPLY_SENTINEL");
            if timing=="before"{assert!(store.record_reply(old.clone()).unwrap());}
            if mode=="stop"{store.stop_chat(task.id,old.id,if timing=="before"{"short streamed prefix"}else{""},&[],"codex").unwrap();}else{store.correct_chat(&ChatCorrectionInput{id:task.id,revision:task.revision,chat_id:old.id,correction_id:Uuid::new_v4(),text:"Use the corrected idea".into()},"short streamed prefix",&[],"codex").unwrap();}
            if timing=="after"{assert!(!store.record_reply(old.clone()).unwrap());}let stopped=store.active().unwrap().unwrap();assert!(!stopped.reply_receipt_allows(old.id,"OLD_COMPLETE_REPLY_SENTINEL"));
            if timing=="before"{assert!(stopped.reply_is_interrupted("OLD_COMPLETE_REPLY_SENTINEL"));}
            let t=store.active().unwrap().unwrap();store.request(Request::Message{id:t.id,revision:t.revision,text:"Continue with fresh context".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"Continue with fresh context"}]}).to_string(),correction_of:None}).unwrap();let fresh=receipt(&store.active().unwrap().unwrap(),"OLD_COMPLETE_REPLY_SENTINEL");assert!(store.record_reply(fresh.clone()).unwrap());assert!(store.active().unwrap().unwrap().reply_receipt_allows(fresh.id,"OLD_COMPLETE_REPLY_SENTINEL"));
            drop(store);drop(p);fs::remove_dir_all(root).unwrap();
        }}
    }
    #[test]
    fn chat_stop_keeps_owned_history_and_draft_without_inventing_a_user_message(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;store.request(Request::Message{id:t.id,revision:t.revision,text:"Preserve the original enclosure".into(),history:serde_json::json!({"version":1,"turns":[{"role":"user","content":"Preserve the original enclosure"}]}).to_string(),correction_of:None}).unwrap();let t=store.active().unwrap().unwrap();let chat=Uuid::new_v4();
        store.stop_chat(t.id,chat,"STOPPED_DRAFT_SENTINEL",&[],"codex").unwrap();let stopped=store.active().unwrap().unwrap();assert_eq!(stopped.goal,t.goal);assert_eq!(stopped.chat_stop.as_ref().unwrap().chat_id,chat);assert_eq!(stopped.last_user_revision,stopped.revision);assert_eq!(stopped.revision,t.revision+1);let history:serde_json::Value=serde_json::from_str(stopped.history.as_deref().unwrap()).unwrap();assert_eq!(history["turns"].as_array().unwrap().len(),2);assert_eq!(history["turns"][1]["interrupted"],true);assert!(stopped.reply_is_interrupted("STOPPED_DRAFT_SENTINEL"));let before=fs::read(store.path(t.id)).unwrap();store.stop_chat(t.id,chat,"later stream body",&[],"codex").unwrap();assert_eq!(fs::read(store.path(t.id)).unwrap(),before);
        drop(store);drop(p);let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();assert_eq!(store.active().unwrap().unwrap().chat_stop.unwrap().chat_id,chat);drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn chat_stop_after_correction_preserves_one_draft_and_recovers_a_locked_save(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;let input=ChatCorrectionInput{id:t.id,revision:t.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Preserve my change".into()};store.correct_chat(&input,"ONE_LOCAL_DRAFT",&[],"codex").unwrap();let before=fs::read(store.path(t.id)).unwrap();let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(store.path(t.id)).unwrap();assert!(store.stop_chat(t.id,input.chat_id,"ONE_LOCAL_DRAFT",&[],"codex").is_err());assert_eq!(fs::read(store.path(t.id)).unwrap(),before);drop(lock);store.stop_chat(t.id,input.chat_id,"ONE_LOCAL_DRAFT",&[],"codex").unwrap();let t=store.active().unwrap().unwrap();let history:serde_json::Value=serde_json::from_str(t.history.as_deref().unwrap()).unwrap();assert_eq!(history["turns"].as_array().unwrap().len(),2);assert!(t.chat_stop.unwrap().revision>t.chat_correction.unwrap().revision);
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn chat_correction_preserves_owned_history_and_draft_without_duplicate_retries(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        save_history(&store,serde_json::json!([{"role":"user","content":"Keep the original idea"},{"role":"assistant","content":"Checked earlier work"}]));
        let input=ChatCorrectionInput{id:t.id,revision:t.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Use inches and preserve my manual changes".into()};
        store.correct_chat(&input,"UNFINISHED_REPLY_SENTINEL",&[],"codex").unwrap();let t=store.active().unwrap().unwrap();let bytes=fs::read(store.path(t.id)).unwrap();assert_eq!(t.revision,input.revision+1);assert_eq!(t.chat_correction.as_ref().unwrap().text,input.text);
        let history:serde_json::Value=serde_json::from_str(t.history.as_deref().unwrap()).unwrap();assert_eq!(history["turns"][2]["interrupted"],true);assert_eq!(history["turns"][3]["content"],input.text);assert!(t.history.as_ref().unwrap().contains("Checked earlier work"));
        store.correct_chat(&input,"different late stream body",&[],"codex").unwrap();assert_eq!(fs::read(store.path(t.id)).unwrap(),bytes);
        let mut wrong=input.clone();wrong.text="different correction".into();assert!(store.correct_chat(&wrong,"",&[],"codex").is_err());assert_eq!(fs::read(store.path(t.id)).unwrap(),bytes);
        // A stale history writer cannot turn this exact unfinished body into authorized context.
        save_history(&store,serde_json::json!([]));save_history(&store,serde_json::json!([{"role":"assistant","content":"UNFINISHED_REPLY_SENTINEL"}]));let saved=store.active().unwrap().unwrap();let history:serde_json::Value=serde_json::from_str(saved.history.as_deref().unwrap()).unwrap();assert_eq!(history["turns"][0]["interrupted"],true);
        drop(store);drop(p);let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();assert_eq!(store.active().unwrap().unwrap().chat_correction.unwrap().id,input.correction_id);drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_chat_correction_does_not_replace_valid_history_or_restore_withdrawn_drafts(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;let input=ChatCorrectionInput{id:t.id,revision:t.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Preserve valid native work".into()};
        let before=fs::read(store.path(t.id)).unwrap();let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(store.path(t.id)).unwrap();assert!(store.correct_chat(&input,"draft",&[],"codex").is_err());assert_eq!(fs::read(store.path(t.id)).unwrap(),before);drop(lock);
        let mut t=store.active().unwrap().unwrap();t.withdrawn_replies.insert(crate::kicad_project::digest(b"WITHDRAWN_STREAM_SENTINEL"),t.revision);store.save(&t).unwrap();store.correct_chat(&input,"WITHDRAWN_STREAM_SENTINEL",&[],"codex").unwrap();let t=store.active().unwrap().unwrap();assert!(!t.history.unwrap().contains("WITHDRAWN_STREAM_SENTINEL"));
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn chat_correction_adapts_to_new_evidence_but_preserves_a_newer_human_instruction(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        let input=ChatCorrectionInput{id:t.id,revision:t.revision,chat_id:Uuid::new_v4(),correction_id:Uuid::new_v4(),text:"Preserve the idea and my changes".into()};
        store.request(Request::AddReference{id:t.id,revision:t.revision,name:"observation.bin".into(),data:STANDARD.encode(b"NEW_LOCAL_EVIDENCE_SENTINEL"),preview:None}).unwrap();
        store.correct_chat(&input,"old authorized draft",&[],"codex").unwrap();let t=store.active().unwrap().unwrap();assert_eq!(t.revision,input.revision+2);assert_eq!(t.references.len(),1);assert!(!t.history.as_ref().unwrap().contains("NEW_LOCAL_EVIDENCE_SENTINEL"));let before=fs::read(store.path(t.id)).unwrap();
        let mut conflicting=input;conflicting.correction_id=Uuid::new_v4();conflicting.text="stale competing instruction".into();assert!(store.correct_chat(&conflicting,"",&[],"codex").is_err());assert_eq!(fs::read(store.path(t.id)).unwrap(),before);
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    fn receipt(task:&Task,text:&str)->ReplyReceipt{
        let (parent_sha256,position)=history_parent(task.history.as_deref()).unwrap();
        ReplyReceipt{id:Uuid::new_v4(),task:task.binding(),provider:task.provider.clone(),content_sha256:crate::kicad_project::digest(text.as_bytes()),parent_sha256,position,references:task.references.iter().filter(|r|r.shared_with.contains(&task.provider)).map(Reference::reply_source).collect(),revoked:false}
    }
    fn save_history(store:&Store,turns:serde_json::Value){
        let t=store.active().unwrap().unwrap();store.request(Request::History{id:t.id,revision:t.revision,value:Some(serde_json::json!({"version":1,"turns":turns}).to_string())}).unwrap();
    }
    #[test]
    fn reply_receipts_preserve_new_authorized_answers_without_reauthorizing_old_context(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        store.request(Request::AddReference{id:t.id,revision:t.revision,name:"source.txt".into(),data:STANDARD.encode(b"private source"),preview:None}).unwrap();
        let t=store.active().unwrap().unwrap();let reference=t.references[0].id;store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:true}).unwrap();
        save_history(&store,serde_json::json!([{"role":"user","content":"Original question"}]));
        let text="SAME_REPLY_SENTINEL";let old=receipt(&store.active().unwrap().unwrap(),text);assert!(store.record_reply(old.clone()).unwrap());
        // Revocation applies even before the renderer saves the reply body.
        let t=store.active().unwrap().unwrap();store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:false}).unwrap();
        let t=store.active().unwrap().unwrap();assert!(t.reply_is_withdrawn(text));assert!(!t.reply_receipt_allows(old.id,text));assert!(!store.record_reply(old.clone()).unwrap());
        save_history(&store,serde_json::json!([{"role":"user","content":"Original question"},{"role":"assistant","content":text,"reply_id":old.id}]));
        assert!(!store.active().unwrap().unwrap().history.unwrap().contains(text));
        let t=store.active().unwrap().unwrap();store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:true}).unwrap();
        save_history(&store,serde_json::json!([{"role":"user","content":"New question"}]));
        let fresh=receipt(&store.active().unwrap().unwrap(),text);assert!(store.record_reply(fresh.clone()).unwrap());assert!(store.record_reply(fresh.clone()).unwrap());
        let before=fs::read(store.path(fresh.task.id)).unwrap();let mut changed=fresh.clone();changed.content_sha256=crate::kicad_project::digest(b"changed");assert!(store.record_reply(changed).is_err());assert_eq!(before,fs::read(store.path(fresh.task.id)).unwrap());
        // A real id cannot transfer permission to a different question, slot or body.
        save_history(&store,serde_json::json!([{"role":"user","content":"Original question"},{"role":"assistant","content":text,"reply_id":fresh.id}]));
        assert!(!store.active().unwrap().unwrap().history.unwrap().contains(text));
        save_history(&store,serde_json::json!([{"role":"user","content":"New question"},{"role":"assistant","content":text,"reply_id":old.id}]));
        assert!(!store.active().unwrap().unwrap().history.unwrap().contains(text));
        save_history(&store,serde_json::json!([{"role":"user","content":"New question"},{"role":"assistant","content":text,"reply_id":fresh.id}]));
        let t=store.active().unwrap().unwrap();assert!(t.history.as_ref().unwrap().contains(text));assert_eq!(t.reply_receipts.len(),2);
        drop(store);drop(p);let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();let t=store.active().unwrap().unwrap();assert!(t.reply_receipt_allows(fresh.id,text));assert!(t.history.unwrap().contains(text));
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reply_receipts_bind_service_versions_and_authenticate_surviving_history_before_rebasing(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        store.request(Request::AddReference{id:t.id,revision:t.revision,name:"source.txt".into(),data:STANDARD.encode(b"private source"),preview:None}).unwrap();
        let t=store.active().unwrap().unwrap();let reference=t.references[0].id;store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:true}).unwrap();
        let t=store.active().unwrap().unwrap();let first=receipt(&t,"private reply");
        let mut wrong=first.clone();wrong.provider="claude".into();assert!(!store.record_reply(wrong).unwrap());
        let mut wrong=first.clone();wrong.references[0].sha256=crate::kicad_project::digest(b"other version");assert!(!store.record_reply(wrong).unwrap());
        let mut wrong=first.clone();wrong.references[0].preview_sha256=Some(crate::kicad_project::digest(b"other preview"));assert!(!store.record_reply(wrong).unwrap());
        let before=fs::read(store.path(t.id)).unwrap();let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(store.path(t.id)).unwrap();
        assert!(store.record_reply(first.clone()).is_err());assert_eq!(before,fs::read(store.path(t.id)).unwrap());drop(lock);
        assert!(store.record_reply(first.clone()).unwrap());assert!(store.record_reply(first.clone()).unwrap());
        save_history(&store,serde_json::json!([{"role":"assistant","content":"private reply","reply_id":first.id},{"role":"user","content":"Keep the design"}]));
        let mut second=receipt(&store.active().unwrap().unwrap(),"unrelated authorized reply");second.references.clear();assert!(store.record_reply(second.clone()).unwrap());
        save_history(&store,serde_json::json!([{"role":"assistant","content":"private reply","reply_id":first.id},{"role":"user","content":"Keep the design"},{"role":"assistant","content":"unrelated authorized reply","reply_id":second.id}]));
        let t=store.active().unwrap().unwrap();store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:false}).unwrap();
        let t=store.active().unwrap().unwrap();let history:serde_json::Value=serde_json::from_str(t.history.as_deref().unwrap()).unwrap();let turns=history["turns"].as_array().unwrap();
        assert_eq!(turns[0]["content"],WITHDRAWN_REPLY);assert!(t.history_reply_parent_allows(&turns[2],2,&parent_digest(&turns[..2]).unwrap()));assert!(!t.reply_receipt_allows(first.id,"private reply"));
        // A forged old frame cannot acquire a new parent during a trusted scrub.
        let mut forged=t.clone();forged.history=Some(serde_json::json!({"version":1,"turns":[{"role":"user","content":"different parent"},{"role":"assistant","content":"unrelated authorized reply","reply_id":second.id}]}).to_string());
        let old_parent=forged.reply_receipts[1].parent_sha256.clone();scrub_history(&mut forged,&std::collections::BTreeSet::from([reference])).unwrap();assert_eq!(forged.reply_receipts[1].parent_sha256,old_parent);
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn withdrawn_reply_cannot_return_from_a_shifted_stale_history_after_removal(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        store.request(Request::AddReference{id:t.id,revision:t.revision,name:"capture.bin".into(),data:STANDARD.encode(b"PRIVATE_CAPTURE_SENTINEL"),preview:None}).unwrap();
        let t=store.state().unwrap().active;let reference=t.references[0].id;
        store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:true}).unwrap();
        let reply="PRIVATE_REVOKED_REPLY_SENTINEL";
        let original=serde_json::json!({"version":1,"turns":[{"role":"user","content":"Preserve the design"},{"role":"assistant","content":reply}]}).to_string();
        let t=store.state().unwrap().active;store.request(Request::History{id:t.id,revision:t.revision,value:Some(original.clone())}).unwrap();
        let t=store.state().unwrap().active;store.request(Request::Provider{id:t.id,revision:t.revision,provider:"claude".into()}).unwrap();
        let shifted=serde_json::json!({"version":1,"turns":[{"role":"user","content":"Preserve another manual correction"},{"role":"user","content":"Preserve the design"},{"role":"assistant","content":reply,"reference_ids":[]}]}).to_string();
        let t=store.state().unwrap().active;store.request(Request::History{id:t.id,revision:t.revision,value:Some(shifted.clone())}).unwrap();
        let saved=store.active().unwrap().unwrap();let history:serde_json::Value=serde_json::from_str(saved.history.as_deref().unwrap()).unwrap();
        assert_eq!(history["turns"][2]["reference_ids"],serde_json::json!([reference]));
        fs::write(root.join("manual-design.txt"),b"preserved native work").unwrap();
        store.request(Request::Remove{id:saved.id,revision:saved.revision,reference}).unwrap();
        let withdrawn=store.active().unwrap().unwrap();assert_eq!(withdrawn.withdrawn_replies.keys().cloned().collect::<Vec<_>>(),vec![crate::kicad_project::digest(reply.as_bytes())]);assert!(!withdrawn.history.as_deref().unwrap().contains(reply));
        store.request(Request::History{id:withdrawn.id,revision:withdrawn.revision,value:Some(shifted.clone())}).unwrap();
        let saved=store.active().unwrap().unwrap();assert!(!saved.history.as_deref().unwrap().contains(reply));assert!(saved.history.as_deref().unwrap().contains("Preserve another manual correction"));
        drop(store);drop(p);let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();let saved=store.state().unwrap().active;
        store.request(Request::Message{id:saved.id,revision:saved.revision,text:"Continue preserving the work".into(),history:original,correction_of:None}).unwrap();
        let saved=store.active().unwrap().unwrap();assert!(!saved.history.as_deref().unwrap().contains(reply));assert_eq!(saved.withdrawn_replies.len(),1);assert!(saved.references.is_empty());assert!(saved.restricted_results);
        assert_eq!(fs::read(root.join("manual-design.txt")).unwrap(),b"preserved native work");drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reply_withdrawal_budget_is_reserved_before_intake_and_never_blocks_revocation(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let t=store.state().unwrap().active;
        store.request(Request::AddReference{id:t.id,revision:t.revision,name:"source.txt".into(),data:STANDARD.encode(b"private source"),preview:None}).unwrap();
        let t=store.state().unwrap().active;let reference=t.references[0].id;store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:true}).unwrap();
        let turns=(0..128).map(|i|serde_json::json!({"role":"assistant","content":format!("LOCAL_REPLY_{i}")})).collect::<Vec<_>>();
        let raw=serde_json::json!({"version":1,"turns":turns}).to_string();let t=store.state().unwrap().active;store.request(Request::History{id:t.id,revision:t.revision,value:Some(raw.clone())}).unwrap();
        let t=store.state().unwrap().active;store.request(Request::Share{id:t.id,revision:t.revision,reference,allow:false}).unwrap();
        let t=store.active().unwrap().unwrap();assert_eq!(t.withdrawn_replies.len(),128);assert!(t.references[0].shared_with.is_empty());assert!(!t.history.as_deref().unwrap().contains("LOCAL_REPLY_"));
        let before=fs::read(store.path(t.id)).unwrap();
        assert!(store.request(Request::History{id:t.id,revision:t.revision,value:Some(serde_json::json!({"version":1,"turns":[{"role":"assistant","content":"another unknown response"}]}).to_string())}).is_err());
        assert_eq!(fs::read(store.path(t.id)).unwrap(),before);
        store.request(Request::History{id:t.id,revision:t.revision,value:Some(raw)}).unwrap();
        let t=store.active().unwrap().unwrap();assert!(!t.history.as_deref().unwrap().contains("LOCAL_REPLY_"));assert_eq!(t.withdrawn_replies.len(),128);
        store.request(Request::Remove{id:t.id,revision:t.revision,reference}).unwrap();assert!(store.state().unwrap().active.references.is_empty());
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn derived_evidence_removal_and_revocation_follow_ancestry_without_deleting_designs() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let add = |name: &str, bytes: &[u8]| {
            let t = store.state().unwrap().active;
            store
                .request(Request::AddReference {
                    id: t.id,
                    revision: t.revision,
                    name: name.into(),
                    data: STANDARD.encode(bytes),
                    preview: None,
                })
                .unwrap();
            store.state().unwrap().active.references.last().unwrap().id
        };
        let original = add("capture.bin", b"PRIVATE_PARENT_SENTINEL");
        let unrelated = add("unrelated.txt", b"keep unrelated reference");
        let t = store.state().unwrap().active;
        let child = store
            .retain_evidence(
                &t.binding(),
                "analysis.json".into(),
                b"PRIVATE_CHILD_SENTINEL".to_vec(),
            )
            .unwrap();
        let t = store.state().unwrap().active;
        assert_eq!(
            t.references.last().unwrap().parents,
            vec![original, unrelated]
        );
        let grandchild = store
            .retain_evidence(
                &t.binding(),
                "encoded.json".into(),
                b"PRIVATE_GRANDCHILD_SENTINEL".to_vec(),
            )
            .unwrap();
        let share = |reference, allow| {
            let t = store.state().unwrap().active;
            store
                .request(Request::Share {
                    id: t.id,
                    revision: t.revision,
                    reference,
                    allow,
                })
                .unwrap();
        };
        share(grandchild, true);
        assert!(store.shared_reference(grandchild, 0).unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("GRANDCHILD"));
        let t = store.state().unwrap().active;
        store
            .request(Request::History {
                id: t.id,
                revision: t.revision,
                value: Some(
                    serde_json::json!({"version":1,"turns":[
                        {"role":"user","content":"preserve my manual work"},
                        {"role":"assistant","content":"PRIVATE_ASSISTANT_SENTINEL"}
                    ]})
                    .to_string(),
                ),
            })
            .unwrap();
        // The ancestor itself was never shared; revoking it must still revoke its shared derivative.
        share(original, false);
        assert!(store.shared_reference(grandchild, 0).is_err());
        assert!(!store
            .active()
            .unwrap()
            .unwrap()
            .history
            .unwrap()
            .contains("PRIVATE_ASSISTANT_SENTINEL"));
        share(grandchild, true);
        let t = store.state().unwrap().active;
        let blob = store
            .root
            .join(t.id.to_string())
            .join(format!("{grandchild}.bin"));
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&blob)
            .unwrap();
        let design = root.join("manual-work.native");
        fs::write(&design, b"valid native work and user edits").unwrap();
        store
            .request(Request::Remove {
                id: t.id,
                revision: t.revision,
                reference: original,
            })
            .unwrap();
        let removed = store.state().unwrap().active;
        assert_eq!(
            removed.references.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![unrelated]
        );
        assert_eq!(removed.pending_removals.len(), 1);
        assert_eq!(removed.pending_removals[0].id, grandchild);
        assert!(removed.restricted_results);
        for id in [original, child, grandchild] {
            assert!(store.shared_reference(id, 0).is_err());
            assert!(store
                .request(Request::PreviewReference {
                    id: t.id,
                    revision: removed.revision,
                    reference: id,
                    offset: 0
                })
                .is_err());
        }
        assert!(store
            .retain_evidence(
                &t.binding(),
                "stale.json".into(),
                b"stale response".to_vec()
            )
            .is_err());
        assert!(store
            .active()
            .unwrap()
            .unwrap()
            .history
            .as_ref()
            .unwrap()
            .contains("preserve my manual work"));
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        let store = Store::new(&p).unwrap();
        assert_eq!(store.state().unwrap().active.pending_removals.len(), 1);
        drop(lock);
        let recovered = store.state().unwrap().active;
        assert!(recovered.pending_removals.is_empty());
        assert!(!blob.exists());
        assert_eq!(recovered.references[0].id, unrelated);
        assert_eq!(
            fs::read(design).unwrap(),
            b"valid native work and user edits"
        );
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn evidence_ancestry_is_host_owned_acyclic_and_backward_compatible() {
        let mut task = Store::empty("codex");
        let a = Reference {
            id: Uuid::new_v4(),
            name: "a.txt".into(),
            bytes: 1,
            sha256: "a".repeat(64),
            image: false,
            preview_sha256: None,
            shared_with: Vec::new(),
            parents: Vec::new(),
        };
        task.references.push(a.clone());
        let mut legacy = serde_json::to_value(&task).unwrap();
        legacy["references"][0]
            .as_object_mut()
            .unwrap()
            .remove("parents");
        serde_json::from_value::<Task>(legacy)
            .unwrap()
            .validate()
            .unwrap();
        let mut b = a.clone();
        b.id = Uuid::new_v4();
        b.parents = vec![a.id];
        task.references.push(b.clone());
        task.validate().unwrap();
        task.references[0].parents = vec![b.id];
        assert!(task.validate().is_err());
        task.references[0].parents = vec![Uuid::new_v4()];
        assert!(task.validate().is_err());
        task.references[0].parents.clear();
        task.references[1].parents = vec![a.id, a.id];
        assert!(task.validate().is_err());
        assert!(serde_json::from_value::<Request>(serde_json::json!({"action":"add_reference","id":task.id,"revision":1,"name":"forged.txt","data":"eA==","preview":null,"parents":[a.id]})).is_err());
    }
    #[test]
    fn interrupted_intake_keeps_only_durable_cleanup_and_never_grants_access() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let mut task = store.state().unwrap().active;
        let reference = Uuid::new_v4();
        let bytes = b"UNCOMMITTED_PRIVATE_REFERENCE_SENTINEL";
        let (folder, pins) = store.blob_dir(task.id).unwrap();
        // Exact persisted reservation/write state if the process stops before reference publication.
        task.revision += 1;
        task.pending_removals.push(PendingRemoval {
            id: reference,
            bytes: bytes.len() as u64,
        });
        store.save(&task).unwrap();
        let blob = folder.join(format!("{reference}.bin"));
        fs::write(&blob, bytes).unwrap();
        assert!(store.shared_reference(reference, 0).is_err());
        drop(pins);
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        let store = Store::new(&p).unwrap();
        let recovered = store.state().unwrap().active;
        assert!(recovered.references.is_empty());
        assert!(recovered.pending_removals.is_empty());
        assert!(!blob.exists());
        // If cleanup reservation itself cannot commit, intake must not create any bytes.
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(store.path(task.id))
            .unwrap();
        assert!(store
            .request(Request::AddReference {
                id: task.id,
                revision: recovered.revision,
                name: "blocked.txt".into(),
                data: STANDARD.encode(bytes),
                preview: None
            })
            .is_err());
        assert_eq!(fs::read_dir(folder).unwrap().count(), 0);
        drop(lock);
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn image_permission_binds_preview_bytes_and_legacy_images_require_new_consent() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        let image = ImageInput {
            media_type: "image/jpeg".into(),
            data: STANDARD.encode([255, 216, 255, 192, 0, 8, 8, 0, 1, 0, 1, 0, 255, 217]),
        };
        store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "part.jpg".into(),
                data: image.data.clone(),
                preview: Some(image.clone()),
            })
            .unwrap();
        let t = store.state().unwrap().active;
        let id = t.references[0].id;
        assert_eq!(
            t.references[0].preview_sha256.as_ref(),
            Some(&preview_hash(&image).unwrap())
        );
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference: id,
                allow: true,
            })
            .unwrap();
        let mut t = store.active().unwrap().unwrap();
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference: id
            })
            .is_ok());
        // Simulate an older task with a valid original and prior grant but no preview receipt.
        t.references[0].preview_sha256 = None;
        store.save(&t).unwrap();
        let t = store.active().unwrap().unwrap();
        assert!(t.references[0].shared_with.is_empty());
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference: id
            })
            .is_err());
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision,
                reference: id,
                offset: 0
            })
            .is_ok());
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference: id,
                allow: true,
            })
            .unwrap();
        let t = store.active().unwrap().unwrap();
        assert!(t.references[0].preview_sha256.is_some());
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference: id
            })
            .is_ok());
        let changed = ImageInput {
            media_type: "image/jpeg".into(),
            data: STANDARD.encode([255, 216, 255, 192, 0, 8, 8, 0, 2, 0, 1, 0, 255, 217]),
        };
        changed.validate().unwrap();
        let (folder, pins) = store.blob_dir(t.id).unwrap();
        atomic_json(&folder.join(format!("{id}.preview.json")), &changed, true).unwrap();
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference: id
            })
            .is_err());
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision,
                reference: id,
                offset: 0
            })
            .is_err());
        assert!(store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference: id,
                allow: true
            })
            .is_err());
        assert_eq!(
            store.reference_bytes(t.id, &t.references[0]).unwrap(),
            STANDARD.decode(&image.data).unwrap()
        );
        drop(pins);
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn owned_preview_is_local_revision_bound_and_does_not_grant_sharing() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        let text = "á🛠 PRIVATE_LOCAL_SENTINEL ".repeat(4000);
        store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "capture.txt".into(),
                data: STANDARD.encode(text.as_bytes()),
                preview: None,
            })
            .unwrap();
        let t = store.state().unwrap().active;
        let reference = t.references[0].id;
        let mut offset = 0;
        let mut joined = String::new();
        loop {
            let page = store
                .request(Request::PreviewReference {
                    id: t.id,
                    revision: t.revision,
                    reference,
                    offset,
                })
                .unwrap();
            assert_eq!(page["kind"], "text");
            let content = page["content"].as_str().unwrap();
            assert!(content.chars().count() <= 4096);
            joined.push_str(content);
            if let Some(next) = page["next_offset"].as_u64() {
                assert!(next as usize > offset);
                offset = next as usize;
            } else {
                break;
            }
        }
        assert_eq!(joined, text);
        assert_eq!(store.state().unwrap().active.revision, t.revision);
        assert!(store.state().unwrap().active.references[0]
            .shared_with
            .is_empty());
        assert!(store.shared_reference(reference, 0).is_err());
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision - 1,
                reference,
                offset: 0
            })
            .is_err());
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision,
                reference: Uuid::new_v4(),
                offset: 0
            })
            .is_err());
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference,
                allow: true,
            })
            .unwrap();
        let mut offset = 0;
        let mut joined = String::new();
        loop {
            let page = store.shared_reference(reference, offset).unwrap();
            let content = page["content"].as_str().unwrap();
            assert!(content.len() <= 65536);
            joined.push_str(content);
            if let Some(next) = page["next_offset"].as_u64() {
                offset = next as usize;
            } else {
                break;
            }
        }
        assert_eq!(joined, text);
        assert!(store.shared_reference(reference, 1).is_err());
        let t = store.state().unwrap().active;
        fs::write(
            store
                .root
                .join(t.id.to_string())
                .join(format!("{reference}.bin")),
            b"tampered",
        )
        .unwrap();
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision,
                reference,
                offset: 0
            })
            .is_err());
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn binary_preview_identifies_data_without_decoding_or_granting_external_access() {
        assert!(!printable_text("private\0firmware"));
        assert!(!printable_text("private\u{1b}log"));
        assert!(printable_text("normal\ntext\twith\rwhitespace"));
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let task = store.state().unwrap().active;
        store
            .request(Request::AddReference {
                id: task.id,
                revision: task.revision,
                name: "firmware.bin".into(),
                data: STANDARD.encode([255, 254, 1]),
                preview: None,
            })
            .unwrap();
        let task = store.state().unwrap().active;
        let reference = task.references[0].id;
        let page = store
            .request(Request::PreviewReference {
                id: task.id,
                revision: task.revision,
                reference,
                offset: 0,
            })
            .unwrap();
        assert_eq!(page["kind"], "binary");
        assert!(task.references[0].shared_with.is_empty());
        assert!(store.shared_reference(reference, 0).is_err());
        store
            .request(Request::Share {
                id: task.id,
                revision: task.revision,
                reference,
                allow: true,
            })
            .unwrap();
        assert!(store.shared_reference(reference, 0).is_err());
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locked_reference_removal_recovers_forward_after_restart_without_restoring_access() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        let image = ImageInput {
            media_type: "image/jpeg".into(),
            data: STANDARD.encode([255, 216, 255, 192, 0, 8, 8, 0, 1, 0, 1, 0, 255, 217]),
        };
        store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "private.jpg".into(),
                data: image.data.clone(),
                preview: Some(image),
            })
            .unwrap();
        let t = store.state().unwrap().active;
        let reference = t.references[0].id;
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference,
                allow: true,
            })
            .unwrap();
        let t = store.state().unwrap().active;
        let blob = store
            .root
            .join(t.id.to_string())
            .join(format!("{reference}.bin"));
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&blob)
            .unwrap();
        let design = root.join("valid-work.native");
        fs::write(&design, b"preserved manual work").unwrap();
        store
            .request(Request::Remove {
                id: t.id,
                revision: t.revision,
                reference,
            })
            .unwrap();
        let removed = store.state().unwrap().active;
        assert_eq!(removed.revision, t.revision + 1);
        assert!(removed.references.is_empty());
        assert!(removed.restricted_results);
        assert_eq!(removed.pending_removals.len(), 1);
        assert!(blob.exists());
        assert!(!blob.with_extension("preview.json").exists());
        assert!(store.shared_reference(reference, 0).is_err());
        assert!(store
            .request(Request::Share {
                id: t.id,
                revision: removed.revision,
                reference,
                allow: true
            })
            .is_err());
        assert!(store
            .request(Request::PreviewReference {
                id: t.id,
                revision: removed.revision,
                reference,
                offset: 0
            })
            .is_err());
        store.request(Request::New).unwrap();
        store.request(Request::Open { id: t.id }).unwrap();
        assert_eq!(store.state().unwrap().active.pending_removals.len(), 1);
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        let store = Store::new(&p).unwrap();
        assert_eq!(store.state().unwrap().active.pending_removals.len(), 1);
        drop(lock);
        // If bytes are removed but committing cleanup is interrupted, keep the tombstone.
        let metadata_lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(store.path(t.id))
            .unwrap();
        assert!(store.state().is_err());
        assert!(!blob.exists());
        assert_eq!(store.active().unwrap().unwrap().pending_removals.len(), 1);
        drop(metadata_lock);
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        let store = Store::new(&p).unwrap();
        let recovered = store.state().unwrap().active;
        assert!(recovered.pending_removals.is_empty());
        assert_eq!(recovered.revision, removed.revision);
        assert!(recovered.restricted_results);
        assert_eq!(fs::read(&design).unwrap(), b"preserved manual work");
        let mut full = recovered.clone();
        full.pending_removals = (0..8)
            .map(|_| PendingRemoval {
                id: Uuid::new_v4(),
                bytes: FILE_LIMIT as u64,
            })
            .collect();
        full.validate().unwrap();
        full.pending_removals.push(PendingRemoval {
            id: Uuid::new_v4(),
            bytes: 1,
        });
        assert!(full.validate().is_err());
        full.pending_removals.pop();
        full.pending_removals[1].id = full.pending_removals[0].id;
        assert!(full.validate().is_err());
        drop(store);
        drop(p);
        fs::remove_file(&design).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    fn setup() -> (PathBuf, Project) {
        let root = std::env::temp_dir().join(format!("hazlo-tasks-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let p = Project::create(&root).unwrap();
        (root, p)
    }
    #[test]
    fn interrupted_creation_reuses_reserved_identity_and_preserves_existing_history(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let original=store.state().unwrap().active;
        store.request(Request::Message{id:original.id,revision:original.revision,text:"Keep my existing work".into(),history:r#"{"version":1,"turns":[{"role":"user","content":"Keep my existing work"}]}"#.into(),correction_of:None}).unwrap();
        let original_bytes=fs::read(store.path(original.id)).unwrap();
        let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(store.root.join("index.json")).unwrap();
        assert!(store.request(Request::New).is_err());
        let plan:Creation=read_json(&store.root.join("creation.json")).unwrap();let reserved=plan.tasks[0].id;
        assert_eq!(store.index().unwrap().unwrap().active,original.id);assert!(store.path(reserved).exists());
        assert!(!fs::read_to_string(store.root.join("creation.json")).unwrap().contains("Keep my existing work"));
        let created=fs::read(store.path(reserved)).unwrap();assert!(store.state().is_err());assert_eq!(fs::read(store.path(reserved)).unwrap(),created);
        drop(store);drop(p);drop(lock);
        let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();let recovered=store.state().unwrap();
        assert_eq!(recovered.active.id,reserved);assert_eq!(recovered.tasks.len(),2);assert_eq!(fs::read(store.path(original.id)).unwrap(),original_bytes);assert_eq!(fs::read(store.path(reserved)).unwrap(),created);
        assert_eq!(recovered.tasks.iter().find(|t|t.id==original.id).unwrap().title,"Keep my existing work");assert!(!store.root.join("creation.json").exists());
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn published_creation_is_adopted_without_rewriting_a_locked_index(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();store.ensure().unwrap();
        let index=store.root.join("index.json");let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&index).unwrap();assert!(store.request(Request::New).is_err());drop(lock);
        let intent=store.root.join("creation.json");let plan:Creation=read_json(&intent).unwrap();
        let intent_lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&intent).unwrap();
        assert!(store.state().is_err());assert!(store.index().unwrap().unwrap()==plan.after);
        let index_bytes=fs::read(&index).unwrap();let task_bytes=fs::read(store.path(plan.tasks[0].id)).unwrap();
        let index_lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&index).unwrap();drop(intent_lock);
        let recovered=store.state().unwrap();assert_eq!(recovered.active.id,plan.tasks[0].id);assert_eq!(recovered.tasks.len(),2);assert_eq!(fs::read(&index).unwrap(),index_bytes);assert_eq!(fs::read(store.path(recovered.active.id)).unwrap(),task_bytes);assert!(!intent.exists());
        drop(index_lock);drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_initial_migration_recovers_exact_tasks_without_another_history_copy(){
        let (root,p)=setup();let raw=r#"{"version":1,"turns":[{"role":"user","content":"LOCAL_MIGRATION_SENTINEL"}]}"#;
        for provider in ["codex","claude"]{p.conversation_history(provider,Some(raw)).unwrap();}
        let store=Store::new(&p).unwrap();let mut entries=Vec::new();let mut after=Index{version:1,active:Uuid::nil(),ids:vec![],titles:Default::default()};
        for provider in ["codex","claude"]{
            let mut task=Store::empty(provider);task.legacy=true;task.title=format!("Conversación anterior · {provider}");task.history=Some(raw.into());
            if entries.is_empty(){atomic_json(&store.path(task.id),&task,false).unwrap();after.active=task.id;}
            after.ids.push(task.id);after.titles.insert(task.id,task.title.clone());entries.push(CreatedTask{id:task.id,provider:provider.into(),legacy_history_sha256:Some(crate::kicad_project::digest(raw.as_bytes())),task_sha256:Store::task_digest(&task).unwrap()});
        }
        let ids=after.ids.clone();let plan=Creation{version:1,before:None,after,tasks:entries};Store::validate_creation(&plan).unwrap();atomic_json(&store.root.join("creation.json"),&plan,false).unwrap();
        assert!(!fs::read_to_string(store.root.join("creation.json")).unwrap().contains("LOCAL_MIGRATION_SENTINEL"));
        drop(store);drop(p);let p=Project::open(&root).unwrap();let store=Store::new(&p).unwrap();
        assert_eq!(store.state().unwrap().tasks.iter().map(|t|t.id).collect::<Vec<_>>(),ids);
        for id in ids{assert_eq!(store.read(id).unwrap().history.as_deref(),Some(raw));}
        for provider in ["codex","claude"]{assert_eq!(p.conversation_history(provider,None).unwrap().as_deref(),Some(raw));}
        assert_eq!(store.state().unwrap().tasks.len(),2);drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn creation_refuses_changed_tasks_or_indices_without_overwriting_them(){
        for change_task in [true,false]{
            let (root,p)=setup();let store=Store::new(&p).unwrap();store.ensure().unwrap();let index=store.root.join("index.json");
            let lock=OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&index).unwrap();assert!(store.request(Request::New).is_err());drop(lock);
            let plan:Creation=read_json(&store.root.join("creation.json")).unwrap();let path=store.path(plan.tasks[0].id);
            if change_task{let mut task=store.read(plan.tasks[0].id).unwrap();task.goal="Preserved newer user work".into();task.title="Preserved newer user work".into();task.revision+=1;atomic_json(&path,&task,true).unwrap();}
            else{let mut changed=plan.before.clone().unwrap();changed.titles.insert(changed.active,"Changed index".into());atomic_json(&index,&changed,true).unwrap();}
            let task_bytes=fs::read(&path).unwrap();let index_bytes=fs::read(&index).unwrap();assert!(store.state().is_err());
            assert_eq!(fs::read(&path).unwrap(),task_bytes);assert_eq!(fs::read(&index).unwrap(),index_bytes);assert!(store.root.join("creation.json").exists());
            drop(store);drop(p);fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn summaries_use_task_titles_and_an_unreadable_archive_does_not_hide_the_active_chat(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let old=store.state().unwrap().active;
        store.request(Request::Message{id:old.id,revision:old.revision,text:"Recovered authoritative title".into(),history:r#"{"version":1,"turns":[]}"#.into(),correction_of:None}).unwrap();store.request(Request::New).unwrap();
        let active=store.state().unwrap().active.id;assert_eq!(store.state().unwrap().tasks[0].title,"Recovered authoritative title");
        let lock=OpenOptions::new().read(true).share_mode(0).open(store.path(old.id)).unwrap();
        let state=store.state().unwrap();assert_eq!(state.active.id,active);assert_eq!(state.tasks[0].id,old.id);assert!(state.tasks[0].title.contains("no disponible"));
        drop(lock);assert_eq!(store.state().unwrap().tasks[0].title,"Recovered authoritative title");drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_index_never_silently_adopts_or_hides_an_unclaimed_conversation(){
        let (root,p)=setup();let store=Store::new(&p).unwrap();let task=Store::empty("codex");atomic_json(&store.path(task.id),&task,false).unwrap();let before=fs::read(store.path(task.id)).unwrap();
        assert!(store.state().err().unwrap().to_string().contains("índice"));assert!(!store.root.join("index.json").exists());assert!(!store.root.join("creation.json").exists());assert_eq!(fs::read(store.path(task.id)).unwrap(),before);
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn changed_legacy_source_pauses_migration_without_recopying_or_overwriting_it(){
        let (root,p)=setup();let raw=r#"{"version":1,"turns":[{"role":"user","content":"Original local history"}]}"#;p.conversation_history("codex",Some(raw)).unwrap();
        let store=Store::new(&p).unwrap();let mut task=Store::empty("codex");task.legacy=true;task.title="Conversación anterior · codex".into();task.history=Some(raw.into());
        let after=Index{version:1,active:task.id,ids:vec![task.id],titles:std::collections::BTreeMap::from([(task.id,task.title.clone())])};
        let plan=Creation{version:1,before:None,after,tasks:vec![CreatedTask{id:task.id,provider:"codex".into(),legacy_history_sha256:Some(crate::kicad_project::digest(raw.as_bytes())),task_sha256:Store::task_digest(&task).unwrap()}]};
        atomic_json(&store.root.join("creation.json"),&plan,false).unwrap();
        let updated=r#"{"version":1,"turns":[{"role":"user","content":"Preserve this newer history"}]}"#;p.conversation_history("codex",Some(updated)).unwrap();
        assert!(store.state().is_err());assert!(!store.path(task.id).exists());assert!(!store.root.join("index.json").exists());assert_eq!(p.conversation_history("codex",None).unwrap().as_deref(),Some(updated));
        drop(store);drop(p);fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn archives_migrates_and_reopens_without_changing_legacy_history() {
        let (root, p) = setup();
        let raw = "{\"version\":1,\"turns\":[{\"role\":\"user\",\"content\":\"Keep original\"}]}";
        p.conversation_history("codex", Some(raw)).unwrap();
        p.conversation_history("claude", Some(raw)).unwrap();
        let store = Store::new(&p).unwrap();
        let state = store.state().unwrap();
        assert_eq!(state.tasks.len(), 2);
        let old = state.active.id;
        store.request(Request::New).unwrap();
        assert_ne!(store.state().unwrap().active.id, old);
        store.request(Request::Open { id: old }).unwrap();
        assert_eq!(
            store.active().unwrap().unwrap().history.as_deref(),
            Some(raw)
        );
        assert_eq!(
            p.conversation_history("codex", None).unwrap().as_deref(),
            Some(raw)
        );
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        assert_eq!(Store::new(&p).unwrap().state().unwrap().active.id, old);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_reference_commit_preserves_task_and_cleans_uncommitted_bytes() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(store.path(t.id))
            .unwrap();
        assert!(store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "capture.bin".into(),
                data: STANDARD.encode(b"local capture"),
                preview: None
            })
            .is_err());
        assert_eq!(store.state().unwrap().active.revision, t.revision);
        assert!(store.state().unwrap().active.references.is_empty());
        assert_eq!(
            fs::read_dir(store.root.join(t.id.to_string()))
                .unwrap()
                .count(),
            0
        );
        drop(lock);
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn image_reopens_with_consent_detects_tampering_and_removal_scrubs_derived_context() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        let bytes = vec![255, 216, 255, 192, 0, 8, 8, 0, 1, 0, 1, 0, 255, 217];
        let image = ImageInput {
            media_type: "image/jpeg".into(),
            data: STANDARD.encode(&bytes),
        };
        store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "part.jpg".into(),
                data: STANDARD.encode(&bytes),
                preview: Some(image.clone()),
            })
            .unwrap();
        let t = store.state().unwrap().active;
        let reference = t.references[0].id;
        let preview = store
            .request(Request::PreviewReference {
                id: t.id,
                revision: t.revision,
                reference,
                offset: 0,
            })
            .unwrap();
        assert_eq!(preview["kind"], "image");
        assert!(store.state().unwrap().active.references[0]
            .shared_with
            .is_empty());
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference
            })
            .is_err());
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference,
                allow: true,
            })
            .unwrap();
        drop(store);
        drop(p);
        let p = Project::open(&root).unwrap();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        assert_eq!(
            store
                .request(Request::Image {
                    id: t.id,
                    revision: t.revision,
                    reference
                })
                .unwrap(),
            serde_json::to_value(image).unwrap()
        );
        let raw=serde_json::json!({"version":1,"turns":[{"role":"user","content":"Keep my intent"},{"role":"assistant","content":"Derived private result"}]}).to_string();
        store
            .request(Request::History {
                id: t.id,
                revision: t.revision,
                value: Some(raw),
            })
            .unwrap();
        let blob = store
            .root
            .join(t.id.to_string())
            .join(format!("{reference}.bin"));
        fs::write(&blob, b"tampered").unwrap();
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference
            })
            .is_err());
        store
            .request(Request::Remove {
                id: t.id,
                revision: t.revision,
                reference,
            })
            .unwrap();
        let task = store.active().unwrap().unwrap();
        assert!(task.restricted());
        assert!(task.history.as_ref().unwrap().contains("Keep my intent"));
        assert!(!task
            .history
            .as_ref()
            .unwrap()
            .contains("Derived private result"));
        assert!(!blob.exists());
        assert!(!blob.with_extension("preview.json").exists());
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn references_are_local_bounded_scoped_and_revision_bound() {
        let (root, p) = setup();
        let store = Store::new(&p).unwrap();
        let t = store.state().unwrap().active;
        store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "board.bin".into(),
                data: STANDARD.encode(b"private fixture"),
                preview: None,
            })
            .unwrap();
        let t = store.state().unwrap().active;
        assert!(t.restricted());
        assert_eq!(t.references[0].shared_with.len(), 0);
        assert!(store
            .request(Request::History {
                id: t.id,
                revision: t.revision - 1,
                value: None
            })
            .is_err());
        assert!(store
            .request(Request::AddReference {
                id: t.id,
                revision: t.revision,
                name: "../escape".into(),
                data: "aA==".into(),
                preview: None
            })
            .is_err());
        assert!(store
            .request(Request::Image {
                id: t.id,
                revision: t.revision,
                reference: t.references[0].id
            })
            .is_err());
        store
            .request(Request::Share {
                id: t.id,
                revision: t.revision,
                reference: t.references[0].id,
                allow: true,
            })
            .unwrap();
        let t = store.state().unwrap().active;
        assert!(!t.restricted());
        store
            .request(Request::Provider {
                id: t.id,
                revision: t.revision,
                provider: "claude".into(),
            })
            .unwrap();
        let t = store.state().unwrap().active;
        assert!(t.restricted());
        store
            .request(Request::Remove {
                id: t.id,
                revision: t.revision,
                reference: t.references[0].id,
            })
            .unwrap();
        assert!(store.state().unwrap().active.references.is_empty());
        drop(store);
        drop(p);
        fs::remove_dir_all(root).unwrap();
    }
}
