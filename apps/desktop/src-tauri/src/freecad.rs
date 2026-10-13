use crate::{
    process::{run_owned, StopReason},
    Dimensions, SolidInput,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{GetShortPathNameW, FILE_ATTRIBUTE_REPARSE_POINT};

const SCRIPT: &str = include_str!("../resources/freecad/create-solid.FCMacro");
pub const SUPPORTED_VERSION: &str = "1.1.4";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub bounds_mm: [f64; 3],
    pub volume_mm3: f64,
    pub solids: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolidResult {
    pub schema_version: u32,
    pub adapter_version: u32,
    pub job_id: Uuid,
    pub success: bool,
    pub freecad_version: String,
    pub dimensions_mm: Dimensions,
    pub geometry: Geometry,
    pub artifacts: [String; 2],
}

pub fn validate_result(result: &SolidResult, input: &SolidInput) -> Result<(), String> {
    if !result.success
        || result.schema_version != 1
        || result.adapter_version != 1
        || result.job_id != input.job_id
        || result.dimensions_mm != input.dimensions_mm
        || result.freecad_version != SUPPORTED_VERSION
        || result.geometry.solids != 1
        || result.artifacts != ["solid.FCStd", "solid.step"]
    {
        return Err("El resultado de FreeCAD no corresponde a la operación aprobada.".into());
    }
    let dimensions = input.dimensions_mm.validate()?;
    let expected = [dimensions.length, dimensions.width, dimensions.height];
    if result
        .geometry
        .bounds_mm
        .iter()
        .zip(expected)
        .any(|(actual, expected)| !actual.is_finite() || (actual - expected).abs() > 0.01)
    {
        return Err("Las medidas del archivo generado no coinciden.".into());
    }
    let volume = dimensions.length * dimensions.width * dimensions.height;
    if !result.geometry.volume_mm3.is_finite()
        || (result.geometry.volume_mm3 - volume).abs() > (volume * 0.000001).max(0.001)
    {
        return Err("El volumen del archivo generado no coincide.".into());
    }
    Ok(())
}

fn write_new(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all(contents)?;
    output.sync_all()
}

pub(crate) fn configuration_path(path: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().to_string_lossy().is_ascii() {
        return Ok(path.to_owned());
    }
    let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let size = unsafe { GetShortPathNameW(name.as_ptr(), std::ptr::null_mut(), 0) };
    if size == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0; size as usize];
    let count = unsafe { GetShortPathNameW(name.as_ptr(), buffer.as_mut_ptr(), size) };
    if count == 0 || count >= size {
        return Err(io::Error::last_os_error());
    }
    let short = PathBuf::from(OsString::from_wide(&buffer[..count as usize]));
    if !short.as_os_str().to_string_lossy().is_ascii() {
        return Err(io::Error::other("FreeCAD 1.1.4 requiere una ruta de configuración ASCII; no hay alias corto compatible."));
    }
    Ok(short)
}

