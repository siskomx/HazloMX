//! Capture and relocate KiCad source dependencies before any CLI dispatch.
use crate::{
    kicad::Units,
    project::{pin_directory, pin_lineage},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::MetadataExt, io::FromRawHandle},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ};

pub(crate) const FILE_LIMIT: u64 = 32 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 256 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KicadInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub kicad_version: String,
    pub source_root: String,
    pub entry: String,
    pub files: Vec<CapturedFile>,
    pub units: Units,
    pub layers: Vec<String>,
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub(crate) fn relative(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 1024
        || value.contains(['\\', ':', '"', '\0'])
        || path
            .components()
            .any(|v| !matches!(v, Component::Normal(_)))
    {
        return Err("Ruta relativa de proyecto no válida.".into());
    }
    Ok(path.to_owned())
}
fn slash(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(|v| v.replace('\\', "/"))
        .ok_or("Nombre de proyecto no válido.".into())
}
impl KicadInput {
    pub fn is_board(&self) -> bool {
        self.entry.ends_with(".kicad_pcb")
    }
    pub fn validate(&self) -> Result<(), String> {
        relative(&self.entry)?;
        crate::project::valid_location(Path::new(&self.source_root)).map_err(|e| e.to_string())?;
        if !matches!(self.schema_version, 1 | 2)
            || self.job_id.is_nil()
            || self.kicad_version != crate::kicad::VERSION
            || !(self.entry.ends_with(".kicad_sch") || self.is_board())
            || self.files.is_empty()
            || self.files.len() > 1024
            || !self.files.iter().any(|v| v.path == self.entry)
        {
            return Err("Propuesta de KiCad no compatible.".into());
        }
        let mut paths = BTreeSet::new();
        let mut total = 0;
        for file in &self.files {
            relative(&file.path)?;
            if !paths.insert(file.path.to_ascii_lowercase())
                || !valid_hash(&file.sha256)
                || file.bytes == 0
                || file.bytes > FILE_LIMIT
            {
                return Err("Lista de fuentes no válida.".into());
            }
            total += file.bytes;
        }
        if total > TOTAL_LIMIT
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 256 * 1024
        {
            return Err("El proyecto supera el límite de importación.".into());
        }
        let unique: BTreeSet<_> = self.layers.iter().collect();
        if (self.is_board()
            && (self.layers.is_empty() || !self.layers.iter().any(|v| v == "Edge.Cuts")))
            || (!self.is_board() && !self.layers.is_empty())
            || self.layers.len() > 40
            || unique.len() != self.layers.len()
            || self.layers.iter().any(|v| !export_layer(v))
        {
            return Err("Capas de exportación no compatibles.".into());
        }
        Ok(())
    }
}
fn export_layer(value: &str) -> bool {
    matches!(
        value,
        "F.Cu" | "B.Cu" | "F.Mask" | "B.Mask" | "F.SilkS" | "B.SilkS" | "Edge.Cuts"
    ) || value
        .strip_prefix("In")
        .and_then(|v| v.strip_suffix(".Cu"))
        .and_then(|v| v.parse::<u8>().ok())
        .is_some_and(|v| (1..=30).contains(&v))
}
fn stop(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("Importación de KiCad cancelada.".into())
    } else {
        Ok(())
    }
}
pub(crate) fn open_regular(path: &Path, limit: u64) -> Result<File, String> {
    open_regular_with_empty(path,limit,false)
}
pub(crate) fn open_regular_allow_empty(path:&Path,limit:u64)->Result<File,String>{
    open_regular_with_empty(path,limit,true)
}
fn open_regular_with_empty(path: &Path, limit: u64, allow_empty:bool) -> Result<File, String> {
    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                NtCreateFile, RtlDosPathNameToNtPathName_U_WithStatus, FILE_COMPLETE_IF_OPLOCKED,
                FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
            },
        },
        Win32::{
            Foundation::{
                RtlNtStatusToDosError, OBJ_CASE_INSENSITIVE, STATUS_OPLOCK_BREAK_IN_PROGRESS,
                UNICODE_STRING,
            },
            Storage::FileSystem::{FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ},
            System::{WindowsProgramming::RtlFreeUnicodeString, IO::IO_STATUS_BLOCK},
        },
    };
    let mut dos: Vec<u16> = path.as_os_str().encode_wide().collect();
    if dos.contains(&0) {
        return Err("La ruta contiene un carácter nulo.".into());
    }
    dos.push(0);
    let mut name = UNICODE_STRING::default();
    // SAFETY: the input is terminated UTF-16; the output is freed after the open call.
    let converted = unsafe {
        RtlDosPathNameToNtPathName_U_WithStatus(
            dos.as_ptr(),
            &mut name,
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };
    if converted != 0 {
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(converted) } as i32
        )
        .to_string());
    }
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        ObjectName: &mut name,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut raw = std::ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // Fail an oplock-busy open instead of blocking the caller's cancellation/deadline checks.
    // SAFETY: all pointers remain valid during this synchronous, read-only existing-file open.
    let status = unsafe {
        NtCreateFile(
            &mut raw,
            FILE_GENERIC_READ,
            &attributes,
            &mut status_block,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ,
            FILE_OPEN,
            FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT | FILE_COMPLETE_IF_OPLOCKED,
            std::ptr::null(),
            0,
        )
    };
    // SAFETY: the successful DOS conversion allocated this string with the matching RTL allocator.
    unsafe { RtlFreeUnicodeString(&mut name) };
    if status < 0 {
        return Err(std::io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32
        )
        .to_string());
    }
    if raw.is_null() {
        return Err("Windows no devolvió el archivo solicitado.".into());
    }
    // SAFETY: a successful NtCreateFile returned a newly owned, non-inherited file handle.
    let file = unsafe { File::from_raw_handle(raw) };
    if status == STATUS_OPLOCK_BREAK_IN_PROGRESS {
        return Err("El archivo está ocupado. Cierra su operación pendiente y vuelve a intentarlo; no se publicará ningún cambio.".into());
    }
    if status != 0 {
        return Err("Windows no confirmó la apertura del archivo.".into());
    }
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file()
        || (!allow_empty && meta.len() == 0)
        || meta.len() > limit
        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("Archivo local ausente, enlazado o demasiado grande.".into());
    }
    Ok(file)
}
pub(crate) fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut file = open_regular(path, limit)?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != length {
        return Err("El archivo cambió al leerlo.".into());
    }
    Ok(bytes)
}
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(path.parent().ok_or("Falta el directorio de salida.")?)
        .map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
