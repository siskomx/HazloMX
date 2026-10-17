//! Exercise the Rust host against the real companion transport in an owned test buffer.
use hazlomexa::{
    agent_editor::{self, Failure},
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
    let name=root.file_name().unwrap().to_str().unwrap();
    let actual=name.starts_with("arduino-native-ui-");
    assert!(actual || name.starts_with("native-arduino-transport-"));
    if actual {
        let marker:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("owned-native-probe.json")).unwrap()).unwrap();
        let result:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("native-api-result.json")).unwrap()).unwrap();
        assert_eq!(marker["owned"],true);
        assert_eq!(result["pass"],true);
        assert_eq!(result["native_undo_verified"],true);
    } else {
        assert_eq!(std::fs::read(root.join("owned-host-probe")).unwrap(),b"owned");
    }
    let discovery=agent_editor::default_directory().unwrap();
    assert_eq!(discovery,root.join("profile/.arduinoIDE/hazlomexa-editor"));
    let registration=if actual {discovery.clone()} else {root.clone()};
    let records = std::fs::read_dir(&registration)
        .unwrap()
        .map(Result::unwrap)
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), if actual {1} else {2});
    for record in &records { agent_editor::Connection::load(&record.path()).unwrap(); }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let sessions = runtime
        .block_on(agent_editor::discover(
            &registration,
            Arc::new(AtomicBool::new(false)),
        ))
        .unwrap();
    assert_eq!(sessions.len(), if actual {1} else {2});
    let serialized = serde_json::to_value(&sessions).unwrap();
    assert!(serialized[0].get("connection").is_none());
    let connection = sessions[0].connection().clone();
    let cancel = AtomicBool::new(false);
    let before = runtime
        .block_on(agent_editor::inspect(&connection, &cancel))
        .unwrap();
    assert_eq!(before.state_sha256, sessions[0].observation.state_sha256);
    assert_eq!(before.name, if actual {"OwnedSketch.ino"} else {"Sketch.ino"});
    assert!(before.source.contains("// unsaved user note\n") && before.dirty);
    assert_eq!(before.source.matches(if actual {"delay(250)"} else {"delay(1000)"}).count(), 1);
    let saved_path=if actual {root.join("sketchbook/OwnedSketch/OwnedSketch.ino")} else {root.join("Sketch.ino")};
    let saved = std::fs::read(&saved_path).unwrap();
    let project_root = root.join(format!("project-{}",Uuid::new_v4()));
    std::fs::create_dir(&project_root).unwrap();
    let workflow =
        Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
    workflow
        .select_project(Project::create(&project_root).unwrap())
        .unwrap();
    let host = HostTools::new(workflow.clone());
    let listed=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    if actual {
        assert_eq!(listed["observation"]["state_sha256"],before.state_sha256);
    } else {
        assert_eq!(listed["selection_required"],true);
        assert_eq!(listed["documents"].as_array().unwrap().len(),2);
        assert!(listed.get("observation").is_none());
        assert!(!listed.to_string().contains("delay("));
    }
    let args=serde_json::json!({"instance_id":before.instance_id});
    let read=host.call("inspect_open_agent_documents",args.clone()).unwrap();
    assert_eq!(read["observation"],serde_json::to_value(&before).unwrap());
    assert_eq!(read["mutations_allowed"],false);
    for private in ["\"token\":", "\"pipe\":",root.to_str().unwrap()] {
        assert!(!read.to_string().contains(private));
    }
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"instance_id":Uuid::new_v4()})).is_err());
    assert!(host.call("inspect_open_agent_documents",serde_json::json!({"path":"Sketch.ino"})).is_err());
    if let Some(other)=sessions.iter().find(|s|s.observation.instance_id!=before.instance_id) {
        std::fs::remove_file(discovery.join(format!("{}.json",other.observation.instance_id))).unwrap();
    }
    let automatic=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
    assert_eq!(automatic["observation"],read["observation"]);
    let mut current = before.clone();
    let changes=if actual {[("delay(250)","delay(200)"),("delay(200)","delay(100)")]}
        else {[("delay(1000)","delay(500)"),("delay(500)","delay(250)")]};
    for (old, new) in changes {
        let source = current.source.replace(old, new);
        let request=serde_json::json!({"instance_id":current.instance_id,"document_token":current.document_token,
            "version":current.version,"state_sha256":current.state_sha256,"description":"Change timing in the owned sketch","source":source});
        let prepared=host.call("propose_open_agent_change",request.clone()).unwrap();
        let pending=workflow.view().unwrap().job.unwrap();
        let job_id=pending.snapshot.input.job_id();
        assert_eq!(prepared["job_id"],job_id.to_string());
        assert_eq!(prepared["human_approval_required"],true);
        assert_eq!(runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap().source,current.source);
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
            .artifact(job_id, ArtifactFormat::LiveAgentReport)
            .unwrap()
            .read_text()
            .unwrap();
        let result: serde_json::Value = serde_json::from_str(&report).unwrap();
        let verified = host
            .call(
                "read_verified_result",
                serde_json::json!({"job_id":job_id}),
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
        assert_eq!(std::fs::read(&saved_path).unwrap(), saved);
        assert!(host.call("propose_open_agent_change",request).is_err());
    }
    assert_eq!(
        runtime
            .block_on(agent_editor::apply(
                &connection,
                Uuid::new_v4(),
                &before,
                &before.source.replace(changes[0].0, changes[0].1),
                &cancel
            ))
            .unwrap_err(),
        Failure::Refused
    );
    let final_state = current;
    assert!(final_state.source.contains("// unsaved user note\n") && final_state.dirty);
    assert_eq!(std::fs::read(&saved_path).unwrap(),saved);
    let saving=std::env::var("HAZLOMEXA_NATIVE_SAVE_TEST").as_deref()==Ok("1");
    if saving {
        let request=serde_json::json!({"instance_id":final_state.instance_id,"document_token":final_state.document_token,
            "version":final_state.version,"state_sha256":final_state.state_sha256,"description":"Save the reviewed current sketch"});
        let reply=host.call("propose_open_agent_save",request.clone()).unwrap();
        assert_eq!(reply["human_approval_required"],true);
        let pending=workflow.view().unwrap().job.unwrap();
        assert_eq!(std::fs::read(&saved_path).unwrap(),saved,"Preparing save must not write");
        let outcome=workflow.execute(workflow.begin(pending.approval_id.unwrap()).unwrap()).unwrap();
        assert_eq!(outcome.snapshot.phase,Phase::Completed,"{:?}",outcome.error);
        let report=host.call("read_verified_result",serde_json::json!({"job_id":outcome.snapshot.input.job_id()})).unwrap();
        assert_eq!(report["format"],"live_agent_report");
        let after=runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap();
        assert_eq!(after.source,final_state.source);assert_eq!(after.version,final_state.version);assert!(!after.dirty);
        assert_eq!(std::fs::read(&saved_path).unwrap(),final_state.source.as_bytes());
        assert!(host.call("propose_open_agent_save",request).is_err(),"A clean buffer must not prepare a second save");
    }
    let cancelling=std::env::var("HAZLOMEXA_NATIVE_CANCEL_TEST").as_deref()==Ok("1");
    if cancelling {
        assert!(!actual && !saving,"Delayed delivery is a controlled IDE fixture only");
        let source=final_state.source.replace("delay(250)","delay(125)");
        assert_ne!(source,final_state.source);
        let request=serde_json::json!({"instance_id":final_state.instance_id,"document_token":final_state.document_token,
            "version":final_state.version,"state_sha256":final_state.state_sha256,"description":"Owned delayed edit","source":source});
        host.call("propose_open_agent_change",request.clone()).unwrap();
        let pending=workflow.view().unwrap().job.unwrap();
        let job_id=pending.snapshot.input.job_id();
        let approval=pending.approval_id.unwrap();
        let dispatch=workflow.begin(approval).unwrap();
        let executing=workflow.clone();
        let worker=std::thread::spawn(move||executing.execute(dispatch).unwrap());
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        while !root.join("edit-delivered").is_file() {
            assert!(std::time::Instant::now()<deadline,"Native delivery not observed");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap().source,final_state.source);
        assert_eq!(workflow.cancel(job_id).unwrap().snapshot.phase,Phase::Cancelled);
        let outcome=worker.join().unwrap();
        assert_eq!(outcome.snapshot.phase,Phase::Cancelled);
        assert!(!outcome.busy);
        assert!(outcome.error.as_deref().unwrap().contains("Cancelar no deshace"));
        assert!(workflow.begin(approval).is_err());
        assert!(workflow.artifact(job_id,ArtifactFormat::LiveAgentReport).is_err());
        assert!(host.call("read_verified_result",serde_json::json!({"job_id":job_id})).is_err());
        std::fs::write(root.join("release-edit"),b"owned").unwrap();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        let after=loop {
            let observed=runtime.block_on(agent_editor::inspect(&connection,&cancel)).unwrap();
            if observed.source==source {break observed;}
            assert!(std::time::Instant::now()<deadline,"Late edit not observed");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(after.instance_id,final_state.instance_id);
        assert_eq!(after.document_token,final_state.document_token);
        assert_eq!(after.version,final_state.version+1);
        assert!(after.dirty && after.source.contains("// unsaved user note"));
        let read=host.call("inspect_open_agent_documents",args).unwrap();
        assert_eq!(read["observation"],serde_json::to_value(&after).unwrap());
        assert!(host.call("propose_open_agent_change",request).is_err());
        assert_eq!(std::fs::read(&saved_path).unwrap(),saved);
        assert_eq!(workflow.view().unwrap().job.unwrap().snapshot.phase,Phase::Cancelled);
        assert!(workflow.artifact(job_id,ArtifactFormat::LiveAgentReport).is_err());
        assert!(host.call("read_verified_result",serde_json::json!({"job_id":job_id})).is_err());
        println!("late native edit after cancellation passed");
    }
    if !actual {
        std::fs::remove_file(discovery.join(format!("{}.json",before.instance_id))).unwrap();
        let disconnected=host.call("inspect_open_agent_documents",serde_json::json!({})).unwrap();
        assert!(disconnected["documents"].as_array().unwrap().is_empty());
        assert!(disconnected.get("observation").is_none());
    }
    drop(host);
    drop(workflow);
    let recovered = Project::open(&project_root).unwrap().recover().unwrap();
    assert_eq!(recovered.len(), if saving || cancelling {3} else {2});
    assert_eq!(recovered.iter().filter(|run|run.phase==Phase::Cancelled).count(),usize::from(cancelling));
    assert!(recovered
        .iter()
        .all(|run| matches!(run.phase,Phase::Completed|Phase::Cancelled) && run.schema_version == 15));
    println!("Rust host and companion protocol: edit/readback/follow-up/stale refusal passed");
}
