//! Authenticated Agent copy edits through the supplied connector; no hardware or GUI approval.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        agent,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf, sync::Arc};
    assert_eq!(std::env::var("USERNAME")?, "WDAGUtilityAccount");
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, source, root] = args.as_slice() else {
        return Err("Pass installed connector, original sketch and fresh project folder".into());
    };
    fs::create_dir(root)?;
    let original = fs::read(source)?;
    let (name, selected) = agent::selected_source(source)?;
    let project = Project::create(root)?;
    let project_id = project.id;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(project)?;
    workflow.set_agent_source(name, selected.clone())?;
    workflow.set_agent(agent::connect().await?)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let bridge = ToolBridge::start_project(host.clone(), helper, project_id)?;
    let config = bridge.config()?;
    let mut command = tokio::process::Command::new(&config.command);
    command
        .env_clear()
        .envs(hazlomexa::claude::environment())
        .envs(&config.env)
        .kill_on_drop(true);
    let service = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
    let call = |name: &'static str, arguments: Value| {
        let service = &service;
        async move {
            let reply = service
                .call_tool(
                    CallToolRequestParams::new(name)
                        .with_arguments(arguments.as_object().unwrap().clone()),
                )
                .await
                .map_err(|e| e.to_string())?;
            if reply.is_error == Some(true) {
                return Err(format!("{:?}", reply.content));
            }
            reply
                .structured_content
                .ok_or_else(|| "Missing tool result".into())
        }
    };
    let inspected = call("inspect_selected_sketch", json!({})).await?;
    assert_eq!(inspected["source"], selected);
    assert_eq!(inspected["agent_connected"], true);
    let mut arguments = json!({"project_id":project_id,"source_name":inspected["source_name"],"source_sha256":inspected["source_sha256"],"source":"void setup() {} void loop() { delay(500); }"});
    for (field, value) in [
        ("source_sha256", json!("0".repeat(64))),
        ("path", json!("C:\\arbitrary.ino")),
    ] {
        let mut invalid = arguments.clone();
        invalid[field] = value;
        assert!(call("propose_selected_sketch_edit", invalid).await.is_err());
        assert!(workflow.view()?.job.is_none());
    }
    let mut records = vec![json!({"inspection":inspected})];
    let mut preserved = Vec::new();
    let mut ids = Vec::new();
    for index in 0..2 {
        let proposal = call("propose_selected_sketch_edit", arguments.clone()).await?;
        assert_eq!(proposal["human_approval_required"], true);
        assert!(proposal.get("approval_id").is_none());
        let pending = workflow.view()?.job.ok_or("Missing review")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let stale = pending.approval_id.ok_or("Missing approval")?;
        workflow.set_agent(agent::connect().await?)?;
        assert!(workflow.begin(stale).is_err());
        let approval = workflow
            .view()?
            .job
            .ok_or("Missing refreshed review")?
            .approval_id
            .ok_or("Missing refreshed approval")?;
        let dispatch = workflow.begin(approval)?;
        assert!(workflow.begin(approval).is_err());
        let worker = workflow.clone();
        let done = tokio::task::spawn_blocking(move || worker.execute(dispatch)).await??;
        assert_eq!(done.snapshot.phase, Phase::Completed, "{:?}", done.error);
        let id = done.snapshot.input.job_id();
        let read = call("read_verified_result", json!({"job_id":id})).await?;
        assert_eq!(read["content"], arguments["source"]);
        let input_original = if index == 0 {
            selected.as_str()
        } else {
            "void setup() {} void loop() { delay(500); }"
        };
        assert_eq!(
            workflow
                .artifact(id, ArtifactFormat::OriginalSketch)?
                .read_text()?,
            input_original
        );
        for format in [ArtifactFormat::Sketch, ArtifactFormat::OriginalSketch] {
            let path = workflow.artifact(id, format)?.path;
            preserved.push((path.clone(), fs::read(path)?));
        }
        arguments = json!({"project_id":project_id,"source_name":read["source_name"],"source_sha256":read["sha256"],"base_job_id":id,"source":"void setup() {} void loop() { delay(1000); }"});
        let mut stale = arguments.clone();
        stale["source_sha256"] = json!("0".repeat(64));
        let count = workflow.view()?.history.len();
        assert!(call("propose_selected_sketch_edit", stale).await.is_err());
        assert_eq!(workflow.view()?.history.len(), count);
        records.push(json!({"proposal":proposal,"readback":read}));
        ids.push(id);
    }
    assert_eq!(
        call("inspect_selected_sketch", json!({})).await?["source"],
        selected
    );
    let denied = workflow.propose_agent("void setup() {} void loop() { delay(250); }".into())?;
    let denied_id = denied.snapshot.input.job_id();
    workflow.cancel(denied_id)?;
    assert!(workflow
        .begin(denied.approval_id.ok_or("Missing denied approval")?)
        .is_err());
    let occupied = workflow.propose_agent("void setup() {} void loop() { delay(750); }".into())?;
    let failed_id = occupied.snapshot.input.job_id();
    let destination = root
        .join(".hazlomexa/artifacts")
        .join(failed_id.to_string());
    fs::create_dir(&destination)?;
    fs::write(
        destination.join("keep.txt"),
        b"Preserve the occupied destination",
    )?;
    let dispatch = workflow.begin(occupied.approval_id.ok_or("Missing occupied review")?)?;
    let worker = workflow.clone();
    let failed = tokio::task::spawn_blocking(move || worker.execute(dispatch)).await??;
    assert_eq!(failed.snapshot.phase, Phase::Failed);
    assert!(failed.snapshot.audit.as_ref().unwrap().files_verified);
    assert!(workflow
        .artifact(failed_id, ArtifactFormat::Sketch)
        .is_err());
    assert_eq!(
        fs::read(destination.join("keep.txt"))?,
        b"Preserve the occupied destination"
    );
    fs::write(
        root.join("calls.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    service.cancel().await?;
    bridge.close().await?;
    drop(host);
    drop(workflow);
    let recovered = Project::open(root)?;
    let history = recovered.recover()?;
    assert_eq!(history.len(), 4);
    for (id, phase) in ids
        .iter()
        .copied()
        .map(|id| (id, Phase::Completed))
        .chain([(denied_id, Phase::Cancelled), (failed_id, Phase::Failed)])
    {
        assert_eq!(
            history
                .iter()
                .find(|run| run.input.job_id() == id)
                .ok_or("Missing recovered run")?
                .phase,
            phase
        );
    }
    let restored = Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles"))?;
    restored.select_project(recovered)?;
    let read = HostTools::new(Arc::new(restored))
        .call("read_verified_result", json!({"job_id":ids[1]}))?;
    assert_eq!(read["sha256"], records[2]["readback"]["sha256"]);
    for (path, bytes) in preserved {
        assert_eq!(fs::read(path)?, bytes);
    }
    assert_eq!(fs::read(source)?, original);
    println!(
        "{}",
        json!({"pass":true,"agent_version":agent::VERSION,"completed_jobs":2,"denied_jobs":1,"occupied_destination_failures":1,"reconnect_invalidates_approval":true,"recovery_verified":true,"original_and_prior_results_preserved":true,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false,"hardware_mutations":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