struct Atom {
    value: String,
    start: usize,
    end: usize,
}
#[derive(Default)]
struct Node {
    head: String,
    atoms: Vec<Atom>,
    parent: String,
}
/// Structural metadata only: KiCad remains responsible for interpreting the design.
fn nodes(bytes: &[u8]) -> Result<Vec<Node>, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "La fuente KiCad no es UTF-8.")?;
    let mut index = 0;
    let mut stack: Vec<Node> = Vec::new();
    let mut result = Vec::new();
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'(' => {
                if stack.len() >= 128 {
                    return Err("La fuente KiCad tiene demasiados niveles.".into());
                }
                stack.push(Node {
                    parent: stack.last().map(|v| v.head.clone()).unwrap_or_default(),
                    ..Node::default()
                });
                index += 1;
            }
            b')' => {
                let node = stack.pop().ok_or("Paréntesis de KiCad no válidos.")?;
                if node.head.is_empty() {
                    return Err("Nodo de KiCad vacío.".into());
                }
                result.push(node);
                index += 1;
            }
            _ => {
                let start = index;
                let value;
                if bytes[index] == b'"' {
                    index += 1;
                    let mut decoded = Vec::new();
                    let mut closed = false;
                    while index < bytes.len() {
                        let byte = bytes[index];
                        index += 1;
                        if byte == b'"' {
                            closed = true;
                            break;
                        }
                        if byte == b'\\' {
                            let escaped = *bytes.get(index).ok_or("Cadena de KiCad incompleta.")?;
                            index += 1;
                            decoded.push(match escaped {
                                b'"' => b'"',
                                b'\\' => b'\\',
                                b'n' => b'\n',
                                b'r' => b'\r',
                                b't' => b'\t',
                                b'b' => 8,
                                b'f' => 12,
                                _ => return Err("Escape de KiCad no compatible.".into()),
                            });
                        } else {
                            decoded.push(byte);
                        }
                    }
                    if !closed {
                        return Err("Cadena de KiCad incompleta.".into());
                    }
                    value = String::from_utf8(decoded).map_err(|_| "Cadena de KiCad no válida.")?;
                } else {
                    while index < bytes.len()
                        && !matches!(bytes[index], b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')')
                    {
                        index += 1;
                    }
                    value = text[start..index].to_owned();
                }
                let node = stack
                    .last_mut()
                    .ok_or("Texto fuera de la estructura de KiCad.")?;
                if node.head.is_empty() {
                    node.head = value;
                } else if node.atoms.len() < 3 {
                    node.atoms.push(Atom {
                        value,
                        start,
                        end: index,
                    });
                }
            }
        }
    }
    if !stack.is_empty() || result.is_empty() {
        return Err("Estructura de KiCad incompleta.".into());
    }
    Ok(result)
}
fn normalize(path: &Path) -> Result<PathBuf, String> {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                if !result.pop() {
                    return Err("La referencia sale del proyecto seleccionado.".into());
                }
            }
            _ => result.push(part),
        }
    }
    Ok(result)
}
fn expand(value: &str, vars: &BTreeMap<String, String>) -> Result<String, String> {
    let mut text = value.to_owned();
    for _ in 0..16 {
        if builtin(&text)? {
            return Ok(text);
        }
        let Some(start) = text.find("${") else {
            return Ok(text);
        };
        let end = text[start + 2..]
            .find('}')
            .ok_or("Variable de KiCad incompleta.")?
            + start
            + 2;
        let key = &text[start + 2..end];
        let replacement = vars
            .get(key)
            .ok_or_else(|| format!("Defina o importe el recurso de KiCad: {key}."))?;
        let replacement = replacement.clone();
        text.replace_range(start..=end, &replacement);
        if text.len() > 8192 {
            return Err("Referencia de KiCad demasiado larga.".into());
        }
    }
    Err("Variables de KiCad cíclicas.".into())
}
fn project_vars(
    bytes: Option<&[u8]>,
    entry_parent: &Path,
) -> Result<BTreeMap<String, String>, String> {
    let mut vars = BTreeMap::new();
    if let Some(bytes) = bytes {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| "Proyecto .kicad_pro no válido.")?;
        if let Some(values) = value.get("text_variables").and_then(|v| v.as_object()) {
            for (key, value) in values {
                if key.len() > 128 || value.as_str().is_none_or(|v| v.len() > 4096) {
                    return Err("Variable del proyecto no válida.".into());
                }
                vars.insert(key.clone(), value.as_str().unwrap().into());
            }
        }
    }
    vars.insert("KIPRJMOD".into(), slash(entry_parent)?);
    Ok(vars)
}
fn builtin(value: &str) -> Result<bool, String> {
    [
        "${KICAD10_SYMBOL_DIR}/",
        "${KICAD10_FOOTPRINT_DIR}/",
        "${KICAD10_3DMODEL_DIR}/",
    ]
    .iter()
    .find_map(|prefix| value.strip_prefix(prefix))
    .map(|suffix| relative(suffix).map(|_| true))
    .unwrap_or(Ok(false))
}
fn reference_nodes(nodes: &[Node]) -> Result<Vec<&Atom>, String> {
    let mut values = Vec::new();
    for node in nodes {
        if node.head == "lib" {
            continue;
        }
        if node.head == "type"
            && node.parent == "lib"
            && node
                .atoms
                .first()
                .is_none_or(|v| !matches!(v.value.as_str(), "KiCad" | "Legacy"))
        {
            return Err(
                "La biblioteca usa un proveedor no admitido para revisiones locales.".into(),
            );
        }
        if node.head == "uri" || node.head == "file" || node.head == "filename" {
            values.push(
                node.atoms
                    .first()
                    .ok_or("Referencia de recurso incompleta.")?,
            );
        } else if node.head == "property"
            && node.parent == "sheet"
            && node.atoms.first().is_some_and(|v| v.value == "Sheetfile")
        {
            values.push(node.atoms.get(1).ok_or("Falta el archivo de la subhoja.")?);
        }
        // Model paths are metadata for these 2D checks/exports; no 3D operation is dispatched.
    }
    Ok(values)
}
fn source_path(
    root: &Path,
    owner: &Path,
    atom: &Atom,
    vars: &BTreeMap<String, String>,
) -> Result<Option<PathBuf>, String> {
    if builtin(&atom.value)? {
        return Ok(None);
    }
    let expanded = expand(&atom.value, vars)?.replace('\\', "/");
    if builtin(&expanded)? {
        return Ok(None);
    }
    if expanded.contains(['\0', '"']) || expanded.contains("://") {
        return Err("El recurso de KiCad no es un archivo local.".into());
    }
    let path = normalize(
        &owner
            .parent()
            .ok_or("Falta el directorio de la fuente.")?
            .join(&expanded),
    )?;
    if !path.starts_with(root) {
        return Err(format!("El recurso está fuera de la carpeta elegida: {}. Seleccione la carpeta que contiene también sus dependencias.",atom.value));
    }
    Ok(Some(path))
}
fn sexpr_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|v| v.to_str()),
        Some("kicad_sch" | "kicad_pcb" | "kicad_sym" | "kicad_mod" | "kicad_dru" | "kicad_wks")
    ) || matches!(
        path.file_name().and_then(|v| v.to_str()),
        Some("sym-lib-table" | "fp-lib-table")
    )
}

