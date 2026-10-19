//! Durable dispatch facts only. Native content is retained separately and stays local.
use crate::{
    native_tools::Input,
    project::{atomic_json, read_json},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pending,
    Dispatched,
    Acknowledged,
    Uncertain,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub call_sha256: String,
    pub outcome: Outcome,
    #[serde(default)]
    pub observation_sha256: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub job_id: Uuid,
    pub connection_id: Uuid,
    pub catalog_sha256: String,
    pub steps: Vec<Step>,
    pub finished: bool,
}
#[derive(Serialize)]
pub struct Summary {
    pub job_id: Uuid,
    pub acknowledged: usize,
    pub uncertain: usize,
    pub not_dispatched: usize,
    pub finished: bool,
    pub needs_review: bool,
}
impl Journal {
    pub fn summary(&self) -> Summary {
        let count = |outcome| self.steps.iter().filter(|s| s.outcome == outcome).count();
        Summary {
            job_id: self.job_id,
            acknowledged: count(Outcome::Acknowledged),
            uncertain: count(Outcome::Dispatched) + count(Outcome::Uncertain),
            not_dispatched: count(Outcome::Pending),
            finished: self.finished,
            needs_review: !self.finished
                && self.steps.iter().any(|s| s.outcome != Outcome::Pending),
        }
    }
}
pub struct Batch {
    path: PathBuf,
    pub journal: Journal,
}
impl Batch {
    pub fn start(directory: &Path, input: &Input) -> Result<Self, String> {
        let path = directory.join("dispatch.json");
        if path
            .try_exists()
            .map_err(|_| "No se pudo leer el registro nativo.")?
        {
            return Err("Este lote ya tiene un registro de ejecución. Conserva el trabajo y revisa su estado; no se repiten sus llamadas.".into());
        }
        let steps = input
            .guard
            .iter()
            .map(|g| &g.call)
            .chain(input.calls.iter())
            .chain([&input.readback])
            .map(|call| {
                Ok(Step {
                    call_sha256: crate::kicad_project::digest(
                        &serde_json::to_vec(call).map_err(|_| "Llamada no válida.")?,
                    ),
                    outcome: Outcome::Pending,
                    observation_sha256: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let batch = Self {
            path,
            journal: Journal {
                version: 1,
                job_id: input.job_id,
                connection_id: input.connection_id,
                catalog_sha256: input.catalog_sha256.clone(),
                steps,
                finished: false,
            },
        };
        atomic_json(&batch.path, &batch.journal, false)
            .map_err(|_| "No se pudo reservar el registro nativo; no se enviaron llamadas.")?;
        Ok(batch)
    }
    pub fn dispatch(&mut self, index: usize) -> Result<(), String> {
        let step = self.journal.steps.get_mut(index).ok_or("Paso no válido.")?;
        if step.outcome != Outcome::Pending {
            return Err("El paso ya se despachó; no se repetirá.".into());
        }
        step.outcome = Outcome::Dispatched;
        self.save()
    }
    pub fn observe(&mut self, index: usize, result: &Result<Value, String>) -> Result<(), String> {
        let step = self.journal.steps.get_mut(index).ok_or("Paso no válido.")?;
        if step.outcome != Outcome::Dispatched {
            return Err("El paso no está en vuelo.".into());
        }
        // Save content before acknowledging: a failed write leaves a durable uncertain step.
        let content = match result {
            Ok(value) => serde_json::json!({"result":value}),
            Err(error) => serde_json::json!({"error":error}),
        };
        atomic_json(
            &self
                .path
                .with_file_name(format!("observation-{index}.json")),
            &content,
            false,
        )
        .map_err(|_| "La llamada pudo terminar; no se pudo guardar su respuesta. No la repitas.")?;
        step.observation_sha256 = Some(crate::kicad_project::digest(
            &serde_json::to_vec_pretty(&content).map_err(|_| "Respuesta local no válida.")?,
        ));
        step.outcome = if result.is_ok() {
            Outcome::Acknowledged
        } else {
            Outcome::Uncertain
        };
        self.save()
    }
    pub fn complete(&mut self) -> Result<(), String> {
        if self
            .journal
            .steps
            .iter()
            .any(|s| s.outcome != Outcome::Acknowledged)
        {
            return Err("El lote tiene pasos sin confirmar.".into());
        }
        self.journal.finished = true;
        self.save()
    }
    fn save(&self) -> Result<(), String> {
        atomic_json(&self.path, &self.journal, true).map_err(|_| {
            "No se confirmó el registro nativo. Conserva el trabajo y no repitas llamadas.".into()
        })
    }
}
pub fn read(directory: &Path, input: &Input) -> io::Result<Option<Journal>> {
    let path = directory.join("dispatch.json");
    if !path.try_exists()? {
        return Ok(None);
    }
    let journal: Journal = read_json(&path)?;
    let expected = input
        .guard
        .iter()
        .map(|g| &g.call)
        .chain(input.calls.iter())
        .chain([&input.readback])
        .map(|call| serde_json::to_vec(call).map(|b| crate::kicad_project::digest(&b)))
        .collect::<Result<Vec<_>, _>>()?;
    if journal.version != 1
        || journal.job_id != input.job_id
        || journal.connection_id != input.connection_id
        || journal.catalog_sha256 != input.catalog_sha256
        || journal.steps.len() != expected.len()
        || journal
            .steps
            .iter()
            .zip(expected)
            .any(|(s, hash)| s.call_sha256 != hash)
        || (journal.finished
            && journal
                .steps
                .iter()
                .any(|s| s.outcome != Outcome::Acknowledged))
    {
        return Err(io::Error::other(
            "El registro nativo no coincide con el lote; se conserva sin repetirlo.",
        ));
    }
    Ok(Some(journal))
}

pub fn validate_observations(directory: &Path, input: &Input) -> Result<(), String> {
    let Some(journal) = read(directory, input).map_err(|_| "El registro nativo cambió.")? else {
        return Ok(());
    };
    if !journal.finished {
        return Err("El registro nativo tiene pasos sin confirmar.".into());
    }
    for (index, step) in journal.steps.iter().enumerate() {
        let bytes = crate::kicad_project::read_regular(
            &directory.join(format!("observation-{index}.json")),
            crate::native_tools::REPORT_LIMIT,
        )?;
        if step.observation_sha256.as_deref() != Some(&crate::kicad_project::digest(&bytes)) {
            return Err(
                "Una observación nativa cambió; se conserva sin repetir las llamadas.".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host_tools::Program,
        native_tools::Call,
        project::{OperationInput, Phase, Project, RunSource},
    };
    use serde_json::json;
    fn input() -> Input {
        Input {
            schema_version: 1,
            native_tools_version: 1,
            job_id: Uuid::new_v4(),
            connection_id: Uuid::new_v4(),
            program: Program::Freecad,
            catalog_sha256: "a".repeat(64),
            description: "Owned recovery fixture".into(),
            calls: vec![],
            readback: Call {
                tool: "state".into(),
                arguments: Default::default(),
            },
            guard: None,
            receipt_sha256: None,
        }
    }
    #[test]
    fn restart_retains_partial_work_and_never_restarts_the_batch() {
        let root = std::env::temp_dir().join(format!("hazlo-journal-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut input = input();
        input.calls = vec![input.readback.clone(), input.readback.clone()];
        let mut batch = Batch::start(&root, &input).unwrap();
        batch.dispatch(0).unwrap();
        batch
            .observe(0, &Ok(json!({"local":"PRIVATE_SENTINEL"})))
            .unwrap();
        batch.dispatch(1).unwrap();
        drop(batch);
        let saved = read(&root, &input).unwrap().unwrap();
        let summary = saved.summary();
        assert_eq!(summary.acknowledged, 1);
        assert_eq!(summary.uncertain, 1);
        assert_eq!(summary.not_dispatched, 1);
        assert!(summary.needs_review);
        assert!(!serde_json::to_string(&saved)
            .unwrap()
            .contains("PRIVATE_SENTINEL"));
        assert!(std::fs::read_to_string(root.join("observation-0.json"))
            .unwrap()
            .contains("PRIVATE_SENTINEL"));
        assert!(Batch::start(&root, &input).is_err());
        let mut batch = Batch {
            path: root.join("dispatch.json"),
            journal: saved,
        };
        assert!(batch.dispatch(1).is_err());
        assert!(batch.complete().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn finished_staging_report_is_published_without_replaying_native_calls() {
        let root = std::env::temp_dir().join(format!("hazlo-recover-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let project = Project::create(&root).unwrap();
        let mut input = input();
        let stage = project.reserve_stage(input.job_id).unwrap();
        let mut batch = Batch::start(&stage.path, &input).unwrap();
        batch.dispatch(0).unwrap();
        batch.observe(0, &Ok(json!({"value":12}))).unwrap();
        batch.complete().unwrap();
        let readback = json!({"value":12});
        let report = json!({"native_tools_version":1,"job_id":input.job_id,"connection_id":input.connection_id,"program":input.program,"catalog_sha256":input.catalog_sha256,"results":[],"readback":readback,"readback_sha256":crate::kicad_project::digest(&serde_json::to_vec(&readback).unwrap())});
        let bytes = serde_json::to_vec(&report).unwrap();
        std::fs::write(stage.path.join("result.json"), &bytes).unwrap();
        input.receipt_sha256 = Some(crate::kicad_project::digest(&bytes));
        let mut snapshot = project
            .snapshot(OperationInput::NativeTools(input.clone()), Phase::Running)
            .unwrap();
        snapshot.audit.as_mut().unwrap().source = RunSource::Mcp;
        project.save(&snapshot).unwrap();
        drop(stage);
        let recovered = project.recover().unwrap();
        assert_eq!(recovered[0].phase, Phase::Completed);
        assert!(project
            .artifact_directory(input.job_id)
            .join("observation-0.json")
            .exists());
        assert!(project.native_recovery(None).unwrap()[0].finished);
        assert!(!project.native_recovery(None).unwrap()[0].needs_review);
        let artifacts = project.artifact_directory(input.job_id);
        std::fs::write(
            artifacts.join("observation-0.json"),
            b"tampered local observation",
        )
        .unwrap();
        assert!(crate::native_tools::validate_artifacts(&artifacts, &input).is_err());
        drop(project);
        std::fs::remove_dir_all(root).unwrap();
    }
}
