//! Installed connector check; no model, native engineering effect or hardware access.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::{catalog, HostTools},
        jobs::Workflow,
        project::Project,
        tool_bridge::ToolBridge,
    };
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    use serde_json::json;
    use std::{path::PathBuf, sync::Arc};
    let mut args = std::env::args_os().skip(1);
    let helper = PathBuf::from(args.next().ok_or("Pass installed hazlomexa-tools.exe")?);
    let root = PathBuf::from(args.next().ok_or("Pass owned test project folder")?);
    std::fs::create_dir_all(&root)?;
    let project = if root.join("hazlomx.json").exists() {
        Project::open(&root)?
    } else {
        Project::create(&root)?
    };
    let id = project.id;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(project)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let bridge = ToolBridge::start_project(host, &helper, id)?;
    let config = bridge.config()?;
    let mut command = tokio::process::Command::new(&config.command);
    command
        .env_clear()
        .envs(hazlomexa::claude::environment())
        .envs(&config.env)
        .kill_on_drop(true);
    let service = ().serve(rmcp::transport::TokioChildProcess::new(command)?).await?;
    let actual = service.list_all_tools().await?;
    let names: Vec<_> = actual.iter().map(|t| t.name.to_string()).collect();
    let expected: Vec<_> = catalog().iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, expected);
    assert!(names.iter().any(|name| name == "propose_drawing_change"));
    assert!(names.iter().any(|name| name == "propose_pcb_move"));
    assert!(names.iter().any(|name| name == "propose_slice_settings"));
    assert!(names.iter().any(|name| name == "propose_kicad_check"));
    let status = service
        .call_tool(
            CallToolRequestParams::new("get_workflow_status").with_arguments(Default::default()),
        )
        .await?;
    assert_eq!(
        status.structured_content.as_ref().unwrap()["project_id"],
        id.to_string()
    );
    let capabilities = service
        .call_tool(
            CallToolRequestParams::new("get_design_capabilities")
                .with_arguments(Default::default()),
        )
        .await?;
    assert!(capabilities
        .structured_content
        .as_ref()
        .unwrap()
        .get("cadcraft")
        .is_some());
    let refused = service
        .call_tool(
            CallToolRequestParams::new("inspect_selected_pcb").with_arguments(
                json!({"selection":uuid::Uuid::new_v4()})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;
    assert_eq!(refused.is_error, Some(true));
    let approval = service
        .call_tool(
            CallToolRequestParams::new("approve_operation").with_arguments(Default::default()),
        )
        .await?;
    assert_eq!(approval.is_error, Some(true));
    assert!(!names.iter().any(|name| name == "approve_operation"));
    assert!(workflow.view()?.job.is_none());
    assert!(workflow.view()?.history.is_empty());
    service.cancel().await?;
    bridge.close().await?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"pass":true,"project_id":id,"tools":names,"tool_count":actual.len(),"no_engineering_jobs":true,"unknown_selection_refused":true,"no_approval_tool":true,"model_inference":false})
        )?
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows connector check required");
}