fn project_resources(value: &serde_json::Value) -> Vec<Atom> {
    let mut result = Vec::new();
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    if key == "page_layout_descr_file" {
                        if let Some(path) = value.as_str().filter(|v| !v.is_empty()) {
                            result.push(Atom {
                                value: path.into(),
                                start: 0,
                                end: 0,
                            });
                        }
                    } else {
                        pending.push(value);
                    }
                }
            }
            serde_json::Value::Array(values) => pending.extend(values),
            _ => (),
        }
    }
    result
}
fn relocated(within: &Path, owner: &Path) -> Result<String, String> {
    let owner: Vec<_> = owner
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .collect();
    let target: Vec<_> = within.components().collect();
    let common = owner
        .iter()
        .zip(&target)
        .take_while(|(a, b)| a == b)
        .count();
    let mut path = PathBuf::new();
    for _ in common..owner.len() {
        path.push("..");
    }
    for part in &target[common..] {
        path.push(part.as_os_str());
    }
    slash(&path)
}
fn relocate_project(
    value: &mut serde_json::Value,
    root: &Path,
    owner: &Path,
    vars: &BTreeMap<String, String>,
) -> Result<(), String> {
    match value {
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                if key == "page_layout_descr_file" {
                    if let Some(path) = value.as_str().filter(|v| !v.is_empty()) {
                        let atom = Atom {
                            value: path.into(),
                            start: 0,
                            end: 0,
                        };
                        if let Some(path) = source_path(root, &root.join(owner), &atom, vars)? {
                            *value = serde_json::json!(relocated(
                                path.strip_prefix(root)
                                    .map_err(|_| "Recurso fuera del proyecto.")?,
                                owner
                            )?);
                        }
                    }
                } else {
                    relocate_project(value, root, owner, vars)?;
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                relocate_project(value, root, owner, vars)?;
            }
        }
        _ => (),
    }
    Ok(())
}

