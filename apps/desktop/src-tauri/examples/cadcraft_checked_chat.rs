//! Owned patched-window acceptance through production chat tools and Workflow approvals.
use hazlomexa::{
    host_tools::HostTools,
    jobs::Workflow,
    project::{ArtifactFormat, Phase, Project},
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

struct Native {
    stream: BufReader<TcpStream>,
    id: u64,
}
impl Native {
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        writeln!(
            self.stream.get_mut(),
            "{}",
            json!({"id":self.id,"method":method,"params":params})
        )
        .unwrap();
        let mut frame = String::new();
        self.stream.read_line(&mut frame).unwrap();
        assert!(frame.len() < 256 * 1024 && frame.ends_with('\n'));
        let reply: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(reply["id"], self.id);
        assert_eq!(reply["ok"], true);
        reply["result"].clone()
    }
    fn drawing(&mut self) -> Value {
        self.call("drawing.inspect", json!({"entities":true,"limit":128}))
    }
    fn execute(&mut self, command: &str, params: Value) -> Value {
        self.call("engine.execute", json!({"command":command,"params":params}))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("Pass owned acceptance root")?);
    let port: u16 = args.next().ok_or("Pass owned native port")?.parse()?;
    let executable = PathBuf::from(
        args.next()
            .ok_or("Pass explicitly selected patched runtime")?,
    );
    assert!(args.next().is_none());
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    assert_eq!(
        root.canonicalize()?.parent(),
        Some(workspace.join("target").canonicalize()?.as_path())
    );
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("cadcraft-chat-native-"));
    let stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(8)))?;
    let mut native = Native {
        stream: BufReader::new(stream),
        id: 0,
    };
    let original = native.drawing();
    assert_eq!(
        Path::new(
            original["path"]
                .as_str()
                .ok_or("Missing owned document path")?
        )
        .canonicalize()?,
        root.join("OwnedDrawing.dxf").canonicalize()?
    );
    let original_session = native.call("ui.inspect", json!({}))["session"].clone();
    assert_eq!(original["entities"].as_array().unwrap().len(), 1);
    assert_eq!(original["dirty"], false);
    let workflow = Arc::new(Workflow::new(
        root.join("UnusedFreeCAD.exe"),
        root.join("profiles"),
    )?);
    fs::create_dir(root.join("project"))?;
    let project = Project::create(&root.join("project"))?;
    let project_id = project.id;
    workflow.select_project(project)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let cancel = Arc::new(AtomicBool::new(false));
    let selected =
        host.select_live_drawing(project_id, executable.clone(), port, cancel.clone())?;
    assert!(!selected.observation.dirty);
    let document_sha256 = selected
        .observation
        .document_sha256
        .clone()
        .ok_or("Missing native document identity")?;
    assert_eq!(selected.observation.insunits, 4);
    assert_eq!(
        native.call("ui.inspect", json!({}))["session"],
        original_session,
        "Native units queries changed document/revision/Undo context"
    );
    let inspect = |selection| host.call("inspect_open_drawing", json!({"selection":selection}));
    let propose = |selection, reading: &Value, units: &str, operation: Value| {
        host.call("propose_open_drawing_change",json!({
        "selection":selection,"binding_sha256":reading["observation"]["binding_sha256"],"agreement":{"kind":"software_operation","outcome":"Apply the explicitly requested owned fixture operation","assumptions":["Physical function is not verified"],"constraints":["Preserve unrelated source and manual edits"],"losses":[],"verification":"Check the native report and final state; independent reopen remains unverified"},"description":"Owned native approval acceptance",
        "units":units,"operation":operation}))
    };
    let approve = || {
        let pending = workflow.view().unwrap().job.unwrap();
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let approval = pending.approval_id.unwrap();
        let dispatch = workflow.begin(approval).unwrap();
        assert!(workflow.begin(approval).is_err());
        workflow.execute(dispatch).unwrap()
    };
    let read = inspect(selected.id)?;
    let before = workflow.view()?.history.len();
    assert!(propose(selected.id, &read, "mm", json!({"action":"qsave"})).is_err());
    assert_eq!(workflow.view()?.history.len(), before);
    propose(
        selected.id,
        &read,
        "mm",
        json!({"action":"move_circle","handle":"100","delta":[1.,0.]}),
    )?;
    let first = approve();
    assert_eq!(first.snapshot.phase, Phase::Completed);
    let first_report = workflow
        .artifact(
            first.snapshot.input.job_id(),
            ArtifactFormat::LiveDrawingReport,
        )?
        .read_text()?;
    assert_eq!(
        serde_json::from_str::<Value>(&first_report)?["after"]["document_sha256"],
        document_sha256
    );
    let first_read = native.drawing();
    assert_eq!(first_read["entities"][0]["geometry"]["center"]["x"], 1.);
    let read = inspect(selected.id)?;
    propose(
        selected.id,
        &read,
        "in",
        json!({"action":"move_circle","handle":"100","delta":[0.1,0.]}),
    )?;
    let second = approve();
    assert_eq!(second.snapshot.phase, Phase::Completed);
    let final_read = native.drawing();
    assert_eq!(final_read["entities"][0]["geometry"]["center"]["x"], 3.54);
    let read = inspect(selected.id)?;
    let export_proposal = propose(selected.id, &read, "mm", json!({"action":"export_dxf"}))?;
    assert_eq!(export_proposal["native_undo"], false);
    assert_eq!(export_proposal["export_copy"], true);
    let exported = approve();
    assert_eq!(exported.snapshot.phase, Phase::Completed);
    let exported_dxf = workflow
        .artifact(exported.snapshot.input.job_id(), ArtifactFormat::Drawing)?
        .read_text()?;
    assert!(!exported_dxf.is_empty());
    let export_report:Value=serde_json::from_str(&workflow.artifact(exported.snapshot.input.job_id(),ArtifactFormat::LiveDrawingReport)?.read_text()?)?;
    let check=&export_report["native_copy_check"];assert_eq!(check["version"],1);assert_eq!(check["native_reopen_checked"],true);assert_eq!(check["native_bytes_roundtrip_equal"],true);assert_eq!(check["model_geometry_checked"],true);assert_eq!(check["native_units"],4);assert_eq!(check["native_open_dependencies"],json!([]));for key in ["rendering_resources_checked","gui_state_checked","physical_function_checked"]{assert_eq!(check[key],false);}
    assert_eq!(
        native.drawing(),
        final_read,
        "Export changed the open source or its Undo history"
    );
    let read = inspect(selected.id)?;
    propose(
        selected.id,
        &read,
        "mm",
        json!({"action":"move_circle","handle":"100","delta":[999.,0.]}),
    )?;
    let pending = workflow.view()?.job.unwrap();
    workflow.cancel(pending.snapshot.input.job_id())?;
    assert!(workflow.begin(pending.approval_id.unwrap()).is_err());
    assert_eq!(native.drawing()["entities"], final_read["entities"]);
    native.execute("u", json!({}));
    assert_eq!(native.drawing()["entities"], first_read["entities"]);
    native.execute("u", json!({}));
    assert_eq!(native.drawing()["entities"], original["entities"]);
    let read = inspect(selected.id)?;
    propose(
        selected.id,
        &read,
        "mm",
        json!({"action":"move_circle","handle":"100","delta":[999.,0.]}),
    )?;
    native.execute("layer.set", json!({"name":"0","lineweight":0.5}));
    assert_eq!(
        native.drawing()["layers"],
        original["layers"],
        "Trial must cover data omitted from projection"
    );
    let refused = approve();
    assert_eq!(refused.snapshot.phase, Phase::Failed);
    assert_eq!(native.drawing()["entities"], original["entities"]);
    assert!(
        inspect(selected.id).is_err(),
        "Failed transport must require reconnect rather than retry"
    );
    native.execute("u", json!({}));
    let selected = host.select_live_drawing(project_id, executable.clone(), port, cancel)?;
    let read = inspect(selected.id)?;
    propose(
        selected.id,
        &read,
        "mm",
        json!({"action":"circle","center":[20.,20.],"diameter":10.}),
    )?;
    let created = approve();
    assert_eq!(created.snapshot.phase, Phase::Completed);
    assert_eq!(native.drawing()["entities"].as_array().unwrap().len(), 2);
    native.execute("u", json!({}));
    assert_eq!(native.drawing()["entities"], original["entities"]);
    let before_reopen = inspect(selected.id)?;
    native.execute("close", json!({}));
    native.execute("open", json!({"path":root.join("OwnedDrawing.dxf")}));
    let reopened = inspect(selected.id)?;
    assert_eq!(
        reopened["observation"]["entities"],
        before_reopen["observation"]["entities"]
    );
    assert_eq!(
        reopened["observation"]["title"],
        before_reopen["observation"]["title"]
    );
    assert_eq!(
        reopened["observation"]["instance_id"],
        before_reopen["observation"]["instance_id"]
    );
    assert_ne!(
        reopened["observation"]["document_sha256"],
        before_reopen["observation"]["document_sha256"]
    );
    assert!(propose(
        selected.id,
        &before_reopen,
        "mm",
        json!({"action":"move_circle","handle":"100","delta":[999.,0.]})
    )
    .is_err());
    let report_read = host.call(
        "read_verified_result",
        json!({"job_id":first.snapshot.input.job_id()}),
    )?;
    assert_eq!(report_read["format"], "live_drawing_report");
    assert_eq!(report_read["content"], first_report);
    let recovered = Workflow::new(
        root.join("UnusedFreeCAD.exe"),
        root.join("recovered-profiles"),
    )?;
    drop(host);
    drop(workflow);
    let jobs = recovered.select_project(Project::open(&root.join("project"))?)?;
    assert_eq!(
        jobs.iter().filter(|j| j.phase == Phase::Completed).count(),
        4
    );
    assert_eq!(
        recovered
            .artifact(
                first.snapshot.input.job_id(),
                ArtifactFormat::LiveDrawingReport
            )?
            .read_text()?,
        first_report
    );
    assert_eq!(
        recovered
            .artifact(exported.snapshot.input.job_id(), ArtifactFormat::Drawing)?
            .read_text()?,
        exported_dxf
    );
    let recorded=jobs.iter().filter(|job|job.phase==Phase::Completed).all(|job|root.join("project/.hazlomexa/artifacts").join(job.input.job_id().to_string()).join("drawing-completion.json").is_file());
    let anchored=recorded && jobs.iter().filter(|job|job.phase==Phase::Completed).all(|job|{let directory=root.join("project/.hazlomexa/artifacts").join(job.input.job_id().to_string());let record:Value=serde_json::from_slice(&fs::read(directory.join("live-dispatch.json")).unwrap()).unwrap();let hash=format!("{:x}",<sha2::Sha256 as sha2::Digest>::digest(fs::read(directory.join("drawing-completion.json")).unwrap()));record["drawing_completion_required"]==true && record["drawing_completion_sha256"]==hash});
    assert!(anchored,"Every completed owned drawing operation needs its original dispatch anchor");
    if recorded {
        drop(recovered);
        let project=Project::open(&root.join("project"))?;
        // Model lost host acknowledgments using owned copies; retain the original reports and journals.
        for id in [first.snapshot.input.job_id(),exported.snapshot.input.job_id()] {
            let directory=project.artifact_directory(id);fs::rename(directory.join("result.json"),root.join(format!("retained-report-{id}.json")))?;
            let journal=directory.join("live-dispatch.json");let bytes=fs::read(&journal)?;fs::write(root.join(format!("retained-dispatch-{id}.json")),&bytes)?;let mut dispatch:Value=serde_json::from_slice(&bytes)?;dispatch["outcome"]=json!("uncertain");dispatch["receipt_sha256"]=Value::Null;fs::write(journal,serde_json::to_vec_pretty(&dispatch)?)?;
            let mut snapshot=jobs.iter().find(|job|job.input.job_id()==id).unwrap().clone();snapshot.phase=Phase::Interrupted;snapshot.audit.as_mut().unwrap().files_verified=false;
            let hazlomexa::project::OperationInput::LiveDrawing(input)=&mut snapshot.input else{panic!("Wrong owned job")};input.receipt_sha256=None;project.save(&snapshot)?;
        }
        drop(project);
        let restarted=Arc::new(Workflow::new(root.join("UnusedFreeCAD.exe"),root.join("completion-recovery-profiles"))?);restarted.select_project(Project::open(&root.join("project"))?)?;
        let retained=HostTools::new(restarted.clone());let current=native.drawing();let session=native.call("ui.inspect",json!({}))["session"].clone();
        for id in [first.snapshot.input.job_id(),exported.snapshot.input.job_id()] {
            let result=retained.call("recover_native_result",json!({"job_id":id}))?;assert_eq!(result["recovered"],true);assert_eq!(result["published"],true);assert_eq!(result["edit_repeated"],false);
            assert_eq!(retained.call("recover_native_result",json!({"job_id":id}))?["published"],true);
        }
        assert_eq!(native.drawing(),current);assert_eq!(native.call("ui.inspect",json!({}))["session"],session);
        assert_eq!(restarted.artifact(first.snapshot.input.job_id(),ArtifactFormat::LiveDrawingReport)?.read_text()?,first_report);
        assert_eq!(restarted.artifact(exported.snapshot.input.job_id(),ArtifactFormat::Drawing)?.read_text()?,exported_dxf);
    }
    fs::write(
        root.join("backend-result.json"),
        serde_json::to_vec_pretty(
            &json!({"pass":true,"completed_jobs":4,"cancelled_jobs":1,"refused_jobs":1,
        "native_mm_move":1,"native_inch_followup":0.1,"final_x_mm":3.54,"native_undo_steps":3,"stale_unprojected_change_refused":true,
        "units_query_preserved_state":true,"atomic_document_identity":true,"identical_reopen_distinguished":true,"native_export_verified":true,"native_dxf_roundtrip":true,"native_copy_check_manifest":true,"export_includes_unsaved_state":true,"export_no_native_undo":true,"approval_replay_refused":true,"recovery_pass":true,"retained_completion_recovery":recorded,"native_completion_anchored":anchored,"real_provider":false,"desktop_approval":false}),
        )?,
    )?;
    println!("PASS: production checked drawing tools, approval/replay/cancel, metric/inch native readback, hidden-state refusal, Undo and report recovery");
    Ok(())
}
