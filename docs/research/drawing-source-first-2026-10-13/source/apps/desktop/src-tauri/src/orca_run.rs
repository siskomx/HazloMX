//! Owned file-only slicing and bounded verification of the identified producer's 3MF.
use crate::{
    kicad_project as files,
    orca::{self, SliceInput},
    process::{run_owned, StopReason},
};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self},
    io::{Cursor, Read},
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

const OUTPUT_LIMIT: u64 = 128 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub layers: u32,
    pub max_z_mm: f64,
    pub bounds_mm: [f64; 3],
    pub settings: Value,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliceResult {
    pub schema_version: u32,
    pub job_id: uuid::Uuid,
    pub orca_version: String,
    pub summary: Summary,
    pub sha256: BTreeMap<String, String>,
}
fn xml(
    bytes: &[u8],
    points: &mut Vec<[f64; 3]>,
    indices: &mut Vec<[usize; 3]>,
) -> Result<(String, u32), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "XML 3MF no válido.")?;
    let mut reader = Reader::from_str(text);
    let mut units = String::new();
    let mut triangles = 0;
    let mut nodes = 0;
    let mut open = Vec::new();
    let mut root_seen = false;
    loop {
        let event = reader.read_event().map_err(|_| "XML 3MF no válido.")?;
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let n = e.local_name();
                if open.is_empty() {
                    if root_seen || n.as_ref() != "model" {
                        return Err("Raíz XML 3MF no válida.".into());
                    }
                    root_seen = true;
                }
                if start {
                    open.push(e.name().as_ref().to_owned());
                }
                if open.len() > 128 {
                    return Err("XML 3MF demasiado anidado.".into());
                }
                let mut attrs = BTreeMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| "Atributo 3MF no válido.")?;
                    let key = a.key.as_ref().to_owned();
                    let value = a
                        .normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map_err(|_| "Atributo 3MF no válido.")?
                        .into_owned();
                    if attrs.insert(key, value).is_some() {
                        return Err("Atributo 3MF duplicado.".into());
                    }
                }
                if n.as_ref() == "model" {
                    units = attrs.get("unit").cloned().ok_or("Faltan unidades 3MF.")?;
                    if attrs.get("xmlns").map(String::as_str)
                        != Some("http://schemas.microsoft.com/3dmanufacturing/core/2015/02")
                    {
                        return Err("Esquema 3MF no compatible.".into());
                    }
                }
                if let Some(transform) = attrs.get("transform") {
                    let a = transform
                        .split_whitespace()
                        .map(str::parse::<f64>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| "Transformación 3MF no válida.")?;
                    if a.len() != 12
                        || a.iter().any(|v| !v.is_finite())
                        || a[..9] != [1., 0., 0., 0., 1., 0., 0., 0., 1.]
                    {
                        return Err(
                            "La transformación de la malla no coincide con el laminado aprobado."
                                .into(),
                        );
                    }
                }
                if n.as_ref() == "vertex" {
                    let mut p = [0.; 3];
                    for (i, key) in ["x", "y", "z"].iter().enumerate() {
                        p[i] = attrs
                            .get(*key)
                            .and_then(|v| v.parse::<f64>().ok())
                            .filter(|v| v.is_finite() && v.abs() < 100_000.)
                            .ok_or("Vértice 3MF no válido.")?;
                    }
                    points.push(p);
                    if points.len() > 1_800_000 {
                        return Err("Malla 3MF demasiado grande.".into());
                    }
                }
                if n.as_ref() == "triangle" {
                    let mut face = [0usize; 3];
                    for (i, key) in ["v1", "v2", "v3"].iter().enumerate() {
                        face[i] = attrs
                            .get(*key)
                            .and_then(|v| v.parse().ok())
                            .filter(|v| *v < points.len())
                            .ok_or("Índice 3MF no válido.")?;
                    }
                    indices.push(face);
                    triangles += 1;
                    if triangles > 600_000 {
                        return Err("Malla 3MF demasiado grande.".into());
                    }
                }
                nodes += 1;
                if nodes > 2_000_000 {
                    return Err("XML 3MF demasiado grande.".into());
                }
            }
            Event::DocType(_) => return Err("No se permiten DTD en 3MF.".into()),
            Event::End(e) => {
                if open.pop().as_deref() != Some(e.name().as_ref()) {
                    return Err("Cierre XML 3MF no válido.".into());
                }
            }
            Event::Text(e) if open.is_empty() && !e.as_ref().trim().is_empty() => {
                return Err("Texto fuera del modelo 3MF.".into())
            }
            Event::Eof => break,
            _ => (),
        }
    }
    if !root_seen || !open.is_empty() {
        return Err("Modelo XML 3MF incompleto.".into());
    }
    Ok((units, triangles))
}
pub(crate) fn inspect(bytes: Vec<u8>, input: &SliceInput) -> Result<(Summary, Vec<u8>), String> {
    input.validate()?;
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Archivo 3MF no válido.")?;
    if archive.len() > 1024 {
        return Err("3MF con demasiados recursos.".into());
    }
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut item = archive.by_index(i).map_err(|_| "Entrada 3MF no válida.")?;
        let name = item
            .name()
            .map_err(|_| "Nombre 3MF no válido.")?
            .into_owned();
        files::relative(&name)?;
        if item.is_dir()
            || item.unix_mode().is_some_and(|v| v & 0o170000 == 0o120000)
            || item.enclosed_name().is_none()
            || item.size() > 64 * 1024 * 1024
        {
            return Err("Recurso 3MF fuera del límite.".into());
        }
        total = total
            .checked_add(item.size())
            .ok_or("3MF demasiado grande.")?;
        if total > 256 * 1024 * 1024 {
            return Err("3MF demasiado grande.".into());
        }
        let mut data = Vec::new();
        (&mut item)
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| "Entrada 3MF dañada.")?;
        if data.len() as u64 != item.size()
            || entries
                .insert(name.to_ascii_lowercase(), (name, data))
                .is_some()
        {
            return Err("Entradas 3MF duplicadas o dañadas.".into());
        }
    }
    let entry = |name: &str| -> Result<&[u8], String> {
        entries
            .get(&name.to_ascii_lowercase())
            .map(|v| v.1.as_slice())
            .ok_or_else(|| format!("Falta {name} en el 3MF."))
    };
    let mut points = Vec::new();
    let mut indices = Vec::new();
    let (unit, _) = xml(entry("3D/3dmodel.model")?, &mut points, &mut indices)?;
    if unit != "millimeter" {
        return Err("Las unidades 3MF no coinciden con el modelo normalizado.".into());
    }
    let parts: Vec<_> = entries
        .values()
        .filter(|(n, _)| n.starts_with("3D/Objects/") && n.ends_with(".model"))
        .collect();
    if parts.len() != 1 {
        return Err("Este laminado requiere un único objeto STL.".into());
    }
    let (unit, triangles) = xml(&parts[0].1, &mut points, &mut indices)?;
    if unit != "millimeter" || points.is_empty() || triangles != input.mesh.triangles {
        return Err("El 3MF alteró la estructura de la malla; revise el original antes de usar el resultado.".into());
    }
    let bounds: [f64; 3] = std::array::from_fn(|i| {
        points
            .iter()
            .map(|p| p[i])
            .fold(f64::NEG_INFINITY, f64::max)
            - points.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min)
    });
    if (0..3).any(|i| {
        (bounds[i] - (input.mesh.bounds_mm[1][i] - input.mesh.bounds_mm[0][i])).abs() > 0.002
    }) {
        return Err("Las dimensiones del 3MF no coinciden con la propuesta.".into());
    }
    let settings: Value = serde_json::from_slice(entry("Metadata/project_settings.config")?)
        .map_err(|_| "Configuración efectiva 3MF no válida.")?;
    for (key, value) in [
        (
            "printer_settings_id",
            Value::String(input.settings.machine.clone()),
        ),
        (
            "print_settings_id",
            Value::String(input.settings.process.clone()),
        ),
        (
            "filament_settings_id",
            serde_json::json!([input.settings.filament]),
        ),
        ("curr_bed_type", Value::String(input.settings.bed.clone())),
    ] {
        if settings.get(key) != Some(&value) {
            return Err("Los perfiles/cama efectivos no coinciden con la propuesta.".into());
        }
    }
    for (key, expected, vector) in [
        ("nozzle_diameter", input.settings.nozzle_mm, true),
        ("layer_height", input.settings.layer_mm, false),
        (
            "nozzle_temperature_initial_layer",
            input.settings.nozzle_c,
            true,
        ),
        (input.bed.temperature_key(), input.settings.bed_c, true),
        ("filament_diameter", input.settings.filament_mm, true),
    ] {
        let value = settings
            .get(key)
            .ok_or("Falta una condición efectiva de laminado.")?;
        let value = if vector {
            let a = value.as_array().ok_or("Condición efectiva no válida.")?;
            if a.len() != 1 {
                return Err("Configuración efectiva de varios materiales no aprobada.".into());
            }
            &a[0]
        } else {
            value
        };
        let n = value
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .or_else(|| value.as_f64())
            .ok_or("Condición efectiva no válida.")?;
        if !n.is_finite() || (n - expected).abs() > 0.0001 {
            return Err("Las condiciones físicas efectivas no coinciden con la propuesta.".into());
        }
    }
    let gcodes: Vec<_> = entries
        .values()
        .filter(|(n, _)| n.ends_with(".gcode"))
        .collect();
    if gcodes.len() != 1 {
        return Err("El laminado no contiene un único G-code.".into());
    }
    let gcode = gcodes[0].1.clone();
    let text = std::str::from_utf8(&gcode).map_err(|_| "G-code UTF-8 no válido.")?;
    if !text.contains(&format!("generated by OrcaSlicer {}", orca::VERSION))
        || !text.contains("; EXECUTABLE_BLOCK_START")
        || !text.contains("; CONFIG_BLOCK_END")
    {
        return Err("El G-code no corresponde al productor identificado.".into());
    }
    if orca::shape_signature(&points, &indices)? != input.mesh.shape_sha256 {
        return Err(
            "La geometría de las facetas del 3MF no coincide con la malla aprobada.".into(),
        );
    }
    let commands: BTreeSet<_> = text
        .lines()
        .filter_map(|l| l.split(';').next()?.split_whitespace().next())
        .collect();
    if !commands.contains("G21") || commands.contains("G20") {
        return Err("El G-code no conserva milímetros.".into());
    }
    let field = |prefix: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(prefix).map(str::trim))
    };
    let layers = field("; total layer number:")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0 && *v <= 100_000)
        .ok_or("Faltan capas verificables en el G-code.")?;
    let max_z = field("; max_z_height:")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0. && *v <= input.settings.printable_height_mm + 0.002)
        .ok_or("Altura del G-code fuera del perfil aprobado.")?;
    let effective = serde_json::json!({"machine":input.settings.machine,"process":input.settings.process,"filament":input.settings.filament,"bed":input.settings.bed,"nozzle_mm":input.settings.nozzle_mm,"layer_mm":input.settings.layer_mm,"nozzle_c":input.settings.nozzle_c,"bed_c":input.settings.bed_c});
    Ok((
        Summary {
            layers,
            max_z_mm: max_z,
            bounds_mm: bounds,
            settings: effective,
        },
        gcode,
    ))
}
pub fn run(
    executable: &Path,
    imported: &Path,
    stage: &Path,
    profile: &Path,
    input: &SliceInput,
    cancel: Arc<AtomicBool>,
) -> Result<SliceResult, String> {
    input.validate()?;
    let _runtime = orca::runtime(executable, &cancel)?;
    orca::validate_capture(imported, input)?;
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio de laminado no está vacío.".into());
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let _profile_pins = crate::project::pin_lineage(profile).map_err(|e| e.to_string())?;
    let original = stage.join("Original");
    fs::create_dir(&original).map_err(|e| e.to_string())?;
    for f in &input.files {
        orca::stop(&cancel)?;
        let rel = files::relative(&f.path)?;
        files::write_new(
            &original.join(rel),
            &files::read_regular(&imported.join(&f.path), orca::MODEL_LIMIT)?,
        )?;
    }
    let normalized = orca::mesh(
        &files::read_regular(&original.join("Original.stl"), orca::MODEL_LIMIT)?,
        input.units,
    )?
    .1;
    files::write_new(&stage.join("Model-mm.stl"), &normalized)?;
    let mut _input_pins = crate::project::pin_lineage(&original).map_err(|e| e.to_string())?;
    for captured in &input.files {
        let path = original.join(files::relative(&captured.path)?);
        _input_pins.extend(
            crate::project::pin_lineage(path.parent().ok_or("Falta la carpeta de fuentes.")?)
                .map_err(|e| e.to_string())?,
        );
        _input_pins.push(files::open_regular(&path, orca::MODEL_LIMIT)?);
    }
    _input_pins.push(files::open_regular(
        &stage.join("Model-mm.stl"),
        orca::MODEL_LIMIT,
    )?);
    let mut env = BTreeMap::new();
    for k in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(v) = std::env::var_os(k) {
            env.insert(k.into(), v);
        }
    }
    env.insert(
        "PATH".into(),
        format!(
            "{};{}\\System32",
            _runtime.root.display(),
            std::env::var("SystemRoot").map_err(|_| "Falta SystemRoot.")?
        )
        .into(),
    );
    for k in ["USERPROFILE", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP"] {
        env.insert(k.into(), profile.as_os_str().to_owned());
    }
    let data = profile.join("data");
    fs::create_dir(&data).map_err(|e| e.to_string())?;
    let private_output = profile.join("output");
    fs::create_dir(&private_output).map_err(|e| e.to_string())?;
    let _output_pins = crate::project::pin_lineage(&private_output).map_err(|e| e.to_string())?;
    // shortcut: upstream splits profile paths at semicolons; reject these until escaping is verified.
    if original.to_string_lossy().contains(';') {
        return Err(
            "OrcaSlicer no admite esta ruta de proyecto con punto y coma; seleccione otra carpeta."
                .into(),
        );
    }
    let mut args: Vec<OsString> = vec![
        "--datadir".into(),
        data.into_os_string(),
        "--debug".into(),
        "3".into(),
        "--load-settings".into(),
        format!(
            "{};{}",
            original.join("Machine.json").display(),
            original.join("Process.json").display()
        )
        .into(),
        "--load-filaments".into(),
        original.join("Filament.json").into_os_string(),
        "--slice".into(),
        "0".into(),
        "--outputdir".into(),
        private_output.as_os_str().to_owned(),
        "--export-3mf".into(),
        "Sliced.3mf".into(),
    ];
    args.push(stage.join("Model-mm.stl").into_os_string());
    let output = run_owned(
        executable,
        &args,
        profile,
        &env,
        cancel.clone(),
        Duration::from_secs(120),
    )
    .map_err(|e| e.to_string())?;
    match output.reason {
        StopReason::Cancelled => return Err("Laminado cancelado; el original se conserva.".into()),
        StopReason::TimedOut => {
            return Err(
                "OrcaSlicer agotó el tiempo; la copia de trabajo se conserva sin publicarse."
                    .into(),
            )
        }
        StopReason::Completed => (),
    }
    if output.exit_code != Some(0) || output.truncated {
        return Err(format!("OrcaSlicer no completó el laminado (código {:?}). Revise la malla y los perfiles; no se publicó el resultado.",output.exit_code));
    }
    orca::stop(&cancel)?;
    let sliced = files::read_regular(&private_output.join("Sliced.3mf"), OUTPUT_LIMIT)?;
    let (summary, gcode) = inspect(sliced.clone(), input)?;
    if files::read_regular(&private_output.join("plate_1.gcode"), 64 * 1024 * 1024)? != gcode {
        return Err("El G-code exportado no coincide con el 3MF.".into());
    }
    files::write_new(&stage.join("Sliced.3mf"), &sliced)?;
    files::write_new(&stage.join("Toolpath.gcode"), &gcode)?;
    let mut hashes = BTreeMap::new();
    for f in &input.files {
        let name = format!("Original/{}", f.path);
        hashes.insert(name, f.sha256.clone());
    }
    for n in ["Model-mm.stl", "Sliced.3mf", "Toolpath.gcode"] {
        hashes.insert(
            n.into(),
            files::digest(&files::read_regular(&stage.join(n), OUTPUT_LIMIT)?),
        );
    }
    let result = SliceResult {
        schema_version: 1,
        job_id: input.job_id,
        orca_version: orca::VERSION.into(),
        summary,
        sha256: hashes,
    };
    validate_artifacts(stage, input, &result)?;
    files::write_new(
        &stage.join("result.json"),
        &serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )?;
    Ok(result)
}
pub fn validate_artifacts(
    directory: &Path,
    input: &SliceInput,
    result: &SliceResult,
) -> Result<(), String> {
    input.validate()?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || result.orca_version != orca::VERSION
        || result.sha256.len() != input.files.len() + 3
    {
        return Err("Resultado de laminado ajeno a la propuesta.".into());
    }
    orca::validate_capture(&directory.join("Original"), input)?;
    let normalized = orca::mesh(
        &files::read_regular(&directory.join("Original/Original.stl"), orca::MODEL_LIMIT)?,
        input.units,
    )?
    .1;
    if files::read_regular(&directory.join("Model-mm.stl"), orca::MODEL_LIMIT)? != normalized {
        return Err("La normalización de unidades no coincide con la propuesta.".into());
    }
    let (summary, gcode) = inspect(
        files::read_regular(&directory.join("Sliced.3mf"), OUTPUT_LIMIT)?,
        input,
    )?;
    if !orca::matches_stored(&summary, &result.summary)?
        || files::read_regular(&directory.join("Toolpath.gcode"), 64 * 1024 * 1024)? != gcode
    {
        return Err("El informe o G-code no coinciden con el archivo laminado.".into());
    }
    for (name, expected) in &result.sha256 {
        files::relative(name)?;
        if !files::valid_hash(expected)
            || files::digest(&files::read_regular(&directory.join(name), OUTPUT_LIMIT)?)
                != *expected
        {
            return Err("Los archivos laminados cambiaron después de verificarse.".into());
        }
    }
    for f in &input.files {
        if result.sha256.get(&format!("Original/{}", f.path)) != Some(&f.sha256) {
            return Err("Faltan fuentes aprobadas.".into());
        }
    }
    for n in ["Model-mm.stl", "Sliced.3mf", "Toolpath.gcode"] {
        if !result.sha256.contains_key(n) {
            return Err("Faltan salidas verificadas de laminado.".into());
        }
    }
    for path in crate::kicad_run::files(directory)? {
        let name = path
            .strip_prefix(directory)
            .map_err(|_| "Salida fuera del laminado.")?
            .to_str()
            .ok_or("Nombre de salida no válido.")?
            .replace('\\', "/");
        if name != "result.json" && !result.sha256.contains_key(&name) {
            return Err("El laminado contiene archivos no verificados.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orca::{Bed, Request, Units};
    use uuid::Uuid;
    #[test]
    fn persisted_inch_report_matches_readback_without_relaxing_measurements() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("docs/research/orca-report-roundtrip-2026-10-12");
        let run: crate::project::Snapshot =
            serde_json::from_slice(&fs::read(root.join("failed-run.json")).unwrap()).unwrap();
        let crate::project::OperationInput::Orca(input) = run.input else {
            panic!("Wrong fixture");
        };
        let directory = root.join("fixture");
        let result: SliceResult =
            serde_json::from_slice(&fs::read(directory.join("result.json")).unwrap()).unwrap();
        let (actual, _) = inspect(fs::read(directory.join("Sliced.3mf")).unwrap(), &input).unwrap();
        assert_ne!(actual.bounds_mm[0], result.summary.bounds_mm[0]);
        validate_artifacts(&directory, &input, &result).unwrap();
        for field in ["bounds", "height", "layers", "settings", "hash"] {
            let mut changed: SliceResult =
                serde_json::from_slice(&fs::read(directory.join("result.json")).unwrap()).unwrap();
            match field {
                "bounds" => changed.summary.bounds_mm[0] += 0.000001,
                "height" => changed.summary.max_z_mm += 0.000001,
                "layers" => changed.summary.layers += 1,
                "settings" => changed.summary.settings["bed_c"] = serde_json::json!(99.),
                _ => {
                    changed
                        .sha256
                        .insert("Toolpath.gcode".into(), "0".repeat(64));
                }
            }
            assert!(
                validate_artifacts(&directory, &input, &changed).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn real_archive_contract_rejects_units_settings_and_gcode_tampering() {
        use std::io::Write;
        let (root, model, profiles) = orca::tests::fixture();
        let input = orca::capture(
            &root.join("profiles"),
            &model,
            profiles,
            &root.join("capture"),
            Uuid::new_v4(),
            Request {
                units: Units::Mm,
                bed: Bed::Textured,
                bed_temperature_c: 45.,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let bytes = include_bytes!("../../../../tests/orca/fixtures/valid-slice.3mf");
        assert_eq!(inspect(bytes.to_vec(), &input).unwrap().0.layers, 50);
        for (entry, old, new) in [
            ("3D/3dmodel.model", "unit=\"millimeter\"", "unit=\"inch\""),
            (
                "3D/Objects/Cube-mm.stl_1.model",
                "x=\"5\" y=\"5\" z=\"-5\"",
                "x=\"4\" y=\"5\" z=\"-5\"",
            ),
            (
                "Metadata/project_settings.config",
                "Prusa MK3S 0.4 nozzle",
                "Other machine",
            ),
            ("Metadata/plate_1.gcode", "G21", "G20"),
        ] {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                let name = file.name().unwrap().into_owned();
                let mut content = Vec::new();
                file.read_to_end(&mut content).unwrap();
                if name == entry {
                    let text = std::str::from_utf8(&content).unwrap();
                    assert!(text.contains(old));
                    content = text.replace(old, new).into_bytes();
                }
                writer
                    .start_file(&name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(&content).unwrap();
            }
            let damaged = writer.finish().unwrap().into_inner();
            assert!(inspect(damaged, &input).is_err(), "accepted {entry} tamper");
        }
        assert!(xml(
            b"<!DOCTYPE model><model/>",
            &mut Vec::new(),
            &mut Vec::new()
        )
        .is_err());
        assert!(xml(b"<model unit=\"millimeter\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">", &mut Vec::new(),&mut Vec::new()).is_err());
    }
    #[test]
    #[ignore = "Actual official OrcaSlicer Windows x64 2.4.2 and explicit stock profiles"]
    fn actual_orca_captures_profiles_and_slices_verified_files() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
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
        let root = std::env::temp_dir().join(format!("hazlomexa-orca-run-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let model = root.join("Original.stl");
        let original =
            include_bytes!("../../../../docs/research/orca-runtime-2026-10-11/Cube-mm.stl");
        fs::write(&model, original).unwrap();
        let imported = root.join("capture");
        let input = orca::capture(
            &profiles,
            &model,
            selected.clone(),
            &imported,
            Uuid::new_v4(),
            Request {
                units: Units::Mm,
                bed: Bed::Textured,
                bed_temperature_c: 45.,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let result = run(
            &exe,
            &imported,
            &stage,
            &root.join("profile"),
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(result.summary.layers, 50);
        assert_eq!(result.summary.bounds_mm, [10., 10., 10.]);
        validate_artifacts(&stage, &input, &result).unwrap();
        assert_eq!(fs::read(&model).unwrap(), original);
        let mut inches = original.to_vec();
        for facet in inches[84..].chunks_exact_mut(50) {
            for vertex in facet[12..48].chunks_exact_mut(12) {
                for coordinate in vertex.chunks_exact_mut(4) {
                    let value = f32::from_le_bytes(coordinate.try_into().unwrap()) / 25.4;
                    coordinate.copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        let inch_model = root.join("Original-in.stl");
        fs::write(&inch_model, &inches).unwrap();
        let inch_capture = root.join("capture-in");
        let inch_input = orca::capture(
            &profiles,
            &inch_model,
            selected,
            &inch_capture,
            Uuid::new_v4(),
            Request {
                units: Units::In,
                bed: Bed::Textured,
                bed_temperature_c: 45.,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let inch_stage = root.join("stage-in");
        fs::create_dir(&inch_stage).unwrap();
        let inch_result = run(
            &exe,
            &inch_capture,
            &inch_stage,
            &root.join("profile-in"),
            &inch_input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(inch_result
            .summary
            .bounds_mm
            .iter()
            .all(|v| (*v - 10.).abs() < 0.002));
        assert_eq!(inch_result.summary.layers, 50);
        validate_artifacts(&inch_stage, &inch_input, &inch_result).unwrap();
        assert_eq!(fs::read(inch_model).unwrap(), inches);
        fs::write(stage.join("Toolpath.gcode"), b"tampered").unwrap();
        assert!(validate_artifacts(&stage, &input, &result).is_err());
        println!("Verified OrcaSlicer captured adapter: {}", root.display());
    }
}