/// Only use fresh, backend-reserved staging and profile directories.
pub fn generate(
    executable: &Path,
    stage: &Path,
    profile: &Path,
    input: &SolidInput,
    cancelled: Arc<AtomicBool>,
) -> Result<SolidResult, String> {
    input.clone().validate()?;
    if !executable.is_absolute() || !stage.is_absolute() || !profile.is_absolute() {
        return Err("Las rutas de ejecución deben ser absolutas.".into());
    }
    let run = || -> Result<SolidResult, String> {
        if fs::read_dir(stage)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
        {
            return Err("El directorio de trabajo no está vacío.".into());
        }
        fs::create_dir(profile).map_err(|e| e.to_string())?;
        let mut environment = BTreeMap::new();
        for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
            if let Some(value) = std::env::var_os(key) {
                environment.insert(OsString::from(key), value);
            }
        }
        for (key, name) in [
            ("FREECAD_USER_HOME", "home"),
            ("FREECAD_USER_DATA", "data"),
            ("FREECAD_USER_TEMP", "temp"),
        ] {
            let directory = profile.join(name);
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            environment.insert(
                OsString::from(key),
                configuration_path(&directory)
                    .map_err(|e| e.to_string())?
                    .into_os_string(),
            );
        }
        let profile = configuration_path(profile).map_err(|e| e.to_string())?;
        environment.insert("TEMP".into(), profile.join("temp").into_os_string());
        environment.insert("TMP".into(), profile.join("temp").into_os_string());
        write_new(
            &stage.join("input.json"),
            &serde_json::to_vec(input).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        // FreeCAD's command-line script loading also needs an ASCII-compatible path.
        let script = profile.join("create-solid.FCMacro");
        write_new(&script, SCRIPT.as_bytes()).map_err(|e| e.to_string())?;
        let arguments = [
            OsString::from("--user-cfg"),
            profile.join("user.cfg").into_os_string(),
            OsString::from("--system-cfg"),
            profile.join("system.cfg").into_os_string(),
            script.into_os_string(),
        ];
        let output = run_owned(
            executable,
            &arguments,
            stage,
            &environment,
            cancelled.clone(),
            Duration::from_secs(60),
        )
        .map_err(|e| e.to_string())?;
        if output.reason != StopReason::Completed || cancelled.load(Ordering::SeqCst) {
            return Err("Operación cancelada o tiempo de espera agotado.".into());
        }
        if output.exit_code != Some(0) {
            let details = [output.stdout, output.stderr].concat();
            let details = String::from_utf8_lossy(&details[..details.len().min(2048)]);
            return Err(format!(
                "FreeCAD no pudo completar la operación ({:?}): {details}",
                output.exit_code
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(stage.join("result.json"))
            .map_err(|e| e.to_string())?
            .take(16385)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 16384 {
            return Err("El resultado supera el límite permitido.".into());
        }
        let result: SolidResult =
            serde_json::from_slice(&bytes).map_err(|e| format!("Resultado inválido: {e}"))?;
        validate_result(&result, input)?;
        for name in &result.artifacts {
            let metadata = fs::symlink_metadata(stage.join(name)).map_err(|e| e.to_string())?;
            if !metadata.is_file()
                || metadata.len() == 0
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err("Archivo de salida ausente o no permitido.".into());
            }
        }
        Ok(result)
    };
    if cancelled.load(Ordering::SeqCst) {
        return Err("Operación cancelada.".into());
    }
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_or_nonfinite_results_are_rejected() {
        let input = SolidInput::new(Dimensions {
            length: 10.0,
            width: 20.0,
            height: 30.0,
        })
        .unwrap();
        let mut result = SolidResult {
            schema_version: 1,
            adapter_version: 1,
            job_id: input.job_id,
            success: true,
            freecad_version: SUPPORTED_VERSION.into(),
            dimensions_mm: input.dimensions_mm,
            geometry: Geometry {
                bounds_mm: [10.0, 20.0, 30.0],
                volume_mm3: 6000.0,
                solids: 1,
            },
            artifacts: ["solid.FCStd".into(), "solid.step".into()],
        };
        assert!(validate_result(&result, &input).is_ok());
        result.job_id = Uuid::new_v4();
        assert!(validate_result(&result, &input).is_err());
        result.job_id = input.job_id;
        result.geometry.volume_mm3 = f64::NAN;
        assert!(validate_result(&result, &input).is_err());
    }

    #[test]
    #[ignore = "requires HAZLOMEXA_FREECAD_CMD; run the native suite explicitly"]
    fn actual_freecad_runs_inside_owned_job_with_private_configuration() {
        let executable = PathBuf::from(
            std::env::var_os("HAZLOMEXA_FREECAD_CMD").expect("Set HAZLOMEXA_FREECAD_CMD"),
        );
        let root = std::env::temp_dir().join(format!("Hazlo Mexa á's {}", Uuid::new_v4()));
        let profile = std::env::temp_dir().join(format!("Hazlo Mexa perfil á {}", Uuid::new_v4()));
        let profile = if std::env::var_os("HAZLOMEXA_TEST_ASCII_PROFILES").is_some() {
            std::env::temp_dir().join(format!("hazlomexa-profile-{}", Uuid::new_v4()))
        } else {
            profile
        };
        fs::create_dir(&root).unwrap();
        let input = SolidInput::new(Dimensions {
            length: 12.5,
            width: 7.25,
            height: 3.5,
        })
        .unwrap();
        let result = generate(
            &executable,
            &root,
            &profile,
            &input,
            Arc::new(AtomicBool::new(false)),
        );
        fs::remove_dir_all(root).unwrap();
        if profile.exists() {
            fs::remove_dir_all(profile).unwrap();
        }
        let result = result.unwrap();
        assert_eq!(result.freecad_version, SUPPORTED_VERSION);
        assert_eq!(result.job_id, input.job_id);
    }
}
