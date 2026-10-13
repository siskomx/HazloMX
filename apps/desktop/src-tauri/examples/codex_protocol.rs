//! Installed Codex app-server/MCP compatibility gate; never starts a model turn.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::HostTools, jobs::Workflow, project::Project, tool_bridge::ToolBridge,
    };
    use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
    use serde_json::{json, Value};
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Stdio,
        sync::Arc,
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
    async fn rpc(
        input: &mut tokio::process::ChildStdin,
        output: &mut BufReader<tokio::process::ChildStdout>,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        Ok(
            hazlomexa::codex::rpc(input, output, id, method, params, &mut Vec::new())
                .await
                .map_err(std::io::Error::other)?,
        )
    }
    let executable = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass the installed codex.exe path")?,
    );
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let base = workspace
        .join("target/codex-probe")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(base.join("home"))?;
    fs::create_dir_all(base.join("project"))?;
    fs::create_dir_all(base.join("sources"))?;
    let workflow = Arc::new(Workflow::new(
        base.join("FreeCADCmd.exe"),
        base.join("profiles"),
    )?);
    let project = Project::create(&base.join("project"))?;
    let project_id = project.id;
    workflow.select_project(project)?;
    workflow.set_agent_source("Blink.ino".into(), "void setup() {} void loop() {}".into())?;
    let source = base.join("sources/Selected.kicad_sch");
    fs::write(&source,br#"(kicad_sch (symbol (lib_id "Device:R") (property "Reference" "R1") (property "Value" "10k")))"#)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let selected = host.select_schematic(project_id, base.join("sources"), source)?;
    let bridge = ToolBridge::start(host, &workspace.join("target/release/hazlomexa-tools.exe"))?;
    let config = bridge.config()?;
    let toml=format!("[analytics]\nenabled = false\n\n[mcp_servers.hazlomexa]\ncommand = {}\nstartup_timeout_sec = 20\n\n[mcp_servers.hazlomexa.env]\n{}\n",serde_json::to_string(&config.command)?,config.env.iter().map(|(k,v)|format!("{k} = {}",serde_json::to_string(v).unwrap())).collect::<Vec<_>>().join("\n"));
    fs::write(base.join("home/config.toml"), toml)?;
    let mut command = tokio::process::Command::new(&executable);
    command
        .args(["app-server", "--stdio"])
        .current_dir(base.join("project"))
        .env_clear()
        .env("CODEX_HOME", base.join("home"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "SystemRoot",
        "WINDIR",
        "PATH",
        "TEMP",
        "TMP",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut command = CommandWrap::from(command);
    let mut flags = CreationFlags(Default::default());
    flags.0 .0 = 0x08000000;
    command.wrap(flags).wrap(KillOnDrop).wrap(JobObject);
    let mut child = command.spawn()?;
    let pid = child.id().ok_or("Missing Codex PID")?;
    let mut input = child.stdin().take().ok_or("Missing Codex input")?;
    let mut output = BufReader::new(child.stdout().take().ok_or("Missing Codex output")?);
    let stderr = child.stderr().take().ok_or("Missing Codex errors")?;
    let errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let _ = stderr.take(65536).read_to_end(&mut bytes).await;
        bytes
    });
    let result=async {
        let initialized=rpc(&mut input,&mut output,1,"initialize",json!({"clientInfo":{"name":"hazlomexa_protocol_gate","version":"0.1.0"},"capabilities":{"experimentalApi":true}})).await?;
        fs::write(base.join("initialize.json"),serde_json::to_vec_pretty(&initialized)?)?;
        input.write_all(b"{\"method\":\"initialized\",\"params\":{}}\n").await?;
        let thread=rpc(&mut input,&mut output,2,"thread/start",json!({"cwd":base.join("project"),"ephemeral":true,"approvalPolicy":"never"})).await?;
        let thread_id=thread["thread"]["id"].as_str().ok_or("Missing thread ID")?;
        let status=rpc(&mut input,&mut output,3,"mcpServerStatus/list",json!({"threadId":thread_id,"serverName":"hazlomexa","detail":"toolsAndAuthOnly"})).await?;
        fs::write(base.join("mcp-status.json"),serde_json::to_vec_pretty(&status)?)?;
        let servers=status["data"].as_array().ok_or("Missing MCP server list")?;
        let expected=hazlomexa::host_tools::catalog();
        if servers.len()!=1 || servers[0]["name"]!="hazlomexa" || servers[0]["runtimeStatus"]!="connected" || servers[0]["tools"].as_object().is_none_or(|tools|tools.len()!=expected.len() || expected.iter().any(|tool|!tools.contains_key(tool.name.as_ref()))){return Err("Codex did not discover the exact Hazlo tool catalog".into());}
        let read=rpc(&mut input,&mut output,4,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"inspect_selected_schematic","arguments":{"selection":selected.id}})).await?;
        fs::write(base.join("tool-read.json"),serde_json::to_vec_pretty(&read)?)?;
        if read["isError"]!=false || read["structuredContent"]["components"][0]["reference"]!="R1" || read["structuredContent"]["components"][0]["value"]!="10k" {return Err("Codex did not return the selected component".into());}
        let refused=rpc(&mut input,&mut output,5,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"inspect_selected_schematic","arguments":{"selection":uuid::Uuid::new_v4()}})).await?;
        if refused["isError"]!=true || refused.get("structuredContent").is_some_and(|value|!value.is_null()){return Err("Codex did not preserve the host tool refusal".into());}
        fs::write(base.join("tool-refusal.json"),serde_json::to_vec_pretty(&refused)?)?;
        let sketch=rpc(&mut input,&mut output,6,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"inspect_selected_sketch","arguments":{}})).await?;
        let inspected=&sketch["structuredContent"];
        if sketch["isError"]!=false || inspected["source"]!="void setup() {} void loop() {}" || inspected["source_name"]!="Blink.ino" {return Err("Codex did not read the selected sketch".into());}
        fs::write(base.join("sketch-read.json"),serde_json::to_vec_pretty(&sketch)?)?;
        let proposal=rpc(&mut input,&mut output,7,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"propose_selected_sketch_edit","arguments":{"project_id":project_id,"source_name":inspected["source_name"],"source_sha256":inspected["source_sha256"],"source":"void setup() {} void loop() { delay(500); }"}})).await?;
        if proposal["isError"]!=false || proposal["structuredContent"]["human_approval_required"]!=true || proposal["structuredContent"].get("approval_id").is_some(){return Err("Codex sketch proposal bypassed review".into());}
        let pending=workflow.view()?.job.ok_or("Missing desktop sketch proposal")?;
        if pending.snapshot.schema_version!=10 || pending.snapshot.phase!=hazlomexa::project::Phase::AwaitingApproval {return Err("Sketch proposal lost MCP provenance/review".into());}
        fs::write(base.join("sketch-proposal.json"),serde_json::to_vec_pretty(&proposal)?)?;
        fs::write(base.join("sketch-run.json"),serde_json::to_vec_pretty(&pending.snapshot)?)?;
        workflow.cancel(pending.snapshot.input.job_id())?;
        workflow.set_agent_source("Blink.ino".into(),"void setup() {} void loop() { delay(100); }".into())?;
        let stale=rpc(&mut input,&mut output,8,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"propose_selected_sketch_edit","arguments":{"project_id":project_id,"source_name":inspected["source_name"],"source_sha256":inspected["source_sha256"],"source":"void setup() {} void loop() { delay(500); }"}})).await?;
        if stale["isError"]!=true {return Err("Codex accepted stale sketch state".into());}
        fs::write(base.join("sketch-refusal.json"),serde_json::to_vec_pretty(&stale)?)?;
        let capabilities=rpc(&mut input,&mut output,9,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"get_design_capabilities","arguments":{}})).await?;
        if capabilities["isError"]!=false || capabilities["structuredContent"]["cadpilot"]["live_editing"]!=false {return Err("Codex did not preserve measured CAD capabilities".into());}
        fs::write(base.join("capabilities.json"),serde_json::to_vec_pretty(&capabilities)?)?;
        let cad=rpc(&mut input,&mut output,10,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"propose_cad_document_change","arguments":{"project_id":project_id,"base_job_id":uuid::Uuid::new_v4(),"result_sha256":"0".repeat(64),"request":{"operation":"edit_box","object":"Target","units":"cm","parameters":{"length":1,"width":2,"height":3.5}}}})).await?;
        if cad["isError"]!=true || cad.get("structuredContent").is_some_and(|value|!value.is_null()){return Err("Codex accepted an unverified CAD edit target".into());}
        fs::write(base.join("cad-refusal.json"),serde_json::to_vec_pretty(&cad)?)?;
        let cancelled=rpc(&mut input,&mut output,11,"mcpServer/tool/call",json!({"threadId":thread_id,"server":"hazlomexa","tool":"read_verified_result","arguments":{"job_id":pending.snapshot.input.job_id()}})).await?;
        if cancelled["isError"]!=true || cancelled.get("structuredContent").is_some_and(|value|!value.is_null()){return Err("Codex reported a cancelled proposal as a verified result".into());}
        fs::write(base.join("result-refusal.json"),serde_json::to_vec_pretty(&cancelled)?)?;
        let view=workflow.view()?;
        if view.history.len()!=1 || view.job.as_ref().is_none_or(|job|job.snapshot.phase!=hazlomexa::project::Phase::Cancelled){return Err("Refused tool calls changed desktop history".into());}
        Ok::<_,Box<dyn std::error::Error>>(())
    }.await;
    drop(input);
    Box::into_pin(child.kill()).await?;
    tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
    let _ = errors.await?;
    let claude_result = if result.is_ok() {
        if let Some(claude) = std::env::args_os().nth(2) {
            let claude = PathBuf::from(claude);
            let claude_base = base.clone();
            let settings = serde_json::to_string(
                &json!({"type":"stdio","command":config.command,"env":config.env}),
            )?;
            let tested = tokio::task::spawn_blocking(move || -> Result<(), String> {
                use std::{collections::BTreeMap, ffi::OsString, sync::atomic::AtomicBool};
                let mut environment = BTreeMap::new();
                for name in [
                    "SystemRoot",
                    "WINDIR",
                    "PATH",
                    "TEMP",
                    "TMP",
                    "LOCALAPPDATA",
                    "APPDATA",
                ] {
                    if let Some(value) = std::env::var_os(name) {
                        environment.insert(OsString::from(name), value);
                    }
                }
                let home = claude_base.join("claude-private");
                fs::create_dir_all(&home).map_err(|e| e.to_string())?;
                for key in ["CLAUDE_CONFIG_DIR", "USERPROFILE", "HOME"] {
                    environment.insert(key.into(), home.clone().into_os_string());
                }
                environment.insert("DISABLE_TELEMETRY".into(), "1".into());
                let add_args = vec![
                    "mcp".into(),
                    "add-json".into(),
                    "--scope".into(),
                    "user".into(),
                    "hazlomexa".into(),
                    settings.into(),
                ];
                let add = hazlomexa::process::run_owned(
                    &claude,
                    &add_args,
                    &claude_base,
                    &environment,
                    Arc::new(AtomicBool::new(false)),
                    Duration::from_secs(30),
                )
                .map_err(|e| e.to_string())?;
                if add.exit_code != Some(0) {
                    return Err("Claude private MCP configuration failed".into());
                }
                let list = hazlomexa::process::run_owned(
                    &claude,
                    &["mcp".into(), "list".into()],
                    &claude_base,
                    &environment,
                    Arc::new(AtomicBool::new(false)),
                    Duration::from_secs(30),
                )
                .map_err(|e| e.to_string())?;
                let output = String::from_utf8_lossy(&list.stdout);
                if list.exit_code != Some(0)
                    || list.truncated
                    || !output.contains("hazlomexa")
                    || !output.contains("Connected")
                {
                    return Err(format!("Claude MCP health check failed: {}", output));
                }
                fs::write(claude_base.join("claude-mcp-list.txt"), list.stdout)
                    .map_err(|e| e.to_string())?;
                Ok(())
            })
            .await?;
            tested
        } else {
            Ok(())
        }
    } else {
        Ok(())
    };
    bridge.close().await?;
    // The temporary capability is revoked; do not retain its config as public evidence.
    fs::remove_file(base.join("home/config.toml"))?;
    result?;
    claude_result?;
    fs::write(
        base.join("status.json"),
        serde_json::to_vec_pretty(
            &json!({"pid":pid,"owned_process_stopped":true,"model_turn_started":false,"codex_mcp_read_passed":true,"exact_catalog_passed":true,"tool_catalog_count":hazlomexa::host_tools::catalog().len(),"selected_sketch_read_and_proposal":true,"stale_sketch_refused":true,"stale_selection_refused":true,"measured_cad_capabilities":true,"unverified_cad_target_refused":true,"cancelled_result_refused":true,"refusals_preserve_history":true,"claude_health_checked":std::env::args_os().nth(2).is_some(),"project_id":project_id}),
        )?,
    )?;
    println!(
        "PASS: installed Codex MCP read without model inference; evidence {}",
        base.display()
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows-only protocol gate")
}
