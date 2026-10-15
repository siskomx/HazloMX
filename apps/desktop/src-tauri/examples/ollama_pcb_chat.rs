//! Real cloud PCB conversation, native saved-copy edits and independent KiCad checks.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        konnect,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};

    fn proposal(
        workflow: &Workflow,
        x: f64,
        y: f64,
        old_x: f64,
        old_y: f64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pending = workflow.view()?.job.ok_or("Missing PCB proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::KonnectPcb(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.pcb_move.reference, "J1");
        assert_eq!((input.pcb_move.x_mm, input.pcb_move.y_mm), (x, y));
        assert_eq!(
            (input.pcb_move.expected_x_mm, input.pcb_move.expected_y_mm),
            (old_x, old_y)
        );
        Ok(())
    }
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
        let root = PathBuf::from(args.next().ok_or("Pass fresh owned root")?);
        assert!(args.next().is_none());
        fs::create_dir(&root)?;
        for folder in ["source", "project", "conversation"] {
            fs::create_dir(root.join(folder))?;
        }
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
        let selected = host.select_pcb(
            id,
            root.join("source"),
            root.join("source/basic_test.kicad_pcb"),
        )?;
        let source = fs::read(root.join("source/basic_test.kicad_pcb"))?;
        let initial = konnect::footprints(&source)?
            .into_iter()
            .find(|c| c.reference == "J1")
            .ok_or("Fixture connector missing")?;
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            hazlomexa::host_tools::Program::Kicad,
            json!({"program":"KiCad / Konnect","pcb_source":{"selection":selected.id,"sha256":selected.sha256},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero mover un conector de esta placa, pero no sé cómo se llama ni dónde está. Inspecciona la PCB seleccionada, explícame sus conectores y pregúntame cuál y a dónde moverlo. No prepares una propuesta todavía.").await?;
        assert!(tools.contains("inspect_selected_pcb"));
        assert!(question.contains('?') && question.contains("J1"));
        assert!(workflow.view()?.job.is_none());
        let (_,tools)=chat.say("El conector J1 que me acabas de mostrar: pon su centro en X 8 pulgadas y Y 6 pulgadas, posición absoluta de la placa. Prepara una copia para revisarla; todavía no está aprobada. Conserva la orientación y avísame sobre las conexiones que deban revisarse.").await?;
        assert!(tools.contains("propose_pcb_move"));
        proposal(&workflow, 203.2, 152.4, initial.x_mm, initial.y_mm)?;
        let first = chat.approve_fixture().await?;
        let first_path = workflow
            .artifact(first, ArtifactFormat::KonnectEdited)?
            .path;
        let first_bytes = fs::read(&first_path)?;
        let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Lee el resultado verificado y la posición real del conector antes de explicarme qué se comprobó. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora mueve ese mismo conector 2 milímetros más hacia la derecha, manteniendo su altura y orientación. Parte de la copia que acabamos de editar. Lee su resultado verificado e inspecciona esa copia antes de preparar otra propuesta para mi revisión.").await?;
        assert!(
            tools.contains("read_verified_result")
                && tools.contains("inspect_selected_pcb")
                && tools.contains("propose_pcb_move")
        );
        proposal(&workflow, 205.2, 152.4, 203.2, 152.4)?;
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último resultado verificado y la posición real final, y explica los resultados y límites de la comprobación. No prepares más cambios ni afirmes haber conectado hardware.").await?;
        assert!(tools.contains("read_verified_result"));
        for (job, x) in [(first, 203.2), (second, 205.2)] {
            let bytes = fs::read(workflow.artifact(job, ArtifactFormat::KonnectEdited)?.path)?;
            let actual = konnect::footprints(&bytes)?
                .into_iter()
                .find(|c| c.uuid == initial.uuid)
                .ok_or("Final connector missing")?;
            assert_eq!(
                (actual.x_mm, actual.y_mm, actual.rotation_deg),
                (x, 152.4, initial.rotation_deg)
            );
            let read = host.call("read_verified_result", json!({"job_id":job}))?;
            assert!(!read["checks"].is_null());
        }
        assert_eq!(fs::read(first_path)?, first_bytes);
        for (path, bytes) in originals {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|r| r.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"pcb","conversation_turns":5,"completed_native_jobs":2,"inspection_and_clarification":true,"metric_imperial_conversion":true,"native_positions_and_rotation_verified":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","native_ui_tested":false,"installed_main_ui_tested":false});
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
