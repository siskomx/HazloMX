//! Actual schematic/PCB edits via supplied connector; approval uses the backend fixture.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        konnect,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::{json, Value};
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, konnect_exe, cli, source, root] = args.as_slice() else {
        return Err(
            "Pass connector, identified Konnect, KiCad CLI, source folder and fresh project folder"
                .into(),
        );
    };
    fs::create_dir(root)?;
    let originals: Vec<_> = fs::read_dir(source)?
        .map(|entry| {
            let path = entry?.path();
            Ok((path.clone(), fs::read(path)?))
        })
        .collect::<Result<_, std::io::Error>>()?;
    let mut completed_jobs = 0;
    for pcb in [false, true] {
        let folder = root.join(if pcb { "pcb" } else { "schematic" });
        fs::create_dir(&folder)?;
        let project = Project::create(&folder)?;
        let project_id = project.id;
        let workflow = Arc::new(Workflow::new(
            folder.join("FreeCADCmd.exe"),
            folder.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_konnect(konnect_exe.clone())?;
        workflow.set_kicad(cli.clone())?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selection = if pcb {
            host.select_pcb(
                project_id,
                source.clone(),
                source.join("basic_test.kicad_pcb"),
            )?
            .id
        } else {
            host.select_schematic(
                project_id,
                source.clone(),
                source.join("basic_test.kicad_sch"),
            )?
            .id
        };
        let bridge = ToolBridge::start_project(host.clone(), helper, project_id)?;
        let config = bridge.config()?;
        let mut command = tokio::process::Command::new(&config.command);
        command
            .env_clear()
            .envs(hazlomexa::claude::environment())
            .envs(&config.env)
            .kill_on_drop(true);
        let service = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
        assert_eq!(
            service.list_all_tools().await?.len(),
            hazlomexa::host_tools::catalog().len()
        );
        let call = |name: &'static str, arguments: Value| {
            let service = &service;
            async move {
                let response = service
                    .call_tool(
                        CallToolRequestParams::new(name)
                            .with_arguments(arguments.as_object().unwrap().clone()),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if response.is_error == Some(true) {
                    return Err(format!("{:?}", response.content));
                }
                response
                    .structured_content
                    .ok_or_else(|| "Missing tool result".to_owned())
            }
        };
        let inspected = call(
            if pcb {
                "inspect_selected_pcb"
            } else {
                "inspect_selected_schematic"
            },
            json!({"selection":selection}),
        )
        .await?;
        let mut request = if pcb {
            let component = inspected["components"]
                .as_array()
                .ok_or("Missing footprints")?
                .iter()
                .find(|c| c["reference"] == "J1")
                .ok_or("Missing J1")?;
            json!({"selection":selection,"component":component["uuid"],"units":"in","x":203.34/25.4,"y":155.33/25.4})
        } else {
            assert!(inspected["components"]
                .as_array()
                .ok_or("Missing symbols")?
                .iter()
                .any(|c| c["reference"] == "R1" && c["value"] == "10k"));
            json!({"selection":selection,"reference":"R1","value":"12.34k"})
        };
        let tool = if pcb {
            "propose_pcb_move"
        } else {
            "propose_schematic_value"
        };
        let mut extra = request.clone();
        extra["path"] = json!("C:\\arbitrary.kicad_pcb");
        assert!(call(tool, extra).await.is_err());
        assert!(workflow.view()?.job.is_none());
        let mut records = vec![json!({"initial_inspection":inspected})];
        let mut preserved = Vec::new();
        let mut ids = Vec::new();
        for index in 0..2 {
            let proposed = call(tool, request.clone()).await?;
            assert_eq!(proposed["human_approval_required"], true);
            assert!(proposed.get("approval_id").is_none());
            let pending = workflow.view()?.job.ok_or("Missing reviewed proposal")?;
            assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
            assert_eq!(pending.snapshot.schema_version, if pcb { 12 } else { 10 });
            assert_eq!(
                pending.snapshot.audit.as_ref().unwrap().source,
                RunSource::Mcp
            );
            let approval = pending.approval_id.ok_or("Missing approval")?;
            let dispatch = workflow.begin(approval)?;
            assert!(workflow.begin(approval).is_err());
            let worker = workflow.clone();
            let completed = tokio::task::spawn_blocking(move || worker.execute(dispatch)).await??;
            assert_eq!(
                completed.snapshot.phase,
                Phase::Completed,
                "{:?}",
                completed.error
            );
            let id = completed.snapshot.input.job_id();
            ids.push(id);
            let read = call("read_verified_result", json!({"job_id":id})).await?;
            let artifact = workflow.artifact(id, ArtifactFormat::KonnectEdited)?.path;
            let bytes = fs::read(&artifact)?;
            preserved.push((artifact, bytes.clone()));
            let context =
                json!({"selection":selection,"base_job_id":id,"result_sha256":read["sha256"]});
            if pcb {
                use sha2::Digest;
                assert_eq!(read["private_paths_redacted"], true);
                let content = read["content"]
                    .as_str()
                    .ok_or("Missing projected PCB report")?;
                let public: serde_json::Value = serde_json::from_str(content)?;
                assert_eq!(
                    public["verification_input"]["source_root"],
                    "private source folder"
                );
                assert_eq!(
                    read["content_sha256"],
                    format!("{:x}", sha2::Sha256::digest(content.as_bytes()))
                );
                assert_ne!(read["content_sha256"], read["sha256"]);
                let expected_x = if index == 0 { 203.34 } else { 205.0 };
                let actual = call("inspect_selected_pcb", context.clone()).await?;
                let component = actual["components"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["reference"] == "J1")
                    .ok_or("Missing saved J1")?;
                assert_eq!(component["x_mm"], expected_x);
                assert_eq!(component["y_mm"], 155.33);
                let native = konnect::footprints(&bytes)?
                    .into_iter()
                    .find(|c| c.reference == "J1")
                    .ok_or("Missing native J1")?;
                assert_eq!(native.x_mm, expected_x);
                assert_eq!(native.y_mm, 155.33);
                request = context;
                request["component"] = component["uuid"].clone();
                request["units"] = json!("mils");
                request["x"] = json!(205.0 / 0.0254);
                request["y"] = json!(155.33 / 0.0254);
                records
                    .push(json!({"proposal":proposed,"readback":read,"saved_components":actual}));
            } else {
                let expected = if index == 0 { "12.34k" } else { "22k" };
                assert_eq!(konnect::current_value(&bytes, "R1")?, expected);
                assert!(read["components"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["reference"] == "R1" && c["value"] == expected));
                request = json!({"selection":selection,"reference":"R1","value":"22k","base_job_id":id,"result_sha256":read["sha256"]});
                records.push(json!({"proposal":proposed,"readback":read}));
            }
            let mut stale = request.clone();
            stale["result_sha256"] = json!("0".repeat(64));
            let before = workflow.view()?.history.len();
            assert!(call(tool, stale).await.is_err());
            assert_eq!(workflow.view()?.history.len(), before);
        }
        fs::write(
            folder.join("calls.json"),
            serde_json::to_vec_pretty(&records)?,
        )?;
        service.cancel().await?;
        bridge.close().await?;
        drop(host);
        drop(workflow);
        let recovered = Project::open(&folder)?;
        let history = recovered.recover()?;
        assert_eq!(history.len(), 2);
        assert!(history.iter().all(|run| run.phase == Phase::Completed));
        let restored = Workflow::new(folder.join("FreeCADCmd.exe"), folder.join("profiles"))?;
        restored.select_project(recovered)?;
        let read = HostTools::new(Arc::new(restored))
            .call("read_verified_result", json!({"job_id":ids[1]}))?;
        assert_eq!(read["sha256"], records[2]["readback"]["sha256"]);
        for (path, bytes) in preserved {
            assert_eq!(fs::read(path)?, bytes);
        }
        completed_jobs += 2;
    }
    for (path, bytes) in originals {
        assert_eq!(fs::read(path)?, bytes);
    }
    println!(
        "{}",
        json!({"pass":true,"konnect_version":"0.13.0","kicad_version":"10.0.7","completed_jobs":completed_jobs,"schematic_jobs":2,"pcb_jobs":2,"pcb_units":["in","mils"],"recovery_verified":true,"sources_and_prior_results_preserved":true,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false,"hardware_mutations":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
