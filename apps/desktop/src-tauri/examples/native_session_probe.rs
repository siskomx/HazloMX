//! Owned native acceptance harness. Approvals here are test-harness approvals, not UI evidence.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::{HostTools, McpTools, Program},
        jobs::Workflow,
        native_tools::McpConfig,
        project::Project,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::{json, Value};
    use std::{
        io::Write,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };
    use tokio::io::{AsyncBufReadExt, BufReader};
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let root = PathBuf::from(args.first().ok_or("Pass owned project directory")?);
    let owned = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("Workspace unavailable")?
        .join("target")
        .canonicalize()?;
    let parent = root
        .parent()
        .ok_or("Pass owned project parent")?
        .canonicalize()?;
    if !root.is_absolute()
        || !parent.starts_with(&owned)
        || root
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Owned workspace target directory required".into());
    }
    let program: Program = serde_json::from_value(json!(args.get(1).ok_or("Pass program")?))?;
    std::fs::create_dir_all(&root)?;
    let workflow = Arc::new(Workflow::new(
        root.join("unused.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(Project::create(&root)?)?;
    let project = workflow.view()?.project.unwrap();
    let host = Arc::new(HostTools::new(workflow.clone()));
    let config = PathBuf::from(
        args.get(2)
            .ok_or("Pass selected native config or CLI executable")?,
    );
    let catalog = if args.get(3).is_some_and(|s| s == "cli") {
        host.connect_native_cli(program, config, Arc::new(AtomicBool::new(false)))?
    } else {
        host.connect_native_mcp(
            program,
            McpConfig::from_file(&config)?,
            Arc::new(AtomicBool::new(false)),
        )?
    };
    let (server_io, client_io) = tokio::io::duplex(1024 * 1024);
    let handler = McpTools::for_program(host.clone(), project, program)?;
    let server = tokio::spawn(async move { handler.serve(server_io).await });
    let client = ().serve(client_io).await?;
    let server = server.await??;
    println!("{}", json!({"ready":true,"catalog":catalog}));
    std::io::stdout().flush()?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let request: Value = serde_json::from_str(&line)?;
        if request["quit"] == true {
            break;
        }
        let result: Result<Value, String> = if request["approve"] == true {
            match workflow.view()?.job.and_then(|j| j.approval_id) {
                Some(approval) => match workflow.begin(approval) {
                    Ok(dispatch) => workflow
                        .execute(dispatch)
                        .and_then(|job| serde_json::to_value(job).map_err(|e| e.to_string())),
                    Err(e) => Err(e),
                },
                None => Err("No pending approval".into()),
            }
        } else {
            let name = request["tool"].as_str().ok_or("Pass tool name")?;
            let args = request["arguments"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            client
                .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(args))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| serde_json::to_value(r).map_err(|e| e.to_string()))
        };
        println!(
            "{}",
            match result {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":error}),
            }
        );
        std::io::stdout().flush()?;
    }
    client.cancel().await?;
    server.cancel().await?;
    host.disconnect_native_tools()?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
