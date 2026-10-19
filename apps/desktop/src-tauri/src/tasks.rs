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
    pub shared_with: Vec<String>,
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
    #[serde(default)]
    pub legacy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<String>,
}
impl Task {
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
        let mut ids = std::collections::BTreeSet::new();
        let mut total = 0;
        for r in &self.references {
            if r.id.is_nil()
                || !ids.insert(r.id)
                || !valid_name(&r.name)
                || r.bytes == 0
                || r.bytes > FILE_LIMIT as u64
                || !crate::kicad_project::valid_hash(&r.sha256)
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
    for (i, turn) in value["turns"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if turn["role"] == "assistant" {
            turn["reference_ids"] = if prior["turns"][i]["content"] == turn["content"] {
                prior["turns"][i]["reference_ids"].clone()
            } else {
                serde_json::json!(task
                    .references
                    .iter()
                    .filter(|r| r.shared_with.contains(&task.provider))
                    .map(|r| r.id)
                    .collect::<Vec<_>>())
            };
        }
    }
    serde_json::to_string(&value).map_err(io::Error::other)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    active: Uuid,
    ids: Vec<Uuid>,
    #[serde(default)]
    titles: std::collections::BTreeMap<Uuid, String>,
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
        Ok(Some(value))
    }
    fn read(&self, id: Uuid) -> io::Result<Task> {
        let task: Task = read_json(&self.path(id))?;
        task.validate()?;
        if task.id != id {
            return Err(error("La identidad de tarea cambió."));
        }
        Ok(task)
    }
    pub fn active(&self) -> io::Result<Option<Task>> {
        self.index()?.map(|i| self.read(i.active)).transpose()
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
    fn preview_image(&self, task: Uuid, reference: Uuid) -> io::Result<ImageInput> {
        let (folder, _pins) = self.blob_dir(task)?;
        let image: ImageInput = read_json(&folder.join(format!("{reference}.preview.json")))?;
        image.validate().map_err(io::Error::other)?;
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
            legacy: false,
            history: Some("{\"version\":1,\"turns\":[]}".into()),
        }
    }
    pub fn ensure(&self) -> io::Result<()> {
        if self.index()?.is_some() {
            return Ok(());
        }
        let mut ids = Vec::new();
        let mut titles = std::collections::BTreeMap::new();
        for provider in ["codex", "claude", "ollama_local", "ollama_cloud"] {
            if let Some(raw) = self.project.conversation_history(provider, None)? {
                let mut task = Self::empty(provider);
                task.history = Some(raw);
                task.legacy = true;
                task.title = format!("Conversación anterior · {provider}");
                task.validate()?;
                atomic_json(&self.path(task.id), &task, false)?;
                titles.insert(task.id, task.title.clone());
                ids.push(task.id);
            }
        }
        if ids.is_empty() {
            let task = Self::empty("codex");
            atomic_json(&self.path(task.id), &task, false)?;
            titles.insert(task.id, task.title.clone());
            ids.push(task.id);
        }
        atomic_json(
            &self.root.join("index.json"),
            &Index {
                version: 1,
                active: ids[0],
                ids,
                titles,
            },
            false,
        )
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
                title: index
                    .titles
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "Conversación guardada".into()),
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
        atomic_json(&self.path(task.id), task, true)?;
        let mut index = self
            .index()?
            .ok_or_else(|| error("No hay índice de tareas."))?;
        if index.titles.get(&task.id) != Some(&task.title) {
            index.titles.insert(task.id, task.title.clone());
            atomic_json(&self.root.join("index.json"), &index, true)?;
        }
        Ok(())
    }
    pub fn request(&self, request: Request) -> io::Result<serde_json::Value> {
        use serde_json::json;
        self.ensure()?;
        match request {
            Request::State => {}
            Request::New => {
                let mut index = self.index()?.unwrap();
                if index.ids.len() >= 128 {
                    return Err(error(
                        "Se alcanzó el límite de 128 tareas en este proyecto.",
                    ));
                }
                let task = Self::empty(&self.read(index.active)?.provider);
                atomic_json(&self.path(task.id), &task, false)?;
                index.ids.push(task.id);
                index.titles.insert(task.id, task.title.clone());
                index.active = task.id;
                atomic_json(&self.root.join("index.json"), &index, true)?;
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
                let mut task = self.scoped(id, revision)?;
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
                    shared_with: Vec::new(),
                };
                let (folder, _pins) = self.blob_dir(task.id)?;
                let reference_id = reference.id;
                let blob = folder.join(format!("{reference_id}.bin"));
                let preview_path = folder.join(format!("{reference_id}.preview.json"));
                task.references.push(reference);
                task.revision += 1;
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
                // A failed title-cache update may follow a committed task; keep its bytes.
                if result.is_err()
                    && self
                        .read(task.id)?
                        .references
                        .iter()
                        .all(|r| r.id != reference_id)
                {
                    for path in [&blob, &preview_path] {
                        if path.try_exists()? {
                            fs::remove_file(path)?;
                        }
                    }
                }
                result?;
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
                    r.shared_with.push(task.provider.clone());
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
                let bytes = task
                    .references
                    .iter()
                    .find(|r| r.id == reference)
                    .ok_or_else(|| error("Referencia no disponible."))?
                    .bytes;
                // Revoke access before deleting bytes; a failed deletion must never restore sharing.
                // shortcut: old native reports have no per-reference lineage; keep them local after removal.
                task.restricted_results |= task
                    .references
                    .iter()
                    .any(|r| r.id == reference && !r.shared_with.is_empty());
                task.references.retain(|r| r.id != reference);
                task.pending_removals.push(PendingRemoval {
                    id: reference,
                    bytes,
                });
                let mut value: serde_json::Value =
                    serde_json::from_str(task.history.as_deref().unwrap_or("{}"))
                        .map_err(io::Error::other)?;
                if let Some(turns) = value["turns"].as_array_mut() {
                    for turn in turns {
                        if turn["reference_ids"]
                            .as_array()
                            .is_some_and(|ids| ids.iter().any(|id| *id == json!(reference)))
                        {
                            *turn = json!({"role":"assistant","content":"Respuesta retirada junto con su referencia local."});
                        }
                    }
                }
                task.history = Some(serde_json::to_string(&value).map_err(io::Error::other)?);
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
                        json!({"kind":"image","image":self.preview_image(id,reference)?,"next_offset":null}),
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
                        r.id == reference && r.image && r.shared_with.contains(&task.provider)
                    })
                    .ok_or_else(|| {
                        error(
                            "Aprueba compartir esta imagen con la IA seleccionada antes de usarla.",
                        )
                    })?;
                self.reference_bytes(id, r)?;
                let image = self.preview_image(id, r.id)?;
                return serde_json::to_value(image).map_err(io::Error::other);
            }
        }
        serde_json::to_value(self.state()?).map_err(io::Error::other)
    }
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
        let p = Project::create(&root).unwrap();
        (root, p)
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
