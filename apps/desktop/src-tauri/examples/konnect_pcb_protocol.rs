//! Actual closed-board move, whole-board readback and KiCad checks; not the app dispatcher.
#[cfg(windows)]
use rmcp::{
    model::CallToolRequestParams, service::NotificationContext, transport::TokioChildProcess,
    ClientHandler, RoleClient, ServiceExt,
};

#[cfg(windows)]
struct Observe(std::sync::Arc<std::sync::atomic::AtomicUsize>);
#[cfg(windows)]
impl ClientHandler for Observe {
    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::{
        fs,
        path::Path,
        process::Stdio,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    };
    use windows_sys::Win32::{
        Foundation::WAIT_OBJECT_0,
        System::{
            JobObjects::IsProcessInJob,
            Threading::{
                OpenProcess, WaitForSingleObject, PROCESS_QUERY_INFORMATION, PROCESS_SYNCHRONIZE,
            },
        },
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let base = workspace.join("target/konnect-probe");
    let output = base.join(format!("pcb-sdk-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&output)?;
    let executable = base.join("runtime/konnect.exe");
    let identity: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("identity.json"))?)?;
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&executable)?)),
        identity["runtime_files"]["konnect.exe"]["sha256"]
            .as_str()
            .unwrap()
    );
    let profile = output.join("profile");
    fs::create_dir(&profile)?;
    let cli = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
    let edited = output.join("Edited");
    let original = output.join("Original");
    fs::create_dir(&edited)?;
    fs::create_dir(&original)?;
    let names = [
        "basic_test.kicad_sch",
        "basic_test.kicad_pcb",
        "basic_test.kicad_pro",
        "custom_ds.kicad_wks",
    ];
    for name in names {
        let bytes = fs::read(workspace.join("target/kicad-probe/fixtures").join(name))?;
        fs::write(original.join(name), &bytes)?;
        fs::write(edited.join(name), &bytes)?;
    }
    let configuration = serde_json::json!({"transport":"stdio","project_dir":edited,"ipc_address":format!("ipc://{}",profile.join("no-kicad-socket").display()),"kicad_cli":cli,"kicad_binary":cli.parent().unwrap().join("kicad.exe"),"log_level":"warn","auto_load_toolsets":false,"eager_toolsets":false});
    let config = output.join("config.json");
    fs::write(&config, serde_json::to_vec_pretty(&configuration)?)?;
    let mut command = tokio::process::Command::new(&executable);
    command
        .arg("--config")
        .arg(&config)
        .current_dir(&output)
        .env_clear()
        .creation_flags(0x08000000)
        .kill_on_drop(true);
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env(
        "PATH",
        format!("{}\\System32", std::env::var("SystemRoot")?),
    );
    for key in [
        "APPDATA",
        "LOCALAPPDATA",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "KONNECT_STATE_DIR",
    ] {
        command.env(key, &profile);
    }
    let mut wrapped = CommandWrap::from(command);
    let mut flags = CreationFlags(Default::default());
    flags.0 .0 = 0x08000000;
    wrapped.wrap(flags).wrap(KillOnDrop).wrap(JobObject);
    let (transport, _) = TokioChildProcess::builder(wrapped)
        .stderr(Stdio::from(fs::File::create(output.join("stderr.log"))?))
        .spawn()?;
    let pid = transport.id().ok_or("Missing child PID")?;
    let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: OpenProcess returned a new owned process handle.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut in_job = 0;
    if unsafe { IsProcessInJob(process.as_raw_handle(), std::ptr::null_mut(), &mut in_job) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    assert_ne!(in_job, 0);
    let notifications = Arc::new(AtomicUsize::new(0));
    let service = tokio::time::timeout(
        Duration::from_secs(20),
        Observe(notifications.clone()).serve(transport),
    )
    .await??;
    let observed=tokio::time::timeout(Duration::from_secs(20),async {
        let peer=service.peer_info().ok_or("Missing server identity")?;
        assert!(peer.server_info.as_ref().is_some_and(|i|i.name=="konnect" && i.version=="0.13.0"));
        assert_eq!(peer.protocol_version.as_str(),"2025-06-18");
        let initial=service.list_all_tools().await?;
        assert_eq!(initial.len(),21);
        let loaded=service.call_tool(CallToolRequestParams::new("load_toolset").with_arguments(serde_json::json!({"name":["pcb_board","pcb_components"]}).as_object().unwrap().clone())).await?;
        assert!(!loaded.is_error.unwrap_or(false));
        let expanded=service.list_all_tools().await?;
        let schema=serde_json::json!({"type":"object","additionalProperties":false,"properties":{"board":{"type":"string"},"reference":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"}},"required":["board","reference","x","y"]});
        assert_eq!(serde_json::to_value(expanded.iter().find(|tool|tool.name=="move_component").ok_or("Missing move tool")?)?["inputSchema"],schema);
        let read_schema=serde_json::json!({"type":"object","additionalProperties":false,"properties":{"board":{"type":"string","description":"Path to .kicad_pcb file"}},"required":["board"]});
        assert_eq!(serde_json::to_value(expanded.iter().find(|tool|tool.name=="get_board_info").ok_or("Missing board read tool")?)?["inputSchema"],read_schema);
        let board=edited.join("basic_test.kicad_pcb");
        let original_board=fs::read(original.join("basic_test.kicad_pcb"))?;
        let inventory=hazlomexa::konnect::footprints(&original_board)?;
        let selected=inventory.iter().find(|fp|fp.reference=="J1").ok_or("Missing J1")?;
        let request=hazlomexa::konnect::FootprintMove {reference:selected.reference.clone(),uuid:selected.uuid,expected_x_mm:selected.x_mm,expected_y_mm:selected.y_mm,x_mm:selected.x_mm+2.54,y_mm:selected.y_mm-1.27};
        request.validate()?;
        let before=component(&service,"get_board_info",serde_json::json!({"board":board})).await?;
        assert_eq!(before["source"],"file");
        let effect=component(&service,"move_component",serde_json::json!({"board":board,"reference":request.reference,"x":request.x_mm,"y":request.y_mm})).await?;
        assert_eq!(effect["source"],"file");
        hazlomexa::konnect::verify_footprint_move(&original_board,&fs::read(&board)?,&request)?;
        let after=component(&service,"get_board_info",serde_json::json!({"board":board})).await?;
        assert_eq!(before,after);
        let reread=hazlomexa::konnect::footprints(&fs::read(&board)?)?;
        fs::write(output.join("pcb-edit.json"),serde_json::to_vec_pretty(&serde_json::json!({"request":request,"before":before,"effect":effect,"after":after,"inventory_before":inventory,"inventory_after":reread}))?)?;
        for name in names {
            assert_eq!(fs::read(original.join(name))?,fs::read(workspace.join("target/kicad-probe/fixtures").join(name))?);
            if name!="basic_test.kicad_pcb" {assert_eq!(fs::read(original.join(name))?,fs::read(edited.join(name))?);}
        }
        let unchanged=fs::read(&board)?;
        let refused=service.call_tool(CallToolRequestParams::new("move_component").with_arguments(serde_json::json!({"board":board,"reference":"MISSING","x":1.0,"y":2.0}).as_object().unwrap().clone())).await?;
        assert!(refused.is_error.unwrap_or(false));assert_eq!(unchanged,fs::read(&board)?);
        fs::write(output.join("refused.json"),serde_json::to_vec_pretty(&refused)?)?;
        Ok::<_,Box<dyn std::error::Error>>(serde_json::json!({"peer":peer,"initial":initial,"expanded":expanded}))
    }).await;
    service.cancel().await?;
    let ceased = unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) } == WAIT_OBJECT_0;
    let record = observed??;
    assert!(ceased);
    let mut checks = Vec::new();
    for (root, label) in [(&original, "Before"), (&edited, "After")] {
        let check_import = output.join(format!("{label}Import"));
        let stage = output.join(format!("{label}KiCadCheck"));
        fs::create_dir(&stage)?;
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let input = hazlomexa::kicad_project::capture(
            root,
            &root.join("basic_test.kicad_pcb"),
            &check_import,
            uuid::Uuid::new_v4(),
            hazlomexa::kicad::Units::Mm,
            cancelled.clone(),
        )?;
        let native = hazlomexa::kicad_run::run(
            &cli,
            &check_import,
            &stage,
            &output.join(format!("{label}-check-profile")),
            &input,
            cancelled,
        )?;
        hazlomexa::kicad_run::validate_artifacts(&stage, &input, &native)?;
        assert_eq!(
            hazlomexa::konnect::footprints(&fs::read(root.join("basic_test.kicad_pcb"))?)?,
            hazlomexa::konnect::footprints(&fs::read(stage.join("Checked").join(&input.entry))?)?
        );
        fs::write(
            output.join(format!("native-{label}.json")),
            serde_json::to_vec_pretty(&native)?,
        )?;
        checks.push(native.summary);
    }
    assert_eq!(checks[0].errors, 0);
    assert_eq!(checks[1].errors, 1);
    assert_eq!(checks[0].warnings, 6);
    assert_eq!(checks[1].warnings, 7);
    fs::write(
        output.join("catalogs.json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    let status = serde_json::json!({"passed":true,"rmcp_version":"3.5.1","protocol_version":"2025-06-18","child_pid":pid,"child_cessation_observed":ceased,"list_change_notifications":notifications.load(Ordering::SeqCst),"mutating_document_tools_called":true,"exact_tool_schemas_verified":true,"independent_whole_board_move_verifier_passed":true,"closed_board_copy_only":true,"native_ipc_verified":false,"native_kicad_checks_and_exports_verified":true,"before_errors":checks[0].errors,"after_errors":checks[1].errors,"originals_and_companions_preserved":true,"job_object_wrapper_used":true,"child_in_job_observed":true,"application_workflow_verified":false});
    fs::write(
        output.join("status.json"),
        serde_json::to_vec_pretty(&status)?,
    )?;
    println!("{}", serde_json::json!({"passed":true,"evidence":output}));
    Ok(())
}
#[cfg(windows)]
async fn component(
    service: &rmcp::service::RunningService<RoleClient, Observe>,
    name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    assert!(matches!(name, "get_board_info" | "move_component"));
    let reply = service
        .call_tool(
            CallToolRequestParams::new(name.to_owned())
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await?;
    assert!(!reply.is_error.unwrap_or(false) && reply.content.len() == 1);
    let text = &reply.content[0]
        .as_text()
        .ok_or("Expected one text result")?
        .text;
    assert!(text.len() <= 65536);
    Ok(serde_json::from_str(text)?)
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows-only compatibility gate")
}
