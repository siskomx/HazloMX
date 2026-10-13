//! Captured STL and explicitly selected profile inheritance for approved file slicing.
use crate::kicad_project::{self as files, CapturedFile};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

pub const VERSION: &str = "2.4.2";
pub const MODEL_LIMIT: u64 = 32 * 1024 * 1024;
pub(crate) fn matches_stored<T: Serialize + serde::de::DeserializeOwned + PartialEq>(
    actual: &T,
    stored: &T,
) -> Result<bool, String> {
    if actual == stored {
        return Ok(true);
    }
    // Accept only the representation produced by our own JSON writer/parser, without a numeric tolerance.
    let persisted: T =
        serde_json::from_slice(&serde_json::to_vec(actual).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    Ok(&persisted == stored)
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
    Cm,
    M,
    In,
    Ft,
}
impl Units {
    pub fn scale(self) -> f64 {
        match self {
            Self::Mm => 1.,
            Self::Cm => 10.,
            Self::M => 1000.,
            Self::In => 25.4,
            Self::Ft => 304.8,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bed {
    Cool,
    Textured,
    Smooth,
}
impl Bed {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cool => "Cool Plate",
            Self::Textured => "Textured PEI Plate",
            Self::Smooth => "High Temp Plate",
        }
    }
    pub fn temperature_key(self) -> &'static str {
        match self {
            Self::Cool => "cool_plate_temp_initial_layer",
            Self::Textured => "textured_plate_temp_initial_layer",
            Self::Smooth => "hot_plate_temp_initial_layer",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub units: Units,
    pub bed: Bed,
    pub bed_temperature_c: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    pub triangles: u32,
    pub bounds_mm: [[f64; 3]; 2],
    pub shape_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub machine: String,
    pub process: String,
    pub filament: String,
    pub bed: String,
    pub nozzle_mm: f64,
    pub layer_mm: f64,
    pub nozzle_c: f64,
    pub bed_c: f64,
    pub filament_mm: f64,
    pub bed_area_mm: Vec<[f64; 2]>,
    pub printable_height_mm: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliceInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub orca_version: String,
    pub source_name: String,
    pub units: Units,
    pub bed: Bed,
    pub mesh: Mesh,
    pub settings: Settings,
    pub files: Vec<CapturedFile>,
}
impl SliceInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.job_id.is_nil()
            || self.orca_version != VERSION
            || self.source_name.len() > 255
            || !self.source_name.to_ascii_lowercase().ends_with(".stl")
            || self.source_name.contains(['/', '\\', ':', '"', '\0'])
            || self.source_name.chars().any(char::is_control)
            || self.files.is_empty()
            || self.files.len() > 64
        {
            return Err("Propuesta de laminado no compatible.".into());
        }
        let mut paths = BTreeSet::new();
        let mut total = 0;
        for f in &self.files {
            files::relative(&f.path)?;
            if !paths.insert(f.path.to_ascii_lowercase())
                || !files::valid_hash(&f.sha256)
                || f.bytes == 0
                || f.bytes > MODEL_LIMIT
            {
                return Err("Fuentes de laminado no válidas.".into());
            }
            total += f.bytes;
        }
        if total > 64 * 1024 * 1024
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 256 * 1024
            || ![
                "Original.stl",
                "Machine.json",
                "Process.json",
                "Filament.json",
            ]
            .iter()
            .all(|p| paths.contains(&p.to_ascii_lowercase()))
        {
            return Err("Faltan fuentes o el laminado supera el límite.".into());
        }
        if self.mesh.triangles == 0
            || self.mesh.triangles > 600_000
            || self
                .mesh
                .bounds_mm
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 100_000.)
            || self.settings.bed != self.bed.label()
            || !files::valid_hash(&self.mesh.shape_sha256)
            || (0..3).any(|i| self.mesh.bounds_mm[1][i] <= self.mesh.bounds_mm[0][i])
            || [
                &self.settings.machine,
                &self.settings.process,
                &self.settings.filament,
            ]
            .iter()
            .any(|name| name.is_empty() || name.len() > 255 || name.chars().any(char::is_control))
            || !self.settings.nozzle_mm.is_finite()
            || !(0.1..=2.).contains(&self.settings.nozzle_mm)
            || !self.settings.layer_mm.is_finite()
            || !(0.02..=self.settings.nozzle_mm).contains(&self.settings.layer_mm)
            || !self.settings.nozzle_c.is_finite()
            || !(0.0..=500.).contains(&self.settings.nozzle_c)
            || !self.settings.bed_c.is_finite()
            || !(0.0..=200.).contains(&self.settings.bed_c)
            || !self.settings.filament_mm.is_finite()
            || !(1.0..=4.).contains(&self.settings.filament_mm)
            || !self.settings.printable_height_mm.is_finite()
            || !(1.0..=10_000.).contains(&self.settings.printable_height_mm)
            || !(3..=256).contains(&self.settings.bed_area_mm.len())
            || self
                .settings
                .bed_area_mm
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 10_000.)
        {
            return Err("Geometría o cama de laminado no válida.".into());
        }
        Ok(())
    }
}
pub(crate) fn stop(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("Laminado cancelado.".into())
    } else {
        Ok(())
    }
}
pub(crate) struct Runtime {
    pub root: PathBuf,
    _pins: Vec<File>,
}
pub(crate) fn runtime(executable: &Path, cancel: &AtomicBool) -> Result<Runtime, String> {
    if !executable
        .file_name()
        .is_some_and(|v| v.eq_ignore_ascii_case("orca-slicer.exe"))
    {
        return Err("Seleccione orca-slicer.exe oficial Windows x64 2.4.2.".into());
    }
    let root = executable
        .parent()
        .ok_or("Falta la carpeta de OrcaSlicer.")?
        .to_owned();
    let mut pins = crate::project::pin_lineage(&root).map_err(|e| e.to_string())?;
    let manifest: BTreeMap<String, String> = serde_json::from_str(include_str!(
        "../../../../docs/research/orca-runtime-2026-10-11/runtime-files.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1024 * 1024];
    for (name, expected) in manifest {
        let mut file = files::open_regular(&root.join(files::relative(&name)?), 512 * 1024 * 1024)?;
        let mut hash = Sha256::new();
        loop {
            stop(cancel)?;
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != expected {
            return Err("OrcaSlicer no coincide con el paquete identificado 2.4.2.".into());
        }
        pins.push(file);
    }
    pins.extend(
        crate::project::pin_lineage(&root.join("resources/profiles")).map_err(|e| e.to_string())?,
    );
    Ok(Runtime { root, _pins: pins })
}
pub fn verify_runtime(executable: &Path) -> Result<(), String> {
    runtime(executable, &AtomicBool::new(false)).map(|_| ())
}

fn number(map: &Map<String, Value>, key: &str, vector: bool) -> Result<f64, String> {
    let v = map
        .get(key)
        .ok_or_else(|| format!("Falta {key} en el perfil."))?;
    let v = if vector {
        let a = v.as_array().ok_or("Valor de perfil no compatible.")?;
        if a.len() != 1 {
            return Err("Seleccione perfiles de un solo extrusor/material.".into());
        }
        &a[0]
    } else {
        v
    };
    let n = v
        .as_str()
        .and_then(|v| v.parse::<f64>().ok())
        .or_else(|| v.as_f64())
        .ok_or_else(|| format!("Valor de {key} no válido."))?;
    if !n.is_finite() {
        return Err("El perfil contiene un número no finito.".into());
    }
    Ok(n)
}
fn name(map: &Map<String, Value>) -> Result<String, String> {
    let s = map
        .get("name")
        .and_then(Value::as_str)
        .ok_or("Falta el nombre del perfil.")?;
    if s.is_empty() || s.len() > 255 || s.chars().any(char::is_control) {
        return Err("Nombre de perfil no válido.".into());
    }
    Ok(s.into())
}
fn safe_profile(map: &Map<String, Value>) -> Result<(), String> {
    for key in [
        "post_process",
        "print_host",
        "printhost_apikey",
        "printhost_cafile",
        "device_id",
    ] {
        if let Some(v) = map.get(key) {
            let empty = v.is_null()
                || v.as_str() == Some("")
                || v.as_array()
                    .is_some_and(|a| a.iter().all(|v| v.as_str() == Some("")));
            if !empty {
                return Err(format!("El perfil contiene {key}; este flujo solo lamina archivos sin scripts ni conexiones."));
            }
        }
    }
    Ok(())
}
pub(crate) fn settings(
    machine: &Map<String, Value>,
    process: &Map<String, Value>,
    filament: &Map<String, Value>,
    bed: Bed,
) -> Result<Settings, String> {
    for m in [machine, process, filament] {
        safe_profile(m)?;
    }
    let machine_name = name(machine)?;
    for (kind, p) in [("proceso", process), ("material", filament)] {
        if let Some(value) = p.get("compatible_printers") {
            let a = value.as_array().ok_or_else(|| format!("El perfil de {kind} tiene una lista de impresoras no válida. Selecciona un perfil compatible con «{machine_name}»."))?;
            if a.iter().any(|v| {
                v.as_str().is_none_or(|s| {
                    s.is_empty() || s.len() > 255 || s.chars().any(char::is_control)
                })
            }) {
                return Err(format!("El perfil de {kind} contiene nombres de impresora no válidos. Selecciona un perfil compatible con «{machine_name}»."));
            }
            if !a.is_empty() && !a.iter().any(|v| v.as_str() == Some(&machine_name)) {
                return Err(format!("El perfil de {kind} no declara compatibilidad con «{machine_name}». Selecciona un perfil de {kind} para esa impresora y boquilla."));
            }
        }
        if let Some(value) = p.get("compatible_printers_condition") {
            let condition = value.as_str().ok_or_else(|| format!("El perfil de {kind} tiene una condición de compatibilidad no válida. Selecciona otro perfil para «{machine_name}»."))?;
            if !condition.is_empty() {
                return Err(format!("El perfil de {kind} usa una condición de compatibilidad que Hazlo todavía no verifica. Selecciona un perfil que declare explícitamente «{machine_name}»."));
            }
        }
    }
    let nozzle = number(machine, "nozzle_diameter", true)?;
    let layer = number(process, "layer_height", false)?;
    let nozzle_c = number(filament, "nozzle_temperature_initial_layer", true)?;
    let bed_c = number(filament, bed.temperature_key(), true)?;
    let diameter = number(filament, "filament_diameter", true)?;
    let height = number(machine, "printable_height", false)?;
    if !(0.1..=2.).contains(&nozzle)
        || !(0.02..=nozzle).contains(&layer)
        || !(0.0..=500.).contains(&nozzle_c)
        || !(0.0..=200.).contains(&bed_c)
        || !(1.0..=4.).contains(&diameter)
        || !(1.0..=10_000.).contains(&height)
    {
        return Err("Parámetros físicos fuera del límite de laminado.".into());
    }
    let area = machine
        .get("printable_area")
        .and_then(Value::as_array)
        .ok_or("Falta el área imprimible.")?;
    let points = area
        .iter()
        .map(|v| {
            let s = v.as_str().ok_or("Área imprimible no válida.")?;
            let (x, y) = s.split_once('x').ok_or("Área imprimible no válida.")?;
            let p = [
                x.parse::<f64>().map_err(|_| "Área imprimible no válida.")?,
                y.parse::<f64>().map_err(|_| "Área imprimible no válida.")?,
            ];
            if p.iter().any(|v| !v.is_finite() || v.abs() > 10_000.) {
                return Err("Área imprimible no válida.");
            }
            Ok(p)
        })
        .collect::<Result<Vec<_>, &str>>()?;
    if !(3..=256).contains(&points.len()) {
        return Err("Área imprimible no compatible.".into());
    }
    Ok(Settings {
        machine: machine_name,
        process: name(process)?,
        filament: name(filament)?,
        bed: bed.label().into(),
        nozzle_mm: nozzle,
        layer_mm: layer,
        nozzle_c,
        bed_c,
        filament_mm: diameter,
        bed_area_mm: points,
        printable_height_mm: height,
    })
}
fn profile(
    root: &Path,
    path: &Path,
    output: &Path,
    captured: &mut BTreeMap<String, CapturedFile>,
    pins: &mut Vec<File>,
    seen: &mut BTreeSet<PathBuf>,
    cancel: &AtomicBool,
) -> Result<Map<String, Value>, String> {
    stop(cancel)?;
    let kind = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        .filter(|v| matches!(*v, "machine" | "process" | "filament"))
        .ok_or("Carpeta de perfil no válida.")?;
    if path.parent() != Some(root.join(kind).as_path())
        || !seen.insert(path.to_owned())
        || seen.len() > 16
    {
        return Err("Perfil fuera de la carpeta seleccionada o herencia circular.".into());
    }
    pins.extend(
        crate::project::pin_lineage(path.parent().ok_or("Falta la carpeta del perfil.")?)
            .map_err(|e| e.to_string())?,
    );
    let mut file = files::open_regular(path, 1024 * 1024)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    pins.push(file);
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "Perfil JSON no válido.")?;
    let map = v.as_object().ok_or("Perfil JSON no válido.")?;
    if map.get("type").and_then(Value::as_str) != Some(kind) {
        return Err("Tipo de perfil no compatible.".into());
    }
    name(map)?;
    safe_profile(map)?;
    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Nombre de perfil no válido.")?;
    let rel = format!("ProfileSources/{kind}/{file_name}");
    files::relative(&rel)?;
    if !captured.contains_key(&rel) {
        files::write_new(&output.join(&rel), &bytes)?;
        captured.insert(
            rel.clone(),
            CapturedFile {
                path: rel,
                sha256: files::digest(&bytes),
                bytes: bytes.len() as u64,
            },
        );
    }
    let inherited = if let Some(parent) = map
        .get("inherits")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        if parent.len() > 255 || parent.contains(['/', '\\', ':', '"', '\0']) {
            return Err("Referencia de perfil no válida.".into());
        }
        profile(
            root,
            &root.join(kind).join(format!("{parent}.json")),
            output,
            captured,
            pins,
            seen,
            cancel,
        )?
    } else {
        Map::new()
    };
    let mut merged = inherited;
    merged.extend(map.clone());
    merged.insert("inherits".into(), Value::String(String::new()));
    Ok(merged)
}

pub(crate) fn mesh(bytes: &[u8], units: Units) -> Result<(Mesh, Vec<u8>), String> {
    let mut vertices = Vec::<[f64; 3]>::new();
    let binary = bytes.len() >= 84 && {
        let n = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        n.checked_mul(50).and_then(|v| v.checked_add(84)) == Some(bytes.len())
    };
    if binary {
        for facet in bytes[84..].chunks_exact(50) {
            if facet[48..50] != [0, 0] {
                return Err("STL con atributos no compatibles.".into());
            }
            for v in facet[12..48].chunks_exact(12) {
                vertices.push(std::array::from_fn(|i| {
                    f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap()) as f64
                }));
            }
        }
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| "STL ASCII/binario no válido.")?;
        if !text.trim_start().starts_with("solid") || !text.trim_end().contains("endsolid") {
            return Err("Seleccione un STL ASCII o binario válido.".into());
        }
        let lines: Vec<_> = text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if lines.len() < 9
            || lines[0].split_whitespace().next() != Some("solid")
            || lines.last().unwrap().split_whitespace().next() != Some("endsolid")
            || !(lines.len() - 2).is_multiple_of(7)
        {
            return Err("Estructura STL ASCII no válida.".into());
        }
        for facet in lines[1..lines.len() - 1].chunks_exact(7) {
            let normal: Vec<_> = facet[0].split_whitespace().collect();
            if normal.len() != 5
                || normal[..2] != ["facet", "normal"]
                || normal[2..]
                    .iter()
                    .any(|v| v.parse::<f64>().ok().is_none_or(|v| !v.is_finite()))
                || facet[1] != "outer loop"
                || facet[5] != "endloop"
                || facet[6] != "endfacet"
            {
                return Err("Faceta STL ASCII no válida.".into());
            }
            for line in &facet[2..5] {
                let tokens: Vec<_> = line.split_whitespace().collect();
                if tokens.first() == Some(&"vertex") {
                    if tokens.len() != 4 {
                        return Err("Vértice STL no válido.".into());
                    }
                    let mut p = [0.; 3];
                    for i in 0..3 {
                        p[i] = tokens[i + 1]
                            .parse()
                            .map_err(|_| "Vértice STL no válido.")?;
                    }
                    vertices.push(p);
                } else {
                    return Err("Vértice STL ASCII no válido.".into());
                }
            }
        }
    }
    if vertices.is_empty() || !vertices.len().is_multiple_of(3) || vertices.len() > 1_800_000 {
        return Err("El STL no contiene una malla dentro del límite.".into());
    }
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    let mut normalized_points = Vec::with_capacity(vertices.len());
    let mut normalized = b"Hazlo Mexa approved millimetres".to_vec();
    normalized.resize(80, 0);
    normalized.extend_from_slice(&((vertices.len() / 3) as u32).to_le_bytes());
    for triangle in vertices.chunks_exact(3) {
        let u: [f64; 3] = std::array::from_fn(|i| triangle[1][i] - triangle[0][i]);
        let v: [f64; 3] = std::array::from_fn(|i| triangle[2][i] - triangle[0][i]);
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if cross.iter().all(|v| *v == 0.) {
            return Err("La malla contiene triángulos degenerados.".into());
        }
        normalized.extend_from_slice(&[0; 12]);
        for p in triangle {
            let mut normalized_point = [0.; 3];
            for (i, v) in p.iter().enumerate() {
                let mm = v * units.scale();
                if !mm.is_finite() || mm.abs() > 100_000. {
                    return Err("Coordenadas STL fuera del límite.".into());
                }
                let stored = mm as f32;
                let actual = stored as f64;
                normalized_point[i] = actual;
                bounds[0][i] = bounds[0][i].min(actual);
                bounds[1][i] = bounds[1][i].max(actual);
                normalized.extend_from_slice(&stored.to_le_bytes());
            }
            normalized_points.push(normalized_point);
        }
        normalized.extend_from_slice(&[0; 2]);
    }
    if (0..3).any(|i| bounds[1][i] <= bounds[0][i]) {
        return Err("La malla debe tener volumen en tres ejes.".into());
    }
    Ok((
        Mesh {
            triangles: (vertices.len() / 3) as u32,
            bounds_mm: bounds,
            shape_sha256: shape_signature(
                &normalized_points,
                &(0..normalized_points.len() / 3)
                    .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
                    .collect::<Vec<_>>(),
            )?,
        },
        normalized,
    ))
}

