//! Real cloud conversation using CADCraft's native tools on saved drawing copies.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        cadcraft::{Format, Units},
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };
    use uuid::Uuid;

    fn proposal(
        workflow: &Workflow,
        handle: &str,
        distance: f64,
        source_hash: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pending = workflow
            .view()?
            .job
            .ok_or("Missing native drawing proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::Drawing(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.source_sha256, source_hash);
        assert_eq!(input.units, Units::Mm);
        assert_eq!(input.output_format, Format::Dxf);
        assert!(!input.allow_loss);
        let plan = input.native_tools.ok_or("Missing native tool plan")?;
        assert!(plan
            .calls
            .iter()
            .all(|call| matches!(call.command.as_str(), "move" | "select")));
        let moves: Vec<_> = plan
            .calls
            .iter()
            .filter(|call| call.command == "move")
            .collect();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].params.get("handles"), Some(&json!([handle])));
        let delta = moves[0]
            .params
            .get("delta")
            .and_then(Value::as_array)
            .ok_or("Missing native move delta")?;
        assert_eq!(delta.len(), 2);
        assert!((delta[0].as_f64().ok_or("Invalid horizontal delta")? - distance).abs() < 1e-8);
        assert_eq!(delta[1].as_f64(), Some(0.));
        Ok(())
    }
    fn report(host: &HostTools, job: Uuid) -> Result<Value, Box<dyn std::error::Error>> {
        let read = host.call("read_verified_result", json!({"job_id":job}))?;
        Ok(serde_json::from_str(
            read["content"]
                .as_str()
                .ok_or("Missing verified drawing report")?,
        )?)
    }
    fn geometry(report: &Value, original: &[Value], handle: &str, distance: f64) {
        let entities = report["native_readback"]["entities"]
            .as_array()
            .expect("Native readback missing");
        assert_eq!(entities.len(), original.len());
        for before in original {
            let after = entities
                .iter()
                .find(|entity| entity["handle"] == before["handle"])
                .expect("Original entity missing");
            if before["handle"] == handle {
                let mut expected = before.clone();
                expected["geometry"]["center"]["x"] =
                    json!(before["geometry"]["center"]["x"].as_f64().unwrap() + distance);
                assert_eq!(
                    after, &expected,
                    "Unexpected native circle geometry or properties"
                );
            } else {
                assert_eq!(after, before, "An unrelated entity changed");
            }
        }
    }
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
        let root = PathBuf::from(args.next().ok_or("Pass fresh owned root")?);
        assert!(args.next().is_none());
        fs::create_dir(&root)?;
        for directory in ["source", "project", "conversation"] {
            fs::create_dir(root.join(directory))?;
        }
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let bytes =
            fs::read(workspace.join("docs/research/cadcraft-runtime-2026-10-11/metric.dxf"))?;
        let source = root.join("source/Plano.dxf");
        fs::write(&source, &bytes)?;
        let project = Project::create(&root.join("project"))?;
        let id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_cadcraft(workspace.join(
            "target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe",
        ))?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selected = host.select_drawing(id, source.clone(), Arc::new(AtomicBool::new(false)))?;
        let inspected = host.call(
            "inspect_drawing_native_tools",
            json!({"selection":selected.id}),
        )?;
        let original = inspected["context"]["native"]["entities"]
            .as_array()
            .ok_or("Native source entities missing")?;
        let circles: Vec<_> = original
            .iter()
            .filter(|entity| entity["type"] == "Circle")
            .collect();
        assert_eq!(
            circles.len(),
            1,
            "Natural target must be unambiguous in this fixture"
        );
        let handle = circles[0]["handle"]
            .as_str()
            .ok_or("Native circle handle missing")?;
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            json!({"program":"CADCraft","drawing_source":{"selection":selected.id,"sha256":selected.inspection.source_sha256},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero mover el círculo del dibujo seleccionado, pero todavía no elegí la distancia ni la dirección. Inspecciona el dibujo con las herramientas del programa, explícame qué hay y pregúntame lo que falta. No prepares una propuesta todavía.").await?;
        assert!(tools.contains("inspect_drawing_native_tools"));
        assert!(question.contains('?'), "Missing movement clarification");
        assert!(
            workflow.view()?.job.is_none(),
            "Clarification created an unrequested job"
        );
        let (_,tools)=chat.say("Confirmo que el dibujo está en milímetros. Mueve el círculo una pulgada a la derecha, sin cambiar su tamaño ni su posición vertical. Conserva los demás elementos. Prepara la propuesta para mi revisión; todavía no la apruebo.").await?;
        assert!(tools.contains("propose_drawing_native_tools"));
        proposal(&workflow, handle, 25.4, &selected.inspection.source_sha256)?;
        assert_eq!(fs::read(&source)?, bytes);
        let first = chat.approve_fixture().await?;
        let first_report = report(&host, first)?;
        geometry(&first_report, original, handle, 25.4);
        let mut outputs = Vec::new();
        for format in [
            ArtifactFormat::Drawing,
            ArtifactFormat::DrawingPreview,
            ArtifactFormat::DrawingReport,
        ] {
            let path = workflow.artifact(first, format)?.path;
            outputs.push((path.clone(), fs::read(path)?));
        }
        let first_drawing = workflow.artifact(first, ArtifactFormat::Drawing)?.path;
        let first_hash = format!("{:x}", Sha256::digest(fs::read(first_drawing)?));
        let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Lee el resultado publicado y verificado antes de explicarme dónde quedó el círculo. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora mueve ese mismo círculo 5 milímetros más a la derecha respecto a la copia que acabamos de editar. Conserva su tamaño, altura y los demás elementos. Lee su resultado verificado antes de preparar otra propuesta para mi revisión.").await?;
        assert!(
            tools.contains("read_verified_result")
                && tools.contains("propose_drawing_native_tools")
        );
        proposal(&workflow, handle, 5., &first_hash)?;
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último resultado verificado y dime dónde terminó el círculo. No prepares más cambios.").await?;
        assert!(tools.contains("read_verified_result"));
        geometry(&report(&host, second)?, original, handle, 30.4);
        assert_eq!(fs::read(source)?, bytes);
        for (path, bytes) in outputs {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|job| job.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"drawing","existing_ollama_cloud_profile":true,"conversation_turns":5,"completed_native_jobs":2,"successful_tool_results_observed":true,"inspection_and_clarification":true,"imperial_move_mm":25.4,"metric_followup_move_mm":5,"unrelated_entities_preserved":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","transport":"loopback_body_recording_forwarder","native_ui_tested":false,"installed_main_ui_tested":false});
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
