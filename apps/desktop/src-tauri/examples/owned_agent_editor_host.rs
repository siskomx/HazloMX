//! Exercise the Rust host against the real companion transport in an owned test buffer.
use hazlomexa::{
    agent_editor::{self, Connection, Failure, Input},
    host_tools::HostTools,
    jobs::Workflow,
    project::{ArtifactFormat, Phase, Project},
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};
use uuid::Uuid;

fn main() {
    let root = PathBuf::from(
        std::env::var_os("HAZLOMEXA_NATIVE_EDITOR_TEST_ROOT").expect("Owned fixture root required"),
    );
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("target")
        .canonicalize()
        .unwrap();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(canonical.parent(), Some(target.as_path()));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("native-arduino-transport-"));
    assert_eq!(
        std::fs::read(root.join("owned-host-probe")).unwrap(),
        b"owned"
    );
    let records = std::fs::read_dir(&root)
        .unwrap()
        .map(Result::unwrap)
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let connection = Connection::load(&records[0].path()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cancel = AtomicBool::new(false);
    let before = runtime
        .block_on(agent_editor::inspect(&connection, &cancel))
        .unwrap();
    assert_eq!(before.name, "Sketch.ino");
    assert!(before.source.starts_with("// unsaved user note\n") && before.dirty);
    assert_eq!(before.source.matches("delay(1000)").count(), 1);
    let saved = std::fs::read(root.join("Sketch.ino")).unwrap();
    let project_root = root.join("project");
    std::fs::create_dir(&project_root).unwrap();
    let workflow =
        Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
    workflow
        .select_project(Project::create(&project_root).unwrap())
        .unwrap();
    let host = HostTools::new(workflow.clone());
    let project = workflow.view().unwrap().project.unwrap();
    let mut current = before.clone();
    for (old, new) in [("delay(1000)", "delay(500)"), ("delay(500)", "delay(250)")] {
        let source = current.source.replace(old, new);
        let input = Input {
            schema_version: 1,
            live_agent_version: 1,
            job_id: Uuid::new_v4(),
            description: "Change timing in the owned sketch".into(),
            before: current.clone(),
            source: source.clone(),
            receipt_sha256: None,
        };
        let pending = workflow
            .propose_live_agent(project.clone(), input.clone(), connection.clone())
            .unwrap();
        let approval = pending.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch).unwrap();
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        let report = workflow
            .artifact(input.job_id, ArtifactFormat::LiveAgentReport)
            .unwrap()
            .read_text()
            .unwrap();
        let result: serde_json::Value = serde_json::from_str(&report).unwrap();
        let verified = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":input.job_id}),
            )
            .unwrap();
        assert_eq!(verified["format"], "live_agent_report");
        assert!(!verified
            .to_string()
            .contains(&root.to_string_lossy().to_string()));
        current = runtime
            .block_on(agent_editor::inspect(&connection, &cancel))
            .unwrap();
        assert_eq!(current.source, source);
        assert_eq!(result["after"]["state_sha256"], current.state_sha256);
        assert_eq!(std::fs::read(root.join("Sketch.ino")).unwrap(), saved);
    }
    assert_eq!(
        runtime
            .block_on(agent_editor::apply(
                &connection,
                Uuid::new_v4(),
                &before,
                &before.source.replace("1000", "500"),
                &cancel
            ))
            .unwrap_err(),
        Failure::Refused
    );
    let final_state = current;
    assert!(final_state.source.starts_with("// unsaved user note\n") && final_state.dirty);
    drop(host);
    drop(workflow);
    let recovered = Project::open(&project_root).unwrap().recover().unwrap();
    assert_eq!(recovered.len(), 2);
    assert!(recovered
        .iter()
        .all(|run| run.phase == Phase::Completed && run.schema_version == 15));
    println!("Rust host and companion protocol: edit/readback/follow-up/stale refusal passed");
}