fn relocate_project_raw(
    raw: &serde_json::value::RawValue,
    root: &Path,
    owner: &Path,
    vars: &BTreeMap<String, String>,
) -> Result<Box<serde_json::value::RawValue>, String> {
    use serde_json::value::{to_raw_value, RawValue};
    match raw.get().as_bytes().first() {
        Some(b'{') => {
            let mut values: BTreeMap<String, Box<RawValue>> =
                serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
            for (key, value) in &mut values {
                if key == "page_layout_descr_file" {
                    if let Ok(path) = serde_json::from_str::<String>(value.get()) {
                        if !path.is_empty() {
                            let atom = Atom {
                                value: path,
                                start: 0,
                                end: 0,
                            };
                            if let Some(path) = source_path(root, &root.join(owner), &atom, vars)? {
                                *value = to_raw_value(&relocated(
                                    path.strip_prefix(root)
                                        .map_err(|_| "Recurso fuera del proyecto.")?,
                                    owner,
                                )?)
                                .map_err(|e| e.to_string())?;
                            }
                        }
                    }
                } else {
                    *value = relocate_project_raw(value, root, owner, vars)?;
                }
            }
            to_raw_value(&values).map_err(|e| e.to_string())
        }
        Some(b'[') => {
            let mut values: Vec<Box<RawValue>> =
                serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
            for value in &mut values {
                *value = relocate_project_raw(value, root, owner, vars)?;
            }
            to_raw_value(&values).map_err(|e| e.to_string())
        }
        _ => Ok(raw.to_owned()),
    }
}

