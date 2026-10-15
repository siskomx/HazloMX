//! Real cloud conversation editing a saved FreeCAD copy, with backend fixture approvals.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        cadpilot::Operation,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::{json, Value};
    use std::{
        fs,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };
    use uuid::Uuid;

    fn dimensions(report: &Value, height: f64) {
        let d = &report["after"]["Target"]["dimensions_mm"];
        for (key, expected) in [("length", 10.), ("width", 20.), ("height", height)] {
            assert!(
                (d[key].as_f64().expect("Native dimension missing") - expected).abs() < 1e-8,
                "Unexpected native dimension"
            );
        }
    }
    fn proposal(workflow: &Workflow, height_mm: f64) -> Result<(), Box<dyn std::error::Error>> {
        let pending = workflow.view()?.job.ok_or("Missing proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::Cadpilot(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.request.operation, Operation::EditBox);
        assert_eq!(input.request.object, "Target");
        for (key, expected) in [("length", 10.), ("width", 20.), ("height", height_mm)] {
            assert!(
                (input.request.parameters[key] * input.request.units.scale() - expected).abs()
                    < 1e-8,
                "Unrequested dimension changed or unit conversion failed"
            );
        }
        Ok(())
    }
    fn report(host: &HostTools, job: Uuid) -> Result<Value, Box<dyn std::error::Error>> {
        let read = host.call("read_verified_result", json!({"job_id":job}))?;
        Ok(serde_json::from_str(
            read["content"].as_str().ok_or("Missing verified report")?,
        )?)
    }
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
        let original = include_bytes!(
            "../../../../docs/research/cadpilot-primitives-2026-10-11/artifacts/Original.FCStd"
        );
        let source = root.join("source/Part.FCStd");
        fs::write(&source, original)?;
        let project = Project::create(&root.join("project"))?;
        let id = project.id;
        let executable = PathBuf::from(
            std::env::var_os("LOCALAPPDATA").ok_or("Missing runtime prerequisite")?,
        )
        .join("HazloMexa/runtimes/FreeCAD-1.1.4/FreeCAD_1.1.4-Windows-x86_64-py311/FreeCADCmd.exe");
        let workflow = Arc::new(Workflow::new(executable, root.join("profiles"))?);
        workflow.select_project(project)?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selected = host.select_cad_document(id,source.clone(),workspace.join("target/cadpilot-probe/source/cadpilot-58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot"),Arc::new(AtomicBool::new(false)))?;
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            hazlomexa::host_tools::Program::Freecad,
            json!({"program":"FreeCAD / CADPilot","cad_document_source":{"selection":selected.id,"source_sha256":selected.inspection.source_sha256},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero que el bloque del documento seleccionado sea más alto, pero todavía no elegí cuánto. Inspecciónalo, explícame sus medidas actuales y pregúntame la altura final y las unidades. No prepares una propuesta hasta que responda.").await?;
        assert!(tools.contains("inspect_selected_cad_document"));
        assert!(
            question.contains('?') && question.contains("30"),
            "Missing measured clarification"
        );
        assert!(
            workflow.view()?.job.is_none(),
            "Clarification created an unrequested job"
        );
        let (_,tools)=chat.say("Quiero que tenga 2 pulgadas de alto en total. Conserva el largo y el ancho actuales. Prepara la propuesta para que pueda revisarla; todavía no está aprobada.").await?;
        assert!(tools.contains("propose_selected_cad_document_change"));
        proposal(&workflow, 50.8)?;
        assert_eq!(fs::read(&source)?, original);
        let first = chat.approve_fixture().await?;
        let first_report = report(&host, first)?;
        dimensions(&first_report, 50.8);
        let mut first_outputs = Vec::new();
        for format in [
            ArtifactFormat::Fcstd,
            ArtifactFormat::Step,
            ArtifactFormat::CadpilotReport,
        ] {
            let path = workflow.artifact(first, format)?.path;
            first_outputs.push((path.clone(), fs::read(path)?));
        }
        let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Lee el resultado publicado y verificado antes de explicarme la nueva altura. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora aumenta la altura de ese mismo bloque 5 milímetros más respecto al resultado que acabamos de editar. Conserva el largo y el ancho. Lee su resultado verificado antes de preparar otra propuesta para mi revisión.").await?;
        assert!(
            tools.contains("read_verified_result") && tools.contains("propose_cad_document_change")
        );
        proposal(&workflow, 55.8)?;
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último resultado verificado y explícame las medidas finales. No prepares más cambios.").await?;
        assert!(tools.contains("read_verified_result"));
        let final_report = report(&host, second)?;
        dimensions(&final_report, 55.8);
        assert_eq!(final_report["before"], first_report["after"]);
        assert_eq!(fs::read(source)?, original);
        for (path, bytes) in first_outputs {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|job| job.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"freecad","existing_ollama_cloud_profile":true,"conversation_turns":5,"completed_native_jobs":2,"successful_tool_results_observed":true,"inspection_and_clarification":true,"imperial_final_height_mm":50.8,"metric_followup_height_mm":55.8,"other_dimensions_preserved":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","transport":"loopback_body_recording_forwarder","native_ui_tested":false,"installed_main_ui_tested":false});
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
