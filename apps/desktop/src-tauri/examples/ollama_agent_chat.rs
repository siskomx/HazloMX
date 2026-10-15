//! Authenticated Arduino Agent conversation using owned sketch copies, never hardware.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        agent,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc};

    fn proposal(
        workflow: &Workflow,
        original: &str,
        delay: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let pending = workflow.view()?.job.ok_or("Missing Agent proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::Agent(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert_eq!(input.original_source, original);
        let compact: String = input
            .source
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            compact.contains("LED_BUILTIN")
                && compact.contains("HIGH")
                && compact.contains("LOW")
                && compact.contains("OUTPUT")
        );
        let delays: Vec<_> = compact
            .split("delay(")
            .skip(1)
            .map(|s| s.split(')').next().unwrap())
            .collect();
        // This fixture checks literal delays, not arbitrary C++ behavior or physical hardware.
        assert_eq!(delays.len(), 2);
        assert!(delays
            .iter()
            .all(|s| s.trim_end_matches(['u', 'U', 'l', 'L']) == delay.to_string()));
        Ok(input.source)
    }
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let mut args = std::env::args().skip(1);
        let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
        let root = PathBuf::from(args.next().ok_or("Pass fresh owned root")?);
        assert!(args.next().is_none());
        let connection = agent::connect().await?;
        fs::create_dir(&root)?;
        for folder in ["project", "conversation"] {
            fs::create_dir(root.join(folder))?;
        }
        let original="void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(1000); digitalWrite(LED_BUILTIN, LOW); delay(1000); }\n";
        let source = root.join("project/Blink.ino");
        fs::write(&source, original)?;
        let project = Project::create(&root.join("project"))?;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        let (name, selected) = agent::selected_source(&source)?;
        workflow.set_agent_source(name, selected)?;
        workflow.set_agent(connection)?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            hazlomexa::host_tools::Program::Agent,
            json!({"program":"Arduino Agent","sketch_source":{"name":"Blink.ino"},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero cambiar el ritmo de parpadeo del sketch seleccionado usando Arduino Agent. Inspecciona su código, explícame el ritmo actual y pregúntame cuál quiero. No prepares una propuesta todavía; no quiero compilar ni cargar nada.").await?;
        assert!(tools.contains("inspect_selected_sketch"));
        assert!(question.contains('?'));
        assert!(workflow.view()?.job.is_none());
        let (_,tools)=chat.say("Quiero medio segundo encendido y medio segundo apagado, con dos llamadas sencillas a delay. Conserva el LED integrado y el resto del comportamiento. Prepara una copia editada mediante Arduino Agent para revisarla, sin compilar ni cargar. Todavía no está aprobada.").await?;
        assert!(tools.contains("propose_selected_sketch_edit"));
        let first_source = proposal(&workflow, original, 500)?;
        let first = chat.approve_fixture().await?;
        let mut preserved = Vec::new();
        for format in [ArtifactFormat::Sketch, ArtifactFormat::OriginalSketch] {
            let path = workflow.artifact(first, format)?.path;
            preserved.push((path.clone(), fs::read(path)?));
        }
        let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Lee el código verificado antes de explicarme el nuevo ritmo y qué comprobó Arduino Agent. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora hazlo dos veces más rápido: un cuarto de segundo encendido y un cuarto apagado. Parte de la copia que acabamos de editar, leyendo su resultado verificado. Prepara otra copia para revisar mediante Arduino Agent, sin compilar ni cargar.").await?;
        assert!(
            tools.contains("read_verified_result")
                && tools.contains("propose_selected_sketch_edit")
        );
        let second_source = proposal(&workflow, &first_source, 250)?;
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último código verificado y explica el ritmo final y los límites de esta comprobación. No prepares más cambios ni afirmes haber probado hardware o compilado.").await?;
        assert!(tools.contains("read_verified_result"));
        for (job, expected) in [(first, first_source), (second, second_source)] {
            let read = host.call("read_verified_result", json!({"job_id":job}))?;
            assert_eq!(read["content"], expected);
            assert!(read["compilation"].is_null());
            assert_eq!(
                workflow
                    .artifact(job, ArtifactFormat::Sketch)?
                    .read_text()?,
                expected
            );
        }
        assert_eq!(fs::read_to_string(source)?, original);
        for (path, bytes) in preserved {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|r| r.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"agent","conversation_turns":5,"completed_native_jobs":2,"inspection_and_clarification":true,"authenticated_native_copy_edits":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","hardware_tested":false,"compilation_tested":false,"native_ui_tested":false,"installed_main_ui_tested":false});
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
