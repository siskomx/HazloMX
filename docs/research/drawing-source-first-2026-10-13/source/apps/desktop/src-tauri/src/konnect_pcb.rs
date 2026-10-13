//! Approved closed-board moves with independent native checking and saved-state readback.
use crate::{kicad_project as source, kicad_run, konnect, konnect_run};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub konnect_version: String,
    pub source: source::KicadInput,
    pub pcb_move: konnect::FootprintMove,
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        self.pcb_move.validate()?;
        if self.schema_version != 1
            || self.job_id.is_nil()
            || self.source.job_id != self.job_id
            || self.konnect_version != konnect_run::VERSION
            || !self.source.is_board()
        {
            return Err("La propuesta requiere una PCB capturada y Konnect 0.13.0.".into());
        }
        Ok(())
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultDocument {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub konnect_version: String,
    pub protocol_version: String,
    pub verification_input: source::KicadInput,
    pub verification: kicad_run::KicadResult,
    pub files: Vec<kicad_run::OutputFile>,
}
pub fn run(
    executable: &Path,
    cli: &Path,
    imported: &Path,
    stage: &Path,
    profile: &Path,
    input: &Input,
    cancel: Arc<AtomicBool>,
) -> Result<ResultDocument, String> {
    input.validate()?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Edición PCB cancelada antes de iniciar.".into());
    }
    let _runtime = konnect_run::runtime(executable)?;
    let _kicad_runtime = kicad_run::runtime(cli, &cancel)?;
    source::validate_capture(imported, &input.source)?;
    let _stage = crate::project::pin_lineage(stage).map_err(|e| e.to_string())?;
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio PCB no está vacío.".into());
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let _profile = crate::project::pin_lineage(profile).map_err(|e| e.to_string())?;
    source::write_new(
        &stage.join("input.json"),
        &serde_json::to_vec(input).map_err(|e| e.to_string())?,
    )?;
    for file in &input.source.files {
        let rel = source::relative(&file.path)?;
        source::write_new(
            &stage.join("Original").join(&rel),
            &source::read_regular(&imported.join(rel), source::FILE_LIMIT)?,
        )?;
    }
    let edited = stage.join("Edited");
    source::working_copy(&stage.join("Original"), &edited, &input.source, &cancel)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(konnect_run::dispatch(
        executable,
        cli,
        &edited,
        profile,
        konnect_run::Edit::Move(input),
        cancel.clone(),
    ))?;
    let verification_input = source::capture(
        &edited,
        &edited.join(source::relative(&input.source.entry)?),
        &profile.join("verification-import"),
        input.job_id,
        input.source.units,
        cancel.clone(),
    )?;
    let verification_dir = stage.join("Verification");
    fs::create_dir(&verification_dir).map_err(|e| e.to_string())?;
    let verification = kicad_run::run(
        cli,
        &profile.join("verification-import"),
        &verification_dir,
        &profile.join("verification-profile"),
        &verification_input,
        cancel.clone(),
    )?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Edición PCB cancelada; no se publicarán resultados.".into());
    }
    let result = ResultDocument {
        schema_version: 1,
        job_id: input.job_id,
        konnect_version: konnect_run::VERSION.into(),
        protocol_version: "2025-06-18".into(),
        verification_input,
        verification,
        files: kicad_run::files(stage)?
            .into_iter()
            .map(|path| {
                Ok(kicad_run::OutputFile {
                    path: path
                        .strip_prefix(stage)
                        .map_err(|e| e.to_string())?
                        .to_str()
                        .ok_or("Ruta PCB no válida.")?
                        .replace('\\', "/"),
                    sha256: source::digest(&source::read_regular(&path, source::FILE_LIMIT)?),
                })
            })
            .collect::<Result<_, String>>()?,
    };
    validate_artifacts(stage, input, &result)?;
    let metadata = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("El resultado PCB supera el límite de recuperación.".into());
    }
    source::write_new(&stage.join("result.json"), &metadata)?;
    Ok(result)
}
pub fn validate_artifacts(
    directory: &Path,
    input: &Input,
    result: &ResultDocument,
) -> Result<(), String> {
    input.validate()?;
    if result.schema_version != 1
        || result.job_id != input.job_id
        || result.konnect_version != input.konnect_version
        || result.protocol_version != "2025-06-18"
        || result.verification_input.job_id != input.job_id
        || result.verification_input.entry != input.source.entry
        || result.verification_input.units != input.source.units
        || result.verification_input.layers != input.source.layers
        || result.verification.job_id != input.job_id
    {
        return Err("El resultado PCB no corresponde a la propuesta.".into());
    }
    if source::read_regular(&directory.join("input.json"), 1024 * 1024)?
        != serde_json::to_vec(input).map_err(|e| e.to_string())?
    {
        return Err("La solicitud PCB cambió.".into());
    }
    source::portable_files(
        &directory.join("Original"),
        &input.source,
        &AtomicBool::new(false),
        |path, before| {
            let after =
                source::read_regular(&directory.join("Edited").join(path), source::FILE_LIMIT)?;
            if path == source::relative(&input.source.entry)? {
                konnect::verify_footprint_move(before, &after, &input.pcb_move)
            } else if after == before {
                Ok(())
            } else {
                Err("Konnect cambió un archivo ajeno a la PCB.".into())
            }
        },
    )?;
    let paths = |files: &[source::CapturedFile]| {
        files
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>()
    };
    if paths(&result.verification_input.files) != paths(&input.source.files) {
        return Err("La verificación PCB cambió sus dependencias.".into());
    }
    let verification = directory.join("Verification");
    kicad_run::validate_artifacts(
        &verification,
        &result.verification_input,
        &result.verification,
    )?;
    let persisted: kicad_run::KicadResult = serde_json::from_slice(&source::read_regular(
        &verification.join("result.json"),
        1024 * 1024,
    )?)
    .map_err(|_| "Resultado KiCad no válido.")?;
    if serde_json::to_value(persisted).map_err(|e| e.to_string())?
        != serde_json::to_value(&result.verification).map_err(|e| e.to_string())?
    {
        return Err("El resultado KiCad cambió.".into());
    }
    let mut expected =
        BTreeSet::from(["input.json".to_string(), "Verification/result.json".into()]);
    for file in &input.source.files {
        expected.insert(format!("Original/{}", file.path));
        expected.insert(format!("Edited/{}", file.path));
        let rel = source::relative(&file.path)?;
        if source::read_regular(&directory.join("Edited").join(&rel), source::FILE_LIMIT)?
            != source::read_regular(&verification.join("Original").join(rel), source::FILE_LIMIT)?
        {
            return Err("KiCad revisó una copia distinta de la edición aprobada.".into());
        }
    }
    for file in &result.verification.files {
        expected.insert(format!("Verification/{}", file.path));
    }
    let rel = source::relative(&input.source.entry)?;
    if konnect::footprints(&source::read_regular(
        &directory.join("Edited").join(&rel),
        source::FILE_LIMIT,
    )?)? != konnect::footprints(&source::read_regular(
        &verification.join("Checked").join(rel),
        source::FILE_LIMIT,
    )?)? {
        return Err("KiCad cambió los componentes de la PCB al verificarla.".into());
    }
    let mut observed = BTreeSet::new();
    for file in &result.files {
        if !observed.insert(file.path.clone())
            || !expected.contains(&file.path)
            || source::digest(&source::read_regular(
                &directory.join(source::relative(&file.path)?),
                source::FILE_LIMIT,
            )?) != file.sha256
        {
            return Err("Los archivos PCB cambiaron o contienen una salida inesperada.".into());
        }
    }
    let actual = kicad_run::files(directory)?
        .into_iter()
        .map(|path| {
            path.strip_prefix(directory)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|path| path != "result.json")
        .collect::<BTreeSet<_>>();
    if observed != expected || actual != expected {
        return Err("Faltan archivos PCB o hay archivos no aprobados.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imperial_proposal_coordinates_survive_json_recovery_exactly() {
        let original = crate::konnect::FootprintMove {
            reference: "J1".into(),
            uuid: Uuid::new_v4(),
            expected_x_mm: 200.8,
            expected_y_mm: 156.6,
            x_mm: serde_json::from_str(&format!("{:.6}", (203.34 / 25.4) * 25.4)).unwrap(),
            y_mm: serde_json::from_str(&format!("{:.6}", (155.33 / 25.4) * 25.4)).unwrap(),
        };
        let bytes = serde_json::to_vec(&original).unwrap();
        let recovered: crate::konnect::FootprintMove = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(serde_json::to_vec(&recovered).unwrap(), bytes);
    }
    #[test]
    fn published_pcb_summary_returns_native_findings() {
        fn copy(source: &Path, destination: &Path) {
            fs::create_dir_all(destination).unwrap();
            for entry in fs::read_dir(source).unwrap() {
                let entry = entry.unwrap();
                let target = destination.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &target);
                } else {
                    fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("docs/research/konnect-pcb-workflow-2026-10-12/Completed");
        let input: Input =
            serde_json::from_slice(&fs::read(directory.join("input.json")).unwrap()).unwrap();
        let root = std::env::temp_dir().join(format!("hazlomexa-pcb-summary-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = crate::project::Project::create(&root).unwrap();
        let mut snapshot = project
            .snapshot(
                crate::project::OperationInput::KonnectPcb(input.clone()),
                crate::project::Phase::Running,
            )
            .unwrap();
        let stage = project.reserve_stage(input.job_id).unwrap();
        copy(&directory, &stage.path);
        project.publish(stage, &snapshot).unwrap();
        snapshot.phase = crate::project::Phase::Completed;
        snapshot.audit.as_mut().unwrap().files_verified = true;
        project.save(&snapshot).unwrap();
        let summary = project.kicad_summary(input.job_id).unwrap();
        assert_eq!(summary.errors, 1);
        assert_eq!(summary.units, crate::kicad::Units::Mils);
    }
    #[test]
    fn actual_saved_result_rejects_forged_identity_summary_units_and_dependencies() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .join("docs/research/konnect-pcb-workflow-2026-10-12/Completed");
        let input: Input =
            serde_json::from_slice(&fs::read(directory.join("input.json")).unwrap()).unwrap();
        let bytes = fs::read(directory.join("result.json")).unwrap();
        let result: ResultDocument = serde_json::from_slice(&bytes).unwrap();
        validate_artifacts(&directory, &input, &result).unwrap();
        for mutate in 0..5 {
            let mut result: ResultDocument = serde_json::from_slice(&bytes).unwrap();
            match mutate {
                0 => result.job_id = Uuid::new_v4(),
                1 => result.verification.summary.errors = 0,
                2 => result.verification_input.units = crate::kicad::Units::Mm,
                3 => result.verification_input.files.pop().map(|_| ()).unwrap(),
                _ => result.verification_input.layers.reverse(),
            }
            assert!(validate_artifacts(&directory, &input, &result).is_err());
        }
    }
}
