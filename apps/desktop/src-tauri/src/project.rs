use crate::{
    agent::{self, EditInput, EditResult},
    arduino::{self, CompileInput, CompileResult},
    cadcraft::{self, DrawingInput, DrawingResult},
    cadpilot,
    freecad::{validate_result, SolidResult},
    kicad_project::KicadInput,
    kicad_run::{self, KicadResult},
    konnect_run::{self, KonnectInput, KonnectResult},
    orca::SliceInput,
    orca_run::{self, SliceResult},
    SolidInput,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    mem::{offset_of, size_of},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf, Prefix},
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::*;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    project_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    AwaitingApproval,
    Running,
    Cancelled,
    Failed,
    Interrupted,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub project_id: Uuid,
    pub input: OperationInput,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<RunAudit>,
}

// Untagged serialization keeps existing FreeCAD input bytes compatible with schema 1/2.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OperationInput {
    Freecad(SolidInput),
    Arduino(CompileInput),
    Agent(EditInput),
    Drawing(DrawingInput),
    Kicad(KicadInput),
    Orca(SliceInput),
    Cadpilot(cadpilot::Input),
    LiveCad(crate::cadpilot_live::Input),
    LivePcb(crate::kicad_live::Input),
    LiveAgent(crate::agent_editor::Input),
    LiveDrawing(crate::cadcraft_live::Input),
    NativeTools(crate::native_tools::Input),
    PublicDocument(crate::public_document::Input),
    Konnect(KonnectInput),
    KonnectPcb(crate::konnect_pcb::Input),
}

impl From<SolidInput> for OperationInput {
    fn from(input: SolidInput) -> Self {
        Self::Freecad(input)
    }
}
impl From<CompileInput> for OperationInput {
    fn from(input: CompileInput) -> Self {
        Self::Arduino(input)
    }
}
impl From<EditInput> for OperationInput {
    fn from(input: EditInput) -> Self {
        Self::Agent(input)
    }
}
impl OperationInput {
    pub(crate) fn native_program(&self)->Option<crate::host_tools::Program> {
        use crate::host_tools::Program;
        match self {
            Self::NativeTools(i)=>Some(i.program),Self::LiveCad(_)=>Some(Program::Freecad),
            Self::LivePcb(_)=>Some(Program::Kicad),Self::LiveAgent(_)=>Some(Program::Agent),
            Self::LiveDrawing(_)=>Some(Program::Drawing),_=>None,
        }
    }
    pub(crate) fn receipt_sha256(&self) -> Option<&str> {
        match self {
            Self::NativeTools(i)=>i.receipt_sha256.as_deref(),
            Self::PublicDocument(i)=>i.receipt_sha256.as_deref(),
            Self::LiveCad(i)=>i.receipt_sha256.as_deref(),
            Self::LivePcb(i)=>i.receipt_sha256.as_deref(),
            Self::LiveAgent(i)=>i.receipt_sha256.as_deref(),
            Self::LiveDrawing(i)=>i.receipt_sha256.as_deref(),
            _=>None,
        }
    }
    pub(crate) fn retain_receipt(&mut self, directory:&Path) -> io::Result<bool> {
        let (receipt,limit)=match self {
            Self::NativeTools(i)=>(&mut i.receipt_sha256,crate::native_tools::REPORT_LIMIT),
            Self::PublicDocument(i)=>(&mut i.receipt_sha256,crate::public_document::REPORT_LIMIT),
            Self::LiveCad(i)=>(&mut i.receipt_sha256,65536),
            Self::LivePcb(i)=>(&mut i.receipt_sha256,65536),
            Self::LiveAgent(i)=>(&mut i.receipt_sha256,65536),
            Self::LiveDrawing(i)=>(&mut i.receipt_sha256,65536),
            _=>return Ok(false),
        };
        let bytes=crate::kicad_project::read_regular(&directory.join("result.json"),limit).map_err(io::Error::other)?;
        *receipt=Some(crate::kicad_project::digest(&bytes));
        Ok(true)
    }
    pub(crate) fn integration(&self) -> &'static str {
        match self {
            Self::Freecad(_) => "freecad",
            Self::Arduino(_) => "arduino",
            Self::Agent(_) => "arduino_agent",
            Self::Drawing(_) => "cadcraft",
            Self::Kicad(_) => "kicad",
            Self::Orca(_) => "orca",
            Self::Cadpilot(_) => "cadpilot",
            Self::LiveCad(_) => "freecad_live",
            Self::LivePcb(_) => "kicad_live",
            Self::LiveAgent(_) => "arduino_agent_live",
            Self::LiveDrawing(_) => "cadcraft_live",
            Self::NativeTools(_) => "native_tools",
            Self::PublicDocument(_) => "public_document",
            Self::Konnect(_) => "konnect",
            Self::KonnectPcb(_) => "konnect_pcb",
        }
    }
    pub fn job_id(&self) -> Uuid {
        match self {
            Self::Freecad(input) => input.job_id,
            Self::Arduino(input) => input.job_id,
            Self::Agent(input) => input.job_id,
            Self::Drawing(input) => input.job_id,
            Self::Kicad(input) => input.job_id,
            Self::Orca(input) => input.job_id,
            Self::Cadpilot(input) => input.job_id,
            Self::LiveCad(input) => input.job_id,
            Self::LivePcb(input) => input.job_id,
            Self::LiveAgent(input) => input.job_id,
            Self::LiveDrawing(input) => input.job_id,
            Self::NativeTools(input) => input.job_id,
            Self::PublicDocument(input) => input.job_id,
            Self::Konnect(input) => input.job_id,
            Self::KonnectPcb(input) => input.job_id,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Freecad(input) => input.clone().validate().map(|_| ()),
            Self::Arduino(input) => input.validate(),
            Self::Agent(input) => input.validate(),
            Self::Drawing(input) => input.validate(),
            Self::Kicad(input) => input.validate(),
            Self::Orca(input) => input.validate(),
            Self::Cadpilot(input) => input.validate(),
            Self::LiveCad(input) => input.validate(),
            Self::LivePcb(input) => input.validate(),
            Self::LiveAgent(input) => input.validate(),
            Self::LiveDrawing(input) => input.validate(),
            Self::NativeTools(input) => input.validate(),
            Self::PublicDocument(input) => input.validate(),
            Self::Konnect(input) => input.validate(),
            Self::KonnectPcb(input) => input.validate(),
        }
    }
    pub fn solid(&self) -> Result<&SolidInput, String> {
        match self {
            Self::Freecad(input) => Ok(input),
            _ => Err("La operación seleccionada no es un sólido de FreeCAD.".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunSource {
    Manual,
    Mcp,
    Ollama {
        model: String,
        digest: String,
        cloud: bool,
        runtime_version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunAudit {
    #[serde(default, skip_serializing_if = "is_false")]
    pub approval_invalidated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_attempted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<crate::tasks::Binding>,
    #[serde(default,skip_serializing_if="Vec::is_empty")]
    pub instruction_sources:Vec<crate::tasks::InstructionSource>,
    pub source: RunSource,
    pub user_edited: bool,
    pub adapter_version: u32,
    pub destination: String,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
    pub sequence: u64,
    pub error: Option<String>,
    #[serde(default)]
    pub files_verified: bool,
}

fn is_false(value:&bool)->bool{!*value}

fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX.into()) as u64
}

impl Snapshot {
    pub fn advance(&mut self) {
        if let Some(audit) = &mut self.audit {
            audit.sequence = audit.sequence.saturating_add(1);
            audit.updated_unix_ms = timestamp().max(audit.updated_unix_ms);
        }
    }
}

pub struct Project {
    pub id: Uuid,
    pub root: PathBuf,
    _pins: Vec<File>,
    _writer: File,
}

fn stage_exists(project:&Project,id:Uuid)->io::Result<bool>{project.store("staging").join(id.to_string()).try_exists()}
fn native_may_have_run(snapshot:&Snapshot)->bool {
    snapshot.audit.as_ref().and_then(|a|a.native_attempted).unwrap_or(
        matches!(snapshot.phase,Phase::Running|Phase::Interrupted|Phase::Failed|Phase::Cancelled))
}
fn unresolved_native(id:Uuid,program:Option<crate::host_tools::Program>,kind:&'static str)->crate::native_journal::Summary {
    crate::native_journal::Summary{job_id:id,program,kind,acknowledged:0,uncertain:1,not_dispatched:0,finished:false,needs_review:true,outcome_checked:false}
}
pub struct Stage {
    pub path: PathBuf,
    directory: File,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    Fcstd,
    Step,
    SolidReport,
    Hex,
    Elf,
    Sketch,
    CompileReport,
    OriginalSketch,
    Drawing,
    OriginalDrawing,
    DrawingPreview,
    DrawingReport,
    KicadSource,
    KicadOutputs,
    KicadReport,
    OriginalMesh,
    Sliced,
    Gcode,
    SliceReport,
    OriginalCad,
    CadpilotReport,
    LivePcbReport,
    LiveAgentReport,
    LiveDrawingReport,
    NativeToolsReport,
    PublicDocumentReport,
    KonnectOriginal,
    KonnectEdited,
    KonnectReport,
    KonnectNetlist,
}

pub struct Artifact {
    pub path: PathBuf,
    _directory: File,
    _file: File,
}

impl Artifact {
    /// Return bounded UTF-8 text without modifying the approved artifact.
    pub fn read_text(&self) -> io::Result<String> {
        self.read_text_bounded(65536)
    }
    pub(crate) fn read_text_bounded(&self, limit: u64) -> io::Result<String> {
        // Read through the pinned artifact boundary; never expose a general frontend file reader.
        let mut text = String::new();
        File::open(&self.path)?
            .take(limit + 1)
            .read_to_string(&mut text)?;
        if text.len() as u64 > limit {
            return Err(error(
                "El archivo supera el límite de vista previa de 64 KiB.",
            ));
        }
        Ok(text)
    }
}

fn error(message: &str) -> io::Error {
    io::Error::other(message)
}

pub(crate) fn valid_location(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(error("Seleccione una carpeta local con ruta absoluta."));
    }
    for component in path.components() {
        match component {
            Component::Prefix(prefix)
                if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
            Component::RootDir => (),
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                let stem = name.split('.').next().unwrap_or("").to_uppercase();
                let device = ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
                    || stem
                        .strip_prefix("COM")
                        .or_else(|| stem.strip_prefix("LPT"))
                        .is_some_and(|suffix| {
                            matches!(
                                suffix,
                                "1" | "2"
                                    | "3"
                                    | "4"
                                    | "5"
                                    | "6"
                                    | "7"
                                    | "8"
                                    | "9"
                                    | "¹"
                                    | "²"
                                    | "³"
                            )
                        });
                if device
                    || name.ends_with(['.', ' '])
                    || name.contains([':', '<', '>', '"', '|', '?', '*'])
                {
                    return Err(error("Nombre de carpeta no permitido."));
                }
            }
            _ => {
                return Err(error(
                    "No se permiten rutas de red, dispositivos ni recorridos relativos.",
                ))
            }
        }
    }
    Ok(())
}

pub(crate) fn pin_directory(path: &Path, rename: bool) -> io::Result<File> {
    let handle = OpenOptions::new()
        .read(true)
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | if rename { DELETE } else { 0 })
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = handle.metadata()?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(error(
            "No se permiten enlaces o puntos de reanálisis en el proyecto.",
        ));
    }
    Ok(handle)
}

