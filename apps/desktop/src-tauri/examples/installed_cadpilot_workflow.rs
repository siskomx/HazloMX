//! Native CADPilot execution through a supplied connector; approvals use the shared backend fixture.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        cadpilot::{Operation, Request, ResultDocument},
        host_tools::HostTools,
        jobs::Workflow,
        orca::Units,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, freecad, addon, source, root] = args.as_slice() else {
        return Err("Pass connector, identified FreeCADCmd, CADPilot addon, FCStd source and fresh project folder".into());
    };
    fs::create_dir(root)?;
    let original = fs::read(source)?;
    let project = Project::create(root)?;
    let project_id = project.id;
    let workflow = Arc::new(Workflow::new(freecad.clone(), root.join("profiles"))?);
    workflow.select_project(project)?;
    let inspection = workflow.propose_cadpilot(
        source.clone(),
        addon.clone(),
        Request {
            operation: Operation::Inspect,
            object: "Document".into(),
            units: Units::Mm,
            parameters: Default::default(),
        },
    )?;
    let completed = workflow.execute(
        workflow.begin(
            inspection
                .approval_id
                .ok_or("Missing inspection approval")?,
        )?,
    )?;
    assert_eq!(
        completed.snapshot.phase,
        Phase::Completed,
        "{:?}",
        completed.error
    );
    let inspection_id = completed.snapshot.input.job_id();
    let host = Arc::new(HostTools::new(workflow.clone()));
    let bridge = ToolBridge::start_project(host.clone(), helper, project_id)?;
    let config = bridge.config()?;
    let mut command = tokio::process::Command::new(&config.command);
    command
        .env_clear()
        .envs(hazlomexa::claude::environment())
        .envs(&config.env)
        .kill_on_drop(true);
    let client = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
    let response = client
        .call_tool(
            CallToolRequestParams::new("read_verified_result")
                .with_arguments(json!({"job_id":inspection_id}).as_object().unwrap().clone()),
        )
        .await?;
    assert_ne!(response.is_error, Some(true));
    let mut read = response
        .structured_content
        .ok_or("Missing inspection readback")?;
    let mut calls = vec![read.clone()];
    let mut saved = vec![];
    let mut jobs = vec![inspection_id];
    for request in [
        json!({"operation":"edit_box","object":"Target","units":"cm","parameters":{"length":1,"width":2,"height":3.5}}),
        json!({"operation":"add_cylinder","object":"Cylinder","units":"in","parameters":{"radius":1,"height":1,"x":1,"y":0,"z":0}}),
    ] {
        let arguments = json!({"project_id":project_id,"base_job_id":read["job_id"],"result_sha256":read["sha256"],"request":request});
        let mut stale = arguments.clone();
        stale["result_sha256"] = json!("0".repeat(64));
        let refused = client
            .call_tool(
                CallToolRequestParams::new("propose_cad_document_change")
                    .with_arguments(stale.as_object().unwrap().clone()),
            )
            .await?;
        assert_eq!(refused.is_error, Some(true));
        let proposal = client
            .call_tool(
                CallToolRequestParams::new("propose_cad_document_change")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(proposal.is_error, Some(true), "{:?}", proposal.content);
        let proposal = proposal.structured_content.ok_or("Missing proposal")?;
        assert_eq!(proposal["human_approval_required"], true);
        assert!(proposal.get("approval_id").is_none());
        let pending = workflow.view()?.job.ok_or("Missing reviewed proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let approval = pending.approval_id.ok_or("Missing approval")?;
        let dispatch = workflow.begin(approval)?;
        assert!(workflow.begin(approval).is_err());
        let done = workflow.execute(dispatch)?;
        assert_eq!(done.snapshot.phase, Phase::Completed, "{:?}", done.error);
        let job_id = done.snapshot.input.job_id();
        let response = client
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":job_id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(response.is_error, Some(true));
        read = response
            .structured_content
            .ok_or("Missing native readback")?;
        let report: ResultDocument =
            serde_json::from_str(read["content"].as_str().ok_or("Missing report")?)?;
        assert_eq!(report.after["Target"]["bbox"]["zmax"], 35.);
        assert_eq!(report.after["Target"]["volume_mm3"], 7000.);
        if request["operation"] == "add_cylinder" {
            assert_eq!(report.before["Target"], report.after["Target"]);
            let bounds = &report.after["Cylinder"]["bbox"];
            for (key, expected) in [
                ("xmin", 0.),
                ("xmax", 50.8),
                ("ymin", -25.4),
                ("ymax", 25.4),
                ("zmin", 0.),
                ("zmax", 25.4),
            ] {
                assert!(
                    (bounds[key].as_f64().ok_or("Missing cylinder bound")? - expected).abs()
                        < 0.000001
                );
            }
            assert_eq!(report.after["Cylinder"]["is_valid"], true);
            assert_eq!(report.after["Cylinder"]["counts"]["solids"], 1);
        }
        assert_eq!(fs::read(source)?, original);
        for (path, bytes) in &saved {
            assert_eq!(fs::read(path)?, *bytes);
        }
        for format in [ArtifactFormat::Fcstd, ArtifactFormat::Step] {
            let path = workflow.artifact(job_id, format)?.path;
            saved.push((path.clone(), fs::read(path)?));
        }
        calls.push(json!({"proposal":proposal,"readback":read}));
        jobs.push(job_id);
    }
    fs::write(root.join("calls.json"), serde_json::to_vec_pretty(&calls)?)?;
    client.cancel().await?;
    bridge.close().await?;
    drop(host);
    drop(workflow);
    let recovered = Project::open(root)?;
    let history = recovered.recover()?;
    assert_eq!(history.len(), 3);
    assert!(history.iter().all(|run| run.phase == Phase::Completed));
    for job_id in jobs {
        assert!(recovered
            .artifact(job_id, ArtifactFormat::OriginalCad)?
            .path
            .is_file());
        assert!(recovered
            .artifact(job_id, ArtifactFormat::CadpilotReport)?
            .path
            .is_file());
    }
    let restored = Arc::new(Workflow::new(freecad.clone(), root.join("profiles"))?);
    restored.select_project(recovered)?;
    let restored_read =
        HostTools::new(restored).call("read_verified_result", json!({"job_id":read["job_id"]}))?;
    assert_eq!(restored_read["sha256"], read["sha256"]);
    println!(
        "{}",
        json!({"pass":true,"completed_jobs":3,"units":["mm","cm","in"],"original_and_prior_results_preserved":true,
        "recovery_verified":true,"connector":helper,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