/// Keep all source readers pinned while discovering dependencies and copying the captured bytes.
pub fn capture(
    root: &Path,
    entry: &Path,
    destination: &Path,
    job_id: Uuid,
    units: Units,
    cancel: Arc<AtomicBool>,
) -> Result<KicadInput, String> {
    source(root, entry, Some(destination), job_id, units, cancel,None)
}
pub(crate) fn capture_native_board(root:&Path,entry:&Path,destination:&Path,job_id:Uuid,units:Units,cancel:Arc<AtomicBool>,board:&[u8])->Result<KicadInput,String>{
    if entry.extension().is_none_or(|e|e!="kicad_pcb") || board.is_empty() || board.len() as u64>FILE_LIMIT{return Err("La captura nativa de PCB no es compatible.".into());}
    source(root,entry,Some(destination),job_id,units,cancel,Some(board))
}
pub(crate) fn inspect(
    root: &Path,
    entry: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<KicadInput, String> {
    // This unit is internal metadata only; native geometry/source bytes are never scaled.
    source(root, entry, None, Uuid::new_v4(), Units::Mm, cancel,None)
}
fn source(
    root: &Path,
    entry: &Path,
    destination: Option<&Path>,
    job_id: Uuid,
    units: Units,
    cancel: Arc<AtomicBool>,
    native_board:Option<&[u8]>,
) -> Result<KicadInput, String> {
    stop(&cancel)?;
    let _root_pins = pin_lineage(root).map_err(|e| e.to_string())?;
    let entry = normalize(entry)?;
    if !entry.starts_with(root)
        || !matches!(
            entry.extension().and_then(|v| v.to_str()),
            Some("kicad_sch" | "kicad_pcb")
        )
    {
        return Err("Seleccione una hoja o PCB dentro de la carpeta de fuentes elegida.".into());
    }
    let parent = entry
        .parent()
        .ok_or("Falta la carpeta del proyecto KiCad.")?;
    let pro = entry.with_extension("kicad_pro");
    let _pro_pin = if pro.try_exists().map_err(|e| e.to_string())? {
        Some(open_regular(&pro, FILE_LIMIT)?)
    } else {
        None
    };
    let pro_bytes = _pro_pin
        .as_ref()
        .map(|_| read_regular(&pro, FILE_LIMIT))
        .transpose()?;
    let vars = project_vars(pro_bytes.as_deref(), parent)?;
    let mut pending = vec![entry.clone()];
    if let Some(bytes) = pro_bytes.as_ref() {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| "Proyecto KiCad no válido.")?;
        for atom in project_resources(&value) {
            if let Some(path) = source_path(root, &pro, &atom, &vars)? {
                pending.push(path);
            }
        }
    }
    for path in [
        pro,
        entry.with_extension("kicad_dru"),
        parent.join("sym-lib-table"),
        parent.join("fp-lib-table"),
        entry.with_extension(if entry.extension().is_some_and(|v| v == "kicad_sch") {
            "kicad_pcb"
        } else {
            "kicad_sch"
        }),
    ] {
        if path.try_exists().map_err(|e| e.to_string())? {
            pending.push(path);
        }
    }
    let mut bytes = BTreeMap::new();
    let mut pins = Vec::new();
    let mut dirs = BTreeSet::new();
    let mut total = 0;
    let mut layers = Vec::new();
    while let Some(path) = pending.pop() {
        stop(&cancel)?;
        if path.is_dir() {
            if dirs.insert(path.clone()) {
                pins.push(pin_directory(&path, false).map_err(|e| e.to_string())?);
                let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;
                for item in entries {
                    pending.push(item.map_err(|e| e.to_string())?.path());
                    if pending.len() > 8192 {
                        return Err("La biblioteca tiene demasiados archivos.".into());
                    }
                }
            }
            continue;
        }
        let name = slash(
            path.strip_prefix(root)
                .map_err(|_| "El recurso sale de la carpeta elegida.")?,
        )?;
        relative(&name)?;
        if bytes.contains_key(&name) {
            continue;
        }
        if bytes.len() >= 1024 {
            return Err("El proyecto tiene demasiadas dependencias.".into());
        }
        pins.extend(
            pin_lineage(
                path.parent()
                    .ok_or("Falta el directorio de la dependencia.")?,
            )
            .map_err(|e| e.to_string())?,
        );
        let data=if let Some(native)=native_board.filter(|_|path==entry){native.to_vec()}else{
            pins.push(open_regular(&path, FILE_LIMIT)?);
            read_regular(&path, FILE_LIMIT)?
        };
        total += data.len() as u64;
        if total > TOTAL_LIMIT {
            return Err("Las fuentes superan 256 MiB.".into());
        }
        if sexpr_file(&path) {
            let metadata = nodes(&data)?;
            for atom in reference_nodes(&metadata)? {
                if let Some(dependency) = source_path(root, &path, atom, &vars)? {
                    pending.push(dependency);
                }
            }
            if path == entry && entry.extension().is_some_and(|v| v == "kicad_pcb") {
                for node in &metadata {
                    if node.parent == "layers" {
                        if let Some(atom) = node.atoms.first() {
                            if export_layer(&atom.value) {
                                layers.push(atom.value.clone());
                            }
                        }
                    }
                }
            }
        }
        bytes.insert(name, data);
    }
    let input = KicadInput {
        schema_version: 2,
        job_id,
        kicad_version: crate::kicad::VERSION.into(),
        source_root: slash(root)?,
        entry: slash(entry.strip_prefix(root).unwrap())?,
        files: bytes
            .iter()
            .map(|(name, data)| CapturedFile {
                path: name.clone(),
                sha256: digest(data),
                bytes: data.len() as u64,
            })
            .collect(),
        units,
        layers,
    };
    input.validate()?;
    if let Some(destination) = destination {
        fs::create_dir(destination).map_err(|e| e.to_string())?;
        for (name, data) in &bytes {
            stop(&cancel)?;
            write_new(&destination.join(relative(name)?), data)?;
        }
    }
    Ok(input)
}
pub fn validate_capture(directory: &Path, input: &KicadInput) -> Result<(), String> {
    input.validate()?;
    for file in &input.files {
        let bytes = read_regular(&directory.join(relative(&file.path)?), FILE_LIMIT)?;
        if bytes.len() as u64 != file.bytes || digest(&bytes) != file.sha256 {
            return Err("La fuente importada cambió; prepare otra propuesta.".into());
        }
    }
    Ok(())
}
/// Relocate only resolved dependency atoms; preserved Original files retain their exact bytes.
pub fn working_copy(
    original: &Path,
    working: &Path,
    input: &KicadInput,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut created = false;
    portable_files(original, input, cancel, |rel, bytes| {
        if !created {
            fs::create_dir(working).map_err(|e| e.to_string())?;
            created = true;
        }
        write_new(&working.join(rel), bytes)
    })
}

