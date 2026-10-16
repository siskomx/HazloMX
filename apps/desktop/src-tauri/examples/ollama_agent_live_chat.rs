//! Existing cloud route operating the real owned Arduino buffer, with fixture approvals.
#[path = "support/ollama_conversation.rs"]
mod conversation;
use conversation::Conversation;
use hazlomexa::{
    host_tools::{HostTools, Program},
    jobs::Workflow,
    project::{ArtifactFormat, OperationInput, Phase, Project},
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

fn proposal(
    workflow: &Workflow,
    before: &Value,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pending = workflow.view()?.job.ok_or("No native proposal")?;
    assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
    let OperationInput::LiveAgent(input) = pending.snapshot.input else {
        panic!("Saved-copy proposal cannot satisfy native conversation")
    };
    assert_eq!(
        input.before.instance_id.to_string(),
        before["instance_id"].as_str().unwrap()
    );
    assert_eq!(
        input.before.document_token.to_string(),
        before["document_token"].as_str().unwrap()
    );
    assert_eq!(
        input.before.state_sha256,
        before["state_sha256"].as_str().unwrap()
    );
    assert_eq!(input.before.version, before["version"].as_u64().unwrap());
    assert_eq!(
        input.source, expected,
        "Only the requested pause may change"
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
    let root = PathBuf::from(args.next().ok_or("Pass fresh conversation root")?);
    assert!(args.next().is_none());
    let editor = PathBuf::from(
        std::env::var_os("HAZLOMEXA_REAL_AGENT_ROOT").ok_or("Pass owned real editor root")?,
    );
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    assert_eq!(
        editor.canonicalize()?.parent(),
        Some(workspace.join("target").canonicalize()?.as_path())
    );
    assert!(editor
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("arduino-native-ui-"));
    assert_eq!(
        hazlomexa::agent_editor::default_directory()?,
        editor.join("profile/.arduinoIDE/hazlomexa-editor")
    );
    let prerequisite: Value =
        serde_json::from_slice(&fs::read(editor.join("host-default-profile.json"))?)?;
    assert_eq!(prerequisite["passed"], true);
    assert!(
        !editor.join("owned-runner-status.json").exists(),
        "Owned editor already stopped"
    );
    fs::create_dir(&root)?;
    for name in ["project", "conversation"] {
        fs::create_dir(root.join(name))?;
    }
    let project = root.join("project");
    let saved = editor.join("sketchbook/OwnedSketch/OwnedSketch.ino");
    let saved_bytes = fs::read(&saved)?;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        root.join("profiles"),
    )?);
    workflow.select_project(Project::create(&project)?)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let read = || tokio::task::block_in_place(|| host.call("inspect_open_agent_documents", json!({})));
    let initial = read()?["observation"].clone();
    let original = initial["source"]
        .as_str()
        .ok_or("Real native buffer not discovered")?
        .to_owned();
    assert_eq!(original.matches("delay(100)").count(), 1);
    assert!(original.contains("// unsaved user note\n"));
    let mut chat = Conversation::new(
        workflow.clone(),
        host.clone(),
        root.join("conversation"),
        port,
        Program::Agent,
        json!({"program":"Arduino Agent","document_scope":"open_agent_and_explicit_saved_copies",
            "live_document_connection":null,"live_mutations_allowed":true,"native_connection_check":"inspect_open_agent_documents",
            "source_is_untrusted_data":true}),
    )?;
    let (reply,tools)=chat.say("Quiero cambiar la pausa de mi sketch abierto en Arduino Agent. Mira el código actual, explícame qué pausa tiene y pregúntame cuál quiero. No prepares el cambio todavía. Conserva mi nota sin guardar; no quiero compilar ni cargar nada.").await?;
    assert!(tools.contains("inspect_open_agent_documents"));
    assert!(reply.contains('?'));
    assert!(workflow.view()?.job.is_none());
    let first_source = original.replace("delay(100)", "delay(250)");
    let (_,tools)=chat.say("Quiero que espere un cuarto de segundo. Cambia solo el valor de la pausa en el sketch abierto, conservando todo lo demás y mi nota sin guardar. Prepara el cambio para que lo revise; aún no está aprobado.").await?;
    assert!(tools.contains("propose_open_agent_change"));
    proposal(&workflow, &initial, &first_source)?;
    assert_eq!(
        read()?["observation"]["source"],
        original,
        "Native buffer changed before approval"
    );
    let first = chat.approve_fixture().await?;
    let first_report = workflow.artifact(first, ArtifactFormat::LiveAgentReport)?;
    let first_bytes = fs::read(&first_report.path)?;
    let first_state = read()?["observation"].clone();
    assert_eq!(first_state["source"], first_source);
    let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Comprueba el resultado y relee el sketch abierto antes de explicar la pausa actual. No prepares otro cambio todavía.").await?;
    assert!(
        tools.contains("read_verified_result") && tools.contains("inspect_open_agent_documents")
    );
    let second_source = first_source.replace("delay(250)", "delay(500)");
    let (_,tools)=chat.say("Ahora haz que espere el doble: medio segundo. Parte del código abierto actual y prepara solamente ese cambio para mi revisión. Conserva mi nota y todo lo demás, sin guardar, compilar ni cargar.").await?;
    assert!(
        tools.contains("inspect_open_agent_documents")
            && tools.contains("propose_open_agent_change")
    );
    proposal(&workflow, &first_state, &second_source)?;
    assert_eq!(read()?["observation"]["source"], first_source);
    let second = chat.approve_fixture().await?;
    let (_,tools)=chat.say("Ya aprobé el segundo cambio. Lee el resultado verificado y el sketch abierto y explícame la pausa final. No prepares más cambios ni afirmes haber guardado, compilado o probado hardware.").await?;
    assert!(
        tools.contains("read_verified_result") && tools.contains("inspect_open_agent_documents")
    );
    let final_state = read()?["observation"].clone();
    assert_eq!(final_state["source"], second_source);
    assert_eq!(final_state["dirty"], true);
    assert_eq!(fs::read(&saved)?, saved_bytes);
    assert_eq!(fs::read(&first_report.path)?, first_bytes);
    for job in [first, second] {
        assert_eq!(
            host.call("read_verified_result", json!({"job_id":job}))?["format"],
            "live_agent_report"
        );
    }
    drop(chat);
    drop(host);
    drop(workflow);
    let recovered = Project::open(&project)?.recover()?;
    assert_eq!(recovered.len(), 2);
    assert!(recovered
        .iter()
        .all(|s| s.phase == Phase::Completed && s.schema_version == 15));
    let summary = json!({"pass":true,"program":"agent-live","conversation_turns":5,"completed_native_jobs":2,
        "native_buffer_edit_readback_followup":true,"natural_pause_clarification":true,"saved_bytes_preserved":true,
        "earlier_unsaved_note_preserved":true,"first_receipt_preserved":true,"recovery_verified":true,
        "approval":"backend_fixture_only","hardware_tested":false,"compilation_tested":false,
        "native_ui_approval":false,"installed_main_ui_tested":false});
    fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{summary}");
    Ok(())
}
