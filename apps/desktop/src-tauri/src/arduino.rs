//! Compile-only adapter using an identified, separately provisioned Arduino toolchain.
use crate::process::{run_owned, StopReason};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

pub const CLI_VERSION: &str = "1.5.1";
pub const CORE_VERSION: &str = "1.8.8";
pub const FQBN: &str = "arduino:avr:uno";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub fqbn: String,
    pub source: String,
}

impl CompileInput {
    pub fn validate(&self) -> Result<(), String> {
        // shortcut: Uno is the first verified board; add other identified core/board fixtures before enabling them.
        if self.schema_version != 1 || self.fqbn != FQBN {
            return Err(
                "Seleccione Arduino Uno con el núcleo AVR 1.8.8 para esta operación.".into(),
            );
        }
        if self.source.trim().is_empty() || self.source.len() > 65536 || self.source.contains('\0')
        {
            return Err("El sketch debe contener código y ocupar como máximo 64 KiB.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileResult {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub cli_version: String,
    pub core_version: String,
    pub fqbn: String,
    pub flash_bytes: usize,
    pub artifacts: [String; 3],
}

fn valid_elf(elf: &[u8]) -> bool {
    elf.len() >= 52 && &elf[..6] == b"\x7fELF\x01\x01" && elf[16..20] == [2, 0, 83, 0]
}

pub fn validate_artifacts(
    directory: &Path,
    input: &CompileInput,
    result: &CompileResult,
) -> Result<(), String> {
    input.validate()?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || result.cli_version != CLI_VERSION
        || result.core_version != CORE_VERSION
        || result.fqbn != input.fqbn
        || result.artifacts != ["firmware.hex", "firmware.elf", "Sketch.ino"]
    {
        return Err("El resultado de Arduino no corresponde a la operación aprobada.".into());
    }
    let hex = read_artifact(&directory.join("firmware.hex"))?;
    let elf = read_artifact(&directory.join("firmware.elf"))?;
    let source = read_artifact(&directory.join("Sketch.ino"))?;
    if hex_flash_size(&hex)? != result.flash_bytes
        || !valid_elf(&elf)
        || source != input.source.as_bytes()
    {
        return Err("Los archivos compilados no corresponden al sketch aprobado.".into());
    }
    Ok(())
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

fn read_artifact(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > 4 * 1024 * 1024
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err("Archivo compilado ausente, demasiado grande o no permitido.".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != metadata.len() {
        return Err("El archivo compilado cambió al verificarlo.".into());
    }
    Ok(bytes)
}

fn hex_flash_size(bytes: &[u8]) -> Result<usize, String> {
    let invalid = || "El firmware no es Intel HEX válido para Arduino Uno.".to_string();
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let mut count = 0;
    let mut ended = false;
    for line in text.lines() {
        if ended
            || !line.starts_with(':')
            || line.len() < 11
            || line.len() > 521
            || line.len() % 2 != 1
        {
            return Err(invalid());
        }
        let mut record = Vec::new();
        for pair in line.as_bytes()[1..].chunks_exact(2) {
            let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
            record.push(u8::from_str_radix(pair, 16).map_err(|_| invalid())?);
        }
        if record.len() != record[0] as usize + 5
            || record.iter().fold(0u8, |a, b| a.wrapping_add(*b)) != 0
        {
            return Err(invalid());
        }
        let address = u16::from_be_bytes([record[1], record[2]]) as usize;
        match record[3] {
            0 if address + record[0] as usize <= 32256 => count += record[0] as usize,
            1 if record[0] == 0 && address == 0 => ended = true,
            // Uno's address space fits in 16 bits; only zero extended addresses are valid.
            2 | 4 if record[0] == 2 && address == 0 && record[4..6] == [0, 0] => (),
            _ => return Err(invalid()),
        }
    }
    if !ended || count == 0 {
        return Err(invalid());
    }
    Ok(count)
}

/// Use only a backend-reserved empty stage and a fresh private profile. This is not a filesystem sandbox.
pub fn compile(
    executable: &Path,
    data: &Path,
    stage: &Path,
    profile: &Path,
    input: &CompileInput,
    cancelled: Arc<AtomicBool>,
) -> Result<CompileResult, String> {
    input.validate()?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Compilación cancelada.".into());
    }
    if [executable, data, stage, profile]
        .iter()
        .any(|path| !path.is_absolute())
    {
        return Err("Las rutas de compilación deben ser absolutas.".into());
    }
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio de trabajo no está vacío.".into());
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let mut environment = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            environment.insert(key.into(), value);
        }
    }
    for name in ["temp", "user", "downloads", "cache"] {
        fs::create_dir(profile.join(name)).map_err(|e| e.to_string())?;
    }
    environment.insert("TEMP".into(), profile.join("temp").into_os_string());
    environment.insert("TMP".into(), profile.join("temp").into_os_string());
    let config = profile.join("config.yaml");
    write_new(&config, &serde_json::to_vec(&serde_json::json!({
        "directories": {"data":data,"downloads":profile.join("downloads"),"user":profile.join("user")},
        "build_cache":{"path":profile.join("cache")},"metrics":{"enabled":false},
        "updater":{"enable_notification":false},"network":{"cloud_api":{"skip_board_detection_calls":true}}
    })).map_err(|e| e.to_string())?)?;
    let invoke = |args: &[OsString]| -> Result<serde_json::Value, String> {
        let mut arguments = vec![
            OsString::from("--config-file"),
            config.as_os_str().to_owned(),
            "--json".into(),
        ];
        arguments.extend_from_slice(args);
        let output = run_owned(
            executable,
            &arguments,
            stage,
            &environment,
            cancelled.clone(),
            Duration::from_secs(120),
        )
        .map_err(|e| e.to_string())?;
        if output.reason != StopReason::Completed || cancelled.load(Ordering::SeqCst) {
            return Err("Compilación cancelada o tiempo de espera agotado.".into());
        }
        if output.truncated {
            return Err("El resultado de Arduino supera el límite permitido.".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| "Arduino devolvió un resultado inválido.".to_string())?;
        if output.exit_code != Some(0) {
            let details: String = value["compiler_err"]
                .as_str()
                .unwrap_or("Revise el núcleo y el código del sketch.")
                .chars()
                .take(2048)
                .collect();
            return Err(format!("No se pudo compilar el sketch: {details}"));
        }
        Ok(value)
    };
    let version = invoke(&["version".into()])?;
    if version["Application"] != "arduino-cli" || version["VersionString"] != CLI_VERSION {
        return Err("Seleccione Arduino CLI 1.5.1.".into());
    }
    let sketch = profile.join("Sketch");
    fs::create_dir(&sketch).map_err(|e| e.to_string())?;
    write_new(
        &stage.join("input.json"),
        &serde_json::to_vec(input).map_err(|e| e.to_string())?,
    )?;
    write_new(&sketch.join("Sketch.ino"), input.source.as_bytes())?;
    write_new(&stage.join("Sketch.ino"), input.source.as_bytes())?;
    let exported = profile.join("exported");
    let compiled = invoke(&[
        "compile".into(),
        "--fqbn".into(),
        input.fqbn.clone().into(),
        "--jobs".into(),
        "2".into(),
        "--build-path".into(),
        profile.join("build").into_os_string(),
        "--output-dir".into(),
        exported.clone().into_os_string(),
        sketch.into_os_string(),
    ])?;
    let builder = &compiled["builder_result"];
    if compiled["success"] != true
        || compiled.get("error").is_some()
        || compiled["upload_result"] != serde_json::json!({})
        || builder["board_platform"]["id"] != "arduino:avr"
        || builder["board_platform"]["version"] != CORE_VERSION
        || builder["build_platform"]["id"] != "arduino:avr"
        || builder["build_platform"]["version"] != CORE_VERSION
        || !builder["build_properties"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item == "build.fqbn=arduino:avr:uno")
        })
    {
        return Err("El resultado no corresponde a la compilación aprobada.".into());
    }
    let hex = read_artifact(&exported.join("Sketch.ino.hex"))?;
    let elf = read_artifact(&exported.join("Sketch.ino.elf"))?;
    let flash_bytes = hex_flash_size(&hex)?;
    if !valid_elf(&elf) {
        return Err("El archivo compilado no es un ejecutable ELF para AVR.".into());
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err("Compilación cancelada.".into());
    }
    write_new(&stage.join("firmware.hex"), &hex)?;
    write_new(&stage.join("firmware.elf"), &elf)?;
    let result = CompileResult {
        schema_version: 1,
        job_id: input.job_id,
        cli_version: CLI_VERSION.into(),
        core_version: CORE_VERSION.into(),
        fqbn: input.fqbn.clone(),
        flash_bytes,
        artifacts: [
            "firmware.hex".into(),
            "firmware.elf".into(),
            "Sketch.ino".into(),
        ],
    };
    write_new(
        &stage.join("result.json"),
        &serde_json::to_vec(&result).map_err(|e| e.to_string())?,
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_requires_checksum_flash_bounds_and_terminal_record() {
        assert_eq!(hex_flash_size(b":0100000001FE\n:00000001FF\n").unwrap(), 1);
        for bad in [
            ":0100000001FF\n:00000001FF\n",
            ":0100000001FE\n",
            ":00000001FF\n",
            ":017E00000180\n:00000001FF\n",
            ":00000001FF\n:0100000001FE\n",
        ] {
            assert!(hex_flash_size(bad.as_bytes()).is_err());
        }
    }
    #[test]
    fn input_rejects_cli_options_unknown_fields_and_empty_source() {
        let mut input = CompileInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            fqbn: FQBN.into(),
            source: "void setup() {} void loop() {}".into(),
        };
        input.validate().unwrap();
        input.fqbn = "arduino:avr:uno --upload".into();
        assert!(input.validate().is_err());
        input.fqbn = FQBN.into();
        input.source.clear();
        assert!(input.validate().is_err());
        assert!(serde_json::from_value::<CompileInput>(serde_json::json!({"schema_version":1,"job_id":Uuid::new_v4(),"fqbn":FQBN,"source":"code","upload":true})).is_err());
    }
    #[test]
    #[ignore = "Requires explicitly provisioned official Arduino CLI and AVR core"]
    fn actual_owned_compilation_and_invalid_sketch() {
        let executable = std::env::var_os("HAZLOMEXA_ARDUINO_CLI")
            .map(std::path::PathBuf::from)
            .expect("Explicit CLI required");
        let data = std::env::var_os("HAZLOMEXA_ARDUINO_DATA")
            .map(std::path::PathBuf::from)
            .expect("Explicit owned toolchain required");
        let root = std::env::temp_dir().join(format!("Hazlo Arduino á {}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        for (name, source, success) in [("valid", "void setup() { pinMode(LED_BUILTIN, OUTPUT); } void loop() { digitalWrite(LED_BUILTIN, HIGH); }", true),
            ("invalid", "void setup() { missing_identifier; } void loop() {}", false)] {
            let stage = root.join(name); fs::create_dir(&stage).unwrap();
            let input = CompileInput {schema_version:1,job_id:Uuid::new_v4(),fqbn:FQBN.into(),source:source.into()};
            let result = compile(&executable,&data,&stage,&root.join(format!("profile-{name}")),&input,Arc::new(AtomicBool::new(false)));
            if success { assert!(result.unwrap().flash_bytes > 0); } else { assert!(result.is_err()); assert!(!stage.join("result.json").exists()); assert!(!stage.join("firmware.hex").exists()); }
            assert_eq!(fs::read_to_string(stage.join("Sketch.ino")).unwrap(),source);
        }
        let stage = root.join("cancelled");
        fs::create_dir(&stage).unwrap();
        let input = CompileInput {
            schema_version: 1,
            job_id: Uuid::new_v4(),
            fqbn: FQBN.into(),
            source: "code".into(),
        };
        assert!(compile(
            &executable,
            &data,
            &stage,
            &root.join("profile-cancelled"),
            &input,
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(fs::read_dir(stage).unwrap().next().is_none());
    }
}
