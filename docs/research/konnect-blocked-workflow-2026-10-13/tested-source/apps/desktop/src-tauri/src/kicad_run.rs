//! Fixed KiCad checks and 2D exports on an approved private project snapshot.
use crate::{
    kicad::{self, Check, CheckSummary, Units},
    kicad_project::{self as source, KicadInput},
    process::{run_owned, StopReason},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputFile {
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KicadResult {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub kicad_version: String,
    pub summary: CheckSummary,
    pub files: Vec<OutputFile>,
}
pub(crate) struct Runtime {
    root: PathBuf,
    _pins: Vec<File>,
}
pub(crate) fn runtime(executable: &Path, cancel: &AtomicBool) -> Result<Runtime, String> {
    if !executable
        .file_name()
        .is_some_and(|v| v.eq_ignore_ascii_case("kicad-cli.exe"))
    {
        return Err("Seleccione kicad-cli.exe del paquete Windows x64 10.0.7.".into());
    }
    let bin = executable
        .parent()
        .ok_or("Falta la carpeta bin de KiCad.")?;
    let root = bin.parent().ok_or("Falta la carpeta de KiCad.")?.to_owned();
    let mut pins = crate::project::pin_lineage(bin).map_err(|e| e.to_string())?;
    let manifest: BTreeMap<String, String> = serde_json::from_str(include_str!(
        "../../../../docs/research/kicad-runtime-2026-10-11/runtime-files.json"
    ))
    .map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1024 * 1024];
    for (path, expected) in manifest {
        if cancel.load(Ordering::SeqCst) {
            return Err("Revisión de KiCad cancelada.".into());
        }
        let relative = path.replace('\\', "/");
        let mut file =
            source::open_regular(&root.join(source::relative(&relative)?), 512 * 1024 * 1024)?;
        let mut hasher = Sha256::new();
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err("Revisión de KiCad cancelada.".into());
            }
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        if format!("{:x}", hasher.finalize()) != expected {
            return Err(
                "El runtime de KiCad no coincide con el paquete identificado 10.0.7.".into(),
            );
        }
        pins.push(file);
    }
    for name in ["template", "symbols", "footprints"] {
        pins.extend(
            crate::project::pin_lineage(&root.join("share/kicad").join(name))
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(Runtime { root, _pins: pins })
}
pub fn verify_runtime(executable: &Path) -> Result<(), String> {
    runtime(executable, &AtomicBool::new(false)).map(|_| ())
}
fn invoke(
    executable: &Path,
    working: &Path,
    env: &BTreeMap<OsString, OsString>,
    args: Vec<OsString>,
    cancel: Arc<AtomicBool>,
    allowed: &[u32],
) -> Result<u32, String> {
    let output = run_owned(
        executable,
        &args,
        working,
        env,
        cancel,
        Duration::from_secs(120),
    )
    .map_err(|e| e.to_string())?;
    match output.reason {
        StopReason::Cancelled => {
            return Err("Revisión de KiCad cancelada; el original se conserva.".into())
        }
        StopReason::TimedOut => {
            return Err(
                "KiCad agotó el tiempo. La copia de trabajo se conserva y no se publica.".into(),
            )
        }
        StopReason::Completed => (),
    }
    let code = output.exit_code.ok_or("KiCad no informó un resultado.")?;
    if output.truncated || !allowed.contains(&code) {
        return Err(format!("KiCad no completó la operación (código {code}). Revise el proyecto, sus bibliotecas y dependencias; el original se conserva."));
    }
    Ok(code)
}
fn source_copies(imported: &Path, destination: &Path, input: &KicadInput) -> Result<(), String> {
    source::validate_capture(imported, input)?;
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    for file in &input.files {
        let path = source::relative(&file.path)?;
        source::write_new(
            &destination.join(&path),
            &source::read_regular(&imported.join(path), source::FILE_LIMIT)?,
        )?;
    }
    Ok(())
}
pub(crate) fn files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut result = Vec::new();
    let mut pending = vec![directory.to_owned()];
    while let Some(dir) = pending.pop() {
        let _pin = crate::project::pin_directory(&dir, false).map_err(|e| e.to_string())?;
        for item in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let path = item.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                result.push(path);
            }
        }
        if result.len() + pending.len() > 4096 {
            return Err("Demasiados archivos de salida de KiCad.".into());
        }
    }
    result.sort();
    Ok(result)
}
fn relative_to(directory: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(directory)
        .ok()
        .and_then(|p| p.to_str())
        .map(|v| v.replace('\\', "/"))
        .ok_or("Salida de KiCad fuera del directorio de trabajo.".into())
}

