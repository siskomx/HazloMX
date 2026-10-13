//! Actual official SDK/stdio compatibility gate, not the product's effect dispatcher.
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
    let output = base.join(format!("sdk-{}", uuid::Uuid::new_v4()));
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
        let loaded=service.call_tool(CallToolRequestParams::new("load_toolset").with_arguments(serde_json::json!({"name":["sch_components","sch_analysis","sch_export"]}).as_object().unwrap().clone())).await?;
        assert!(!loaded.is_error.unwrap_or(false));
        let expanded=service.list_all_tools().await?;assert_eq!(expanded.len(),64);
        assert!(expanded.iter().any(|tool|tool.name=="edit_schematic_component"));
        let identified:serde_json::Value=serde_json::from_str(include_str!("../../../../docs/research/konnect-runtime-2026-10-11/raw/expanded-tools.json"))?;
        for name in ["get_schematic_component","edit_schematic_component"] {
            let expected=identified["tools"].as_array().unwrap().iter().find(|tool|tool["name"]==name).unwrap();
            let actual=serde_json::to_value(expanded.iter().find(|tool|tool.name==name).unwrap())?;
            assert_eq!(actual["inputSchema"],expected["inputSchema"]);
        }
        let request=hazlomexa::konnect::ValueEdit {reference:"R1".into(),expected_value:"10k".into(),value:"12.34k".into()};
        request.validate()?;
        let schematic=edited.join("basic_test.kicad_sch");
        let before=component(&service,"get_schematic_component",serde_json::json!({"schematic":schematic,"reference":request.reference})).await?;
        assert_eq!(before["value"],request.expected_value);
        let effect=component(&service,"edit_schematic_component",serde_json::json!({"schematic":schematic,"reference":request.reference,"value":request.value})).await?;
        let after=component(&service,"get_schematic_component",serde_json::json!({"schematic":schematic,"reference":request.reference})).await?;
        let mut expected=before.clone();expected["value"]=request.value.clone().into();expected["properties"]["Value"]=request.value.clone().into();
        assert_eq!(after,expected);
        hazlomexa::konnect::verify_value_edit(&fs::read(original.join("basic_test.kicad_sch"))?,&fs::read(&schematic)?,&request)?;
        for name in names {
            assert_eq!(fs::read(original.join(name))?,fs::read(workspace.join("target/kicad-probe/fixtures").join(name))?);
            if name!="basic_test.kicad_sch" {assert_eq!(fs::read(original.join(name))?,fs::read(edited.join(name))?);}
        }
        fs::write(output.join("schematic-edit.json"),serde_json::to_vec_pretty(&serde_json::json!({"request":request,"before":before,"edit":effect,"after":after}))?)?;
        let unloaded=service.call_tool(CallToolRequestParams::new("unload_toolset").with_arguments(serde_json::json!({"name":"sch_components"}).as_object().unwrap().clone())).await?;
        assert!(!unloaded.is_error.unwrap_or(false));
        let reduced=service.list_all_tools().await?;assert_eq!(reduced.len(),46);
        assert!(!reduced.iter().any(|tool|tool.name=="edit_schematic_component"));
        assert!(notifications.load(Ordering::SeqCst)>=2);
        Ok::<_,Box<dyn std::error::Error>>(serde_json::json!({"peer":peer,"initial":initial,"expanded":expanded,"reduced":reduced}))
    }).await;
    service.cancel().await?;
    let ceased = unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) } == WAIT_OBJECT_0;
    let record = observed??;
    assert!(ceased);
    fs::write(
        output.join("catalogs.json"),
        serde_json::to_vec_pretty(&record)?,
    )?;
    let status = serde_json::json!({"passed":true,"rmcp_version":"3.5.1","protocol_version":"2025-06-18","child_pid":pid,"child_cessation_observed":ceased,"list_change_notifications":notifications.load(Ordering::SeqCst),"mutating_document_tools_called":true,"exact_tool_schemas_verified":true,"independent_schematic_value_verifier_passed":true,"originals_and_companions_preserved":true,"job_object_wrapper_used":true,"child_in_job_observed":true,"application_workflow_verified":false});
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
    assert!(matches!(
        name,
        "get_schematic_component" | "edit_schematic_component"
    ));
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