fn directory_identity(handle: &File) -> io::Result<(u32, u32, u32)> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ))
}

pub(crate) fn pin_lineage(path: &Path) -> io::Result<Vec<File>> {
    valid_location(path)?;
    let mut current = PathBuf::new();
    let mut pins = Vec::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !matches!(component, Component::Prefix(_)) {
            pins.push(pin_directory(&current, false)?);
        }
    }
    let volume: Vec<_> = path
        .components()
        .take(2)
        .collect::<PathBuf>()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut format = [0u16; 32];
    if unsafe { GetDriveTypeW(volume.as_ptr()) } == 4 {
        return Err(error("Las unidades de red no están admitidas."));
    }
    if unsafe {
        GetVolumeInformationW(
            volume.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            format.as_mut_ptr(),
            format.len() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let end = format
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(format.len());
    if String::from_utf16_lossy(&format[..end]) != "NTFS" {
        return Err(error("Esta versión requiere un proyecto en NTFS local."));
    }
    Ok(pins)
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(error("Archivo de proyecto no permitido."));
    }
    let mut bytes = Vec::new();
    // A bounded 64 KiB sketch can expand when JSON escapes its characters.
    file.take(1048577).read_to_end(&mut bytes)?;
    if bytes.len() > 1048576 {
        return Err(error("Archivo de proyecto demasiado grande."));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

// Rename the pinned object, rather than releasing it and trusting a mutable source path.
pub(crate) fn rename_handle(source: &File, target: &Path, replace: bool) -> io::Result<()> {
    let name: Vec<_> = target.as_os_str().encode_wide().collect();
    let size = offset_of!(FILE_RENAME_INFO, FileName) + (name.len() + 1) * size_of::<u16>();
    let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.ReplaceIfExists = replace;
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = (name.len() * size_of::<u16>()) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        if SetFileInformationByHandle(
            source.as_raw_handle(),
            FileRenameInfo,
            info.cast(),
            size as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(crate) fn atomic_json<T: Serialize>(target: &Path, value: &T, replace: bool) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let temporary = target.with_file_name(format!("{}.tmp", Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .access_mode(FILE_GENERIC_WRITE | DELETE)
        .share_mode(FILE_SHARE_READ)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    rename_handle(&file, target, replace)
}

impl Project {
    pub fn conversation_history(&self, provider: &str, value: Option<&str>) -> io::Result<Option<String>> {
        if !["codex", "claude", "ollama_local", "ollama_cloud"].contains(&provider) {
            return Err(error("La IA del historial no es válida."));
        }
        let path = self.store(&format!("conversation-{provider}.json"));
        let validate = |raw: &str| -> io::Result<()> {
            if raw.len() > 512 * 1024 { return Err(error("La conversación supera el límite de guardado.")); }
            let data: serde_json::Value = serde_json::from_str(raw).map_err(io::Error::other)?;
            if data["version"] != 1 || data["turns"].as_array().is_none_or(|turns| turns.len() > 128 || turns.iter().any(|t| !matches!(t["role"].as_str(), Some("user" | "assistant")) || !t["content"].is_string())) {
                return Err(error("La conversación guardada no es válida."));
            }
            Ok(())
        };
        if let Some(raw) = value {
            validate(raw)?;
            atomic_json(&path, &raw, true)?;
            return Ok(None);
        }
        let bytes = match crate::kicad_project::read_regular(&path, 2 * 1024 * 1024) {
            Ok(bytes) => bytes,
            Err(_) if !path.try_exists()? => return Ok(None),
            Err(_) => return Err(error("No se pudo leer el historial. Sus datos se conservan.")),
        };
        let raw: String = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        validate(&raw)?;
        Ok(Some(raw))
    }

    fn acquire(root: &Path, create: bool) -> io::Result<Self> {
        let mut pins = pin_lineage(root)?;
        let store = root.join(".hazlomexa");
        if create {
            fs::create_dir(&store)?;
        }
        pins.push(pin_directory(&store, false)?);
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(store.join("writer.lock"))?;
        if writer.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(error("Bloqueo de proyecto no permitido."));
        }
        for name in ["runs", "staging", "artifacts"] {
            let path = store.join(name);
            if create {
                fs::create_dir(&path)?;
            }
            pins.push(pin_directory(&path, false)?);
        }
        let manifest = root.join("hazlomx.json");
        if create {
            atomic_json(
                &manifest,
                &Manifest {
                    schema_version: 1,
                    project_id: Uuid::new_v4(),
                },
                false,
            )?;
        }
        let metadata: Manifest = read_json(&manifest)?;
        if metadata.schema_version != 1 || metadata.project_id.is_nil() {
            return Err(error("Formato de proyecto no compatible."));
        }
        Ok(Self {
            id: metadata.project_id,
            root: root.to_owned(),
            _pins: pins,
            _writer: writer,
        })
    }

    pub fn create(root: &Path) -> io::Result<Self> {
        Self::acquire(root, true)
    }
    pub fn open(root: &Path) -> io::Result<Self> {
        Self::acquire(root, false)
    }
    pub(crate) fn store(&self, name: &str) -> PathBuf {
        self.root.join(".hazlomexa").join(name)
    }
    fn snapshot_path(&self, job: Uuid) -> PathBuf {
        self.store("runs").join(format!("{job}.json"))
    }
    pub fn artifact_directory(&self, job: Uuid) -> PathBuf {
        self.store("artifacts").join(job.to_string())
    }

    pub(crate) fn drawing_source(&self, job: Uuid, format: cadcraft::Format) -> PathBuf {
        self.store("runs")
            .join(format!("{job}.{}", format.extension()))
    }
    pub(crate) fn captured_source(&self, job: Uuid) -> PathBuf {
        self.store("runs").join(format!("{job}.sources"))
    }
    pub(crate) fn kicad_summary(&self, job: Uuid) -> io::Result<crate::kicad::CheckSummary> {
        let snapshot: Snapshot = read_json(&self.snapshot_path(job))?;
        self.validate_snapshot(&snapshot)?;
        if !matches!(
            snapshot.input,
            OperationInput::Kicad(_) | OperationInput::KonnectPcb(_)
        ) {
            return Err(error("La operación no es de KiCad."));
        }
        if snapshot.input.job_id() != job || snapshot.phase != Phase::Completed {
            return Err(error("El reporte todavía no está publicado."));
        }
        let directory = self.artifact_directory(job);
        let _pin = pin_directory(&directory, false)?;
        self.validate_outputs(&directory, &snapshot.input)?;
        match snapshot.input {
            OperationInput::Kicad(_) => {
                let result: KicadResult = read_json(&directory.join("result.json"))?;
                Ok(result.summary)
            }
            OperationInput::KonnectPcb(_) => {
                let result: crate::konnect_pcb::ResultDocument =
                    read_json(&directory.join("result.json"))?;
                Ok(result.verification.summary)
            }
            _ => unreachable!(),
        }
    }

    pub fn artifact(&self, job: Uuid, format: ArtifactFormat) -> io::Result<Artifact> {
        let snapshot: Snapshot = read_json(&self.snapshot_path(job))?;
        self.validate_snapshot(&snapshot)?;
        if snapshot.input.job_id() != job || snapshot.phase != Phase::Completed {
            return Err(error("La operación no tiene artefactos publicados."));
        }
        let directory = self.artifact_directory(job);
        let pin = pin_directory(&directory, false)?;
        self.validate_outputs(&directory, &snapshot.input)?;
        let name: String = match (&snapshot.input, format) {
            (OperationInput::Freecad(_), ArtifactFormat::Fcstd) => "solid.FCStd".into(),
            (OperationInput::Freecad(_), ArtifactFormat::Step) => "solid.step".into(),
            (OperationInput::Freecad(_), ArtifactFormat::SolidReport) => "result.json".into(),
            (OperationInput::Arduino(_), ArtifactFormat::Hex) => "firmware.hex".into(),
            (OperationInput::Arduino(_), ArtifactFormat::Elf) => "firmware.elf".into(),
            (OperationInput::Arduino(_), ArtifactFormat::Sketch) => "Sketch.ino".into(),
            (OperationInput::Arduino(_), ArtifactFormat::CompileReport) => "result.json".into(),
            (OperationInput::Agent(_), ArtifactFormat::Sketch) => "Sketch.ino".into(),
            (OperationInput::Agent(_), ArtifactFormat::OriginalSketch) => "Original.ino".into(),
            (OperationInput::Drawing(input), ArtifactFormat::Drawing) => {
                format!("Drawing.{}", input.output_format.extension())
            }
            (OperationInput::Drawing(input), ArtifactFormat::OriginalDrawing) => {
                format!("Original.{}", input.source_format.extension())
            }
            (OperationInput::Drawing(_), ArtifactFormat::DrawingPreview) => "Preview.png".into(),
            (OperationInput::Drawing(_), ArtifactFormat::DrawingReport) => "result.json".into(),
            (OperationInput::Kicad(input), ArtifactFormat::KicadSource) => {
                format!("Checked/{}", input.entry)
            }
            (OperationInput::Kicad(_), ArtifactFormat::KicadReport) => "Check.json".into(),
            (OperationInput::Kicad(_), ArtifactFormat::KicadOutputs) => "Exports".into(),
            (OperationInput::Orca(_), ArtifactFormat::OriginalMesh) => {
                "Original/Original.stl".into()
            }
            (OperationInput::Orca(_), ArtifactFormat::Sliced) => "Sliced.3mf".into(),
            (OperationInput::Orca(_), ArtifactFormat::Gcode) => "Toolpath.gcode".into(),
            (OperationInput::Orca(_), ArtifactFormat::SliceReport) => "result.json".into(),
            (OperationInput::Cadpilot(_), ArtifactFormat::OriginalCad) => "Original.FCStd".into(),
            (OperationInput::Cadpilot(input), ArtifactFormat::Fcstd)
                if input.request.operation != cadpilot::Operation::Inspect =>
            {
                "Edited.FCStd".into()
            }
            (OperationInput::Cadpilot(input), ArtifactFormat::Step)
                if input.request.operation != cadpilot::Operation::Inspect =>
            {
                "Edited.step".into()
            }
            (OperationInput::Cadpilot(_), ArtifactFormat::CadpilotReport) => "result.json".into(),
            (OperationInput::LiveCad(_), ArtifactFormat::CadpilotReport) => "result.json".into(),
            (OperationInput::LivePcb(input), ArtifactFormat::KicadSource) if input.export_copy => "Board.kicad_pcb".into(),
            (OperationInput::LivePcb(_), ArtifactFormat::LivePcbReport) => "result.json".into(),
            (OperationInput::LiveAgent(_), ArtifactFormat::LiveAgentReport) => "result.json".into(),
            (OperationInput::LiveDrawing(crate::cadcraft_live::Input {operation:crate::cadcraft_live::Operation::ExportDxf {},..}), ArtifactFormat::Drawing) => "Drawing.dxf".into(),
            (OperationInput::LiveDrawing(_), ArtifactFormat::LiveDrawingReport) => "result.json".into(),
            (OperationInput::NativeTools(_), ArtifactFormat::NativeToolsReport) => "result.json".into(),
            (OperationInput::PublicDocument(_), ArtifactFormat::PublicDocumentReport) => "result.json".into(),
            (OperationInput::KonnectPcb(input), ArtifactFormat::KonnectOriginal) => {
                format!("Original/{}", input.source.entry)
            }
            (OperationInput::KonnectPcb(input), ArtifactFormat::KonnectEdited) => {
                format!("Verification/Checked/{}", input.source.entry)
            }
            (OperationInput::KonnectPcb(_), ArtifactFormat::KonnectReport) => "result.json".into(),
            (OperationInput::KonnectPcb(_), ArtifactFormat::KicadReport) => {
                "Verification/Check.json".into()
            }
            (OperationInput::KonnectPcb(_), ArtifactFormat::KicadOutputs) => {
                "Verification/Exports".into()
            }
            (OperationInput::Konnect(input), ArtifactFormat::KonnectOriginal) => {
                format!("Original/{}", input.source.entry)
            }
            (OperationInput::Konnect(input), ArtifactFormat::KonnectEdited) => {
                format!("Edited/{}", input.source.entry)
            }
            (OperationInput::Konnect(_), ArtifactFormat::KonnectReport) => "result.json".into(),
            (OperationInput::Konnect(input), ArtifactFormat::KonnectNetlist)
                if input.schema_version == 2 =>
            {
                "Netlist.xml".into()
            }
            _ => return Err(error("El formato no corresponde a esta operación.")),
        };
        let path = directory.join(name);
        if matches!(format, ArtifactFormat::KicadOutputs) {
            let file = pin_directory(&path, false)?;
            return Ok(Artifact {
                path,
                _directory: pin,
                _file: file,
            });
        }
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(error("Artefacto no permitido."));
        }
        Ok(Artifact {
            path,
            _directory: pin,
            _file: file,
        })
    }

    pub fn snapshot(&self, input: impl Into<OperationInput>, phase: Phase) -> io::Result<Snapshot> {
        let input = input.into();
        input.clone().validate().map_err(io::Error::other)?;
        let now = timestamp();
        let destination = format!(".hazlomexa/artifacts/{}", input.job_id());
        let native_attempted=input.native_program().map(|_|false);
        Ok(Snapshot {
            schema_version: match &input {
                OperationInput::Freecad(_) => 2,
                OperationInput::Arduino(_) => 3,
                OperationInput::Agent(_) => 4,
                OperationInput::Drawing(_) => 5,
                OperationInput::Kicad(_) => 6,
                OperationInput::Orca(_) => 7,
                OperationInput::Cadpilot(_) => 8,
                OperationInput::LiveCad(_) => 13,
                OperationInput::LivePcb(_) => 14,
                OperationInput::LiveAgent(_) => 15,
                OperationInput::LiveDrawing(_) => 16,
                OperationInput::NativeTools(_) => 17,
                OperationInput::PublicDocument(_) => 18,
                OperationInput::Konnect(_) => 9,
                OperationInput::KonnectPcb(_) => 11,
            },
            project_id: self.id,
            input,
            phase,
            audit: Some(RunAudit {
                approval_invalidated: false,
                native_attempted,
                task: crate::tasks::Store::new(self)?.binding()?,
                instruction_sources:crate::tasks::Store::new(self)?.active()?.map(|task|task.instruction_sources()).unwrap_or_default(),
                source: RunSource::Manual,
                user_edited: false,
                adapter_version: 1,
                destination,
                created_unix_ms: now,
                updated_unix_ms: now,
                sequence: 1,
                error: None,
                files_verified: false,
            }),
        })
    }

    pub fn save(&self, snapshot: &Snapshot) -> io::Result<()> {
        self.validate_snapshot(snapshot)?;
        atomic_json(&self.snapshot_path(snapshot.input.job_id()), snapshot, true)
    }

    fn validate_snapshot(&self, snapshot: &Snapshot) -> io::Result<()> {
        if matches!(&snapshot.input,OperationInput::Cadpilot(input) if input.preview.is_none()) {
            return Err(error(
                "Falta la inspección inicial de la propuesta CADPilot.",
            ));
        }
        if (snapshot.schema_version == 10
            && matches!(snapshot.input, OperationInput::KonnectPcb(_) | OperationInput::LiveCad(_) | OperationInput::LivePcb(_) | OperationInput::LiveAgent(_) | OperationInput::LiveDrawing(_) | OperationInput::NativeTools(_) | OperationInput::PublicDocument(_)))
            || !matches!(
                (&snapshot.input, snapshot.schema_version),
                (OperationInput::Freecad(_), 1 | 2)
                    | (OperationInput::Arduino(_), 3)
                    | (OperationInput::Agent(_), 4)
                    | (OperationInput::Drawing(_), 5)
                    | (OperationInput::Kicad(_), 6)
                    | (OperationInput::Orca(_), 7)
                    | (OperationInput::Cadpilot(_), 8)
                    | (OperationInput::LiveCad(_), 13)
                    | (OperationInput::LivePcb(_), 14)
                    | (OperationInput::LiveAgent(_), 15)
                    | (OperationInput::LiveDrawing(_), 16)
                    | (OperationInput::NativeTools(_), 17)
                    | (OperationInput::PublicDocument(_), 18)
                    | (OperationInput::Konnect(_), 9)
                    | (OperationInput::KonnectPcb(_), 11 | 12)
                    | (_, 10)
            )
            || snapshot.project_id != self.id
            || snapshot.input.job_id().is_nil()
        {
            return Err(error(
                "Estado de ejecución no compatible con este proyecto.",
            ));
        }
        match (snapshot.schema_version, &snapshot.audit) {
            (1, None) => (),
            (2..=18, Some(audit)) => {
                if audit.task.as_ref().is_some_and(|task|task.id.is_nil() || task.revision==0) {
                    return Err(error("La identidad de tarea de la ejecución no es válida."));
                }
                if !crate::tasks::valid_instruction_sources(&audit.instruction_sources){return Err(error("Proveniencia de instrucciones de la ejecución no válida."));}
                if matches!(audit.source, RunSource::Mcp)
                    != matches!(snapshot.schema_version, 10 | 12 | 13 | 14 | 15 | 16 | 17 | 18)
                {
                    return Err(error(
                        "Origen de herramienta no compatible con la versión del estado.",
                    ));
                }
                if audit.adapter_version != 1
                    || (audit.approval_invalidated && (snapshot.phase!=Phase::Cancelled || audit.native_attempted==Some(true)))
                    || (audit.files_verified
                        && matches!(snapshot.phase, Phase::AwaitingApproval | Phase::Running))
                    || audit.sequence == 0
                    || audit.created_unix_ms == 0
                    || audit.updated_unix_ms < audit.created_unix_ms
                    || audit.destination
                        != format!(".hazlomexa/artifacts/{}", snapshot.input.job_id())
                    || audit
                        .error
                        .as_ref()
                        .is_some_and(|error| error.len() > 16384)
                {
                    return Err(error("Historial de ejecución no compatible."));
                }
                if let RunSource::Ollama {
                    model,
                    digest,
                    cloud,
                    runtime_version,
                } = &audit.source
                {
                    if model.is_empty()
                        || model.len() > 128
                        || model.ends_with(":cloud") != *cloud
                        || digest.len() != 64
                        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                        || runtime_version != "0.35.1"
                    {
                        return Err(error("Identidad de modelo no compatible."));
                    }
                }
            }
            _ => return Err(error("Versión e historial de ejecución inconsistentes.")),
        }
        snapshot
            .input
            .clone()
            .validate()
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn reserve_stage(&self, job: Uuid) -> io::Result<Stage> {
        let path = self.store("staging").join(job.to_string());
        fs::create_dir(&path)?;
        Ok(Stage {
            directory: pin_directory(&path, false)?,
            path,
        })
    }

    pub(crate) fn validate_outputs(&self, directory: &Path, input: &OperationInput) -> io::Result<()> {
        if let OperationInput::PublicDocument(input)=input {
            return crate::public_document::validate_artifacts(directory,input).map_err(io::Error::other);
        }
        if let OperationInput::KonnectPcb(input) = input {
            let result: crate::konnect_pcb::ResultDocument =
                read_json(&directory.join("result.json"))?;
            return crate::konnect_pcb::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        if let OperationInput::Konnect(input) = input {
            let result: KonnectResult = read_json(&directory.join("result.json"))?;
            return konnect_run::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        if let OperationInput::NativeTools(input) = input {
            return crate::native_tools::validate_artifacts(directory,input).map_err(io::Error::other);
        }
        if let OperationInput::LiveDrawing(input) = input {
            let bytes=crate::kicad_project::read_regular(&directory.join("result.json"),65536).map_err(io::Error::other)?;
            if input.receipt_sha256.as_deref()!=Some(&crate::kicad_project::digest(&bytes)) {
                return Err(error("El informe de CADCraft cambió."));
            }
            let report=serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            return crate::cadcraft_live::validate_artifacts(directory,input,&report).map_err(io::Error::other);
        }
        if let OperationInput::LiveAgent(input) = input {
            let bytes=crate::kicad_project::read_regular(&directory.join("result.json"),65536).map_err(io::Error::other)?;
            if input.receipt_sha256.as_deref()!=Some(&crate::kicad_project::digest(&bytes)) {
                return Err(error("El informe de la edición del sketch cambió."));
            }
            let report=serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            return crate::agent_editor::validate_report(input,&report).map_err(io::Error::other);
        }
        if let OperationInput::LivePcb(input) = input {
            let bytes=crate::kicad_project::read_regular(&directory.join("result.json"),65536).map_err(io::Error::other)?;
            if input.receipt_sha256.as_deref()!=Some(&crate::kicad_project::digest(&bytes)) {
                return Err(error("El informe de la edición PCB cambió."));
            }
            let result:crate::kicad_live::Receipt=serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            return crate::kicad_live::validate_artifacts(directory,input,&result).map_err(io::Error::other);
        }
        if let OperationInput::LiveCad(input) = input {
            let bytes = crate::kicad_project::read_regular(&directory.join("result.json"), 65536).map_err(io::Error::other)?;
            if input.receipt_sha256.as_deref() != Some(&crate::kicad_project::digest(&bytes)) {
                return Err(error("El informe de la transacción nativa cambió."));
            }
            let result: serde_json::Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            return crate::cadpilot_live::validate_result(input, &result).map_err(io::Error::other);
        }
        if let OperationInput::Cadpilot(input) = input {
            let result: cadpilot::ResultDocument = read_json(&directory.join("result.json"))?;
            return cadpilot::validate_result(directory, input, &result).map_err(io::Error::other);
        }
        if let OperationInput::Orca(input) = input {
            let result: SliceResult = read_json(&directory.join("result.json"))?;
            return orca_run::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        if let OperationInput::Kicad(input) = input {
            let result: KicadResult = read_json(&directory.join("result.json"))?;
            return kicad_run::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        if let OperationInput::Drawing(input) = input {
            let result: DrawingResult = read_json(&directory.join("result.json"))?;
            return cadcraft::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        if let OperationInput::Agent(input) = input {
            let result: EditResult = read_json(&directory.join("result.json"))?;
            return agent::validate_artifacts(directory, input, &result).map_err(io::Error::other);
        }
        if let OperationInput::Arduino(input) = input {
            let result: CompileResult = read_json(&directory.join("result.json"))?;
            return arduino::validate_artifacts(directory, input, &result)
                .map_err(io::Error::other);
        }
        let input = input.solid().map_err(io::Error::other)?;
        let result: SolidResult = read_json(&directory.join("result.json"))?;
        validate_result(&result, input).map_err(io::Error::other)?;
        for name in ["solid.FCStd", "solid.step"] {
            let file = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(directory.join(name))?;
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || metadata.len() == 0
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(error("Artefacto no permitido."));
            }
        }
        Ok(())
    }

    pub fn publish(&self, stage: Stage, snapshot: &Snapshot) -> io::Result<PathBuf> {
        self.validate_snapshot(snapshot)?;
        if snapshot.phase != Phase::Running
            || stage.path
                != self
                    .store("staging")
                    .join(snapshot.input.job_id().to_string())
        {
            return Err(error(
                "La publicación no corresponde a una ejecución activa.",
            ));
        }
        let identity = directory_identity(&stage.directory)?;
        // Recursive validators pin child ancestry; validate before requesting exclusive DELETE access.
        self.validate_outputs(&stage.path, &snapshot.input)?;
        // FreeCAD's current-directory handle cannot coexist with DELETE access.
        drop(stage.directory);
        let directory = pin_directory(&stage.path, true)?;
        if directory_identity(&directory)? != identity {
            return Err(error("El directorio de trabajo cambió antes de publicar."));
        }
        let destination = self.artifact_directory(snapshot.input.job_id());
        rename_handle(&directory, &destination, false)?;
        Ok(destination)
    }

    pub fn history(&self) -> io::Result<Vec<Snapshot>> {
        let mut results = Vec::new();
        for entry in fs::read_dir(self.store("runs"))? {
            let entry = entry?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let snapshot: Snapshot = read_json(&entry.path())?;
            self.validate_snapshot(&snapshot)?;
            if entry.path() != self.snapshot_path(snapshot.input.job_id()) {
                return Err(error("Identidad de ejecución inconsistente."));
            }
            results.push(snapshot);
        }
        Ok(results)
    }

    pub fn native_recovery(&self,task:Option<&crate::tasks::Binding>) -> io::Result<Vec<crate::native_journal::Summary>> {
        let mut summaries=Vec::new();
        for snapshot in self.history()? {
            if task.is_some_and(|task|snapshot.audit.as_ref().and_then(|a|a.task.as_ref()).is_none_or(|bound|bound.id!=task.id)){continue;}
            if let OperationInput::NativeTools(input)=&snapshot.input {
                let stage=self.store("staging").join(input.job_id.to_string());
                let directory=if stage.try_exists()?{stage}else{self.artifact_directory(input.job_id)};
                if directory.try_exists()? {if let Some(journal)=crate::native_journal::read(&directory,input)? {let mut summary=journal.summary();summary.program=Some(input.program);summaries.push(summary);continue;}}
                if !input.calls.is_empty() && native_may_have_run(&snapshot) {
                    summaries.push(unresolved_native(input.job_id,Some(input.program),"catalog_batch"));
                }
            } else if matches!(&snapshot.input,OperationInput::LiveCad(_)|OperationInput::LivePcb(_)|OperationInput::LiveAgent(_)|OperationInput::LiveDrawing(_)) {
                let id=snapshot.input.job_id();let staged=stage_exists(self,id)?;
                let directory=if staged {self.store("staging").join(id.to_string())}else{self.artifact_directory(id)};
                if directory.try_exists()? {if let Some(record)=crate::live_dispatch::read(&directory,&snapshot.input).map_err(io::Error::other)? {summaries.push(record.summary(&snapshot.input));continue;}}
                let finished=snapshot.input.receipt_sha256().is_some() && (snapshot.phase==Phase::Completed || directory.try_exists()? && self.validate_outputs(&directory,&snapshot.input).is_ok());
                if finished {summaries.push(crate::native_journal::Summary{job_id:id,program:snapshot.input.native_program(),kind:"live_adapter",acknowledged:1,uncertain:0,not_dispatched:0,finished:true,needs_review:false,outcome_checked:false});}
                else if native_may_have_run(&snapshot) {summaries.push(unresolved_native(id,snapshot.input.native_program(),"live_adapter"));}
            }
        }
        Ok(summaries)
    }
    pub fn recover(&self) -> io::Result<Vec<Snapshot>> {
        let mut results = Vec::new();
        for mut snapshot in self.history()? {
            let published = self.artifact_directory(snapshot.input.job_id());
            let previous_phase = snapshot.phase;
            if matches!(&snapshot.input,OperationInput::LiveCad(_)|OperationInput::LivePcb(_)|OperationInput::LiveAgent(_)|OperationInput::LiveDrawing(_)) && snapshot.input.receipt_sha256().is_none() {
                let stage=self.store("staging").join(snapshot.input.job_id().to_string());
                let directory=if stage.try_exists()?{stage}else{published.clone()};
                if directory.try_exists()? && crate::live_dispatch::read(&directory,&snapshot.input).map_err(io::Error::other)?.is_some_and(|r|r.outcome==crate::native_journal::Outcome::Acknowledged) {
                        let _pin=pin_directory(&directory,false)?;
                        let mut checked=snapshot.input.clone();checked.retain_receipt(&directory)?;
                        self.validate_outputs(&directory,&checked)?;snapshot.input=checked;self.save(&snapshot)?;
                }
            }
            if published.try_exists()? {
                let _pin = pin_directory(&published, false)?;
                match self.validate_outputs(&published, &snapshot.input) {
                    Ok(()) => snapshot.phase = Phase::Completed,
                    Err(error) if snapshot.phase == Phase::Completed => return Err(error),
                    Err(_) => {
                        // An occupied destination is not evidence that this run committed.
                        if matches!(snapshot.phase, Phase::Running | Phase::AwaitingApproval) {
                            snapshot.phase = Phase::Interrupted;
                        }
                    }
                }
            } else if snapshot.input.receipt_sha256().is_some()
                && self.store("staging").join(snapshot.input.job_id().to_string()).try_exists()?
            {
                let path=self.store("staging").join(snapshot.input.job_id().to_string());
                let finished=match &snapshot.input {OperationInput::NativeTools(input)=>crate::native_journal::read(&path,input)?.is_some_and(|j|j.finished),_=>true};
                // Publish only an already checked, finished report. Never invoke a native call in recovery.
                if finished && self.validate_outputs(&path,&snapshot.input).is_ok() {
                    let mut running=snapshot.clone();running.phase=Phase::Running;
                    match self.publish(Stage{directory:pin_directory(&path,false)?,path},&running) {
                        Ok(_)=>snapshot.phase=Phase::Completed,
                        Err(_)=>{snapshot.phase=Phase::Interrupted;if let Some(audit)=&mut snapshot.audit {audit.error=Some("El informe nativo terminado se conserva en el área local de trabajo. Su publicación sigue pendiente; no repitas las llamadas.".into());}}
                    }
                } else if snapshot.phase==Phase::Completed {return Err(error("Faltan los artefactos publicados de una ejecución terminada."));}
                else if matches!(snapshot.phase,Phase::Running|Phase::AwaitingApproval) {snapshot.phase=Phase::Interrupted;}
            } else if snapshot.phase == Phase::Completed {
                return Err(error("Faltan los artefactos de una ejecución terminada."));
            } else if matches!(snapshot.phase, Phase::Running | Phase::AwaitingApproval) {
                snapshot.phase = Phase::Interrupted;
            }
            if snapshot.phase != previous_phase {
                snapshot.advance();
                if snapshot.phase == Phase::Completed {
                    if let Some(audit) = &mut snapshot.audit {
                        audit.error = None;
                        audit.files_verified = true;
                    }
                }
                self.save(&snapshot)?;
            }
            results.push(snapshot);
        }
        Ok(results)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{freecad::Geometry, Dimensions};

    #[test]
    fn all_live_receipts_recover_without_native_calls_and_reject_tampering() {
        let agent=crate::agent_editor::tests::job_fixture();
        let pcb=crate::kicad_live::tests::move_fixture();
        let drawing=crate::cadcraft_live::tests::fixture_input();
        let cad=crate::cadpilot_live::Input {schema_version:1,live_cad_version:1,job_id:Uuid::new_v4(),document_name:"Owned".into(),document_token:Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Owned recovery fixture".into(),operations:vec![serde_json::json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}})],before:serde_json::json!([]),receipt_sha256:None};
        let cad_report=serde_json::json!({"live_cad_version":1,"job_id":cad.job_id,"document_token":cad.document_token,"before_sha256":cad.state_sha256,"after":{"document_name":cad.document_name,"state_sha256":"b".repeat(64),"objects":[]},"operations":[{"action":"edit_object","success":true}],"undo_available":true});
        let cases=vec![
            (OperationInput::LiveAgent(agent.clone()),crate::agent_editor::tests::report_fixture(&agent)),
            (OperationInput::LivePcb(pcb.clone()),serde_json::to_value(crate::kicad_live::tests::receipt_fixture(&pcb)).unwrap()),
            (OperationInput::LiveDrawing(drawing.clone()),serde_json::to_value(crate::cadcraft_live::tests::receipt(&drawing)).unwrap()),
            (OperationInput::LiveCad(cad),cad_report),
        ];
        for (input,report) in cases {for mode in ["valid","occupied","tampered","published_without_final_snapshot"] {
            let root=root();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
            let project=Project::create(&root).unwrap();let id=input.job_id();
            let mut snapshot=project.snapshot(input.clone(),Phase::Running).unwrap();
            let audit=snapshot.audit.as_mut().unwrap();audit.source=RunSource::Mcp;audit.native_attempted=Some(true);
            let stage=project.reserve_stage(id).unwrap();let bytes=serde_json::to_vec_pretty(&report).unwrap();
            fs::write(stage.path.join("result.json"),&bytes).unwrap();
            snapshot.input.retain_receipt(&stage.path).unwrap();project.save(&snapshot).unwrap();
            project.validate_outputs(&stage.path,&snapshot.input).unwrap();
            let original=root.join("manual-work.native");fs::write(&original,b"keep user edits").unwrap();
            if mode=="published_without_final_snapshot"{
                let fence=crate::live_dispatch::Dispatch::start(&stage.path,&input,std::sync::Arc::new(std::sync::Mutex::new(())),std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))).unwrap();drop(fence.enter().unwrap());fence.complete(&snapshot.input).unwrap();drop(fence);
                let destination=project.publish(stage,&snapshot).unwrap();snapshot.input=input.clone();snapshot.phase=Phase::Interrupted;snapshot.audit.as_mut().unwrap().files_verified=false;project.save(&snapshot).unwrap();drop(project);
                let project=Project::open(&root).unwrap();let recovered=project.recover().unwrap();assert_eq!(recovered[0].phase,Phase::Completed);assert!(recovered[0].audit.as_ref().unwrap().files_verified);assert_eq!(recovered[0].input.receipt_sha256(),Some(crate::kicad_project::digest(&bytes).as_str()));assert_eq!(fs::read(destination.join("result.json")).unwrap(),bytes);assert_eq!(fs::read(&original).unwrap(),b"keep user edits");assert!(!project.native_recovery(None).unwrap()[0].needs_review);drop(project);fs::remove_dir_all(root).unwrap();continue;
            }
            let destination=project.artifact_directory(id);
            if mode=="occupied" {fs::create_dir(&destination).unwrap();fs::write(destination.join("unrelated.txt"),b"keep occupied destination").unwrap();}
            if mode=="tampered" {fs::write(stage.path.join("result.json"),b"changed report").unwrap();}
            drop(stage);drop(project);
            let project=Project::open(&root).unwrap();
            let recovered=project.recover().unwrap();
            if mode=="valid" {assert_eq!(recovered[0].phase,Phase::Completed);assert_eq!(fs::read(destination.join("result.json")).unwrap(),bytes);}
            else {assert_eq!(recovered[0].phase,Phase::Interrupted);assert!(project.store("staging").join(id.to_string()).join("result.json").exists());}
            assert_eq!(project.native_recovery(None).unwrap()[0].needs_review,mode=="tampered");
            if mode=="occupied" {
                assert_eq!(fs::read(destination.join("unrelated.txt")).unwrap(),b"keep occupied destination");
                assert!(destination.canonicalize().unwrap().starts_with(root.canonicalize().unwrap()));
                fs::remove_file(destination.join("unrelated.txt")).unwrap();fs::remove_dir(&destination).unwrap();
                assert_eq!(project.recover().unwrap()[0].phase,Phase::Completed);
                assert_eq!(fs::read(destination.join("result.json")).unwrap(),bytes);
            }
            assert_eq!(fs::read(original).unwrap(),b"keep user edits");
            if mode!="tampered" {let before=serde_json::to_value(project.history().unwrap()).unwrap();assert_eq!(serde_json::to_value(project.recover().unwrap()).unwrap(),before);}
            drop(project);fs::remove_dir_all(root).unwrap();
        }}
    }
    #[test]
    fn missing_dispatch_records_do_not_clear_an_attempted_mutation_or_legacy_uncertainty() {
        let root=root();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let project=Project::create(&root).unwrap();
        let call=crate::native_tools::Call{tool:"state".into(),arguments:Default::default()};
        let input=crate::native_tools::Input{schema_version:1,native_tools_version:1,job_id:Uuid::new_v4(),connection_id:Uuid::new_v4(),program:crate::host_tools::Program::Freecad,catalog_sha256:"a".repeat(64),description:"Owned missing-journal recovery".into(),calls:vec![crate::native_tools::Call{tool:"edit".into(),arguments:Default::default()}],readback:call.clone(),guard:Some(crate::native_tools::Guard{call,result_sha256:"b".repeat(64),checks:Vec::new()}),receipt_sha256:None};
        let mut snapshot=project.snapshot(OperationInput::NativeTools(input),Phase::Running).unwrap();snapshot.audit.as_mut().unwrap().source=RunSource::Mcp;
        project.save(&snapshot).unwrap();assert!(project.native_recovery(None).unwrap().is_empty());
        snapshot.audit.as_mut().unwrap().native_attempted=Some(true);project.save(&snapshot).unwrap();
        assert!(project.native_recovery(None).unwrap()[0].needs_review);
        for phase in [Phase::Running,Phase::Cancelled,Phase::Failed,Phase::Interrupted] {
            snapshot.phase=phase;snapshot.audit.as_mut().unwrap().native_attempted=None;project.save(&snapshot).unwrap();
            assert!(project.native_recovery(None).unwrap()[0].needs_review);
        }
        snapshot.phase=Phase::AwaitingApproval;project.save(&snapshot).unwrap();assert!(project.native_recovery(None).unwrap().is_empty());
        drop(project);fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Retained owned legacy native project fixture required; never run on a user project"]
    fn retained_legacy_project_number_format_recovery() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let project =
            Project::open(&workspace.join("target/kicad-number-fidelity/legacy-project")).unwrap();
        let before = project.history().unwrap();
        assert!(before.iter().any(|snapshot| snapshot.phase == Phase::Completed
            && matches!(&snapshot.input, OperationInput::Konnect(input) if input.source.schema_version == 1)));
        let by_id = |snapshots: Vec<Snapshot>| {
            snapshots
                .into_iter()
                .map(|s| (s.input.job_id(), serde_json::to_vec(&s).unwrap()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let count = before.len();
        assert_eq!(by_id(before), by_id(project.recover().unwrap()));
        println!("Legacy completed project recovery verified: {count} records");
    }

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("Hazlo proyecto á's {}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn conversation_history_survives_reopen_and_refuses_invalid_writes() {
        let root = root();
        let raw = serde_json::json!({"version":1,"turns":[{"role":"user","content":"A device, with no cloud"},{"role":"assistant","content":"Which control application do you use?"}]}).to_string();
        let project = Project::create(&root).unwrap();
        assert!(project.conversation_history("codex", None).unwrap().is_none());
        project.conversation_history("codex", Some(&raw)).unwrap();
        assert!(project.conversation_history("claude", None).unwrap().is_none());
        for (provider, bad) in [("../outside",raw.as_str()),("codex","{broken"),("codex",r#"{"version":1,"turns":[{"role":"system","content":"ignored"}]}"#)] {
            assert!(project.conversation_history(provider, Some(bad)).is_err());
        }
        assert!(project.conversation_history("codex",Some(&"x".repeat(512*1024+1))).is_err());
        drop(project);
        let project = Project::open(&root).unwrap();
        assert_eq!(project.conversation_history("codex", None).unwrap(),Some(raw));
        let path=project.store("conversation-codex.json");
        fs::write(&path,b"invalid retained file").unwrap();
        assert!(project.conversation_history("codex",None).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"invalid retained file");
    }
    fn input() -> SolidInput {
        SolidInput::new(Dimensions {
            length: 10.0,
            width: 20.0,
            height: 30.0,
        })
        .unwrap()
    }
    pub(crate) fn outputs(stage: &Stage, input: &OperationInput) {
        let input = input.solid().unwrap();
        let result = SolidResult {
            schema_version: 1,
            adapter_version: 1,
            success: true,
            job_id: input.job_id,
            freecad_version: crate::freecad::SUPPORTED_VERSION.into(),
            dimensions_mm: input.dimensions_mm,
            geometry: Geometry {
                bounds_mm: [10.0, 20.0, 30.0],
                volume_mm3: 6000.0,
                solids: 1,
            },
            artifacts: ["solid.FCStd".into(), "solid.step".into()],
        };
        fs::write(
            stage.path.join("result.json"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        fs::write(stage.path.join("solid.FCStd"), b"fixture").unwrap();
        fs::write(stage.path.join("solid.step"), b"fixture").unwrap();
    }

    #[test]
    fn artifact_text_is_bounded_utf8_and_does_not_modify_source() {
        let root = root();
        let path = root.join("Sketch.ino");
        for bytes in ["á".repeat(32768).into_bytes(), vec![b'x'; 65537], vec![255]] {
            fs::write(&path, &bytes).unwrap();
            let artifact = Artifact {
                path: path.clone(),
                _directory: pin_directory(&root, false).unwrap(),
                _file: File::open(&path).unwrap(),
            };
            if bytes.len() == 65536 {
                assert_eq!(artifact.read_text().unwrap().as_bytes(), bytes);
            } else {
                assert!(artifact.read_text().is_err());
            }
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn single_writer_unknown_versions_and_originals_are_preserved() {
        let root = root();
        fs::write(root.join("original.txt"), b"keep").unwrap();
        let project = Project::create(&root).unwrap();
        assert!(Project::open(&root).is_err());
        drop(project);
        let original = fs::read(root.join("hazlomx.json")).unwrap();
        let mut manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
        manifest["schema_version"] = 999.into();
        fs::write(
            root.join("hazlomx.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        assert!(Project::open(&root).is_err());
        assert_eq!(fs::read(root.join("original.txt")).unwrap(), b"keep");
        assert_eq!(
            read_json::<serde_json::Value>(&root.join("hazlomx.json")).unwrap()["schema_version"],
            999
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_is_whole_set_nonreplacing_and_recovered_after_lost_status() {
        let root = root();
        let project = Project::create(&root).unwrap();
        let snapshot = project.snapshot(input(), Phase::Running).unwrap();
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        assert!(fs::rename(&stage.path, root.join("moved-stage")).is_err());
        outputs(&stage, &snapshot.input);
        let final_path = project.publish(stage, &snapshot).unwrap();
        assert!(project
            .artifact(snapshot.input.job_id(), ArtifactFormat::Step)
            .is_err());
        assert!(final_path.join("solid.FCStd").exists() && final_path.join("solid.step").exists());
        assert!(!project
            .store("staging")
            .join(snapshot.input.job_id().to_string())
            .exists());
        drop(project);
        let project = Project::open(&root).unwrap();
        assert_eq!(project.recover().unwrap()[0].phase, Phase::Completed);
        let artifact = project
            .artifact(snapshot.input.job_id(), ArtifactFormat::Step)
            .unwrap();
        assert_eq!(artifact.path, final_path.join("solid.step"));
        assert_eq!(artifact.read_text().unwrap(), "fixture");
        assert!(fs::rename(&artifact.path, root.join("moved.step")).is_err());
        drop(artifact);
        assert!(project
            .artifact(Uuid::new_v4(), ArtifactFormat::Step)
            .is_err());
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        outputs(&stage, &snapshot.input);
        assert!(project.publish(stage, &snapshot).is_err());
        assert_eq!(fs::read(final_path.join("solid.step")).unwrap(), b"fixture");
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_and_unapproved_runs_never_publish_or_resume() {
        let root = root();
        let project = Project::create(&root).unwrap();
        let snapshot = project.snapshot(input(), Phase::AwaitingApproval).unwrap();
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        outputs(&stage, &snapshot.input);
        assert!(project.publish(stage, &snapshot).is_err());
        assert_eq!(project.recover().unwrap()[0].phase, Phase::Interrupted);
        assert!(!project.artifact_directory(snapshot.input.job_id()).exists());
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn path_escape_device_and_stream_names_are_rejected() {
        for path in [
            r"C:\project\..\outside",
            r"C:\project\out:stream",
            r"C:\project\CON.txt",
            r"C:\project\COM¹",
            r"C:\project\com².txt",
            r"C:\project\COM³.tar.gz",
            r"C:\project\LPT¹",
            r"C:\project\lpt².txt",
            r"C:\project\LPT³.tar.gz",
            r"\\server\share\project",
            r"C:\project\ending.",
        ] {
            assert!(valid_location(Path::new(path)).is_err(), "{path}");
        }
        for path in [
            r"C:\project\COM10",
            r"C:\project\LPT0",
            r"C:\project\COM¹extra",
            r"C:\project\COM⁴",
            r"C:\project\LPT³extra",
        ] {
            assert!(valid_location(Path::new(path)).is_ok(), "{path}");
        }
        let root = root();
        let project = Project::create(&root).unwrap();
        assert!(fs::rename(root.join(".hazlomexa/staging"), root.join("moved")).is_err());
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn junction_escape_is_rejected_without_touching_its_target() {
        let root = root();
        let outside = root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("original.txt"), b"keep").unwrap();
        let project = Project::create(&root).unwrap();
        drop(project);
        let link = root.join(".hazlomexa/artifacts");
        fs::remove_dir(&link).unwrap();
        let output = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Project::open(&root).is_err());
        assert_eq!(fs::read(outside.join("original.txt")).unwrap(), b"keep");
        fs::remove_dir(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_snapshot_replace_and_partial_temporary_file_preserve_state() {
        let root = root();
        let project = Project::create(&root).unwrap();
        let mut snapshot = project.snapshot(input(), Phase::AwaitingApproval).unwrap();
        project.save(&snapshot).unwrap();
        let path = project.snapshot_path(snapshot.input.job_id());
        let original = fs::read(&path).unwrap();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        snapshot.phase = Phase::Running;
        assert!(project.save(&snapshot).is_err());
        drop(locked);
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::write(project.store("runs").join("partial.tmp"), b"{invalid").unwrap();
        drop(project);
        let project = Project::open(&root).unwrap();
        assert_eq!(project.recover().unwrap()[0].phase, Phase::Interrupted);
        assert!(!project.artifact_directory(snapshot.input.job_id()).exists());
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abrupt_exit_at_publication_boundaries_recovers_without_regeneration() {
        use std::os::windows::process::CommandExt;
        for checkpoint in ["before_validation", "before_commit", "after_commit"] {
            let root = root();
            fs::write(root.join("original.txt"), b"keep").unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "project::tests::publication_crash_helper",
                ])
                .env("HAZLOMEXA_CRASH_ROOT", &root)
                .env("HAZLOMEXA_CRASH_CHECKPOINT", checkpoint)
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(73),
                "{checkpoint}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let project = Project::open(&root).unwrap();
            let recovered = project.recover().unwrap();
            assert_eq!(recovered.len(), 1);
            let job = recovered[0].input.job_id();
            if checkpoint == "after_commit" {
                assert_eq!(recovered[0].phase, Phase::Completed);
                assert!(project.artifact_directory(job).join("solid.step").is_file());
                assert!(!project.store("staging").join(job.to_string()).exists());
            } else {
                assert_eq!(recovered[0].phase, Phase::Interrupted);
                assert!(!project.artifact_directory(job).exists());
                assert!(project.store("staging").join(job.to_string()).is_dir());
            }
            let runs = fs::read_dir(project.store("runs"))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                .count();
            assert_eq!(runs, 1);
            assert_eq!(project.recover().unwrap()[0].phase, recovered[0].phase);
            assert_eq!(fs::read(root.join("original.txt")).unwrap(), b"keep");
            drop(project);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    #[ignore = "subprocess helper for publication crash boundary checks"]
    fn publication_crash_helper() {
        let root = PathBuf::from(std::env::var_os("HAZLOMEXA_CRASH_ROOT").unwrap());
        let checkpoint = std::env::var("HAZLOMEXA_CRASH_CHECKPOINT").unwrap();
        let project = Project::create(&root).unwrap();
        let snapshot = project.snapshot(input(), Phase::Running).unwrap();
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        fs::write(
            stage.path.join("input.json"),
            serde_json::to_vec(&snapshot.input).unwrap(),
        )
        .unwrap();
        if checkpoint != "before_validation" {
            outputs(&stage, &snapshot.input);
            project
                .validate_outputs(&stage.path, &snapshot.input)
                .unwrap();
        }
        if checkpoint == "after_commit" {
            project.publish(stage, &snapshot).unwrap();
        }
        // Bypass destructors at the actual persisted boundary, including immediately after rename.
        unsafe {
            windows_sys::Win32::System::Threading::ExitProcess(73);
        }
    }

    #[test]
    fn locked_output_fails_publication_and_preserves_private_input() {
        let root = root();
        let project = Project::create(&root).unwrap();
        let snapshot = project.snapshot(input(), Phase::Running).unwrap();
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        outputs(&stage, &snapshot.input);
        let input_bytes = serde_json::to_vec(&snapshot.input).unwrap();
        fs::write(stage.path.join("input.json"), &input_bytes).unwrap();
        let path = stage.path.clone();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path.join("solid.step"))
            .unwrap();
        assert!(project.publish(stage, &snapshot).is_err());
        assert!(!project.artifact_directory(snapshot.input.job_id()).exists());
        assert_eq!(fs::read(path.join("input.json")).unwrap(), input_bytes);
        drop(locked);
        assert_eq!(project.recover().unwrap()[0].phase, Phase::Interrupted);
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires the owned 64 MiB NTFS fixture in Windows Sandbox"]
    fn actual_disk_exhaustion_preserves_snapshot_and_inputs() {
        assert_eq!(std::env::var("USERNAME").unwrap(), "WDAGUtilityAccount");
        let root = PathBuf::from(r"T:\HazloDiskProject");
        assert_eq!(
            fs::read(root.join("owned-fixture.txt")).unwrap(),
            b"Hazlo owned 64 MiB NTFS fixture"
        );
        let project = Project::create(&root).unwrap();
        let snapshot = project.snapshot(input(), Phase::Running).unwrap();
        project.save(&snapshot).unwrap();
        let previous = fs::read(project.snapshot_path(snapshot.input.job_id())).unwrap();
        let stage = project.reserve_stage(snapshot.input.job_id()).unwrap();
        let input_bytes = serde_json::to_vec(&snapshot.input).unwrap();
        fs::write(stage.path.join("input.json"), &input_bytes).unwrap();
        let filler_path = root.join("owned-filler.bin");
        let mut filler = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&filler_path)
            .unwrap();
        let mut written = 0;
        let full = loop {
            assert!(
                written < 64 * 1024 * 1024,
                "Volume is not the bounded test disk"
            );
            match filler.write_all(&[0x5a; 4096]) {
                Ok(()) => written += 4096,
                Err(error) => break error,
            }
        };
        assert!(
            matches!(full.raw_os_error(), Some(112 | 39)),
            "Not disk exhaustion: {full}"
        );
        let mut failed = snapshot.clone();
        failed.phase = Phase::Failed;
        failed.audit.as_mut().unwrap().error = Some("disk exhaustion fixture ".repeat(512));
        let save_error = project.save(&failed).unwrap_err();
        assert!(
            matches!(save_error.raw_os_error(), Some(112 | 39)),
            "Unexpected snapshot error: {save_error}"
        );
        assert_eq!(
            fs::read(project.snapshot_path(snapshot.input.job_id())).unwrap(),
            previous
        );
        assert_eq!(
            fs::read(stage.path.join("input.json")).unwrap(),
            input_bytes
        );
        assert!(!project.artifact_directory(snapshot.input.job_id()).exists());
        drop(filler);
        fs::remove_file(&filler_path).unwrap();
        drop(stage);
        drop(project);
        let reopened = Project::open(&root).unwrap();
        let recovered = reopened.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].phase, Phase::Interrupted);
        assert!(!reopened
            .artifact_directory(snapshot.input.job_id())
            .exists());
        let result = serde_json::json!({"pass":true,"filled_bytes":written,"disk_full_error":full.raw_os_error(),"snapshot_error":save_error.raw_os_error(),"previous_snapshot_preserved":true,"input_preserved":true,"no_artifacts_published":true,"recovered_phase":"interrupted"});
        fs::write(
            r"C:\HazloProbe\output\disk-full-result.json",
            serde_json::to_vec_pretty(&result).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn occupied_destination_does_not_block_uncommitted_history_or_hide_completed_corruption() {
        for phase in [
            Phase::AwaitingApproval,
            Phase::Running,
            Phase::Failed,
            Phase::Cancelled,
            Phase::Interrupted,
            Phase::Completed,
        ] {
            let root = root();
            let project = Project::create(&root).unwrap();
            let snapshot = project.snapshot(input(), phase).unwrap();
            project.save(&snapshot).unwrap();
            let destination = project.artifact_directory(snapshot.input.job_id());
            fs::create_dir(&destination).unwrap();
            fs::write(destination.join("original.txt"), b"keep").unwrap();
            let recovered = project.recover();
            if phase == Phase::Completed {
                assert!(recovered.is_err());
            } else {
                let expected = if matches!(phase, Phase::AwaitingApproval | Phase::Running) {
                    Phase::Interrupted
                } else {
                    phase
                };
                assert_eq!(recovered.unwrap()[0].phase, expected);
            }
            assert_eq!(fs::read(destination.join("original.txt")).unwrap(), b"keep");
            assert!(!destination.join("solid.FCStd").exists());
            drop(project);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn legacy_runs_remain_readable_and_invalid_audit_cannot_change_paths() {
        let root = root();
        let project = Project::create(&root).unwrap();
        let mut legacy = project.snapshot(input(), Phase::Cancelled).unwrap();
        legacy.schema_version = 1;
        legacy.audit = None;
        project.save(&legacy).unwrap();
        let path = project.snapshot_path(legacy.input.job_id());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(project.recover().unwrap()[0].schema_version, 1);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let valid = project.snapshot(input(), Phase::AwaitingApproval).unwrap();
        for fault in [
            "path",
            "digest",
            "version",
            "clock",
            "sequence",
            "extra",
            "premature-verification",
        ] {
            let mut value = serde_json::to_value(&valid).unwrap();
            match fault {
                "path" => value["audit"]["destination"] = "../outside".into(),
                "digest" => {
                    value["audit"]["source"] = serde_json::json!({"provider":"ollama","model":"fixture:cloud","digest":"wrong","cloud":true,"runtime_version":"0.35.1"})
                }
                "version" => value["schema_version"] = 3.into(),
                "clock" => value["audit"]["updated_unix_ms"] = 0.into(),
                "sequence" => value["audit"]["sequence"] = 0.into(),
                "extra" => value["audit"]["shell"] = "execute".into(),
                "premature-verification" => value["audit"]["files_verified"] = true.into(),
                _ => unreachable!(),
            }
            if let Ok(snapshot) = serde_json::from_value::<Snapshot>(value) {
                assert!(project.save(&snapshot).is_err(), "{fault}");
            }
            assert!(!project.snapshot_path(valid.input.job_id()).exists());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn frontend_fixture_matches_native_snapshot_and_dimension_schema() {
        let source = include_str!("../../src/contract-fixture.ts");
        let literal = source
            .split_once("// JSON-BEGIN")
            .unwrap()
            .1
            .split_once("// JSON-END")
            .unwrap()
            .0;
        let mut fixture: serde_json::Value = serde_json::from_str(literal).unwrap();
        assert_eq!(
            fixture["dimension_limits"]["minimum"],
            crate::MIN_DIMENSION_MM
        );
        assert_eq!(
            fixture["dimension_limits"]["maximum"],
            crate::MAX_DIMENSION_MM
        );
        let schema = crate::ollama::proposal_schema();
        let dimensions = &schema["anyOf"][0]["properties"]["dimensions_mm"];
        let mut required: Vec<String> =
            serde_json::from_value(dimensions["required"].clone()).unwrap();
        required.sort();
        let keys: Vec<_> = fixture["snapshot"]["input"]["dimensions_mm"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(required, keys);
        for name in keys {
            assert_eq!(
                dimensions["properties"][&name]["minimum"],
                fixture["dimension_limits"]["minimum"]
            );
            assert_eq!(
                dimensions["properties"][&name]["maximum"],
                fixture["dimension_limits"]["maximum"]
            );
        }
        for name in fixture["phases"].as_object().unwrap().keys() {
            let phase: Phase = serde_json::from_value(name.clone().into()).unwrap();
            assert_eq!(serde_json::to_value(phase).unwrap(), *name);
        }
        let root = root();
        let project = Project::create(&root).unwrap();
        fixture["snapshot"]["project_id"] = project.id.to_string().into();
        let snapshot: Snapshot = serde_json::from_value(fixture["snapshot"].clone()).unwrap();
        project.save(&snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(&project.history().unwrap()[0]).unwrap(),
            fixture["snapshot"]
        );
        fixture["compile_snapshot"]["project_id"] = project.id.to_string().into();
        let compile: Snapshot =
            serde_json::from_value(fixture["compile_snapshot"].clone()).unwrap();
        project.save(&compile).unwrap();
        assert_eq!(
            serde_json::to_value(
                project
                    .history()
                    .unwrap()
                    .iter()
                    .find(|run| run.input.job_id() == compile.input.job_id())
                    .unwrap()
            )
            .unwrap(),
            fixture["compile_snapshot"]
        );
        fixture["agent_snapshot"]["project_id"] = project.id.to_string().into();
        let agent: Snapshot = serde_json::from_value(fixture["agent_snapshot"].clone()).unwrap();
        project.save(&agent).unwrap();
        assert_eq!(
            serde_json::to_value(
                project
                    .history()
                    .unwrap()
                    .iter()
                    .find(|run| run.input.job_id() == agent.input.job_id())
                    .unwrap()
            )
            .unwrap(),
            fixture["agent_snapshot"]
        );
        fixture["drawing_snapshot"]["project_id"] = project.id.to_string().into();
        let drawing: Snapshot =
            serde_json::from_value(fixture["drawing_snapshot"].clone()).unwrap();
        project.save(&drawing).unwrap();
        assert_eq!(
            serde_json::to_value(
                project
                    .history()
                    .unwrap()
                    .iter()
                    .find(|run| run.input.job_id() == drawing.input.job_id())
                    .unwrap()
            )
            .unwrap(),
            fixture["drawing_snapshot"]
        );
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_publication_recovery_preserves_imported_original_and_rejects_other_formats() {
        let root = root();
        let project = Project::create(&root).unwrap();
        fs::write(root.join("UserOriginal.ino"), "user original stays intact").unwrap();
        let input = EditInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            agent_version: agent::VERSION.into(),
            source_name: "UserOriginal.ino".into(),
            original_source: "user original stays intact".into(),
            source: "void setup() {} void loop() {}".into(),
        };
        let mut snapshot = project.snapshot(input.clone(), Phase::Running).unwrap();
        let mut wrong = snapshot.clone();
        wrong.schema_version = 3;
        assert!(project.save(&wrong).is_err());
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(input.job_id).unwrap();
        let result = EditResult {
            schema_version: 1,
            job_id: input.job_id,
            agent_version: agent::VERSION.into(),
            protocol_version: "2025-11-25".into(),
            artifacts: ["Original.ino".into(), "Sketch.ino".into()],
        };
        fs::write(stage.path.join("Original.ino"), &input.original_source).unwrap();
        fs::write(stage.path.join("Sketch.ino"), &input.source).unwrap();
        fs::write(
            stage.path.join("input.json"),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        fs::write(
            stage.path.join("result.json"),
            serde_json::to_vec(&result).unwrap(),
        )
        .unwrap();
        project.publish(stage, &snapshot).unwrap();
        drop(project);
        let project = Project::open(&root).unwrap();
        snapshot = project.recover().unwrap().remove(0);
        assert_eq!(snapshot.phase, Phase::Completed);
        assert!(snapshot.audit.unwrap().files_verified);
        assert_eq!(
            project
                .artifact(input.job_id, ArtifactFormat::OriginalSketch)
                .unwrap()
                .read_text()
                .unwrap(),
            input.original_source
        );
        assert_eq!(
            project
                .artifact(input.job_id, ArtifactFormat::Sketch)
                .unwrap()
                .read_text()
                .unwrap(),
            input.source
        );
        assert!(project.artifact(input.job_id, ArtifactFormat::Hex).is_err());
        assert!(project
            .artifact(input.job_id, ArtifactFormat::Fcstd)
            .is_err());
        assert_eq!(
            fs::read_to_string(root.join("UserOriginal.ino")).unwrap(),
            input.original_source
        );
        drop(project);
        fs::remove_dir_all(root).unwrap();
    }
}
