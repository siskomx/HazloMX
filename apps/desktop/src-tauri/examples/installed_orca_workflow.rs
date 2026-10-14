//! Native file slicing through a supplied connector; approval uses the shared backend fixture.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        orca_run::SliceResult,
        project::{ArtifactFormat, OperationInput, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, cli, profiles, sources, root] = args.as_slice() else {
        return Err("Pass connector, identified OrcaSlicer, explicit Prusa profile folder, source fixtures and fresh project folder".into());
    };
    fs::create_dir(root)?;
    for (name, label) in [("metric", "mm"), ("imperial", "in")] {
        let source = sources.join(format!("{name}.stl"));
        let original = fs::read(&source)?;
        let selected = [
            profiles.join("machine/Prusa MK3S 0.4 nozzle.json"),
            profiles.join("process/0.20mm Standard @MK3S 0.4.json"),
            profiles.join("filament/Prusa Generic PLA.json"),
        ];
        let profile_bytes = selected
            .iter()
            .map(|path| Ok((path.clone(), fs::read(path)?)))
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        let project_root = root.join(name);
        fs::create_dir(&project_root)?;
        let project = Project::create(&project_root)?;
        let project_id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_orca(cli.clone())?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selection = host.select_slice(
            project_id,
            profiles.clone(),
            source.clone(),
            selected,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )?;
        assert!(workflow.view()?.job.is_none());
        let bridge = ToolBridge::start_project(host.clone(), helper, project_id)?;
        let config = bridge.config()?;
        let mut command = tokio::process::Command::new(&config.command);
        command
            .env_clear()
            .envs(hazlomexa::claude::environment())
            .envs(&config.env)
            .creation_flags(0x08000000)
            .kill_on_drop(true);
        let client = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
        let inspected = client
            .call_tool(
                CallToolRequestParams::new("inspect_selected_slice_sources").with_arguments(
                    json!({"selection":selection.id})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await?;
        assert_ne!(inspected.is_error, Some(true));
        let inspection = inspected
            .structured_content
            .ok_or("Missing source inspection")?;
        assert_eq!(inspection["units_require_confirmation"], true);
        assert!(inspection["stl_units"].is_null());
        let initial=client.call_tool(CallToolRequestParams::new("propose_selected_slice").with_arguments(json!({"selection":selection.id,"request":{"units":label,"bed":"textured","bed_temperature_c":45.}}).as_object().unwrap().clone())).await?;
        assert_ne!(initial.is_error, Some(true));
        let initial_proposal = initial
            .structured_content
            .ok_or("Missing initial proposal")?;
        assert_eq!(initial_proposal["human_approval_required"], true);
        assert!(initial_proposal.get("approval_id").is_none());
        let proposal = workflow
            .view()?
            .job
            .ok_or("Missing reviewed initial proposal")?;
        let first =
            workflow.execute(workflow.begin(proposal.approval_id.ok_or("Missing approval")?)?)?;
        assert_eq!(first.snapshot.phase, Phase::Completed, "{:?}", first.error);
        let first_id = first.snapshot.input.job_id();
        let baseline = [ArtifactFormat::Sliced, ArtifactFormat::Gcode]
            .into_iter()
            .map(|format| {
                let path = workflow.artifact(first_id, format)?.path;
                Ok((path.clone(), fs::read(path).map_err(|e| e.to_string())?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let reply = client
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":first_id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(reply.is_error, Some(true));
        let read = reply.structured_content.ok_or("Missing readback")?;
        assert_eq!(read["slicing"]["units"], label);
        assert_eq!(read["slicing"]["settings"]["bed_c"], 45.);
        let arguments = json!({"project_id":project_id,"base_job_id":first_id,"result_sha256":read["sha256"],"bed":"textured","bed_temperature_c":50.});
        for (key, value) in [
            ("result_sha256", json!("0".repeat(64))),
            ("bed_temperature_c", json!(201.)),
            ("path", json!(source)),
        ] {
            let mut wrong = arguments.clone();
            wrong[key] = value;
            let refused = client
                .call_tool(
                    CallToolRequestParams::new("propose_slice_settings")
                        .with_arguments(wrong.as_object().unwrap().clone()),
                )
                .await?;
            assert_eq!(refused.is_error, Some(true));
        }
        let proposed = client
            .call_tool(
                CallToolRequestParams::new("propose_slice_settings")
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
        let (OperationInput::Orca(before), OperationInput::Orca(after)) =
            (&first.snapshot.input, &pending.snapshot.input)
        else {
            return Err("Wrong operation".into());
        };
        assert_eq!(before.mesh, after.mesh);
        assert_eq!(before.settings.machine, after.settings.machine);
        assert_eq!(before.settings.process, after.settings.process);
        assert_eq!(before.settings.filament, after.settings.filament);
        let approval = pending.approval_id.ok_or("Missing follow-up approval")?;
        let dispatch = workflow.begin(approval)?;
        assert!(workflow.begin(approval).is_err());
        let final_job = workflow.execute(dispatch)?;
        assert_eq!(
            final_job.snapshot.phase,
            Phase::Completed,
            "{:?}",
            final_job.error
        );
        let final_id = final_job.snapshot.input.job_id();
        let reply = client
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":final_id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(reply.is_error, Some(true));
        let final_read = reply.structured_content.ok_or("Missing final readback")?;
        assert_eq!(final_read["slicing"]["units"], label);
        let report: SliceResult =
            serde_json::from_str(final_read["content"].as_str().ok_or("Missing report")?)?;
        assert_eq!(report.summary.settings["bed_c"], 50.);
        assert_eq!(report.summary.layers, 50);
        assert!(report
            .summary
            .bounds_mm
            .iter()
            .all(|v| (*v - 10.).abs() < 0.002));
        assert_eq!(fs::read(source)?, original);
        for (path, bytes) in baseline.into_iter().chain(profile_bytes) {
            assert_eq!(fs::read(path)?, bytes);
        }
        fs::write(
            project_root.join("calls.json"),
            serde_json::to_vec_pretty(
                &json!({"inspection":inspection,"initial_proposal":initial_proposal,"first_read":read,"proposal":proposed,"final_read":final_read}),
            )?,
        )?;
        client.cancel().await?;
        bridge.close().await?;
        drop(host);
        drop(workflow);
        let recovered = Project::open(&project_root)?;
        let history = recovered.recover()?;
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|run| run.phase == Phase::Completed));
        for id in [first_id, final_id] {
            for format in [
                ArtifactFormat::OriginalMesh,
                ArtifactFormat::Sliced,
                ArtifactFormat::Gcode,
                ArtifactFormat::SliceReport,
            ] {
                assert!(recovered.artifact(id, format)?.path.is_file());
            }
        }
        let restored = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        restored.select_project(recovered)?;
        assert_eq!(
            HostTools::new(restored).call("read_verified_result", json!({"job_id":final_id}))?
                ["sha256"],
            final_read["sha256"]
        );
    }
    println!(
        "{}",
        json!({"pass":true,"completed_jobs":4,"units":["mm","in"],"orca_version":"2.4.2","recovery_verified":true,"original_profiles_and_prior_outputs_preserved":true,"connector":helper,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false,"printer_control":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
