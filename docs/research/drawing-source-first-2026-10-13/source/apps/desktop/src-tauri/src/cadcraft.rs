//! Typed, copy-only operations using the identified official Windows CLI.
use crate::process::{run_owned, StopReason};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

pub const VERSION: &str = "0.5.0";
// The release archive is identified separately; the executable hash is checked before any invocation.
pub const CLI_SHA256: &str = "d9517678cf9216a63ecb7f4adfe4ab485af9541849d465245251bba91845724f";
const MAX_FILE: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Dxf,
    Dwg,
    Png,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Dxf => "dxf",
            Self::Dwg => "dwg",
            Self::Png => "png",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Mm,
    In,
}
impl Units {
    fn insertion_code(self) -> u16 {
        match self {
            Self::Mm => 4,
            Self::In => 1,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circle {
    pub center: [f64; 2],
    pub radius: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub output_format: Format,
    pub units: Units,
    pub circle: Option<Circle>,
    pub allow_loss: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub entity_count: u64,
    pub counts: BTreeMap<String, u64>,
    pub extents: Option<[[f64; 2]; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inspection {
    pub source_name: String,
    pub source_format: Format,
    pub source_sha256: String,
    pub insunits: u64,
    pub summary: Summary,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub cadcraft_version: String,
    pub source_name: String,
    pub source_format: Format,
    pub source_sha256: String,
    pub output_format: Format,
    pub units: Units,
    pub circle: Option<Circle>,
    pub allow_loss: bool,
    pub original_summary: Summary,
}
impl DrawingInput {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.schema_version, 1 | 2)
            || self.job_id.is_nil()
            || self.cadcraft_version != VERSION
            || self.source_format == Format::Png
            || self.source_name.is_empty()
            || self.source_name.len() > 255
            || self.source_name.contains(['/', '\\', ':', '"', '\0'])
            || !self
                .source_name
                .to_ascii_lowercase()
                .ends_with(&format!(".{}", self.source_format.extension()))
            || !valid_hash(&self.source_sha256)
        {
            return Err("La propuesta de dibujo no es compatible.".into());
        }
        self.original_summary.validate()?;
        if let Some(circle) = &self.circle {
            if !circle.radius.is_finite()
                || circle.radius <= 0.0
                || circle.radius > 1_000_000.0
                || circle
                    .center
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
            {
                return Err("El círculo requiere coordenadas finitas y radio positivo dentro del límite de dibujo.".into());
            }
        }
        Ok(())
    }
}
impl Summary {
    fn validate(&self) -> Result<(), String> {
        if self.entity_count > 1_000_000
            || self.counts.len() > 128
            || self
                .counts
                .iter()
                .any(|(key, value)| key.is_empty() || key.len() > 128 || *value > 1_000_000)
            || self.counts.values().sum::<u64>() != self.entity_count
            || self.extents.is_some_and(|bounds| {
                bounds.iter().flatten().any(|v| !v.is_finite())
                    || bounds[0][0] > bounds[1][0]
                    || bounds[0][1] > bounds[1][1]
            })
        {
            return Err("El resumen del dibujo no es válido.".into());
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircleReadback {
    pub before_matches: u64,
    pub edited_matches: u64,
    pub final_matches: Option<u64>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrawingResult {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub cadcraft_version: String,
    pub source_sha256: String,
    pub output_sha256: String,
    pub preview_sha256: String,
    pub output_summary: Option<Summary>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle_readback: Option<CircleReadback>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file()
        || meta.len() == 0
        || meta.len() > MAX_FILE
        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("Seleccione un archivo regular local de hasta 32 MiB.".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != meta.len() {
        return Err("El archivo cambió al leerlo.".into());
    }
    Ok(bytes)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}
pub fn verify_runtime(executable: &Path) -> Result<(), String> {
    crate::project::valid_location(executable).map_err(|e| e.to_string())?;
    if !executable
        .file_name()
        .is_some_and(|v| v.eq_ignore_ascii_case("cadcraft-cli.exe"))
        || hash(&read_file(executable)?) != CLI_SHA256
    {
        return Err(
            "Seleccione cadcraft-cli.exe del paquete oficial Windows x64 0.5.0 identificado."
                .into(),
        );
    }
    Ok(())
}
fn environment(profile: &Path) -> Result<BTreeMap<OsString, OsString>, String> {
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let mut env = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    env.insert("TEMP".into(), profile.as_os_str().to_owned());
    env.insert("TMP".into(), profile.as_os_str().to_owned());
    env.insert("USERPROFILE".into(), profile.as_os_str().to_owned());
    Ok(env)
}
fn invoke(
    executable: &Path,
    cwd: &Path,
    env: &BTreeMap<OsString, OsString>,
    args: &[OsString],
    cancel: Arc<AtomicBool>,
) -> Result<(Vec<u8>, Vec<String>), String> {
    // Pin the exact executable while CreateProcess loads it; owned Job Object controls descendants.
    let _lineage = crate::project::pin_lineage(
        executable
            .parent()
            .ok_or("Falta el directorio del runtime.")?,
    )
    .map_err(|e| e.to_string())?;
    let _pin = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(executable)
        .map_err(|e| e.to_string())?;
    verify_runtime(executable)?;
    let result = run_owned(executable, args, cwd, env, cancel, Duration::from_secs(60))
        .map_err(|e| e.to_string())?;
    match result.reason {
        StopReason::Cancelled => {
            return Err(
                "Operación de dibujo cancelada; solo pudo cambiar la copia de trabajo.".into(),
            )
        }
        StopReason::TimedOut => return Err(
            "CADCraft agotó el tiempo; revise la copia de trabajo, no se publicaron resultados."
                .into(),
        ),
        StopReason::Completed => (),
    }
    if result.truncated || result.exit_code != Some(0) {
        return Err("CADCraft rechazó el dibujo u operación; el original se conserva.".into());
    }
    let stderr =
        String::from_utf8(result.stderr).map_err(|_| "Respuesta de CADCraft no válida.")?;
    // All unfamiliar stderr is retained, not silently treated as an innocuous progress line.
    let warnings = stderr
        .lines()
        .filter(|v| !v.starts_with("wrote ") && !v.trim().is_empty())
        .map(str::to_owned)
        .collect();
    Ok((result.stdout, warnings))
}
fn inspect(
    executable: &Path,
    path: &Path,
    cwd: &Path,
    env: &BTreeMap<OsString, OsString>,
    cancel: Arc<AtomicBool>,
) -> Result<Summary, String> {
    let (stdout, warnings) = invoke(
        executable,
        cwd,
        env,
        &["info".into(), path.as_os_str().to_owned()],
        cancel,
    )?;
    if !warnings.is_empty() {
        return Err("CADCraft informó problemas al inspeccionar el original.".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&stdout).map_err(|_| "Resumen de CADCraft no válido.")?;
    let summary = Summary {
        entity_count: value["entityCount"]
            .as_u64()
            .ok_or("Falta la cantidad de entidades.")?,
        counts: serde_json::from_value(value["counts"].clone())
            .map_err(|_| "Conteos no válidos.")?,
        extents: serde_json::from_value(value["extents"].clone())
            .map_err(|_| "Extensión no válida.")?,
    };
    summary.validate()?;
    Ok(summary)
}
fn insertion_units(bytes: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines();
    while let (Some(code), Some(value)) = (lines.next(), lines.next()) {
        if code.trim() == "9" && value.trim() == "$INSUNITS" {
            return (lines.next()?.trim() == "70")
                .then(|| lines.next()?.trim().parse().ok())
                .flatten();
        }
    }
    None
}
fn check_source_units(insertion: u64, units: Units) -> Result<(), String> {
    if insertion == 0 || insertion == units.insertion_code() as u64 {
        return Ok(());
    }
    let declared = match insertion {
        1 => "pulgadas",
        4 => "milímetros",
        _ => return Err("El dibujo declara unidades que esta integración todavía no admite. Convierte una copia a milímetros o pulgadas en el programa original y confirma su tamaño antes de seleccionarla. El original se conserva.".into()),
    };
    Err(format!("El dibujo declara {declared}. En «Unidades del dibujo original», elige {declared} y vuelve a seleccionar el archivo. Las medidas del círculo pueden seguir en su propia unidad; Hazlo las convierte al preparar la copia. El original se conserva."))
}
fn circle_matches(value: &serde_json::Value, circle: &Circle) -> Result<bool, String> {
    if value["type"] != "Circle" || value["geometry"]["type"] != "circle" {
        return Err("CADCraft devolvió una entidad que no es un círculo.".into());
    }
    let geometry = &value["geometry"];
    let actual = [
        geometry["center"]["x"].as_f64(),
        geometry["center"]["y"].as_f64(),
        geometry["center"]["z"].as_f64(),
        geometry["radius"].as_f64(),
    ];
    let expected = [circle.center[0], circle.center[1], 0., circle.radius];
    let mut matches = true;
    for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
        let actual = actual
            .filter(|v| v.is_finite())
            .ok_or("Falta la geometría finita del círculo.")?;
        if index == 3 && actual <= 0. {
            return Err("CADCraft devolvió un radio no válido.".into());
        }
        matches &= (actual - expected).abs() <= 1e-8 + 1e-12 * actual.abs().max(expected.abs());
    }
    Ok(matches)
}
fn circle_page(
    stdout: &[u8],
    offset: u64,
    total: Option<u64>,
    circle: &Circle,
) -> Result<(u64, u64, u64), String> {
    let page: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|_| "Geometría de CADCraft no válida.")?;
    let count = page["count"]
        .as_u64()
        .filter(|n| *n <= 1_000_000)
        .ok_or("Cantidad de círculos no válida.")?;
    let entities = page["entities"]
        .as_array()
        .ok_or("Falta la geometría de los círculos.")?;
    let returned = entities.len() as u64;
    if total.is_some_and(|n| n != count)
        || returned > 512
        || page["offset"].as_u64() != Some(offset)
        || page["returned"].as_u64() != Some(returned)
        || offset > count
        || offset + returned > count
        || (returned == 0 && offset < count)
    {
        return Err("CADCraft devolvió una página incompleta o inconsistente de círculos.".into());
    }
    let mut matches = 0;
    for entity in entities {
        matches += u64::from(circle_matches(entity, circle)?);
    }
    Ok((count, returned, matches))
}
fn read_circle_matches(
    executable: &Path,
    path: &Path,
    cwd: &Path,
    env: &BTreeMap<OsString, OsString>,
    circle: &Circle,
    expected_count: u64,
    cancelled: Arc<AtomicBool>,
) -> Result<u64, String> {
    let mut offset = 0u64;
    let mut total = Some(expected_count);
    let mut matches = 0;
    loop {
        let command = format!(
            "entities {}",
            serde_json::json!({"type":"Circle","limit":512,"offset":offset})
        );
        let (stdout, warnings) = invoke(
            executable,
            cwd,
            env,
            &[
                "run".into(),
                path.as_os_str().to_owned(),
                "--cmd".into(),
                command.into(),
            ],
            cancelled.clone(),
        )?;
        if !warnings.is_empty() {
            return Err("CADCraft informó problemas al leer la geometría de los círculos.".into());
        }
        let (count, returned, page_matches) = circle_page(&stdout, offset, total, circle)?;
        total = Some(count);
        matches += page_matches;
        offset += returned;
        if offset == count {
            return Ok(matches);
        }
    }
}
pub fn prepare(
    executable: &Path,
    source: &Path,
    saved: &Path,
    profile: &Path,
    request: Request,
    job_id: Uuid,
    cancel: Arc<AtomicBool>,
) -> Result<DrawingInput, String> {
    let Request {
        output_format,
        units,
        circle,
        allow_loss,
    } = request;
    let inspected = inspect_original(executable, source, saved, profile, Some(units), cancel)?;
    let input = DrawingInput {
        schema_version: 2,
        job_id,
        cadcraft_version: VERSION.into(),
        source_name: inspected.source_name,
        source_format: inspected.source_format,
        source_sha256: inspected.source_sha256,
        output_format,
        units,
        circle,
        allow_loss,
        original_summary: inspected.summary,
    };
    input.validate()?;
    Ok(input)
}

pub fn discover(
    executable: &Path,
    source: &Path,
    saved: &Path,
    profile: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Inspection, String> {
    inspect_original(executable, source, saved, profile, None, cancel)
}

fn inspect_original(
    executable: &Path,
    source: &Path,
    saved: &Path,
    profile: &Path,
    confirmed_units: Option<Units>,
    cancel: Arc<AtomicBool>,
) -> Result<Inspection, String> {
    crate::project::valid_location(source).map_err(|e| e.to_string())?;
    let source_format = match source
        .extension()
        .and_then(|v| v.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("dxf") => Format::Dxf,
        Some("dwg") => Format::Dwg,
        _ => return Err("Seleccione un dibujo DXF o DWG.".into()),
    };
    let bytes = read_file(source)?;
    if source_format == Format::Dxf {
        if let (Some(insertion), Some(units)) = (insertion_units(&bytes), confirmed_units) {
            check_source_units(insertion as u64, units)?;
        }
    }
    write_new(saved, &bytes)?;
    let env = environment(profile)?;
    let (unit_reply, unit_warnings) = invoke(
        executable,
        profile,
        &env,
        &[
            "run".into(),
            saved.as_os_str().to_owned(),
            "--cmd".into(),
            "units {}".into(),
        ],
        cancel.clone(),
    )?;
    let unit_value: serde_json::Value = serde_json::from_slice(&unit_reply)
        .map_err(|_| "No se pudieron identificar las unidades del dibujo.")?;
    let insertion = unit_value["insunits"]
        .as_u64()
        .ok_or("Faltan las unidades del dibujo.")?;
    if !unit_warnings.is_empty() {
        return Err("CADCraft informó advertencias al leer las unidades del dibujo. Revisa o corrige una copia en el programa original antes de importarla; todavía no se puede confirmar su escala.".into());
    }
    if let Some(units) = confirmed_units {
        check_source_units(insertion, units)?;
    }
    let original_summary = inspect(executable, saved, profile, &env, cancel)?;
    if read_file(saved)? != bytes {
        return Err("La inspección cambió la copia del dibujo; no se usarán sus medidas.".into());
    }
    Ok(Inspection {
        source_name: source
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("Nombre de dibujo no válido.")?
            .into(),
        source_format,
        source_sha256: hash(&bytes),
        insunits: insertion,
        summary: original_summary,
    })
}
fn valid_png(bytes: &[u8]) -> bool {
    bytes.len() >= 33
        && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
        && &bytes[12..16] == b"IHDR"
        && u32::from_be_bytes(bytes[16..20].try_into().unwrap()) == 2400
        && u32::from_be_bytes(bytes[20..24].try_into().unwrap()) == 1600
}
pub fn validate_artifacts(
    directory: &Path,
    input: &DrawingInput,
    result: &DrawingResult,
) -> Result<(), String> {
    input.validate()?;
    if !matches!(result.schema_version, 1 | 2)
        || (input.schema_version == 2 && result.schema_version != 2)
        || result.job_id != input.job_id
        || result.cadcraft_version != VERSION
        || result.source_sha256 != input.source_sha256
        || !valid_hash(&result.output_sha256)
        || !valid_hash(&result.preview_sha256)
        || result.warnings.len() > 128
        || result.warnings.iter().any(|v| v.len() > 4096)
        || (!input.allow_loss && !result.warnings.is_empty())
    {
        return Err("El resultado no corresponde a la operación aprobada.".into());
    }
    if result.schema_version == 2 {
        match (&input.circle, &result.circle_readback) {
            (None, None) => (),
            (Some(_), Some(readback))
                if readback.before_matches
                    <= input
                        .original_summary
                        .counts
                        .get("Circle")
                        .copied()
                        .unwrap_or(0)
                    && readback.edited_matches == readback.before_matches + 1
                    && (if input.output_format == Format::Png {
                        readback.final_matches.is_none()
                    } else {
                        readback.final_matches == Some(readback.edited_matches)
                            && result.output_summary.as_ref().is_some_and(|summary| {
                                readback.edited_matches
                                    <= summary.counts.get("Circle").copied().unwrap_or(0)
                            })
                    }) => {}
            _ => {
                return Err(
                    "No se verificó la geometría del círculo aprobado en la copia nativa.".into(),
                )
            }
        }
    } else if result.circle_readback.is_some() {
        return Err(
            "Un resultado anterior no puede declarar la nueva verificación geométrica.".into(),
        );
    }
    let original =
        read_file(&directory.join(format!("Original.{}", input.source_format.extension())))?;
    let output =
        read_file(&directory.join(format!("Drawing.{}", input.output_format.extension())))?;
    let preview = read_file(&directory.join("Preview.png"))?;
    if hash(&original) != input.source_sha256
        || hash(&output) != result.output_sha256
        || hash(&preview) != result.preview_sha256
        || !valid_png(&preview)
    {
        return Err("Los archivos del dibujo cambiaron o no son válidos.".into());
    }
    match input.output_format {
        Format::Png if valid_png(&output) && result.output_summary.is_none() => (),
        Format::Dxf if output.ends_with(b"EOF\n") || output.ends_with(b"EOF\r\n") => {
            result
                .output_summary
                .as_ref()
                .ok_or("Falta el resumen del resultado.")?
                .validate()?;
        }
        Format::Dwg if output.starts_with(b"AC10") => {
            result
                .output_summary
                .as_ref()
                .ok_or("Falta el resumen del resultado.")?
                .validate()?;
        }
        _ => return Err("Formato de dibujo no válido.".into()),
    }
    if !input.allow_loss {
        if let Some(summary) = &result.output_summary {
            let mut expected = input.original_summary.counts.clone();
            if input.circle.is_some() {
                *expected.entry("Circle".into()).or_default() += 1;
            }
            if summary.counts != expected {
                return Err("La conversión cambió las cantidades de entidades sin consentimiento de pérdida.".into());
            }
        }
    }
    Ok(())
}
pub fn convert(
    executable: &Path,
    saved: &Path,
    stage: &Path,
    profile: &Path,
    input: &DrawingInput,
    cancel: Arc<AtomicBool>,
) -> Result<DrawingResult, String> {
    input.validate()?;
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio de trabajo no está vacío.".into());
    }
    let original = read_file(saved)?;
    if hash(&original) != input.source_sha256 {
        return Err("El original importado cambió; prepare otra propuesta.".into());
    }
    let original_path = stage.join(format!("Original.{}", input.source_format.extension()));
    write_new(&original_path, &original)?;
    let env = environment(profile)?;
    if inspect(executable, &original_path, stage, &env, cancel.clone())? != input.original_summary {
        return Err("El resumen del original ya no coincide con la propuesta.".into());
    }
    let mut warnings = Vec::new();
    if input
        .original_summary
        .counts
        .get("Proxy")
        .copied()
        .unwrap_or(0)
        > 0
    {
        warnings.push("El original contiene objetos no compatibles; pueden omitirse del dibujo o de su vista previa.".into());
    }
    let working = if let Some(circle) = &input.circle {
        let edited = profile.join("Edited.dxf");
        let command = format!(
            "circle {}",
            serde_json::to_string(circle).map_err(|e| e.to_string())?
        );
        let (_, messages) = invoke(
            executable,
            stage,
            &env,
            &[
                "run".into(),
                original_path.as_os_str().to_owned(),
                "--cmd".into(),
                command.into(),
                "--save".into(),
                edited.as_os_str().to_owned(),
            ],
            cancel.clone(),
        )?;
        warnings.extend(messages);
        let summary = inspect(executable, &edited, stage, &env, cancel.clone())?;
        if summary.counts.get("Circle").copied().unwrap_or(0)
            != input
                .original_summary
                .counts
                .get("Circle")
                .copied()
                .unwrap_or(0)
                + 1
        {
            return Err("No se verificó la adición del círculo aprobado.".into());
        }
        edited
    } else {
        original_path.clone()
    };
    let mut circle_readback = if let Some(circle) = &input.circle {
        let before_matches = read_circle_matches(
            executable,
            &original_path,
            stage,
            &env,
            circle,
            input
                .original_summary
                .counts
                .get("Circle")
                .copied()
                .unwrap_or(0),
            cancel.clone(),
        )?;
        let edited_matches = read_circle_matches(
            executable,
            &working,
            stage,
            &env,
            circle,
            input
                .original_summary
                .counts
                .get("Circle")
                .copied()
                .unwrap_or(0)
                + 1,
            cancel.clone(),
        )?;
        if edited_matches != before_matches + 1 {
            return Err("La copia no contiene el círculo con el centro y radio aprobados.".into());
        }
        Some(CircleReadback {
            before_matches,
            edited_matches,
            final_matches: None,
        })
    } else {
        None
    };
    let output_path = stage.join(format!("Drawing.{}", input.output_format.extension()));
    let mut args = vec![
        "convert".into(),
        working.as_os_str().to_owned(),
        output_path.as_os_str().to_owned(),
    ];
    if input.output_format == Format::Png {
        args.extend(["--view".into(), "extents".into()]);
    }
    warnings.extend(invoke(executable, stage, &env, &args, cancel.clone())?.1);
    if !input.allow_loss && !warnings.is_empty() {
        return Err(format!("CADCraft informó pérdida o advertencias: {}. El original se conserva; revise la copia y prepare otra propuesta con consentimiento de pérdida si procede.", warnings.join("; ").chars().take(2048).collect::<String>()));
    }
    let preview_path = stage.join("Preview.png");
    if input.output_format == Format::Png {
        write_new(&preview_path, &read_file(&output_path)?)?;
    } else {
        warnings.extend(
            invoke(
                executable,
                stage,
                &env,
                &[
                    "convert".into(),
                    output_path.as_os_str().to_owned(),
                    preview_path.as_os_str().to_owned(),
                    "--view".into(),
                    "extents".into(),
                ],
                cancel.clone(),
            )?
            .1,
        );
    }
    let output_summary = if input.output_format == Format::Png {
        None
    } else {
        Some(inspect(
            executable,
            &output_path,
            stage,
            &env,
            cancel.clone(),
        )?)
    };
    if let (Some(circle), Some(readback)) = (&input.circle, &mut circle_readback) {
        if input.output_format != Format::Png {
            readback.final_matches = Some(read_circle_matches(
                executable,
                &output_path,
                stage,
                &env,
                circle,
                output_summary
                    .as_ref()
                    .unwrap()
                    .counts
                    .get("Circle")
                    .copied()
                    .unwrap_or(0),
                cancel,
            )?);
            if readback.final_matches != Some(readback.edited_matches) {
                return Err(
                    "El dibujo final cambió el círculo aprobado durante la conversión.".into(),
                );
            }
        }
    }
    let result = DrawingResult {
        schema_version: 2,
        job_id: input.job_id,
        cadcraft_version: VERSION.into(),
        source_sha256: input.source_sha256.clone(),
        output_sha256: hash(&read_file(&output_path)?),
        preview_sha256: hash(&read_file(&preview_path)?),
        output_summary,
        warnings,
        circle_readback,
    };
    validate_artifacts(stage, input, &result)?;
    write_new(
        &stage.join("result.json"),
        &serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> DrawingInput {
        DrawingInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            cadcraft_version: VERSION.into(),
            source_name: "pieza.dxf".into(),
            source_format: Format::Dxf,
            source_sha256: "a".repeat(64),
            output_format: Format::Dxf,
            units: Units::Mm,
            circle: None,
            allow_loss: false,
            original_summary: Summary {
                entity_count: 1,
                counts: BTreeMap::from([("Circle".into(), 1)]),
                extents: Some([[-10.0, -10.0], [10.0, 10.0]]),
            },
        }
    }
    #[test]
    fn rejects_unbounded_or_forged_drawing_inputs() {
        let base = fixture();
        assert!(base.validate().is_ok());
        for radius in [0.0, -1.0, f64::NAN, f64::INFINITY, 1_000_001.0] {
            let mut input = base.clone();
            input.circle = Some(Circle {
                center: [0.0, 0.0],
                radius,
            });
            assert!(input.validate().is_err());
        }
        let mut input = base.clone();
        input.source_name = "../pieza.dxf".into();
        assert!(input.validate().is_err());
        input = base.clone();
        input.original_summary.entity_count = 2;
        assert!(input.validate().is_err());
        input = base.clone();
        input.source_format = Format::Png;
        assert!(input.validate().is_err());
        input = base.clone();
        input.source_sha256 = "A".repeat(64);
        assert!(input.validate().is_err());
        input = base;
        input.cadcraft_version = "0.6.0".into();
        assert!(input.validate().is_err());
        assert!(serde_json::from_str::<Request>(r#"{"output_format":"dxf","units":"mm","circle":null,"allow_loss":false,"script":"erase all"}"#).is_err());
    }
    #[test]
    fn legacy_circle_reports_remain_readable_without_downgrading_new_requests() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let legacy = workspace.join("docs/research/cadcraft-native-2026-10-12");
        let run: serde_json::Value =
            serde_json::from_slice(&fs::read(legacy.join("run.json")).unwrap()).unwrap();
        let mut input: DrawingInput = serde_json::from_value(run["input"].clone()).unwrap();
        let mut result: DrawingResult =
            serde_json::from_slice(&fs::read(legacy.join("result.json")).unwrap()).unwrap();
        assert_eq!(input.schema_version, 1);
        assert_eq!(result.schema_version, 1);
        assert!(result.circle_readback.is_none());
        validate_artifacts(&legacy, &input, &result).unwrap();
        input.schema_version = 2;
        assert!(validate_artifacts(&legacy, &input, &result).is_err());
        result.schema_version = 2;
        assert!(validate_artifacts(&legacy, &input, &result).is_err());
        result.circle_readback = Some(CircleReadback {
            before_matches: 0,
            edited_matches: 1,
            final_matches: Some(1),
        });
        validate_artifacts(&legacy, &input, &result).unwrap();
        input.allow_loss = true;
        result.output_summary.as_mut().unwrap().counts = BTreeMap::from([("Circle".into(), 0)]);
        assert!(validate_artifacts(&legacy, &input, &result).is_err());
    }
    #[test]
    fn rejects_incomplete_or_inconsistent_circle_pages() {
        let circle = Circle {
            center: [30., 0.],
            radius: 2.,
        };
        let entity = serde_json::json!({"type":"Circle","geometry":{"type":"circle","center":{"x":30.,"y":0.,"z":0.},"radius":2.}});
        let page = serde_json::json!({"count":2,"returned":1,"offset":0,"entities":[entity]});
        let encode = |value: &serde_json::Value| serde_json::to_vec(value).unwrap();
        assert_eq!(
            circle_page(&encode(&page), 0, None, &circle).unwrap(),
            (2, 1, 1)
        );
        for (key, value) in [
            ("count", 1_000_001),
            ("offset", 1),
            ("returned", 2),
            ("count", 0),
        ] {
            let mut invalid = page.clone();
            invalid[key] = serde_json::json!(value);
            assert!(circle_page(&encode(&invalid), 0, None, &circle).is_err());
        }
        assert!(circle_page(&encode(&page), 0, Some(3), &circle).is_err());
        let mut empty = serde_json::json!({"count":1,"returned":0,"offset":0,"entities":[]});
        assert!(circle_page(&encode(&empty), 0, None, &circle).is_err());
        empty["count"] = serde_json::json!(0);
        assert_eq!(
            circle_page(&encode(&empty), 0, None, &circle).unwrap(),
            (0, 0, 0)
        );
        assert!(circle_page(b"not json", 0, None, &circle).is_err());
    }
    #[test]
    fn verifies_circle_geometry_instead_of_only_counts() {
        let circle = Circle {
            center: [30., 0.],
            radius: 2.,
        };
        let original = serde_json::json!({"type":"Circle","geometry":{"type":"circle","center":{"x":30.,"y":0.,"z":0.},"radius":2.}});
        assert!(circle_matches(&original, &circle).unwrap());
        for (field, value) in [("x", 31.), ("y", 1.), ("z", 1.)] {
            let mut altered = original.clone();
            altered["geometry"]["center"][field] = serde_json::json!(value);
            assert!(!circle_matches(&altered, &circle).unwrap());
        }
        let mut altered = original.clone();
        altered["geometry"]["radius"] = serde_json::json!(3.);
        assert!(!circle_matches(&altered, &circle).unwrap());
        altered["geometry"]["radius"] = serde_json::json!(0.);
        assert!(circle_matches(&altered, &circle).is_err());
        altered = original.clone();
        altered["geometry"]["center"]["z"] = serde_json::Value::Null;
        assert!(circle_matches(&altered, &circle).is_err());
        altered = original.clone();
        altered["type"] = serde_json::json!("Line");
        assert!(circle_matches(&altered, &circle).is_err());
        altered = original;
        altered["geometry"]["radius"] = serde_json::json!(2. + 1e-10);
        assert!(circle_matches(&altered, &circle).unwrap());
        altered["geometry"]["radius"] = serde_json::json!(2. + 1e-5);
        assert!(!circle_matches(&altered, &circle).unwrap());
    }
    #[test]
    fn reads_only_valid_insertion_unit_header() {
        assert_eq!(insertion_units(b"9\n$INSUNITS\n70\n4\n"), Some(4));
        assert_eq!(insertion_units(b"9\n$INSUNITS\n70\n1\n"), Some(1));
        assert_eq!(insertion_units(b"9\n$INSUNITS\n1\n4\n"), None);
        assert_eq!(insertion_units(b"9\n$INSUNITS\n70\nNaN\n"), None);
        assert!(!valid_png(b"\x89PNG\r\n\x1a\n"));
    }
    #[test]
    fn source_units_explain_the_confirmed_choice_without_reinterpreting_coordinates() {
        for units in [Units::Mm, Units::In] {
            assert!(check_source_units(0, units).is_ok());
            assert!(check_source_units(units.insertion_code() as u64, units).is_ok());
        }
        assert!(check_source_units(1, Units::Mm)
            .unwrap_err()
            .contains("elige pulgadas"));
        assert!(check_source_units(4, Units::In)
            .unwrap_err()
            .contains("elige milímetros"));
        assert!(check_source_units(2, Units::Mm)
            .unwrap_err()
            .contains("Convierte una copia"));
        let root = std::env::temp_dir().join(format!("hazlomexa-drawing-units-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("original.dxf");
        let original = b"9\n$INSUNITS\n70\n1\n";
        fs::write(&source, original).unwrap();
        let saved = root.join("captured.dxf");
        let error = prepare(
            &root.join("unavailable.exe"),
            &source,
            &saved,
            &root.join("profile"),
            Request {
                output_format: Format::Dxf,
                units: Units::Mm,
                circle: None,
                allow_loss: false,
            },
            Uuid::new_v4(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert!(error.contains("elige pulgadas"));
        assert!(!saved.exists());
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    #[ignore = "Actual read-only CADCraft source discovery before units/geometry are confirmed"]
    fn actual_cli_discovers_units_and_bounds_without_edit_parameters() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join(
            "target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe",
        );
        let root =
            std::env::temp_dir().join(format!("hazlomexa-drawing-discovery-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let metric =
            read_file(&workspace.join("docs/research/cadcraft-runtime-2026-10-11/metric.dxf"))
                .unwrap();
        let text = String::from_utf8(metric.clone())
            .unwrap()
            .replace("\r\n", "\n");
        let header = "$INSUNITS\n 70\n4";
        assert_eq!(text.matches(header).count(), 1);
        let mut metric_summary = None;
        let mut receipts = Vec::new();
        for (index, (name, bytes, code)) in [
            ("metric.dxf", metric, 4),
            (
                "imperial.dxf",
                read_file(
                    &workspace.join("docs/research/cadcraft-runtime-2026-10-11/imperial.dxf"),
                )
                .unwrap(),
                1,
            ),
            (
                "unknown.dxf",
                text.replace(header, "$INSUNITS\n 70\n0").into_bytes(),
                0,
            ),
            (
                "feet.dxf",
                text.replace(header, "$INSUNITS\n 70\n2").into_bytes(),
                2,
            ),
            (
                "native-inch.dwg",
                read_file(&workspace.join(
                    "docs/research/drawing-unit-guidance-2026-10-12/native-inch-drawing.dwg",
                ))
                .unwrap(),
                1,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let original = root.join(name);
            fs::write(&original, &bytes).unwrap();
            let saved = root.join(format!("captured-{name}"));
            let inspection = discover(
                &exe,
                &original,
                &saved,
                &root.join(format!("profile-{index}")),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(inspection.insunits, code);
            assert_eq!(inspection.source_sha256, hash(&bytes));
            assert_eq!(read_file(&original).unwrap(), bytes);
            assert_eq!(read_file(&saved).unwrap(), bytes);
            assert!(inspection.summary.entity_count > 0);
            assert!(inspection.summary.extents.is_some());
            if index == 0 {
                metric_summary = Some(inspection.summary.clone());
            }
            if index == 2 || index == 3 {
                assert_eq!(Some(&inspection.summary), metric_summary.as_ref());
            }
            receipts.push(inspection);
        }
        fs::write(
            root.join("discovery.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
        println!("Verified CADCraft read-only discovery: {}", root.display());
    }

    #[test]
    #[ignore = "Actual identified official CADCraft CLI and frozen drawing fixtures; run explicitly"]
    fn actual_cli_copies_converts_and_adds_circle() {
        let root = std::env::temp_dir().join(format!("hazlomexa-cadcraft-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join(
            "target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe",
        );
        let fixtures = workspace.join("docs/research/cadcraft-runtime-2026-10-11");
        for (index, (name, units, format, circle, allow_loss, success)) in [
            ("metric.dxf", Units::Mm, Format::Dxf, None, false, true),
            ("imperial.dxf", Units::In, Format::Dwg, None, false, true),
            (
                "metric.dxf",
                Units::Mm,
                Format::Dxf,
                Some(Circle {
                    center: [30.0, 0.0],
                    radius: 2.0,
                }),
                false,
                true,
            ),
            ("metric.dxf", Units::Mm, Format::Png, None, false, true),
            ("foreign.dxf", Units::Mm, Format::Dxf, None, false, false),
            ("foreign.dxf", Units::Mm, Format::Dxf, None, true, true),
            ("foreign.dxf", Units::Mm, Format::Png, None, false, false),
            (
                "imperial.dxf",
                Units::In,
                Format::Dwg,
                Some(Circle {
                    center: [30., 0.],
                    radius: 2.,
                }),
                false,
                true,
            ),
            (
                "metric.dxf",
                Units::Mm,
                Format::Png,
                Some(Circle {
                    center: [30., 0.],
                    radius: 2.,
                }),
                false,
                true,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let source = fixtures.join(name);
            let before = read_file(&source).unwrap();
            let saved = root.join(format!("{index}.dxf"));
            let stage = root.join(format!("stage-{index}"));
            fs::create_dir(&stage).unwrap();
            let request = Request {
                output_format: format,
                units,
                circle,
                allow_loss,
            };
            let input = prepare(
                &exe,
                &source,
                &saved,
                &root.join(format!("import-{index}")),
                request,
                Uuid::new_v4(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            let result = convert(
                &exe,
                &saved,
                &stage,
                &root.join(format!("profile-{index}")),
                &input,
                Arc::new(AtomicBool::new(false)),
            );
            assert_eq!(result.is_ok(), success, "{index}: {result:?}");
            assert_eq!(read_file(&source).unwrap(), before);
            if index == 1 {
                let dwg = stage.join("Drawing.dwg");
                let dwg_before = read_file(&dwg).unwrap();
                let error = prepare(
                    &exe,
                    &dwg,
                    &root.join("wrong-unit.dwg"),
                    &root.join("wrong-unit-profile"),
                    Request {
                        output_format: Format::Dxf,
                        units: Units::Mm,
                        circle: None,
                        allow_loss: false,
                    },
                    Uuid::new_v4(),
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap_err();
                assert!(error.contains("elige pulgadas"), "{error}");
                assert_eq!(read_file(&dwg).unwrap(), dwg_before);
                let confirmed = prepare(
                    &exe,
                    &dwg,
                    &root.join("confirmed-unit.dwg"),
                    &root.join("confirmed-unit-profile"),
                    Request {
                        output_format: Format::Dxf,
                        units: Units::In,
                        circle: None,
                        allow_loss: false,
                    },
                    Uuid::new_v4(),
                    Arc::new(AtomicBool::new(false)),
                )
                .unwrap();
                assert_eq!(confirmed.units, Units::In);
            }
            if let Ok(mut result) = result {
                assert_eq!(input.schema_version, 2);
                assert_eq!(result.schema_version, 2);
                validate_artifacts(&stage, &input, &result).unwrap();
                if input.circle.is_some() {
                    let proof = result.circle_readback.take().unwrap();
                    assert_eq!(proof.before_matches, 0);
                    assert_eq!(proof.edited_matches, 1);
                    assert_eq!(
                        proof.final_matches,
                        if input.output_format == Format::Png {
                            None
                        } else {
                            Some(1)
                        }
                    );
                    assert!(validate_artifacts(&stage, &input, &result).is_err());
                    result.circle_readback = Some(proof);
                }
                result.schema_version = 1;
                assert!(validate_artifacts(&stage, &input, &result).is_err());
                result.schema_version = 2;
                if input.allow_loss {
                    assert!(!result.warnings.is_empty());
                }
                fs::write(stage.join("Preview.png"), b"tampered").unwrap();
                assert!(validate_artifacts(&stage, &input, &result).is_err());
            } else {
                assert!(!stage.join("result.json").exists());
            }
        }
        let dwg_source = root.join("stage-1/Drawing.dwg");
        let request = Request {
            output_format: Format::Dxf,
            units: Units::In,
            circle: None,
            allow_loss: false,
        };
        let saved = root.join("dwg-import.dwg");
        let input = prepare(
            &exe,
            &dwg_source,
            &saved,
            &root.join("dwg-import-profile"),
            request.clone(),
            Uuid::new_v4(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let stage = root.join("dwg-stage");
        fs::create_dir(&stage).unwrap();
        let result = convert(
            &exe,
            &saved,
            &stage,
            &root.join("dwg-profile"),
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        validate_artifacts(&stage, &input, &result).unwrap();
        assert_eq!(
            insertion_units(&read_file(&stage.join("Drawing.dxf")).unwrap()),
            Some(1)
        );
        let mut wrong_units = request;
        wrong_units.units = Units::Mm;
        assert!(prepare(
            &exe,
            &dwg_source,
            &root.join("wrong-units.dwg"),
            &root.join("wrong-unit-profile"),
            wrong_units,
            Uuid::new_v4(),
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        let paginated_source = root.join("paginated.dxf");
        let mut dxf = String::from(
            "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n",
        );
        for index in 0..513 {
            dxf.push_str(&format!(
                "0\nCIRCLE\n5\n{:X}\n8\n0\n10\n{index}\n20\n0\n30\n0\n40\n2\n",
                0x1000 + index
            ));
        }
        dxf.push_str("0\nENDSEC\n0\nEOF\n");
        fs::write(&paginated_source, dxf).unwrap();
        let saved = root.join("paginated-original.dxf");
        let stage = root.join("paginated-stage");
        fs::create_dir(&stage).unwrap();
        let input = prepare(
            &exe,
            &paginated_source,
            &saved,
            &root.join("paginated-import-profile"),
            Request {
                output_format: Format::Dxf,
                units: Units::Mm,
                circle: Some(Circle {
                    center: [512., 0.],
                    radius: 2.,
                }),
                allow_loss: false,
            },
            Uuid::new_v4(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(input.original_summary.counts["Circle"], 513);
        let result = convert(
            &exe,
            &saved,
            &stage,
            &root.join("paginated-convert-profile"),
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let proof = result.circle_readback.as_ref().unwrap();
        assert_eq!(proof.before_matches, 1);
        assert_eq!(proof.edited_matches, 2);
        assert_eq!(proof.final_matches, Some(2));
        assert_eq!(
            result.output_summary.as_ref().unwrap().counts["Circle"],
            514
        );
        println!("Verified actual CADCraft fixture root: {}", root.display());
    }
}
