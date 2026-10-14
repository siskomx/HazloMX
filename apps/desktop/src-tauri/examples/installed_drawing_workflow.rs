//! Native drawing edits through an installed connector; approval uses the shared backend fixture.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        cadcraft::DrawingResult,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, cli, sources, root] = args.as_slice() else {
        return Err("Pass installed helper, identified CADCraft CLI, fixture folder and fresh project folder".into());
    };
    fs::create_dir(root)?;
    for (name, unit_name, insunits) in [("metric", "mm", 4), ("imperial", "in", 1)] {
        let source = sources.join(format!("{name}.dxf"));
        let original = fs::read(&source)?;
        let project_root = root.join(name);
        fs::create_dir(&project_root)?;
        let project = Project::create(&project_root)?;
        let project_id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_cadcraft(cli.clone())?;
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
        let selected = host.select_drawing(
            project_id,
            source.clone(),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )?;
        assert!(workflow.view()?.job.is_none());
        assert!(workflow.view()?.history.is_empty());
        let inspected = service
            .call_tool(
                CallToolRequestParams::new("inspect_selected_drawing").with_arguments(
                    json!({"selection":selected.id})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await?;
        assert_ne!(inspected.is_error, Some(true), "{:?}", inspected.content);
        let inspected = inspected
            .structured_content
            .ok_or("Missing selected source inspection")?;
        assert_eq!(inspected["inspection"]["insunits"], insunits);
        assert!(inspected.get("entry").is_none());
        let source_proposal = service.call_tool(CallToolRequestParams::new("propose_selected_drawing_change")
            .with_arguments(json!({"selection":selected.id,"request":{"output_format":"dwg","units":unit_name,"circle":{"center":[30.0,0.0],"radius":2.0},"allow_loss":false}}).as_object().unwrap().clone())).await?;
        assert_ne!(
            source_proposal.is_error,
            Some(true),
            "{:?}",
            source_proposal.content
        );
        let source_proposal = source_proposal
            .structured_content
            .ok_or("Missing source proposal")?;
        assert_eq!(source_proposal["human_approval_required"], true);
        assert!(source_proposal.get("approval_id").is_none());
        let first = workflow.view()?.job.ok_or("Missing initial proposal")?;
        assert_eq!(first.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(
            first.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let completed = workflow
            .execute(workflow.begin(first.approval_id.ok_or("Missing initial approval")?)?)?;
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        let first_id = completed.snapshot.input.job_id();
        let saved = workflow.artifact(first_id, ArtifactFormat::Drawing)?.path;
        let first_bytes = fs::read(&saved)?;
        let read = service
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":first_id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(read.is_error, Some(true), "{:?}", read.content);
        let read = read.structured_content.ok_or("Missing first readback")?;
        assert_eq!(read["drawing"]["units"], unit_name);
        assert_eq!(read["drawing"]["editable"], true);
        let first_report: DrawingResult =
            serde_json::from_str(read["content"].as_str().ok_or("Missing report")?)?;
        assert_eq!(
            first_report.output_summary.as_ref().unwrap().entity_count,
            2
        );
        assert_eq!(
            first_report.circle_readback.as_ref().unwrap().final_matches,
            Some(1)
        );
        let arguments = json!({"project_id":project_id,"base_job_id":first_id,"result_sha256":read["sha256"],
            "request":{"output_format":"dxf","units":unit_name,"circle":{"center":[40.0,0.0],"radius":3.0},"allow_loss":false}});
        let mut stale = arguments.clone();
        stale["result_sha256"] = json!("0".repeat(64));
        let refused = service
            .call_tool(
                CallToolRequestParams::new("propose_drawing_change")
                    .with_arguments(stale.as_object().unwrap().clone()),
            )
            .await?;
        assert_eq!(refused.is_error, Some(true));
        let proposed = service
            .call_tool(
                CallToolRequestParams::new("propose_drawing_change")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(proposed.is_error, Some(true), "{:?}", proposed.content);
        let proposed = proposed.structured_content.ok_or("Missing proposal")?;
        assert_eq!(proposed["human_approval_required"], true);
        assert!(proposed.get("approval_id").is_none());
        let pending = workflow.view()?.job.ok_or("Missing reviewed proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(pending.snapshot.schema_version, 10);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let approval = pending.approval_id.ok_or("Missing follow-up approval")?;
        let dispatch = workflow.begin(approval)?;
        assert!(workflow.begin(approval).is_err());
        let edited = workflow.execute(dispatch)?;
        assert_eq!(
            edited.snapshot.phase,
            Phase::Completed,
            "{:?}",
            edited.error
        );
        let second_id = edited.snapshot.input.job_id();
        let final_read = service
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":second_id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(final_read.is_error, Some(true));
        let final_read = final_read
            .structured_content
            .ok_or("Missing final readback")?;
        assert_eq!(final_read["drawing"]["units"], unit_name);
        let report: DrawingResult = serde_json::from_str(
            final_read["content"]
                .as_str()
                .ok_or("Missing final report")?,
        )?;
        assert_eq!(report.output_summary.as_ref().unwrap().entity_count, 3);
        assert_eq!(
            report.circle_readback.as_ref().unwrap().final_matches,
            Some(1)
        );
        assert_eq!(fs::read(&source)?, original);
        assert_eq!(fs::read(saved)?, first_bytes);
        fs::write(
            project_root.join("calls.json"),
            serde_json::to_vec_pretty(
                &json!({"source_inspection":inspected,"source_proposal":source_proposal,"first_read":read,"proposal":proposed,"final_read":final_read}),
            )?,
        )?;
        service.cancel().await?;
        bridge.close().await?;
        drop(host);
        drop(workflow);
        let recovered = Project::open(&project_root)?;
        let history = recovered.recover()?;
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|run| run.phase == Phase::Completed));
        for id in [first_id, second_id] {
            for format in [
                ArtifactFormat::OriginalDrawing,
                ArtifactFormat::Drawing,
                ArtifactFormat::DrawingPreview,
                ArtifactFormat::DrawingReport,
            ] {
                assert!(recovered.artifact(id, format)?.path.is_file());
            }
        }
        let restored = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        restored.select_project(recovered)?;
        let restored_read =
            HostTools::new(restored).call("read_verified_result", json!({"job_id":second_id}))?;
        assert_eq!(restored_read["sha256"], final_read["sha256"]);
    }
    println!(
        "{}",
        json!({"pass":true,"cadcraft_version":"0.5.0","completed_jobs":4,"units":["mm","in"],
        "formats":["dwg","dxf"],"originals_and_first_results_preserved":true,"recovery_verified":true,
        "installed_connector":true,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
