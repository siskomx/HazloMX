//! Real cloud code/review/compile/readback conversation; no hardware or native UI.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        arduino,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf, sync::Arc};
    use uuid::Uuid;

    fn proposal(
        workflow: &Workflow,
        milliseconds: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let pending = workflow.view()?.job.ok_or("Missing compile proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::Arduino(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.fqbn, arduino::FQBN);
        // This acceptance fixture checks a simple literal-delay sketch, not arbitrary C++ behavior or hardware.
        let compact: String = input
            .source
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            compact.contains("LED_BUILTIN")
                && compact.contains("OUTPUT")
                && compact.contains("HIGH")
                && compact.contains("LOW")
        );
        let delays: Vec<_> = compact
            .split("delay(")
            .skip(1)
            .map(|tail| tail.split(')').next().unwrap())
            .collect();
        assert_eq!(delays.len(), 2, "Expected a simple two-delay example");
        assert!(
            delays
                .iter()
                .all(|delay| delay.trim_end_matches(['u', 'U', 'l', 'L'])
                    == milliseconds.to_string()),
            "Requested delay literal missing"
        );
        Ok(input.source)
    }
    fn readback(
        host: &HostTools,
        workflow: &Workflow,
        job: Uuid,
        source: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let read = host.call("read_verified_result", json!({"job_id":job}))?;
        assert_eq!(read["content"], source);
        assert_eq!(
            read["sha256"],
            format!("{:x}", Sha256::digest(source.as_bytes()))
        );
        assert_eq!(read["compilation"]["operation"], "compile_only");
        let text = workflow
            .artifact(job, ArtifactFormat::CompileReport)?
            .read_text()?;
        let report: arduino::CompileResult = serde_json::from_str(&text)?;
        assert_eq!(report.job_id, job);
        assert_eq!(report.cli_version, arduino::CLI_VERSION);
        assert_eq!(report.core_version, arduino::CORE_VERSION);
        assert_eq!(report.fqbn, arduino::FQBN);
        assert!(report.flash_bytes > 0);
        assert_eq!(
            read["compilation"]["report"],
            serde_json::to_value(&report)?
        );
        assert_eq!(
            read["compilation"]["report_sha256"],
            format!("{:x}", Sha256::digest(text.as_bytes()))
        );
        Ok(report.flash_bytes)
    }
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
        let root = PathBuf::from(args.next().ok_or("Pass fresh owned root")?);
        assert!(args.next().is_none());
        fs::create_dir(&root)?;
        fs::create_dir(root.join("project"))?;
        fs::create_dir(root.join("conversation"))?;
        let sentinel = b"void setup() {}\nvoid loop() {}\n";
        let original = root.join("project/user-original.ino");
        fs::write(&original, sentinel)?;
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let project = Project::create(&root.join("project"))?;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_arduino(
            workspace.join("target/arduino-probe/runtime/arduino-cli.exe"),
            workspace.join("target/arduino-probe/data"),
        )?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            json!({"program":"Arduino CLI","document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero un ejemplo sencillo para hacer parpadear el LED integrado de una placa. Todavía no elegí la placa ni cuánto tiempo debe estar encendido y apagado. Consulta las capacidades disponibles y pregúntame lo que falta. No prepares una propuesta todavía.").await?;
        assert!(tools.contains("get_design_capabilities"));
        assert!(question.contains('?'), "Missing board/timing clarification");
        assert!(
            workflow.view()?.job.is_none(),
            "Clarification created an unrequested proposal"
        );
        let (_,tools)=chat.say("Usaré Arduino Uno y su LED integrado. Quiero un ejemplo sencillo: un segundo encendido y un segundo apagado, repetidos. Prepara el código para revisarlo y compilarlo; no lo cargues a ninguna placa. Todavía no está aprobado.").await?;
        assert!(tools.contains("propose_arduino_compile"));
        let first_source = proposal(&workflow, 1000)?;
        let first = chat.approve_fixture().await?;
        let first_flash = readback(&host, &workflow, first, &first_source)?;
        let mut preserved = Vec::new();
        for format in [
            ArtifactFormat::Sketch,
            ArtifactFormat::Hex,
            ArtifactFormat::Elf,
            ArtifactFormat::CompileReport,
        ] {
            let path = workflow.artifact(first, format)?.path;
            preserved.push((path.clone(), fs::read(path)?));
        }
        let (_,tools)=chat.say("Ya aprobé la compilación en Hazlo. Lee el resultado verificado y su informe antes de explicarme qué se comprobó y qué no. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora hazlo dos veces más rápido: medio segundo encendido y medio segundo apagado. Parte del código que acabamos de compilar, conservando Arduino Uno y el LED integrado. Lee su resultado verificado antes de preparar otra propuesta para mi revisión. Sigue siendo solo compilación, sin cargarlo a la placa.").await?;
        assert!(
            tools.contains("read_verified_result") && tools.contains("propose_arduino_compile")
        );
        let second_source = proposal(&workflow, 500)?;
        assert_ne!(second_source, first_source);
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último código y el informe verificados, y dime el ritmo final y el resultado de la compilación. No prepares más cambios ni afirmes haber probado una placa física.").await?;
        assert!(tools.contains("read_verified_result"));
        let second_flash = readback(&host, &workflow, second, &second_source)?;
        assert_eq!(fs::read(original)?, sentinel);
        for (path, bytes) in preserved {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|job| job.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"arduino","existing_ollama_cloud_profile":true,"conversation_turns":5,"completed_native_jobs":2,"successful_tool_results_observed":true,"inspection_and_clarification":true,"requested_delay_ms":[1000,500],"flash_bytes":[first_flash,second_flash],"code_and_compiler_report_verified":true,"hardware_tested":false,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","transport":"loopback_body_recording_forwarder","native_ui_tested":false,"installed_main_ui_tested":false});
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
