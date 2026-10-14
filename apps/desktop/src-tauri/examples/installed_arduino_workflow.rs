//! Real compile/readback through the supplied installed connector; no hardware or GUI approval.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        arduino,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, Phase, Project, RunSource},
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf, sync::Arc};
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [helper, cli, data, root] = args.as_slice() else {
        return Err(
            "Pass connector, identified Arduino CLI, owned AVR data and fresh project folder"
                .into(),
        );
    };
    fs::create_dir(root)?;
    let original = b"Preserve this user's original sketch.\n";
    fs::write(root.join("user-original.txt"), original)?;
    let project = Project::create(root)?;
    let project_id = project.id;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(project)?;
    workflow.set_arduino(cli.clone(), data.clone())?;
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
    let source = "void setup() { pinMode(LED_BUILTIN, OUTPUT); } void loop() { digitalWrite(LED_BUILTIN, HIGH); }";
    let mut arguments = json!({"project_id":project_id,"fqbn":arduino::FQBN,"source":source});
    for (field, value) in [
        ("fqbn", json!("arduino:avr:mega")),
        ("path", json!("C:\\arbitrary.ino")),
    ] {
        let mut invalid = arguments.clone();
        invalid[field] = value;
        let refused = service
            .call_tool(
                CallToolRequestParams::new("propose_arduino_compile")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await?;
        assert_eq!(refused.is_error, Some(true));
        assert!(workflow.view()?.job.is_none());
    }
    let mut records = Vec::new();
    let mut completed_ids = Vec::new();
    let mut preserved = Vec::new();
    for index in 0..2 {
        let proposal = service
            .call_tool(
                CallToolRequestParams::new("propose_arduino_compile")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(proposal.is_error, Some(true), "{:?}", proposal.content);
        let proposal = proposal.structured_content.ok_or("Missing proposal")?;
        assert_eq!(proposal["human_approval_required"], true);
        assert!(proposal.get("approval_id").is_none());
        let pending = workflow.view()?.job.ok_or("Missing reviewed job")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(
            pending.snapshot.audit.as_ref().unwrap().source,
            RunSource::Mcp
        );
        let approval = pending.approval_id.ok_or("Missing approval")?;
        let dispatch = workflow.begin(approval)?;
        assert!(workflow.begin(approval).is_err());
        let completed = workflow.execute(dispatch)?;
        assert_eq!(
            completed.snapshot.phase,
            Phase::Completed,
            "{:?}",
            completed.error
        );
        let id = completed.snapshot.input.job_id();
        let read = service
            .call_tool(
                CallToolRequestParams::new("read_verified_result")
                    .with_arguments(json!({"job_id":id}).as_object().unwrap().clone()),
            )
            .await?;
        assert_ne!(read.is_error, Some(true));
        let read = read.structured_content.ok_or("Missing saved sketch")?;
        assert_eq!(read["content"], arguments["source"]);
        assert_eq!(read["source_name"], "Sketch.ino");
        assert_eq!(read["compilation"]["operation"], "compile_only");
        let report = workflow
            .artifact(id, ArtifactFormat::CompileReport)?
            .read_text()?;
        let verified: arduino::CompileResult = serde_json::from_str(&report)?;
        assert!(verified.flash_bytes > 0);
        assert_eq!(verified.job_id, id);
        assert_eq!(verified.fqbn, arduino::FQBN);
        assert_eq!(
            read["compilation"]["report"],
            serde_json::to_value(verified)?
        );
        assert_eq!(
            read["compilation"]["report_sha256"],
            format!("{:x}", Sha256::digest(report.as_bytes()))
        );
        assert_eq!(
            read["sha256"],
            format!(
                "{:x}",
                Sha256::digest(
                    read["content"]
                        .as_str()
                        .ok_or("Missing verified source")?
                        .as_bytes()
                )
            )
        );
        for format in [
            ArtifactFormat::Sketch,
            ArtifactFormat::Hex,
            ArtifactFormat::Elf,
            ArtifactFormat::CompileReport,
        ] {
            let path = workflow.artifact(id, format)?.path;
            preserved.push((path.clone(), fs::read(path)?));
        }
        if index == 0 {
            arguments["source"] = json!(read["content"]
                .as_str()
                .ok_or("Missing source text")?
                .replace("HIGH", "LOW"));
        }
        records.push(json!({"proposal":proposal,"readback":read}));
        completed_ids.push(id);
    }
    let invalid = workflow.propose_compile(
        arduino::FQBN.into(),
        "void setup() { missing_symbol; } void loop() {}".into(),
    )?;
    let failed_id = invalid.snapshot.input.job_id();
    let failed = workflow
        .execute(workflow.begin(invalid.approval_id.ok_or("Missing invalid-code review")?)?)?;
    assert_eq!(failed.snapshot.phase, Phase::Failed);
    assert!(!root
        .join(".hazlomexa/artifacts")
        .join(failed_id.to_string())
        .exists());
    let denied = workflow.propose_compile(arduino::FQBN.into(), source.into())?;
    let denied_id = denied.snapshot.input.job_id();
    workflow.cancel(denied_id)?;
    assert!(workflow
        .begin(denied.approval_id.ok_or("Missing denied review")?)
        .is_err());
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
    for (id, phase) in completed_ids
        .iter()
        .copied()
        .map(|id| (id, Phase::Completed))
        .chain([(failed_id, Phase::Failed), (denied_id, Phase::Cancelled)])
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
    let final_read = HostTools::new(Arc::new(restored))
        .call("read_verified_result", json!({"job_id":completed_ids[1]}))?;
    assert_eq!(final_read["sha256"], records[1]["readback"]["sha256"]);
    assert_eq!(
        final_read["compilation"],
        records[1]["readback"]["compilation"]
    );
    for (path, bytes) in preserved {
        assert_eq!(fs::read(path)?, bytes);
    }
    assert_eq!(fs::read(root.join("user-original.txt"))?, original);
    println!(
        "{}",
        json!({"pass":true,"cli_version":arduino::CLI_VERSION,"core_version":arduino::CORE_VERSION,"fqbn":arduino::FQBN,"completed_jobs":2,"failed_jobs":1,"denied_jobs":1,"recovery_verified":true,"prior_outputs_preserved":true,"approval_driver":"shared_backend_test","native_desktop_ui":false,"model_inference":false,"hardware_mutations":false})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
