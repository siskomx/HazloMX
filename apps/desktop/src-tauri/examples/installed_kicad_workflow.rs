//! Actual KiCad execution through an installed connector and the shared backend; no GUI approval.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, cli, source, root] = args.as_slice() else {
        return Err("Pass installed helper, identified KiCad CLI, fixture source folder and fresh project folder".into());
    };
    fs::create_dir_all(root)?;
    let originals: Vec<_> = fs::read_dir(source)?
        .map(|entry| {
            let path = entry?.path();
            Ok((path.clone(), fs::read(path)?))
        })
        .collect::<Result<_, std::io::Error>>()?;
    let project = Project::create(root)?;
    let project_id = project.id;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(project)?;
    workflow.set_kicad(cli.clone())?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let selected = host.select_schematic(
        project_id,
        source.clone(),
        source.join("basic_test.kicad_sch"),
    )?;
    let bridge = ToolBridge::start_project(host.clone(), helper, project_id)?;
    let config = bridge.config()?;
    let mut command = tokio::process::Command::new(&config.command);
    command
        .env_clear()
        .envs(hazlomexa::claude::environment())
        .envs(&config.env)
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    let service = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
    let inspection = service
        .call_tool(
            CallToolRequestParams::new("inspect_selected_kicad_check_source").with_arguments(
                json!({"selection":selected.id})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;
    assert_ne!(inspection.is_error, Some(true), "{:?}", inspection.content);
    let inspection = inspection
        .structured_content
        .ok_or("Missing source inspection")?;
    assert_eq!(inspection["report_units_require_confirmation"], true);
    assert!(workflow.view()?.job.is_none());
    let initial = service.call_tool(CallToolRequestParams::new("propose_selected_kicad_check").with_arguments(json!({"selection":selected.id,"source_set_sha256":inspection["source_set_sha256"],"units":"in"}).as_object().unwrap().clone())).await?;
    assert_ne!(initial.is_error, Some(true), "{:?}", initial.content);
    let initial = initial
        .structured_content
        .ok_or("Missing initial check proposal")?;
    assert_eq!(initial["human_approval_required"], true);
    assert!(initial.get("approval_id").is_none());
    let first = workflow.view()?.job.ok_or("Missing initial approval")?;
    let completed =
        workflow.execute(workflow.begin(first.approval_id.ok_or("Missing initial approval")?)?)?;
    assert_eq!(
        completed.snapshot.phase,
        Phase::Completed,
        "{:?}",
        completed.error
    );
    let first_id = completed.snapshot.input.job_id();
    let saved_path = workflow
        .artifact(first_id, ArtifactFormat::KicadSource)?
        .path;
    let saved_bytes = fs::read(&saved_path)?;
    let read = service
        .call_tool(
            CallToolRequestParams::new("read_verified_result")
                .with_arguments(json!({"job_id":first_id}).as_object().unwrap().clone()),
        )
        .await?;
    assert_ne!(read.is_error, Some(true));
    let read = read.structured_content.ok_or("Missing initial result")?;
    assert_eq!(read["checks"]["errors"], 2);
    assert_eq!(read["checks"]["units"], "in");
    let proposed=service.call_tool(CallToolRequestParams::new("propose_kicad_check").with_arguments(json!({"project_id":project_id,"base_job_id":first_id,"result_sha256":read["sha256"],"units":"mils"}).as_object().unwrap().clone())).await?;
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
    let second = workflow.execute(dispatch)?;
    assert_eq!(
        second.snapshot.phase,
        Phase::Completed,
        "{:?}",
        second.error
    );
    let second_id = second.snapshot.input.job_id();
    let final_read = service
        .call_tool(
            CallToolRequestParams::new("read_verified_result")
                .with_arguments(json!({"job_id":second_id}).as_object().unwrap().clone()),
        )
        .await?;
    assert_ne!(final_read.is_error, Some(true));
    let final_read = final_read
        .structured_content
        .ok_or("Missing final result")?;
    assert_eq!(final_read["checks"]["errors"], 2);
    assert_eq!(final_read["checks"]["units"], "mils");
    assert_eq!(fs::read(saved_path)?, saved_bytes);
    for (path, bytes) in originals {
        assert_eq!(fs::read(path)?, bytes);
    }
    fs::write(
        root.join("calls.json"),
        serde_json::to_vec_pretty(
            &json!({"source_inspection":inspection,"initial_proposal":initial,"first_read":read,"proposal":proposed,"final_read":final_read}),
        )?,
    )?;
    service.cancel().await?;
    bridge.close().await?;
    drop(host);
    drop(workflow);
    let recovered = Project::open(root)?;
    let history = recovered.recover()?;
    assert_eq!(history.len(), 2);
    assert!(history.iter().all(|s| s.phase == Phase::Completed));
    assert!(recovered
        .artifact(second_id, ArtifactFormat::KicadOutputs)?
        .path
        .is_dir());
    let restored = Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles"))?;
    restored.select_project(recovered)?;
    assert_eq!(restored.kicad_summary(second_id)?.errors, 2);
    println!(
        "{}",
        serde_json::to_string(
            &json!({"pass":true,"kicad_version":"10.0.7","completed_jobs":2,"initial_coordinate_units":"in","followup_coordinate_units":"mils","initial_source_first_mcp":true,"native_errors":2,"originals_and_first_result_preserved":true,"recovery_verified":true,"installed_connector":true,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false})
        )?
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