pub(crate) fn portable_files(
    original: &Path,
    input: &KicadInput,
    cancel: &AtomicBool,
    mut consume: impl FnMut(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    validate_capture(original, input)?;
    let source_root = Path::new(&input.source_root);
    let source_entry = source_root.join(relative(&input.entry)?);
    let pro_path = relative(&input.entry)?.with_extension("kicad_pro");
    let pro_bytes = if input.files.iter().any(|f| Path::new(&f.path) == pro_path) {
        Some(read_regular(&original.join(&pro_path), FILE_LIMIT)?)
    } else {
        None
    };
    let vars = project_vars(pro_bytes.as_deref(), source_entry.parent().unwrap())?;
    for file in &input.files {
        stop(cancel)?;
        let rel = relative(&file.path)?;
        let mut bytes = read_regular(&original.join(&rel), FILE_LIMIT)?;
        if sexpr_file(&rel) {
            let metadata = nodes(&bytes)?;
            let mut changes = Vec::new();
            for atom in reference_nodes(&metadata)? {
                if let Some(resolved) =
                    source_path(source_root, &source_root.join(&rel), atom, &vars)?
                {
                    let within = resolved
                        .strip_prefix(source_root)
                        .map_err(|_| "Recurso fuera de las fuentes.")?;
                    if !input.files.iter().any(|v| Path::new(&v.path) == within) {
                        // Directory libraries are represented by their captured descendants.
                        if !input
                            .files
                            .iter()
                            .any(|v| Path::new(&v.path).starts_with(within))
                        {
                            return Err("Falta una dependencia en la captura aprobada.".into());
                        }
                    }
                    let relocated = relocated(within, &rel)?;
                    changes.push((
                        atom.start,
                        atom.end,
                        serde_json::to_string(&relocated).map_err(|e| e.to_string())?,
                    ));
                }
            }
            changes.sort_by_key(|v| v.0);
            for (start, end, value) in changes.into_iter().rev() {
                bytes.splice(start..end, value.bytes());
            }
        } else if rel.extension().is_some_and(|v| v == "kicad_pro") {
            let mut value: serde_json::Value =
                serde_json::from_slice(&bytes).map_err(|_| "Proyecto KiCad no válido.")?;
            if input.schema_version == 1 {
                // Keep the original serializer for validation of already-published copies.
                relocate_project(&mut value, source_root, &rel, &vars)?;
                bytes = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
            } else {
                // Value above retains the existing JSON/depth validation; raw leaves preserve numbers.
                let raw: Box<serde_json::value::RawValue> =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                bytes = relocate_project_raw(&raw, source_root, &rel, &vars)?
                    .get()
                    .as_bytes()
                    .to_vec();
            }
        }
        consume(&rel, &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_board_capture_uses_unsaved_bytes_and_preserves_saved_dependencies(){
        let temp=root();let source=temp.join("sources");fs::create_dir(&source).unwrap();
        let entry=source.join("Board.kicad_pcb");let disk=b"saved board must never substitute native state";fs::write(&entry,disk).unwrap();
        fs::write(source.join("Board.kicad_pro"),br#"{"text_variables":{"OWNER":"preserved"}}"#).unwrap();
        let native=br#"(kicad_pcb (layers (0 "F.Cu" signal) (44 "Edge.Cuts" user)) (gr_text "unsaved user note"))"#;
        let destination=temp.join("capture");let input=capture_native_board(&source,&entry,&destination,Uuid::new_v4(),Units::Mm,Arc::new(AtomicBool::new(false)),native).unwrap();
        assert_eq!(read_regular(&destination.join("Board.kicad_pcb"),FILE_LIMIT).unwrap(),native);
        assert_eq!(fs::read(&entry).unwrap(),disk);assert_eq!(input.files.len(),2);
        validate_capture(&destination,&input).unwrap();
        fs::write(destination.join("Board.kicad_pcb"),disk).unwrap();assert!(validate_capture(&destination,&input).is_err());
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn native_capture_refuses_unapproved_external_dependencies_without_saved_fallback(){
        let temp=root();let entry=temp.join("Board.kicad_pcb");fs::write(&entry,b"saved").unwrap();
        let native=br#"(kicad_pcb (layers (44 "Edge.Cuts" user)) (image (file "../outside.png")))"#;
        let destination=temp.join("capture");
        assert!(capture_native_board(&temp,&entry,&destination,Uuid::new_v4(),Units::Mm,Arc::new(AtomicBool::new(false)),native).is_err());
        assert!(!destination.exists());assert_eq!(fs::read(&entry).unwrap(),b"saved");fs::remove_dir_all(temp).unwrap();
    }
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("hazlomexa-kicad-capture-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn regular_read_preserves_unicode_limits_and_write_exclusion() {
        let root = root();
        let path = root.join("caja-é Ω.kicad_pcb");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(read_regular(&path, 3).unwrap(), b"abc");
        assert!(read_regular(&path, 2).is_err());
        assert!(read_regular(&root, 3).is_err());
        assert!(read_regular(&root.join("missing"), 3).is_err());
        assert!(read_regular(&PathBuf::from(format!("{}\0ignored", path.display())), 3).is_err());
        let held = open_regular(&path, 3).unwrap();
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        drop(held);
        assert!(OpenOptions::new().write(true).open(&path).is_ok());
        fs::write(&path, b"").unwrap();
        assert!(read_regular(&path, 3).is_err());
        assert!(root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn captures_hierarchy_rules_libraries_and_relocates_only_dependency_atoms() {
        let temp = root();
        let source = temp.join("sources");
        fs::create_dir(&source).unwrap();
        fs::create_dir(source.join("sheets")).unwrap();
        fs::create_dir(source.join("lib.pretty")).unwrap();
        fs::write(source.join("main.kicad_sch"),r#"(kicad_sch (version 20220914) (sheet (property "Sheetfile" "sheets/child.kicad_sch")) (text "Sheetfile sheets/not-a-dependency.kicad_sch"))"#).unwrap();
        fs::write(
            source.join("sheets/child.kicad_sch"),
            r#"(kicad_sch (version 20220914) (sheet (property "Sheetfile" "../third.kicad_sch")))"#,
        )
        .unwrap();
        fs::write(
            source.join("third.kicad_sch"),
            "(kicad_sch (version 20220914))",
        )
        .unwrap();
        fs::write(
            source.join("main.kicad_pro"),
            r#"{"text_variables":{"LIB":"${KIPRJMOD}/lib.pretty"}}"#,
        )
        .unwrap();
        fs::write(
            source.join("main.kicad_dru"),
            "# Custom rule\n(version 1)\n(rule \"My rule\" (constraint clearance (min 0.2)))",
        )
        .unwrap();
        fs::write(
            source.join("fp-lib-table"),
            r#"(fp_lib_table (lib (name "Local") (type "KiCad") (uri "${LIB}")))"#,
        )
        .unwrap();
        fs::write(
            source.join("lib.pretty/part.kicad_mod"),
            "(footprint \"part\")",
        )
        .unwrap();
        let imported = temp.join("capture");
        let input = capture(
            &source,
            &source.join("main.kicad_sch"),
            &imported,
            Uuid::new_v4(),
            Units::In,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(input.files.len(), 7);
        assert!(!input
            .files
            .iter()
            .any(|v| v.path.contains("not-a-dependency")));
        let working = temp.join("working");
        working_copy(&imported, &working, &input, &AtomicBool::new(false)).unwrap();
        let top = fs::read_to_string(working.join("main.kicad_sch")).unwrap();
        assert!(top.contains("\"sheets/child.kicad_sch\""));
        assert!(top.contains("Sheetfile sheets/not-a-dependency.kicad_sch"));
        assert_eq!(
            fs::read(imported.join("main.kicad_sch")).unwrap(),
            fs::read(source.join("main.kicad_sch")).unwrap()
        );
        fs::write(imported.join("third.kicad_sch"), b"tampered").unwrap();
        assert!(validate_capture(&imported, &input).is_err());
    }
    #[test]
    fn captures_absolute_local_resources_and_worksheets_without_footprint_sheet_metadata() {
        let temp = root();
        let source = temp.join("sources");
        fs::create_dir(&source).unwrap();
        let worksheet = source.join("custom.kicad_wks");
        fs::write(&worksheet, "(kicad_wks)").unwrap();
        fs::write(source.join("local.kicad_sym"), "(kicad_symbol_lib)").unwrap();
        fs::write(
            source.join("main.kicad_pro"),
            serde_json::to_vec(&serde_json::json!({
                "schematic":{"page_layout_descr_file":slash(&worksheet).unwrap()},
                "text_variables":{"LOCAL":slash(&source.join("local.kicad_sym")).unwrap()}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            source.join("sym-lib-table"),
            r#"(sym_lib_table (lib (name "Local") (type "KiCad") (uri "${LOCAL}")))"#,
        )
        .unwrap();
        fs::write(source.join("main.kicad_sch"), "(kicad_sch)").unwrap();
        fs::write(source.join("main.kicad_pcb"), r#"(kicad_pcb (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user)) (footprint "part" (property "Sheetfile" "old-missing.kicad_sch")))"#).unwrap();
        let imported = temp.join("capture");
        let input = capture(
            &source,
            &source.join("main.kicad_pcb"),
            &imported,
            Uuid::new_v4(),
            Units::Mils,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(input.files.len(), 6);
        let working = temp.join("working");
        working_copy(&imported, &working, &input, &AtomicBool::new(false)).unwrap();
        let pro: serde_json::Value =
            serde_json::from_slice(&fs::read(working.join("main.kicad_pro")).unwrap()).unwrap();
        assert_eq!(
            pro["schematic"]["page_layout_descr_file"],
            "custom.kicad_wks"
        );
        assert!(fs::read_to_string(working.join("sym-lib-table"))
            .unwrap()
            .contains("\"local.kicad_sym\""));
        assert!(fs::read_to_string(working.join("main.kicad_pcb"))
            .unwrap()
            .contains("old-missing.kicad_sch"));
        assert_eq!(
            fs::read(imported.join("main.kicad_pro")).unwrap(),
            fs::read(source.join("main.kicad_pro")).unwrap()
        );
    }
    #[test]
    fn working_project_preserves_number_literals_and_legacy_copy_format() {
        let temp = root();
        let source = temp.join("sources");
        fs::create_dir(&source).unwrap();
        let worksheet = source.join("custom.kicad_wks");
        fs::write(&worksheet, "(kicad_wks)").unwrap();
        fs::write(source.join("main.kicad_sch"), "(kicad_sch)").unwrap();
        let path = serde_json::to_string(&slash(&worksheet).unwrap()).unwrap();
        let settings = format!(
            r#"{{"board":{{"min":0.19999999999999998}},"schematic":{{"page_layout_descr_file":{path}}},"nested":[{{"page_layout_descr_file":{path},"width":0.049999999999999996}}],"scale":1.234567890123456789e-6,"id":9007199254740993,"label":"0.19999999999999998","enabled":true,"empty":null}}"#
        );
        fs::write(source.join("main.kicad_pro"), &settings).unwrap();
        let captured = temp.join("capture");
        let input = capture(
            &source,
            &source.join("main.kicad_sch"),
            &captured,
            Uuid::new_v4(),
            Units::Mm,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let working = temp.join("working");
        working_copy(&captured, &working, &input, &AtomicBool::new(false)).unwrap();
        let output = fs::read_to_string(working.join("main.kicad_pro")).unwrap();
        use serde_json::value::RawValue;
        let fields: BTreeMap<String, &RawValue> = serde_json::from_str(&output).unwrap();
        let board: BTreeMap<String, &RawValue> =
            serde_json::from_str(fields["board"].get()).unwrap();
        let nested: Vec<BTreeMap<String, &RawValue>> =
            serde_json::from_str(fields["nested"].get()).unwrap();
        assert_eq!(board["min"].get(), "0.19999999999999998");
        assert_eq!(nested[0]["width"].get(), "0.049999999999999996");
        assert_eq!(fields["scale"].get(), "1.234567890123456789e-6");
        assert_eq!(fields["id"].get(), "9007199254740993");
        assert_eq!(fields["label"].get(), "\"0.19999999999999998\"");
        assert_eq!(fields["enabled"].get(), "true");
        assert_eq!(fields["empty"].get(), "null");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            parsed["schematic"]["page_layout_descr_file"],
            "custom.kicad_wks"
        );
        assert_eq!(
            parsed["nested"][0]["page_layout_descr_file"],
            "custom.kicad_wks"
        );
        assert_eq!(
            fs::read_to_string(captured.join("main.kicad_pro")).unwrap(),
            settings
        );
        assert_eq!(input.schema_version, 2);
        let mut unsupported = input.clone();
        unsupported.schema_version = 3;
        assert!(unsupported.validate().is_err());
        let mut legacy = input;
        legacy.schema_version = 1;
        let working = temp.join("legacy");
        working_copy(&captured, &working, &legacy, &AtomicBool::new(false)).unwrap();
        let output = fs::read_to_string(working.join("main.kicad_pro")).unwrap();
        assert!(output.contains("\"min\": 0.2"));
    }
    #[test]
    fn rejects_missing_outside_remote_and_builtin_escape_dependencies() {
        let temp = root();
        let source = temp.join("sources");
        fs::create_dir(&source).unwrap();
        for (index, value) in [
            "missing.kicad_sch",
            "../outside.kicad_sch",
            "https://example.com/remote.kicad_sch",
            "${KICAD10_SYMBOL_DIR}/../outside.kicad_sym",
        ]
        .iter()
        .enumerate()
        {
            let text = format!("(kicad_sch (sheet (property \"Sheetfile\" \"{value}\")))");
            fs::write(source.join("main.kicad_sch"), text).unwrap();
            assert!(capture(
                &source,
                &source.join("main.kicad_sch"),
                &temp.join(format!("capture-{index}")),
                Uuid::new_v4(),
                Units::Mm,
                Arc::new(AtomicBool::new(false))
            )
            .is_err());
        }
        for path in [
            "../secret",
            "C:/secret",
            "/absolute",
            "a/../../secret",
            "a\\b",
        ] {
            assert!(relative(path).is_err());
        }
        assert!(nodes(b"(kicad_sch (sheet)").is_err());
        assert!(nodes(b"(kicad_sch (text \"unfinished))").is_err());
        assert!(nodes(b"(a \"escape\\q\")").is_err());
    }
    #[test]
    fn cancelled_capture_does_not_create_a_source_copy() {
        let temp = root();
        let entry = temp.join("main.kicad_sch");
        fs::write(&entry, "(kicad_sch)").unwrap();
        let output = temp.join("capture");
        assert!(capture(
            &temp,
            &entry,
            &output,
            Uuid::new_v4(),
            Units::Mm,
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(!output.exists());
    }
}