pub fn run(
    executable: &Path,
    imported: &Path,
    stage: &Path,
    profile: &Path,
    input: &KicadInput,
    cancel: Arc<AtomicBool>,
) -> Result<KicadResult, String> {
    input.validate()?;
    let identified = runtime(executable, &cancel)?;
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio de KiCad no está vacío.".into());
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let _profile_pins = crate::project::pin_lineage(profile).map_err(|e| e.to_string())?;
    let original = stage.join("Original");
    source_copies(imported, &original, input)?;
    let working = profile.join("working");
    source::working_copy(&original, &working, input, &cancel)?;
    let mut _working_pins = crate::project::pin_lineage(&working).map_err(|e| e.to_string())?;
    for file in &input.files {
        let path = working.join(source::relative(&file.path)?);
        _working_pins.extend(
            crate::project::pin_lineage(path.parent().ok_or("Falta el directorio de trabajo.")?)
                .map_err(|e| e.to_string())?,
        );
    }
    let config = profile.join("config");
    fs::create_dir(&config).map_err(|e| e.to_string())?;
    fs::create_dir(config.join("10.0")).map_err(|e| e.to_string())?;
    for table in ["sym-lib-table", "fp-lib-table"] {
        source::write_new(
            &config.join("10.0").join(table),
            &source::read_regular(
                &identified.root.join("share/kicad/template").join(table),
                source::FILE_LIMIT,
            )?,
        )?;
    }
    let mut env = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for key in ["TEMP", "TMP", "APPDATA", "LOCALAPPDATA", "USERPROFILE"] {
        env.insert(key.into(), profile.as_os_str().to_owned());
    }
    env.insert("KICAD_CONFIG_HOME".into(), config.into_os_string());
    for (key, path) in [
        ("KICAD10_SYMBOL_DIR", "symbols"),
        ("KICAD10_FOOTPRINT_DIR", "footprints"),
        ("KICAD10_3DMODEL_DIR", "3dmodels"),
    ] {
        env.insert(
            key.into(),
            identified
                .root
                .join("share/kicad")
                .join(path)
                .into_os_string(),
        );
    }
    let entry = working.join(source::relative(&input.entry)?);
    let name = entry
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Nombre de fuente no válido.")?;
    let report = stage.join("Check.json");
    let mode = if input.is_board() { "pcb" } else { "sch" };
    let check = if input.is_board() {
        Check::Drc
    } else {
        Check::Erc
    };
    let units = match input.units {
        Units::Mm => "mm",
        Units::In => "in",
        Units::Mils => "mils",
    };
    let mut args: Vec<OsString> = vec![
        mode.into(),
        if input.is_board() {
            "drc".into()
        } else {
            "erc".into()
        },
        "--format".into(),
        "json".into(),
        "--units".into(),
        units.into(),
        "--severity-all".into(),
        "--exit-code-violations".into(),
        "--output".into(),
        report.as_os_str().to_owned(),
    ];
    if input.is_board() {
        args.extend(["--refill-zones".into(), "--save-board".into()]);
    }
    args.push(entry.as_os_str().to_owned());
    let code = invoke(executable, &working, &env, args, cancel.clone(), &[0, 5])?;
    let summary = kicad::validate_report(
        &source::read_regular(&report, kicad::REPORT_LIMIT as u64)?,
        check,
        input.units,
        name,
        code,
    )?;
    let export = stage.join("Exports");
    fs::create_dir(&export).map_err(|e| e.to_string())?;
    if input.is_board() {
        for (format, options) in [
            (
                "gerbers",
                vec![
                    "--no-protel-ext".to_owned(),
                    "--layers".into(),
                    input.layers.join(","),
                ],
            ),
            (
                "drill",
                vec![
                    "--format".into(),
                    "excellon".into(),
                    "--excellon-units".into(),
                    "mm".into(),
                ],
            ),
        ] {
            let directory = export.join(format);
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            let mut args: Vec<OsString> = vec![
                "pcb".into(),
                "export".into(),
                format.into(),
                "--output".into(),
                format!("{}\\", directory.display()).into(),
            ];
            args.extend(options.into_iter().map(OsString::from));
            args.push(entry.as_os_str().to_owned());
            invoke(executable, &working, &env, args, cancel.clone(), &[0])?;
        }
    } else {
        let pdf = export.join("Schematic.pdf");
        invoke(
            executable,
            &working,
            &env,
            vec![
                "sch".into(),
                "export".into(),
                "pdf".into(),
                "--exclude-pdf-property-popups".into(),
                "--exclude-pdf-hierarchical-links".into(),
                "--output".into(),
                pdf.into_os_string(),
                entry.as_os_str().to_owned(),
            ],
            cancel.clone(),
            &[0],
        )?;
        let netlist = export.join("Netlist.xml");
        invoke(
            executable,
            &working,
            &env,
            vec![
                "sch".into(),
                "export".into(),
                "netlist".into(),
                "--format".into(),
                "kicadxml".into(),
                "--output".into(),
                netlist.into_os_string(),
                entry.as_os_str().to_owned(),
            ],
            cancel.clone(),
            &[0],
        )?;
    }
    // Publish a portable checked copy; local preferences generated by KiCad are deliberately not included.
    let checked = stage.join("Checked");
    fs::create_dir(&checked).map_err(|e| e.to_string())?;
    for file in &input.files {
        let rel = source::relative(&file.path)?;
        source::write_new(
            &checked.join(&rel),
            &source::read_regular(&working.join(rel), source::FILE_LIMIT)?,
        )?;
    }
    let outputs = files(stage)?
        .into_iter()
        .map(|path| {
            Ok(OutputFile {
                path: relative_to(stage, &path)?,
                sha256: source::digest(&source::read_regular(&path, source::FILE_LIMIT)?),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let result = KicadResult {
        schema_version: 1,
        job_id: input.job_id,
        kicad_version: kicad::VERSION.into(),
        summary,
        files: outputs,
    };
    validate_artifacts(stage, input, &result)?;
    let metadata = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("El manifiesto de resultados supera el límite de recuperación.".into());
    }
    source::write_new(&stage.join("result.json"), &metadata)?;
    Ok(result)
}
pub fn validate_artifacts(
    directory: &Path,
    input: &KicadInput,
    result: &KicadResult,
) -> Result<(), String> {
    input.validate()?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || result.kicad_version != kicad::VERSION
        || result.files.len() > 4096
        || result.files.is_empty()
    {
        return Err("El resultado de KiCad no corresponde a la propuesta.".into());
    }
    source::validate_capture(&directory.join("Original"), input)?;
    let mut paths = BTreeSet::new();
    let mut gerbers = 0;
    let mut drills = 0;
    let mut pdf = false;
    let mut netlist = false;
    for output in &result.files {
        let rel = source::relative(&output.path)?;
        if !paths.insert(output.path.to_ascii_lowercase()) || !source::valid_hash(&output.sha256) {
            return Err("Lista de salidas de KiCad no válida.".into());
        }
        let bytes = source::read_regular(&directory.join(&rel), source::FILE_LIMIT)?;
        if source::digest(&bytes) != output.sha256 {
            return Err("Los archivos de KiCad cambiaron después de verificarse.".into());
        }
        if output.path == "Check.json" {
            continue;
        }
        if let Some(name) = output
            .path
            .strip_prefix("Original/")
            .or_else(|| output.path.strip_prefix("Checked/"))
        {
            if !input.files.iter().any(|v| v.path == name) {
                return Err("Fuente de KiCad no aprobada.".into());
            }
            continue;
        }
        if input.is_board() {
            if output.path.starts_with("Exports/gerbers/")
                && rel.extension().is_some_and(|v| v == "gbr")
                && bytes.windows(7).any(|v| v == b"%MOMM*%")
                && (bytes.ends_with(b"M02*\n") || bytes.ends_with(b"M02*\r\n"))
            {
                gerbers += 1;
            } else if output.path.starts_with("Exports/drill/")
                && rel.extension().is_some_and(|v| v == "drl")
                && bytes.starts_with(b"M48")
                && bytes.windows(6).any(|v| v == b"METRIC")
            {
                drills += 1;
            } else if output.path.starts_with("Exports/gerbers/")
                && rel.extension().is_some_and(|v| v == "gbrjob")
            {
                let _: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| "Archivo Gerber job no válido.")?;
            } else {
                return Err("Salida PCB no permitida.".into());
            }
        } else if output.path == "Exports/Schematic.pdf" && bytes.starts_with(b"%PDF-") {
            pdf = true;
        } else if output.path == "Exports/Netlist.xml"
            && bytes.starts_with(b"<?xml")
            && bytes.windows(7).any(|v| v == b"<export")
            && bytes.ends_with(b"</export>\n")
        {
            netlist = true;
        } else {
            return Err("Salida de esquema no permitida.".into());
        }
    }
    for file in &input.files {
        for prefix in ["Original", "Checked"] {
            if !paths.contains(&format!("{prefix}/{}", file.path).to_ascii_lowercase()) {
                return Err("Faltan fuentes verificadas.".into());
            }
        }
    }
    if !paths.contains("check.json")
        || (input.is_board() && (gerbers != input.layers.len() || drills == 0))
        || (!input.is_board() && (!pdf || !netlist))
    {
        return Err("Faltan reportes o exportaciones de KiCad.".into());
    }
    let report = source::read_regular(&directory.join("Check.json"), kicad::REPORT_LIMIT as u64)?;
    let active = result.summary.errors + result.summary.warnings + result.summary.excluded;
    let code = if active == 0 { 0 } else { 5 };
    let name = Path::new(&input.entry)
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("Fuente no válida.")?;
    let summary = kicad::validate_report(
        &report,
        if input.is_board() {
            Check::Drc
        } else {
            Check::Erc
        },
        input.units,
        name,
        code,
    )?;
    if summary != result.summary {
        return Err("Los hallazgos de KiCad no corresponden al reporte verificado.".into());
    }
    for path in files(directory)? {
        let name = relative_to(directory, &path)?;
        if name != "result.json" && !paths.contains(&name.to_ascii_lowercase()) {
            return Err("El directorio contiene archivos de KiCad no verificados.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Actual KiCad command with a short supervisor deadline; run with --test-threads=1"]
    fn actual_kicad_process_short_deadline() {
        use crate::process::tests::{child_processes, is_alive};
        use std::time::Instant;
        assert!(child_processes("kicad-cli.exe").is_empty());
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        let _identified = runtime(&exe, &AtomicBool::new(false)).unwrap();
        let root =
            std::env::temp_dir().join(format!("hazlomexa-kicad-deadline-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let working = root.join("working");
        fs::create_dir(&working).unwrap();
        let config = root.join("config");
        fs::create_dir_all(config.join("10.0")).unwrap();
        for name in [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ] {
            fs::copy(
                workspace.join("target/kicad-probe/fixtures").join(name),
                working.join(name),
            )
            .unwrap();
        }
        for table in ["sym-lib-table", "fp-lib-table"] {
            fs::copy(
                _identified.root.join("share/kicad/template").join(table),
                config.join("10.0").join(table),
            )
            .unwrap();
        }
        let original = source::digest(&fs::read(working.join("basic_test.kicad_pcb")).unwrap());
        let mut env = BTreeMap::new();
        for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.into(), value);
            }
        }
        for key in ["TEMP", "TMP", "APPDATA", "LOCALAPPDATA", "USERPROFILE"] {
            env.insert(key.into(), root.as_os_str().to_owned());
        }
        env.insert("KICAD_CONFIG_HOME".into(), config.into_os_string());
        for (key, folder) in [
            ("KICAD10_SYMBOL_DIR", "symbols"),
            ("KICAD10_FOOTPRINT_DIR", "footprints"),
        ] {
            env.insert(
                key.into(),
                _identified
                    .root
                    .join("share/kicad")
                    .join(folder)
                    .into_os_string(),
            );
        }
        let args = vec![
            "pcb".into(),
            "drc".into(),
            "--format".into(),
            "json".into(),
            "--output".into(),
            root.join("Check.json").into_os_string(),
            working.join("basic_test.kicad_pcb").into_os_string(),
        ];
        let test_working = working.clone();
        let started = Instant::now();
        let worker = std::thread::spawn(move || {
            run_owned(
                &exe,
                &args,
                &test_working,
                &env,
                Arc::new(AtomicBool::new(false)),
                Duration::from_millis(100),
            )
            .unwrap()
        });
        let mut observed = std::collections::BTreeSet::new();
        while !worker.is_finished() {
            observed.extend(child_processes("kicad-cli.exe"));
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        let output = worker.join().unwrap();
        assert_eq!(output.reason, StopReason::TimedOut);
        assert!(
            !observed.is_empty(),
            "No actual CLI process observed before deadline"
        );
        assert!(observed.iter().all(|pid| !is_alive(*pid)));
        assert!(child_processes("kicad-cli.exe").is_empty());
        assert_eq!(
            source::digest(&fs::read(working.join("basic_test.kicad_pcb")).unwrap()),
            original
        );
        let record = serde_json::json!({"passed":true,"root":root,"observed_owned_pids":observed,"deadline_ms":100,"elapsed_ms":started.elapsed().as_millis(),"reason":"timed_out","scope":"real CLI startup/check under shortened supervisor deadline; production 120-second deadline not elapsed","published":false});
        fs::write(
            root.join("deadline-evidence.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        println!("Verified KiCad short deadline: {}", root.display());
    }
    #[test]
    #[ignore = "Actual official KiCad Windows x64 10.0.7 runtime and captured upstream project"]
    fn actual_kicad_captured_project_checks_and_exports() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let exe = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        let root = std::env::temp_dir().join(format!("hazlomexa-kicad-run-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let sources = root.join("sources");
        fs::create_dir(&sources).unwrap();
        for name in [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ] {
            fs::copy(
                workspace.join("target/kicad-probe/fixtures").join(name),
                sources.join(name),
            )
            .unwrap();
        }
        let before: BTreeMap<_, _> = files(&sources)
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p.file_name().unwrap().to_owned(),
                    source::digest(&fs::read(p).unwrap()),
                )
            })
            .collect();
        for (index, (name, units)) in [
            ("basic_test.kicad_sch", Units::In),
            ("basic_test.kicad_pcb", Units::Mm),
        ]
        .into_iter()
        .enumerate()
        {
            let imported = root.join(format!("imported-{index}"));
            let input = source::capture(
                &sources,
                &sources.join(name),
                &imported,
                Uuid::new_v4(),
                units,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(input.files.len(), 4);
            let stage = root.join(format!("stage-{index}"));
            fs::create_dir(&stage).unwrap();
            let result = run(
                &exe,
                &imported,
                &stage,
                &root.join(format!("profile-{index}")),
                &input,
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            validate_artifacts(&stage, &input, &result).unwrap();
            if input.is_board() {
                assert!(result.summary.warnings > 0);
                assert_eq!(input.layers.len(), 7);
            } else {
                assert_eq!(result.summary.errors, 2);
            }
            for (file, hash) in &before {
                assert_eq!(
                    source::digest(&fs::read(sources.join(file)).unwrap()),
                    *hash
                );
            }
            fs::write(stage.join("Check.json"), b"tampered").unwrap();
            assert!(validate_artifacts(&stage, &input, &result).is_err());
        }
        println!("Verified captured KiCad projects: {}", root.display());
    }
}
