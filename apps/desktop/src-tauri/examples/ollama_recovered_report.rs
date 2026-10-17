//! Real existing cloud-profile read-only conversation on an owned copied project.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        host_tools::{HostTools, McpTools, Program},
        jobs::{ModelProfile, Workflow},
        ollama::{self, OllamaClient},
        project::Project,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };

    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().ok_or("Pass owned root")?).canonicalize()?;
    let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
    assert!(args.next().is_none());
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .canonicalize()?;
    assert!(root.starts_with(workspace.join("target")));
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(Project::open(&root.join("project"))?)?;
    let before = serde_json::to_value(workflow.view()?.history)?;
    let project = workflow.view()?.project.ok_or("Missing project")?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let (model, digest) = ModelProfile::Cloud.identity();
    for turn in 1..=2 {
        fs::write(root.join("conversation/current-turn"), turn.to_string())?;
        let prompt = fs::read_to_string(root.join(format!("prompt-{turn}.json")))?;
        let tools = McpTools::for_program(host.clone(), project.clone(), Program::Agent)?;
        let result = ollama::chat::run_scoped(
            OllamaClient::cloud(port, model, digest)?,
            workflow.clone(),
            tools,
            prompt,
            None,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await;
        fs::write(
            root.join(format!("response-{turn}.json")),
            serde_json::to_vec_pretty(&result)?,
        )?;
        result?;
        assert_eq!(serde_json::to_value(workflow.view()?.history)?, before);
        assert!(workflow.view()?.job.is_none());
    }
    println!("{{\"pass\":true,\"real_cloud_turns\":2,\"new_jobs_created\":0}}");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows owned-project test required");
}
