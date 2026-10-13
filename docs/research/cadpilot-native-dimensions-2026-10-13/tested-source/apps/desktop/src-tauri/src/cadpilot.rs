//! Fixed, source-bound selected-copy CADPilot operations, without its broad RPC endpoint.
use crate::{
    kicad_project as files,
    orca::Units,
    process::{run_owned, StopReason},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;

const MACRO: &str = include_str!("../resources/freecad/cadpilot-copy.FCMacro");
const POLICY: &str = include_str!("../resources/freecad/cadpilot_policy.py");
const MODULE_MANIFEST: &str =
    include_str!("../../../../docs/research/cadpilot-primitives-2026-10-11/status.json");
const LIMIT: u64 = 32 * 1024 * 1024;
pub const VERSION: &str = "0.7.7";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Inspect,
    EditBox,
    AddBox,
    AddCylinder,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub operation: Operation,
    pub object: String,
    pub units: Units,
    pub parameters: BTreeMap<String, f64>,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.object.is_empty()
            || self.object.len() > 64
            || !self
                .object
                .bytes()
                .enumerate()
                .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || i > 0 && c.is_ascii_digit())
        {
            return Err("Seleccione el nombre interno del objeto.".into());
        }
        let keys: &[&str] = match self.operation {
            Operation::Inspect => &[],
            Operation::EditBox => &["length", "width", "height"],
            Operation::AddBox => &["length", "width", "height", "x", "y", "z"],
            Operation::AddCylinder => &["radius", "height", "x", "y", "z"],
        };
        if self.parameters.len() != keys.len()
            || !keys.iter().all(|k| self.parameters.contains_key(*k))
        {
            return Err("Parámetros de CADPilot no compatibles.".into());
        }
        for (key, value) in &self.parameters {
            let mm = value * self.units.scale();
            if !mm.is_finite()
                || if ["x", "y", "z"].contains(&key.as_str()) {
                    mm.abs() > 10000.
                } else {
                    !(0.1..=1000.).contains(&mm)
                }
            {
                return Err("Use medidas finitas dentro del rango permitido.".into());
            }
        }
        Ok(())
    }
    fn mm(&self) -> BTreeMap<String, f64> {
        self.parameters
            .iter()
            .map(|(k, v)| (k.clone(), v * self.units.scale()))
            .collect()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub cadpilot_version: String,
    pub source_name: String,
    pub request: Request,
    pub files: Vec<files::CapturedFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<Preview>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub catalog: BTreeMap<String, String>,
    pub measurements: BTreeMap<String, Value>,
}
fn modules() -> Result<BTreeMap<String, String>, String> {
    let manifest: Value = serde_json::from_str(MODULE_MANIFEST).map_err(|e| e.to_string())?;
    serde_json::from_value(manifest["module_hashes"].clone()).map_err(|e| e.to_string())
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        self.request.validate()?;
        if self.schema_version != 1
            || self.job_id.is_nil()
            || self.cadpilot_version != VERSION
            || self.source_name.len() > 255
            || !self.source_name.to_ascii_lowercase().ends_with(".fcstd")
            || self.source_name.contains(['/', '\\', ':', '"'])
            || self.source_name.chars().any(char::is_control)
        {
            return Err("Propuesta CADPilot no compatible.".into());
        }
        let expected = modules()?;
        if self.files.len() != expected.len() + 1 {
            return Err("Fuentes CADPilot incompletas.".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for f in &self.files {
            files::relative(&f.path)?;
            if !seen.insert(f.path.clone())
                || !files::valid_hash(&f.sha256)
                || f.bytes == 0
                || f.bytes > LIMIT
            {
                return Err("Fuente CADPilot no válida.".into());
            }
            if f.path != "Original.FCStd"
                && !expected.iter().any(|(name, hash)| {
                    f.path == format!("addon/rpc_server/{name}") && f.sha256 == *hash
                })
            {
                return Err("La identidad de CADPilot cambió.".into());
            }
        }
        if !seen.contains("Original.FCStd") {
            return Err("Falta el documento seleccionado.".into());
        }
        if let Some(preview) = &self.preview {
            if preview.catalog.is_empty()
                || preview.catalog.len() > 32
                || preview.catalog.keys().ne(preview.measurements.keys())
                || preview.catalog.values().any(|kind| {
                    ![
                        "Part::Box",
                        "Part::Cylinder",
                        "Part::Sphere",
                        "Part::Cone",
                        "Part::Torus",
                    ]
                    .contains(&kind.as_str())
                })
                || serde_json::to_vec(preview)
                    .map_err(|e| e.to_string())?
                    .len()
                    > 65536
            {
                return Err("Vista de medición CADPilot no compatible.".into());
            }
        }
        Ok(())
    }
}
pub fn capture(
    source: &Path,
    addon: &Path,
    destination: &Path,
    id: Uuid,
    request: Request,
    cancel: Arc<AtomicBool>,
) -> Result<Input, String> {
    request.validate()?;
    let source_name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Nombre de documento no válido.")?
        .to_owned();
    let mut pins = crate::project::pin_lineage(source.parent().ok_or("Falta carpeta de origen.")?)
        .map_err(|e| e.to_string())?;
    pins.extend(crate::project::pin_lineage(addon).map_err(|e| e.to_string())?);
    let addon = if addon.join("rpc_server").is_dir() {
        addon.to_owned()
    } else if addon.join("addon/CADPilot/rpc_server").is_dir() {
        addon.join("addon/CADPilot")
    } else {
        return Err(
            "Selecciona la carpeta del paquete CADPilot 0.7.7 o su carpeta addon/CADPilot.".into(),
        );
    };
    let mut contents = vec![("Original.FCStd".to_owned(), source.to_owned(), None)];
    for (name, hash) in modules()? {
        contents.push((
            format!("addon/rpc_server/{name}"),
            addon.join("rpc_server").join(name),
            Some(hash),
        ));
    }
    let mut captured = Vec::new();
    let mut bytes = Vec::new();
    for (name, path, expected) in contents {
        crate::orca::stop(&cancel)?;
        pins.extend(
            crate::project::pin_lineage(path.parent().ok_or("Falta carpeta de fuente.")?)
                .map_err(|e| e.to_string())?,
        );
        let file = files::open_regular(&path, LIMIT)?;
        let data = files::read_regular(&path, LIMIT)?;
        let hash = files::digest(&data);
        if expected.is_some_and(|expected| expected != hash) {
            return Err("Seleccione CADPilot 0.7.7 identificado.".into());
        }
        pins.push(file);
        captured.push(files::CapturedFile {
            path: name.clone(),
            sha256: hash,
            bytes: data.len() as u64,
        });
        bytes.push((name, data));
    }
    let input = Input {
        schema_version: 1,
        job_id: id,
        cadpilot_version: VERSION.into(),
        source_name,
        request,
        files: captured,
        preview: None,
    };
    input.validate()?;
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    for (name, data) in bytes {
        let path = destination.join(name);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        files::write_new(&path, &data)?;
    }
    Ok(input)
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultDocument {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub success: bool,
    pub cadpilot_version: String,
    pub freecad_version: String,
    pub operation: Operation,
    pub object: String,
    pub parameters_mm: BTreeMap<String, f64>,
    pub source_sha256: String,
    pub catalog: BTreeMap<String, String>,
    pub before: BTreeMap<String, Value>,
    pub after: BTreeMap<String, Value>,
    pub sha256: BTreeMap<String, String>,
}
fn operation_bytes(input: &Input) -> Result<Vec<u8>, String> {
    let mut raw = serde_json::to_value(&input.request).map_err(|e| e.to_string())?;
    raw["schema_version"] = 1.into();
    raw["job_id"] = input.job_id.to_string().into();
    raw["cadpilot_version"] = VERSION.into();
    serde_json::to_vec(&raw).map_err(|e| e.to_string())
}
fn rejection(text: &str) -> String {
    let reason = text
        .lines()
        .find_map(|line| line.strip_prefix("ValueError: "));
    let message=match reason {
        Some("GUI document state is not yet supported by the selected-copy adapter")=>"Este flujo todavía no admite documentos con estado GUI; conserve el original y use una combinación de documento verificada.",
        Some("This document requires an uncertified object type")=>"El documento contiene tipos de objeto aún no admitidos; este flujo acepta primitivas nativas independientes.",
        Some("Document dependencies are not certified"|"Document expressions, Python and external links are not certified")=>"El documento contiene dependencias o expresiones aún no verificadas; no se cargó ni modificó el original.",
        Some("CADPilot primitive identity mismatch")=>"Seleccione la carpeta del CADPilot 0.7.7 identificado; sus archivos no coinciden.",
        Some("STEP round trip differs from the edited copy"|"STEP solid has invalid measurements"|"STEP solid count differs from the edited copy"|"STEP solid dimensions or position differ from the edited copy")=>"El STEP exportado no coincide con las medidas o la posición de la copia editada. No se publicó ningún resultado; el original se conserva. Revisa la copia y vuelve a preparar la operación.",
        _=>"CADPilot rechazó el documento o no completó la operación; revise los tipos de objetos y parámetros. No se publicó ningún resultado.",
    };
    message.into()
}
pub fn validate_result(
    directory: &Path,
    input: &Input,
    result: &ResultDocument,
) -> Result<(), String> {
    input.validate()?;
    if files::read_regular(&directory.join("input.json"), 16384)? != operation_bytes(input)? {
        return Err("La solicitud conservada no coincide con la operación aprobada.".into());
    }
    let source = input
        .files
        .iter()
        .find(|f| f.path == "Original.FCStd")
        .ok_or("Falta original.")?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || !result.success
        || result.cadpilot_version != VERSION
        || result.freecad_version != crate::freecad::SUPPORTED_VERSION
        || result.operation != input.request.operation
        || result.object != input.request.object
        || result.parameters_mm != input.request.mm()
        || result.source_sha256 != source.sha256
        || result.before.is_empty()
        || result.before.len() > 32
        || result.catalog.len() != result.before.len()
        || result.catalog.keys().ne(result.before.keys())
    {
        return Err("El resultado no corresponde a la operación CADPilot aprobada.".into());
    }
    for f in &input.files {
        let data = files::read_regular(&directory.join(&f.path), LIMIT)?;
        if data.len() as u64 != f.bytes || files::digest(&data) != f.sha256 {
            return Err("Una fuente CADPilot cambió.".into());
        }
    }
    if input.preview.as_ref().is_some_and(|preview| {
        preview.catalog != result.catalog || preview.measurements != result.before
    }) {
        return Err("La geometría inicial no coincide con la propuesta revisada.".into());
    }
    let mut expected = result
        .before
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if matches!(result.operation, Operation::AddBox | Operation::AddCylinder)
        && !expected.insert(result.object.clone())
    {
        return Err("El nuevo objeto ya existía.".into());
    }
    if result
        .after
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        != expected
    {
        return Err("CADPilot alteró el catálogo de objetos.".into());
    }
    for report in result.before.values().chain(result.after.values()) {
        let volume = report["volume_mm3"].as_f64().ok_or("Falta volumen.")?;
        let bounds = report["bbox"].as_object().ok_or("Faltan medidas.")?;
        if report["success"] != true
            || report["is_valid"] != true
            || report["counts"]["solids"] != 1
            || volume <= 0.
            || !volume.is_finite()
            || bounds.len() != 6
            || bounds.values().any(|v| {
                v.as_f64()
                    .is_none_or(|n| !n.is_finite() || n.abs() > 20000.)
            })
        {
            return Err("Geometría CADPilot no válida.".into());
        }
        if let Some(dimensions) = report.get("dimensions_mm") {
            let dimensions = dimensions
                .as_object()
                .ok_or("Faltan dimensiones nativas.")?;
            if dimensions.len() != 3
                || ["length", "width", "height"].iter().any(|key| {
                    dimensions
                        .get(*key)
                        .and_then(Value::as_f64)
                        .is_none_or(|n| !n.is_finite() || n <= 0.)
                })
            {
                return Err("Dimensiones nativas CADPilot no válidas.".into());
            }
            let native_volume: f64 = dimensions
                .values()
                .map(|value| value.as_f64().unwrap())
                .product();
            if !native_volume.is_finite()
                || (native_volume - volume).abs() > (volume * 0.00002).max(0.01)
            {
                return Err("Las dimensiones nativas no coinciden con la geometría medida.".into());
            }
        }
    }
    for (name, before) in &result.before {
        if (result.operation != Operation::EditBox || *name != result.object)
            && result.after.get(name) != Some(before)
        {
            return Err("Cambió un objeto no seleccionado.".into());
        }
    }
    if result.operation != Operation::Inspect {
        let target = result
            .after
            .get(&result.object)
            .ok_or("Falta el objeto aprobado.")?;
        let mm = input.request.mm();
        let expected_volume = if result.operation == Operation::AddCylinder {
            std::f64::consts::PI * mm["radius"].powi(2) * mm["height"]
        } else {
            mm["length"] * mm["width"] * mm["height"]
        };
        let volume = target["volume_mm3"]
            .as_f64()
            .ok_or("Falta volumen aprobado.")?;
        if (volume - expected_volume).abs() > (expected_volume * 0.00002).max(0.01) {
            return Err("El volumen no coincide con los parámetros aprobados.".into());
        }
    }
    let expected_outputs = if result.operation == Operation::Inspect {
        vec![]
    } else {
        vec!["Edited.FCStd", "Edited.step"]
    };
    if result.sha256.len() != expected_outputs.len()
        || !expected_outputs
            .iter()
            .all(|name| result.sha256.contains_key(*name))
    {
        return Err("Salidas CADPilot incompletas.".into());
    }
    for (name, hash) in &result.sha256 {
        if !files::valid_hash(hash)
            || files::digest(&files::read_regular(&directory.join(name), LIMIT)?) != *hash
        {
            return Err("Cambió un archivo CADPilot verificado.".into());
        }
    }
    if files::digest(&files::read_regular(
        &directory.join("Working.FCStd"),
        LIMIT,
    )?) != source.sha256
    {
        return Err("Cambió la copia inicial del documento.".into());
    }
    for path in crate::kicad_run::files(directory)? {
        let name = path
            .strip_prefix(directory)
            .map_err(|_| "Salida fuera del proyecto.")?
            .to_str()
            .ok_or("Nombre de salida no válido.")?
            .replace('\\', "/");
        if !["Working.FCStd", "input.json", "result.json"].contains(&name.as_str())
            && !input.files.iter().any(|f| f.path == name)
            && !result.sha256.contains_key(&name)
        {
            return Err("CADPilot produjo archivos no verificados.".into());
        }
    }
    Ok(())
}
pub fn run(
    executable: &Path,
    captured: &Path,
    stage: &Path,
    profile: &Path,
    input: &Input,
    cancel: Arc<AtomicBool>,
) -> Result<ResultDocument, String> {
    input.validate()?;
    crate::orca::stop(&cancel)?;
    if !executable.is_absolute() || !stage.is_absolute() || !profile.is_absolute() {
        return Err("Use rutas absolutas para CADPilot.".into());
    }
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El área de trabajo no está vacía.".into());
    }
    let mut pins = crate::project::pin_lineage(captured).map_err(|e| e.to_string())?;
    for f in &input.files {
        let path = captured.join(&f.path);
        pins.extend(
            crate::project::pin_lineage(path.parent().unwrap()).map_err(|e| e.to_string())?,
        );
        pins.push(files::open_regular(&path, LIMIT)?);
        let data = files::read_regular(&path, LIMIT)?;
        if data.len() as u64 != f.bytes || files::digest(&data) != f.sha256 {
            return Err("Las fuentes CADPilot cambiaron.".into());
        }
        let path = stage.join(&f.path);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        files::write_new(&path, &data)?;
        pins.extend(
            crate::project::pin_lineage(path.parent().unwrap()).map_err(|e| e.to_string())?,
        );
        pins.push(files::open_regular(&path, LIMIT)?);
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let profile = crate::freecad::configuration_path(profile).map_err(|e| e.to_string())?;
    let mut env = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(v) = std::env::var_os(key) {
            env.insert(key.into(), v);
        }
    }
    for (key, name) in [
        ("FREECAD_USER_HOME", "home"),
        ("FREECAD_USER_DATA", "data"),
        ("FREECAD_USER_TEMP", "temp"),
    ] {
        let path = profile.join(name);
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        env.insert(key.into(), path.into_os_string());
    }
    for key in ["TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
        env.insert(key.into(), profile.join("temp").into_os_string());
    }
    files::write_new(&profile.join("cadpilot-copy.FCMacro"), MACRO.as_bytes())?;
    files::write_new(&profile.join("cadpilot_policy.py"), POLICY.as_bytes())?;
    files::write_new(&stage.join("input.json"), &operation_bytes(input)?)?;
    pins.extend(crate::project::pin_lineage(&profile).map_err(|e| e.to_string())?);
    for name in ["cadpilot-copy.FCMacro", "cadpilot_policy.py"] {
        pins.push(files::open_regular(&profile.join(name), LIMIT)?);
    }
    pins.push(files::open_regular(&stage.join("input.json"), 16384)?);
    let args: Vec<OsString> = vec![
        "--user-cfg".into(),
        profile.join("user.cfg").into_os_string(),
        "--system-cfg".into(),
        profile.join("system.cfg").into_os_string(),
        profile.join("cadpilot-copy.FCMacro").into_os_string(),
    ];
    let output = run_owned(
        executable,
        &args,
        stage,
        &env,
        cancel.clone(),
        Duration::from_secs(120),
    )
    .map_err(|e| e.to_string())?;
    if output.reason != StopReason::Completed || cancel.load(Ordering::SeqCst) {
        return Err("CADPilot cancelado o tiempo agotado; la copia permanece privada.".into());
    }
    if output.exit_code != Some(0) || output.truncated {
        let failure = files::read_regular(&stage.join("failure.txt"), 16384)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default();
        return Err(rejection(&failure));
    }
    let result: ResultDocument = serde_json::from_slice(&files::read_regular(
        &stage.join("result.json"),
        1024 * 1024,
    )?)
    .map_err(|e| e.to_string())?;
    validate_result(stage, input, &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_rejections_are_actionable_and_unknown_text_is_not_exposed() {
        assert!(rejection(
            "ValueError: GUI document state is not yet supported by the selected-copy adapter"
        )
        .contains("estado GUI"));
        assert!(!rejection("RuntimeError: fixture-private-token").contains("fixture-private-token"));
        assert!(rejection(
            "ValueError: STEP solid dimensions or position differ from the edited copy"
        )
        .contains("medidas o la posición"));
    }
    #[test]
    fn package_folder_selection_keeps_module_identity_checks() {
        let root =
            std::env::temp_dir().join(format!("hazlomexa-cadpilot-package-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("Original.FCStd");
        fs::write(&source, b"owned source fixture").unwrap();
        let request = Request {
            operation: Operation::Inspect,
            object: "Document".into(),
            units: Units::Mm,
            parameters: Default::default(),
        };
        let destination = root.join("capture");
        let capture_error = |addon: &Path| {
            capture(
                &source,
                addon,
                &destination,
                Uuid::new_v4(),
                request.clone(),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap_err()
        };
        assert!(capture_error(&root).contains("carpeta del paquete CADPilot"));
        let addon = root.join("addon/CADPilot");
        fs::create_dir_all(addon.join("rpc_server")).unwrap();
        for (name, _) in modules().unwrap() {
            fs::write(addon.join("rpc_server").join(name), b"wrong module").unwrap();
        }
        for selected in [&root, &addon] {
            assert_eq!(
                capture_error(selected),
                "Seleccione CADPilot 0.7.7 identificado."
            );
            assert!(!destination.exists());
        }
        assert_eq!(fs::read(&source).unwrap(), b"owned source fixture");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn requests_reject_untyped_or_unbounded_parameters() {
        let mut request = Request {
            operation: Operation::EditBox,
            object: "Target".into(),
            units: Units::In,
            parameters: [
                ("length".into(), 0.5),
                ("width".into(), 1.),
                ("height".into(), 0.3),
            ]
            .into(),
        };
        request.validate().unwrap();
        assert_eq!(request.mm()["length"], 12.7);
        for bad in [f64::NAN, f64::INFINITY, -1., 0., 1e300] {
            request.parameters.insert("length".into(), bad);
            assert!(request.validate().is_err());
        }
        request.parameters.insert("length".into(), 0.5);
        request.object = "../outside".into();
        assert!(request.validate().is_err());
    }
    #[test]
    #[ignore = "Actual identified FreeCAD 1.1.4/CADPilot 0.7.7 owned selected-copy adapter"]
    fn actual_cadpilot_owned_selected_copy_adapter() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let root = std::env::temp_dir().join(format!("hazlomexa-cadpilot-run-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("Selected.FCStd");
        let bytes = include_bytes!(
            "../../../../docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd"
        );
        fs::write(&source, bytes).unwrap();
        let addon=workspace.join("target/cadpilot-probe/source/cadpilot-58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot");
        let input = capture(
            &source,
            &addon,
            &root.join("capture"),
            Uuid::new_v4(),
            Request {
                operation: Operation::EditBox,
                object: "Target".into(),
                units: Units::In,
                parameters: [
                    ("length".into(), 0.5),
                    ("width".into(), 1.),
                    ("height".into(), 0.3),
                ]
                .into(),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let exe=std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).unwrap().join("HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe");
        let result = run(
            &exe,
            &root.join("capture"),
            &stage,
            &root.join("profile"),
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert_eq!(result.parameters_mm["length"], 12.7);
        assert_eq!(
            result.before["Target"]["dimensions_mm"],
            serde_json::json!({"length":10.,"width":20.,"height":30.})
        );
        assert_eq!(
            result.after["Target"]["dimensions_mm"],
            serde_json::to_value(input.request.mm()).unwrap()
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        validate_result(&stage, &input, &result).unwrap();
        for bad in [
            serde_json::json!({"length":11.,"width":20.,"height":30.}),
            serde_json::json!({"length":"10","width":20.,"height":30.}),
            serde_json::json!({"length":0.,"width":20.,"height":30.}),
        ] {
            let mut forged: ResultDocument =
                serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();
            forged.before.get_mut("Target").unwrap()["dimensions_mm"] = bad;
            assert!(validate_result(&stage, &input, &forged).is_err());
        }
        let cylinder_input = capture(
            &source,
            &addon,
            &root.join("cylinder-capture"),
            Uuid::new_v4(),
            Request {
                operation: Operation::AddCylinder,
                object: "Cylinder".into(),
                units: Units::In,
                parameters: [
                    ("radius".into(), 1.),
                    ("height".into(), 1.),
                    ("x".into(), 1.),
                    ("y".into(), 0.),
                    ("z".into(), 0.),
                ]
                .into(),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let cylinder_stage = root.join("cylinder-stage");
        fs::create_dir(&cylinder_stage).unwrap();
        let cylinder_result = run(
            &exe,
            &root.join("cylinder-capture"),
            &cylinder_stage,
            &root.join("cylinder-profile"),
            &cylinder_input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        validate_result(&cylinder_stage, &cylinder_input, &cylinder_result).unwrap();
        let bounds = &cylinder_result.after["Cylinder"]["bbox"];
        for (key, expected) in [
            ("xmin", 0.),
            ("xmax", 50.8),
            ("ymin", -25.4),
            ("ymax", 25.4),
            ("zmin", 0.),
            ("zmax", 25.4),
        ] {
            assert!((bounds[key].as_f64().unwrap() - expected).abs() < 0.000001);
        }
        assert_eq!(fs::read(&source).unwrap(), bytes);
        fs::write(stage.join("Edited.step"), b"tamper").unwrap();
        assert!(validate_result(&stage, &input, &result).is_err());
        println!(
            "Verified CADPilot owned selected-copy adapter: {}",
            root.display()
        );
    }
}
