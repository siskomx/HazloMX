//! Real existing cloud-profile conversation; a body-forwarding recorder observes tool results.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
        let root = PathBuf::from(args.next().ok_or("Pass fresh owned root")?);
        assert!(args.next().is_none());
        fs::create_dir(&root)?;
        fs::create_dir(root.join("source"))?;
        fs::create_dir(root.join("project"))?;
        fs::create_dir(root.join("conversation"))?;
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let mut originals = Vec::new();
        for name in [
            "basic_test.kicad_sch",
            "basic_test.kicad_pcb",
            "basic_test.kicad_pro",
            "custom_ds.kicad_wks",
        ] {
            let bytes = fs::read(
                workspace
                    .join("docs/research/konnect-runtime-2026-10-11/raw/Original")
                    .join(name),
            )?;
            let path = root.join("source").join(name);
            fs::write(&path, &bytes)?;
            originals.push((path, bytes));
        }
        let project = Project::create(&root.join("project"))?;
        let id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_konnect(workspace.join("target/konnect-probe/runtime/konnect.exe"))?;
        workflow.set_kicad(workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe"))?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selected = host.select_schematic(
            id,
            root.join("source"),
            root.join("source/basic_test.kicad_sch"),
        )?;
        let resistor = selected
            .components
            .iter()
            .find(|c| c.reference.starts_with('R'))
            .ok_or("Fixture resistor missing")?
            .clone();
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            json!({"program":"KiCad / Konnect","schematic_source":{"selection":selected.id,"sha256":selected.sha256},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero cambiar una resistencia de este esquema, pero todavía no elegí cuál ni su valor. Inspecciona el esquema seleccionado, explícame las resistencias que tiene y pregúntame cuál cambiar. No prepares una propuesta hasta que responda.").await?;
        assert!(
            tools.contains("inspect_selected_schematic"),
            "No successful native source inspection was observed"
        );
        assert!(
            question.contains('?') && question.contains(&resistor.reference),
            "No source-grounded clarification"
        );
        assert!(
            workflow.view()?.job.is_none(),
            "Clarification prepared an unrequested proposal"
        );
        let (_,tools)=chat.say(&format!("La resistencia {} que me acabas de mostrar: cámbiala a 4.7k. Prepara la propuesta para que pueda revisarla; todavía no está aprobada.",resistor.reference)).await?;
        assert!(tools.contains("propose_schematic_value"));
        let pending = workflow.view()?.job.ok_or("Missing first proposal")?;
        let OperationInput::Konnect(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.edit.reference, resistor.reference);
        assert_eq!(input.edit.expected_value, resistor.value);
        assert_eq!(input.edit.value, "4.7k");
        let first = chat.approve_fixture().await?;
        let first_path = workflow
            .artifact(first, ArtifactFormat::KonnectEdited)?
            .path;
        let first_bytes = fs::read(&first_path)?;
        let (text,tools)=chat.say("El cambio ya se aprobó en Hazlo. Lee el resultado publicado y verificado antes de explicarme qué cambió. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        assert!(text.contains("4.7") || text.contains("4,7"));
        let (_,tools)=chat.say("Ahora deja esa misma resistencia en 10k. Parte de la copia que acabamos de editar, no del original. Lee su resultado verificado antes de preparar otra propuesta para mi revisión.").await?;
        assert!(
            tools.contains("read_verified_result") && tools.contains("propose_schematic_value")
        );
        let pending = workflow.view()?.job.ok_or("Missing follow-up proposal")?;
        let OperationInput::Konnect(input) = pending.snapshot.input else {
            return Err("Wrong follow-up integration".into());
        };
        assert_eq!(input.edit.reference, resistor.reference);
        assert_eq!(input.edit.expected_value, "4.7k");
        assert_eq!(input.edit.value, "10k");
        let second = chat.approve_fixture().await?;
        let (text,tools)=chat.say("Lee el último resultado verificado y dime el valor final. No prepares más cambios.").await?;
        assert!(tools.contains("read_verified_result"));
        assert!(text.contains("10"));
        let actual = hazlomexa::konnect::inspect_file(
            &workflow
                .artifact(second, ArtifactFormat::KonnectEdited)?
                .path,
        )?
        .1;
        assert_eq!(
            actual
                .iter()
                .find(|c| c.reference == resistor.reference)
                .ok_or("Final resistor missing")?
                .value,
            "10k"
        );
        assert_eq!(fs::read(first_path)?, first_bytes);
        for (path, bytes) in originals {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        assert_eq!(
            Project::open(&root.join("project"))?
                .recover()?
                .iter()
                .filter(|r| r.phase == Phase::Completed)
                .count(),
            2
        );
        let summary = json!({"pass":true,"existing_ollama_cloud_profile":true,"conversation_turns":5,"completed_native_jobs":2,"successful_tool_results_observed":true,"inspection_and_clarification":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","transport":"loopback_body_recording_forwarder","native_ui_tested":false,"installed_main_ui_tested":false});
        fs::write(
            root.join("summary.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        println!("{summary}");
        Ok(())
    }
}
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    probe::run().await
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows workflow required");
}