pub(crate) fn shape_signature(
    points: &[[f64; 3]],
    triangles: &[[usize; 3]],
) -> Result<String, String> {
    if points.is_empty() || triangles.is_empty() {
        return Err("Malla sin geometría verificable.".into());
    }
    let center: [f64; 3] = std::array::from_fn(|i| {
        (points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min)
            + points
                .iter()
                .map(|p| p[i])
                .fold(f64::NEG_INFINITY, f64::max))
            / 2.
    });
    let mut canonical = Vec::with_capacity(triangles.len());
    for indices in triangles {
        let mut face = [[0i64; 3]; 3];
        for (i, index) in indices.iter().enumerate() {
            let p = points
                .get(*index)
                .ok_or("Índice de malla fuera del recurso 3MF.")?;
            face[i] = std::array::from_fn(|axis| ((p[axis] - center[axis]) * 1000.).round() as i64);
        }
        face.sort();
        canonical.push(face);
    }
    canonical.sort();
    let mut hash = Sha256::new();
    for face in canonical {
        for point in face {
            for c in point {
                hash.update(c.to_le_bytes());
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn capture(
    profile_root: &Path,
    model: &Path,
    profiles: [PathBuf; 3],
    output: &Path,
    id: Uuid,
    request: Request,
    cancel: Arc<AtomicBool>,
) -> Result<SliceInput, String> {
    stop(&cancel)?;
    if !request.bed_temperature_c.is_finite() || !(0.0..=200.).contains(&request.bed_temperature_c)
    {
        return Err("Confirme la temperatura inicial de cama en °C, entre 0 y 200.".into());
    }
    let mut pins = crate::project::pin_lineage(profile_root).map_err(|e| e.to_string())?;
    pins.extend(
        crate::project::pin_lineage(model.parent().ok_or("Falta la carpeta STL.")?)
            .map_err(|e| e.to_string())?,
    );
    let mut file = files::open_regular(model, MODEL_LIMIT)?;
    let mut original = Vec::new();
    file.read_to_end(&mut original).map_err(|e| e.to_string())?;
    pins.push(file);
    let (geometry, _) = mesh(&original, request.units)?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    files::write_new(&output.join("Original.stl"), &original)?;
    let mut captured = BTreeMap::new();
    captured.insert(
        "Original.stl".into(),
        CapturedFile {
            path: "Original.stl".into(),
            sha256: files::digest(&original),
            bytes: original.len() as u64,
        },
    );
    let mut maps = Vec::new();
    for ((kind, name), path) in [
        ("machine", "Machine.json"),
        ("process", "Process.json"),
        ("filament", "Filament.json"),
    ]
    .into_iter()
    .zip(profiles)
    {
        let mut map = profile(
            profile_root,
            &path,
            output,
            &mut captured,
            &mut pins,
            &mut BTreeSet::new(),
            &cancel,
        )?;
        if map.get("type").and_then(Value::as_str) != Some(kind) {
            return Err("Seleccione el tipo de perfil solicitado.".into());
        }
        if kind == "machine" {
            map.insert(
                "curr_bed_type".into(),
                Value::String(request.bed.label().into()),
            );
        }
        if kind == "filament" {
            map.insert(
                request.bed.temperature_key().into(),
                serde_json::json!([request.bed_temperature_c.to_string()]),
            );
        }
        let bytes = serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?;
        files::write_new(&output.join(name), &bytes)?;
        captured.insert(
            name.into(),
            CapturedFile {
                path: name.into(),
                sha256: files::digest(&bytes),
                bytes: bytes.len() as u64,
            },
        );
        maps.push(map);
    }
    let selected = settings(&maps[0], &maps[1], &maps[2], request.bed)?;
    let input = SliceInput {
        schema_version: 1,
        job_id: id,
        orca_version: VERSION.into(),
        source_name: model
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("Nombre STL no válido.")?
            .into(),
        units: request.units,
        bed: request.bed,
        mesh: geometry,
        settings: selected,
        files: captured.into_values().collect(),
    };
    input.validate()?;
    stop(&cancel)?;
    Ok(input)
}
pub(crate) fn recapture(
    original: &Path,
    previous: &SliceInput,
    output: &Path,
    id: Uuid,
    bed: Bed,
    temperature_c: f64,
    cancel: Arc<AtomicBool>,
) -> Result<SliceInput, String> {
    stop(&cancel)?;
    validate_capture(original, previous)?;
    let root = original.join("ProfileSources");
    let mut selected = Vec::new();
    for (kind, wanted) in [
        ("machine", &previous.settings.machine),
        ("process", &previous.settings.process),
        ("filament", &previous.settings.filament),
    ] {
        let mut matches = Vec::new();
        for file in &previous.files {
            stop(&cancel)?;
            if file.path.starts_with(&format!("ProfileSources/{kind}/")) {
                let bytes =
                    files::read_regular(&original.join(files::relative(&file.path)?), 1024 * 1024)?;
                let map: Map<String, Value> =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if map.get("name").and_then(Value::as_str) == Some(wanted.as_str()) {
                    matches.push(original.join(files::relative(&file.path)?));
                }
            }
        }
        if matches.len() != 1 {
            return Err("No se pudo identificar un perfil original único. Selecciona los perfiles de nuevo.".into());
        }
        selected.push(matches.pop().unwrap());
    }
    let next = capture(
        &root,
        &original.join("Original.stl"),
        selected
            .try_into()
            .map_err(|_| "Faltan perfiles originales.")?,
        output,
        id,
        Request {
            units: previous.units,
            bed,
            bed_temperature_c: temperature_c,
        },
        cancel,
    )?;
    if !matches_stored(&next.mesh, &previous.mesh)? {
        return Err("La geometría del modelo cambió durante la captura.".into());
    }
    for file in next
        .files
        .iter()
        .filter(|f| f.path == "Original.stl" || f.path.starts_with("ProfileSources/"))
    {
        if !previous.files.iter().any(|old| {
            old.path == file.path && old.sha256 == file.sha256 && old.bytes == file.bytes
        }) {
            return Err("Los perfiles o el modelo cambiaron durante la captura.".into());
        }
    }
    Ok(next)
}
pub(crate) fn validate_capture(directory: &Path, input: &SliceInput) -> Result<(), String> {
    input.validate()?;
    for f in &input.files {
        let bytes = files::read_regular(&directory.join(files::relative(&f.path)?), MODEL_LIMIT)?;
        if bytes.len() as u64 != f.bytes || files::digest(&bytes) != f.sha256 {
            return Err("Las fuentes aprobadas de laminado cambiaron.".into());
        }
    }
    let bytes = files::read_regular(&directory.join("Original.stl"), MODEL_LIMIT)?;
    if !matches_stored(&mesh(&bytes, input.units)?.0, &input.mesh)? {
        return Err("La malla no coincide con la propuesta.".into());
    }
    let maps = ["Machine.json", "Process.json", "Filament.json"].map(|n| {
        files::read_regular(&directory.join(n), 1024 * 1024).and_then(|b| {
            serde_json::from_slice::<Map<String, Value>>(&b).map_err(|e| e.to_string())
        })
    });
    let [m, p, f] = maps;
    if !matches_stored(&settings(&m?, &p?, &f?, input.bed)?, &input.settings)? {
        return Err("Los perfiles no coinciden con la propuesta.".into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn ascii_and_binary_units_preserve_geometry() {
        let mut binary =
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl")
                .to_vec();
        for facet in binary[84..].chunks_exact_mut(50) {
            for vertex in facet[12..48].chunks_exact_mut(12) {
                for (i, origin) in [105., 105., 5.].into_iter().enumerate() {
                    let coordinate =
                        f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap()) - origin;
                    vertex[i * 4..i * 4 + 4].copy_from_slice(&coordinate.to_le_bytes());
                }
            }
        }
        let mut ascii = String::from("solid\n");
        for facet in binary[84..].chunks_exact(50) {
            ascii.push_str("facet normal 0 0 0\nouter loop\n");
            for vertex in facet[12..48].chunks_exact(12) {
                let p: [f32; 3] = std::array::from_fn(|i| {
                    f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
                });
                ascii.push_str(&format!("vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            ascii.push_str("endloop\nendfacet\n");
        }
        ascii.push_str("endsolid\n");
        for unit in [Units::Mm, Units::Cm, Units::M, Units::In, Units::Ft] {
            let expected = mesh(&binary, unit).unwrap();
            assert_eq!(mesh(ascii.as_bytes(), unit).unwrap(), expected);
            assert!(
                (expected.0.bounds_mm[1][0] - expected.0.bounds_mm[0][0] - 10. * unit.scale())
                    .abs()
                    < 0.002
            );
        }
        assert!(mesh(ascii.replace("endloop", "unexpected").as_bytes(), Units::Mm).is_err());
    }
    pub(crate) fn fixture() -> (PathBuf, PathBuf, [PathBuf; 3]) {
        let root = std::env::temp_dir().join(format!("hazlomexa-orca-input-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let model = root.join("Cube.stl");
        fs::write(
            &model,
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl"),
        )
        .unwrap();
        let base = root.join("profiles");
        fs::create_dir(&base).unwrap();
        let maps = [
            (
                "machine",
                include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/machine.json")
                    .as_slice(),
            ),
            (
                "process",
                include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/process.json")
                    .as_slice(),
            ),
            (
                "filament",
                include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/filament.json")
                    .as_slice(),
            ),
        ];
        let paths = maps.map(|(kind, bytes)| {
            let dir = base.join(kind);
            fs::create_dir(&dir).unwrap();
            let path = dir.join("Selected.json");
            fs::write(&path, bytes).unwrap();
            path
        });
        (root, model, paths)
    }
    #[test]
    fn captured_profiles_units_and_sources_are_bound_to_the_proposal() {
        let (root, model, paths) = fixture();
        let imported = root.join("capture");
        let input = capture(
            &root.join("profiles"),
            &model,
            paths,
            &imported,
            Uuid::new_v4(),
            Request {
                units: Units::In,
                bed: Bed::Textured,
                bed_temperature_c: 45.,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(input.mesh.triangles, 12);
        assert!((input.mesh.bounds_mm[1][0] - input.mesh.bounds_mm[0][0] - 254.).abs() < 0.002);
        assert_eq!(input.settings.bed, "Textured PEI Plate");
        validate_capture(&imported, &input).unwrap();
        let repeated = recapture(
            &imported,
            &input,
            &root.join("followup"),
            Uuid::new_v4(),
            Bed::Textured,
            50.,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(repeated.mesh, input.mesh);
        assert_eq!(serde_json::to_value(repeated.units).unwrap(), "in");
        assert_eq!(repeated.settings.bed_c, 50.);
        assert_eq!(repeated.settings.machine, input.settings.machine);
        assert_eq!(repeated.settings.process, input.settings.process);
        assert_eq!(repeated.settings.filament, input.settings.filament);
        assert!(recapture(
            &imported,
            &input,
            &root.join("cancelled"),
            Uuid::new_v4(),
            Bed::Textured,
            50.,
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(!root.join("cancelled").exists());
        assert!(recapture(
            &imported,
            &input,
            &root.join("invalid"),
            Uuid::new_v4(),
            Bed::Textured,
            201.,
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        assert!(!root.join("invalid").exists());
        fs::write(imported.join("Machine.json"), b"{}").unwrap();
        assert!(validate_capture(&imported, &input).is_err());
        assert!(recapture(
            &imported,
            &input,
            &root.join("tampered"),
            Uuid::new_v4(),
            Bed::Textured,
            50.,
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        assert_eq!(
            fs::read(model).unwrap(),
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl")
        );
    }
    #[test]
    fn rejects_profile_scripts_credentials_cycles_and_incompatible_machine() {
        for (key, value) in [
            ("post_process", serde_json::json!(["cmd.exe /c external"])),
            ("printhost_apikey", serde_json::json!("fixture-token")),
            ("compatible_printers", serde_json::json!(["Other machine"])),
        ] {
            let (root, model, paths) = fixture();
            let mut map: Value = serde_json::from_slice(&fs::read(&paths[1]).unwrap()).unwrap();
            map[key] = value;
            fs::write(&paths[1], serde_json::to_vec(&map).unwrap()).unwrap();
            assert!(capture(
                &root.join("profiles"),
                &model,
                paths,
                &root.join("capture"),
                Uuid::new_v4(),
                Request {
                    units: Units::Mm,
                    bed: Bed::Textured,
                    bed_temperature_c: 45.
                },
                Arc::new(AtomicBool::new(false))
            )
            .is_err());
        }
        for parent in ["Selected", "../outside", "missing"] {
            let (root, model, paths) = fixture();
            let mut map: Value = serde_json::from_slice(&fs::read(&paths[0]).unwrap()).unwrap();
            map["inherits"] = serde_json::json!(parent);
            fs::write(&paths[0], serde_json::to_vec(&map).unwrap()).unwrap();
            assert!(capture(
                &root.join("profiles"),
                &model,
                paths,
                &root.join("capture"),
                Uuid::new_v4(),
                Request {
                    units: Units::Mm,
                    bed: Bed::Textured,
                    bed_temperature_c: 45.
                },
                Arc::new(AtomicBool::new(false))
            )
            .is_err());
        }
    }
    #[test]
    fn compatibility_metadata_is_typed_and_identifies_the_profile_to_correct() {
        let (_, _, paths) = fixture();
        let mut maps: Vec<Map<String, Value>> = paths
            .iter()
            .map(|path| serde_json::from_slice(&fs::read(path).unwrap()).unwrap())
            .collect();
        maps[2].insert(
            Bed::Textured.temperature_key().into(),
            serde_json::json!(["45"]),
        );
        let machine = name(&maps[0]).unwrap();
        for (index, kind) in [(1, "proceso"), (2, "material")] {
            for (key, value) in [
                ("compatible_printers", serde_json::json!("Other printer")),
                ("compatible_printers", Value::Null),
                ("compatible_printers", serde_json::json!([machine, 1])),
                ("compatible_printers", serde_json::json!([""])),
                ("compatible_printers", serde_json::json!(["bad\nname"])),
                ("compatible_printers", serde_json::json!(["Other printer"])),
                ("compatible_printers_condition", Value::Null),
                ("compatible_printers_condition", serde_json::json!(true)),
                (
                    "compatible_printers_condition",
                    serde_json::json!("unverified_expression"),
                ),
            ] {
                let mut changed = maps.clone();
                changed[index].insert(key.into(), value);
                let error =
                    settings(&changed[0], &changed[1], &changed[2], Bed::Textured).unwrap_err();
                assert!(error.contains(kind) && error.contains(&machine), "{error}");
            }
            for names in [serde_json::json!([]), serde_json::json!([machine])] {
                let mut changed = maps.clone();
                changed[index].insert("compatible_printers".into(), names);
                changed[index].insert(
                    "compatible_printers_condition".into(),
                    serde_json::json!(""),
                );
                settings(&changed[0], &changed[1], &changed[2], Bed::Textured).unwrap();
            }
        }
    }
    #[test]
    fn cancelled_and_malformed_mesh_imports_are_rejected() {
        let (root, model, paths) = fixture();
        assert!(capture(
            &root.join("profiles"),
            &model,
            paths,
            &root.join("capture"),
            Uuid::new_v4(),
            Request {
                units: Units::Mm,
                bed: Bed::Textured,
                bed_temperature_c: 45.
            },
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(!root.join("capture").exists());
        for bytes in [
            b"not an STL".as_slice(),
            b"solid x\nvertex NaN 0 0\nendsolid x",
            b"solid x\nvertex 0 0 0\nendsolid x",
        ] {
            assert!(mesh(bytes, Units::Mm).is_err());
        }
    }
}
