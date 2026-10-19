//! One admitted native operation; content stays in its existing local result artifact.
use crate::{
    native_journal::{Outcome, Summary},
    project::{atomic_json, read_json, OperationInput},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};
use uuid::Uuid;

const REFUSED: &str = "Los pasos pendientes están pausados. No se envió esta operación nativa.";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    version: u32,
    job_id: Uuid,
    input_sha256: String,
    pub outcome: Outcome,
    pub receipt_sha256: Option<String>,
}
fn digest(input: &OperationInput) -> Result<String, String> {
    if !matches!(
        input,
        OperationInput::LiveCad(_)
            | OperationInput::LiveAgent(_)
            | OperationInput::LivePcb(_)
            | OperationInput::LiveDrawing(_)
    ) {
        return Err("Este registro requiere un adaptador nativo en vivo.".into());
    }
    let mut value = serde_json::to_value(input).map_err(|_| "Entrada nativa no válida.")?;
    value
        .as_object_mut()
        .ok_or("Entrada nativa no válida.")?
        .remove("receipt_sha256");
    Ok(crate::kicad_project::digest(
        &serde_json::to_vec(&value).map_err(|_| "Entrada nativa no válida.")?,
    ))
}
impl Record {
    pub fn summary(&self, input: &OperationInput) -> Summary {
        let finished = self.outcome == Outcome::Acknowledged;
        Summary {
            job_id: self.job_id,
            program: input.native_program(),
            kind: "live_adapter",
            acknowledged: usize::from(finished),
            uncertain: usize::from(matches!(
                self.outcome,
                Outcome::Dispatched | Outcome::Uncertain
            )),
            not_dispatched: usize::from(self.outcome == Outcome::Pending),
            finished,
            needs_review: matches!(self.outcome, Outcome::Dispatched | Outcome::Uncertain),
            outcome_checked: false,
        }
    }
}
pub(crate) fn read(directory: &Path, input: &OperationInput) -> Result<Option<Record>, String> {
    let path = directory.join("live-dispatch.json");
    if !path
        .try_exists()
        .map_err(|_| "No se pudo comprobar el registro nativo.")?
    {
        return Ok(None);
    }
    let record: Record = read_json(&path).map_err(|_| "No se pudo leer el registro nativo.")?;
    if record.version != 1
        || record.job_id != input.job_id()
        || record.input_sha256 != digest(input)?
        || (record.outcome == Outcome::Acknowledged) != record.receipt_sha256.is_some()
        || record
            .receipt_sha256
            .as_ref()
            .is_some_and(|h| !crate::kicad_project::valid_hash(h))
    {
        return Err("El registro no coincide con la operación aprobada. No se repetirá.".into());
    }
    if let Some(hash) = &record.receipt_sha256 {
        let bytes = crate::kicad_project::read_regular(&directory.join("result.json"), 65536)?;
        if crate::kicad_project::digest(&bytes) != *hash
            || input.receipt_sha256().is_some_and(|h| h != hash)
        {
            return Err("El resultado nativo cambió; se conserva la pausa.".into());
        }
    }
    Ok(Some(record))
}
pub(crate) fn recover_checked(directory:&Path,checked:&OperationInput)->Result<(),String>{
    let mut record=read(directory,checked)?.ok_or("Falta el registro de la llamada; no se adoptó el resultado.")?;
    let hash=checked.receipt_sha256().ok_or("Falta el recibo del resultado comprobado.")?;
    if record.outcome==Outcome::Acknowledged{return Ok(());}
    if !matches!(record.outcome,Outcome::Dispatched|Outcome::Uncertain){return Err("La llamada no fue admitida; no se adoptó un resultado ajeno.".into());}
    record.receipt_sha256=Some(hash.into());record.outcome=Outcome::Acknowledged;
    atomic_json(&directory.join("live-dispatch.json"),&record,true).map_err(|_|"El resultado sigue local, pero no se confirmó la recuperación. No repitas la operación.".into())
}
pub(crate) struct Dispatch {
    path: PathBuf,
    record: Mutex<Record>,
    gate: Arc<Mutex<()>>,
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}
impl Dispatch {
    pub fn start(
        directory: &Path,
        input: &OperationInput,
        gate: Arc<Mutex<()>>,
        paused: Arc<AtomicBool>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Arc<Self>, String> {
        let record = Record {
            version: 1,
            job_id: input.job_id(),
            input_sha256: digest(input)?,
            outcome: Outcome::Pending,
            receipt_sha256: None,
        };
        let path = directory.join("live-dispatch.json");
        atomic_json(&path,&record,false).map_err(|_|"La operación ya tiene un registro o no se pudo reservar. No se enviaron nuevas llamadas.")?;
        Ok(Arc::new(Self {
            path,
            record: Mutex::new(record),
            gate,
            paused,
            cancelled,
        }))
    }
    pub fn enter(&self) -> Result<MutexGuard<'_, ()>, String> {
        let permit = self.gate.lock().map_err(|_| REFUSED)?;
        if self.paused.load(Ordering::SeqCst) || self.cancelled.load(Ordering::SeqCst) {
            return Err(REFUSED.into());
        }
        let mut record = self.record.lock().map_err(|_| REFUSED)?;
        if record.outcome != Outcome::Pending {
            return Err("Esta operación ya fue admitida; no se repetirá.".into());
        }
        record.outcome = Outcome::Dispatched;
        atomic_json(&self.path, &*record, true).map_err(|_| {
            "No se confirmó el registro; no se envió una nueva llamada.".to_string()
        })?;
        Ok(permit)
    }
    pub fn was_dispatched(&self) -> bool {
        self.record
            .lock()
            .map_or(true, |r| r.outcome != Outcome::Pending)
    }
    pub fn complete(&self, checked: &OperationInput) -> Result<(), String> {
        let mut record = self
            .record
            .lock()
            .map_err(|_| "No se pudo conservar el resultado nativo.")?;
        if record.outcome != Outcome::Dispatched || record.input_sha256 != digest(checked)? {
            return Err("El resultado no corresponde a la llamada admitida.".into());
        }
        let hash = checked
            .receipt_sha256()
            .ok_or("Falta el recibo del resultado comprobado.")?;
        record.receipt_sha256 = Some(hash.into());
        record.outcome = Outcome::Acknowledged;
        atomic_json(&self.path,&*record,true).map_err(|_|"El resultado sigue local, pero no se confirmó su registro. No repitas la operación.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovered_acknowledgment_survives_locks_and_rejects_changed_evidence(){
        use std::os::windows::fs::OpenOptionsExt;
        let root=std::env::temp_dir().join(format!("hazlo-recovered-ack-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input=crate::agent_editor::tests::job_fixture();let original=OperationInput::LiveAgent(input.clone());let fence=Dispatch::start(&root,&original,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap()).unwrap();let mut checked=original.clone();checked.retain_receipt(&root).unwrap();assert!(recover_checked(&root,&checked).is_err());drop(fence.enter().unwrap());drop(fence);
        let journal=root.join("live-dispatch.json");let before=std::fs::read(&journal).unwrap();let lock=std::fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&journal).unwrap();
        assert!(recover_checked(&root,&checked).is_err());assert_eq!(std::fs::read(&journal).unwrap(),before);assert!(read(&root,&original).unwrap().unwrap().summary(&original).needs_review);drop(lock);
        recover_checked(&root,&checked).unwrap();let after=std::fs::read(&journal).unwrap();recover_checked(&root,&checked).unwrap();assert_eq!(std::fs::read(&journal).unwrap(),after);assert!(!read(&root,&original).unwrap().unwrap().summary(&original).needs_review);
        std::fs::write(root.join("result.json"),b"changed native report").unwrap();assert!(read(&root,&original).is_err());assert!(recover_checked(&root,&checked).is_err());assert_eq!(std::fs::read(&journal).unwrap(),after);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn durable_fence_distinguishes_unsent_and_admitted_calls_without_replay() {
        let root = std::env::temp_dir().join(format!("hazlo-live-fence-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input = OperationInput::LiveAgent(crate::agent_editor::tests::job_fixture());
        let gate = Arc::new(Mutex::new(()));
        let paused = Arc::new(AtomicBool::new(true));
        let cancel = Arc::new(AtomicBool::new(false));
        let dispatch = Dispatch::start(&root, &input, gate.clone(), paused, cancel).unwrap();
        assert!(dispatch.enter().is_err());
        assert!(!dispatch.was_dispatched());
        assert_eq!(
            read(&root, &input).unwrap().unwrap().outcome,
            Outcome::Pending
        );
        assert!(
            !read(&root, &input)
                .unwrap()
                .unwrap()
                .summary(&input)
                .needs_review
        );
        assert!(Dispatch::start(
            &root,
            &input,
            gate.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false))
        )
        .is_err());
        let other = root.join("admitted");
        std::fs::create_dir(&other).unwrap();
        let dispatch = Dispatch::start(
            &other,
            &input,
            gate.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let permit = dispatch.enter().unwrap();
        assert!(gate.try_lock().is_err());
        drop(permit);
        assert!(gate.try_lock().is_ok());
        assert!(dispatch.enter().is_err());
        assert!(
            read(&other, &input)
                .unwrap()
                .unwrap()
                .summary(&input)
                .needs_review
        );
        let mut altered = input.clone();
        if let OperationInput::LiveAgent(i) = &mut altered {
            i.source.push_str("\n// changed intent");
        }
        assert!(read(&other, &altered).is_err());
        assert!(!std::fs::read_to_string(other.join("live-dispatch.json"))
            .unwrap()
            .contains("unsaved note"));
        drop(dispatch);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checked_live_record_recovers_a_receipt_if_the_app_stops_before_finalization() {
        use crate::project::{Phase, Project, RunSource};
        let root = std::env::temp_dir().join(format!("hazlo-live-commit-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let project = Project::create(&root).unwrap();
        let input = crate::agent_editor::tests::job_fixture();
        let id = input.job_id;
        let mut snapshot = project
            .snapshot(OperationInput::LiveAgent(input.clone()), Phase::Running)
            .unwrap();
        snapshot.audit.as_mut().unwrap().source = RunSource::Mcp;
        snapshot.audit.as_mut().unwrap().native_attempted = Some(true);
        project.save(&snapshot).unwrap();
        let stage = project.reserve_stage(id).unwrap();
        let bytes =
            serde_json::to_vec_pretty(&crate::agent_editor::tests::report_fixture(&input)).unwrap();
        std::fs::write(stage.path.join("result.json"), &bytes).unwrap();
        let dispatch = Dispatch::start(
            &stage.path,
            &snapshot.input,
            Arc::new(Mutex::new(())),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        drop(dispatch.enter().unwrap());
        let mut checked = snapshot.input.clone();
        checked.retain_receipt(&stage.path).unwrap();
        project.validate_outputs(&stage.path, &checked).unwrap();
        dispatch.complete(&checked).unwrap();
        assert!(snapshot.input.receipt_sha256().is_none());
        drop(dispatch);
        drop(stage);
        drop(project);
        let project = Project::open(&root).unwrap();
        let recovered = project.recover().unwrap();
        assert_eq!(recovered[0].phase, Phase::Completed);
        assert!(recovered[0].input.receipt_sha256().is_some());
        assert_eq!(
            std::fs::read(project.artifact_directory(id).join("result.json")).unwrap(),
            bytes
        );
        assert!(!project.native_recovery(None).unwrap()[0].needs_review);
        drop(project);
        std::fs::remove_dir_all(root).unwrap();
    }
}
