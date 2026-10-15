//! Exercise the Rust host against the real companion transport in an owned test buffer.
use hazlomexa::{
    agent_editor::{self, Failure, Input},
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
    assert_eq!(records.len(), 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let sessions = runtime
        .block_on(agent_editor::discover(
            &root,
            Arc::new(AtomicBool::new(false)),
        ))
        .unwrap();
    assert_eq!(sessions.len(), 2);
    let serialized = serde_json::to_value(&sessions).unwrap();
    assert!(serialized[0].get("connection").is_none());
    let connection = sessions[0].connection().clone();
    let cancel = AtomicBool::new(false);
    let before = runtime
        .block_on(agent_editor::inspect(&connection, &cancel))
        .unwrap();
    assert_eq!(before.state_sha256, sessions[0].observation.state_sha256);
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
    let listed=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    assert_eq!(listed["selection_required"],true);
    assert_eq!(listed["documents"].as_array().unwrap().len(),2);
    assert!(listed.get("observation").is_none());
    assert!(!listed.to_string().contains("delay("));
    let args=serde_json::json!({"instance_id":before.instance_id});
    let read=host.call("inspect_open_agent_documents",args.clone()).unwrap();
    assert_eq!(read["observation"],serde_json::to_value(&before).unwrap());
    assert_eq!(read["mutations_allowed"],false);
    for private in ["\"token\":", "\"pipe\":",root.to_str().unwrap()] {
        assert!(!read.to_string().contains(private));
    }
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"instance_id":Uuid::new_v4()})).is_err());
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"path":"Sketch.ino"})).is_err());
    let discovery=agent_editor::default_directory().unwrap();
    assert_eq!(discovery,root.join("profile/.arduinoIDE/hazlomexa-editor"));
    let other=sessions.iter().find(|s|s.observation.instance_id!=before.instance_id).unwrap();
    std::fs::remove_file(discovery.join(format!("{}.json",other.observation.instance_id))).unwrap();
    let automatic=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    assert_eq!(automatic["observation"],read["observation"]);
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
        let live=host.call("inspect_open_agent_documents",args.clone()).unwrap();
        assert_eq!(live["observation"]["source"],source);
        assert_eq!(live["observation"]["state_sha256"],current.state_sha256);
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
    std::fs::remove_file(discovery.join(format!("{}.json",before.instance_id))).unwrap();
    let disconnected=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    assert!(disconnected["documents"].as_array().unwrap().is_empty());
    assert!(disconnected.get("observation").is_none());
    assert_eq!(std::fs::read(root.join("Sketch.ino")).unwrap(),saved);
    drop(host);
    drop(workflow);
    let recovered = Project::open(&project_root).unwrap().recover().unwrap();
    assert_eq!(recovered.len(), 2);
    assert!(recovered
        .iter()
        .all(|run| run.phase == Phase::Completed && run.schema_version == 15));
    println!("Rust host and companion protocol: edit/readback/follow-up/stale refusal passed");
}
