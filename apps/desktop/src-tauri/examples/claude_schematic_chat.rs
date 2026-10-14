//! Real configured Claude conversation against owned saved-source copies; approval is a backend fixture.
#[cfg(windows)]
mod probe {
    use hazlomexa::{
        claude,
        host_tools::HostTools,
        jobs::Workflow,
        project::{ArtifactFormat, OperationInput, Phase, Project},
        tool_bridge::ToolBridge,
    };
    use serde_json::json;
    use std::{
        collections::BTreeSet,
        fs,
        path::PathBuf,
        sync::{atomic::AtomicBool, Arc},
    };
    use uuid::Uuid;

    struct Conversation {
        runtime: claude::Runtime,
        workflow: Arc<Workflow>,
        host: Arc<HostTools>,
        helper: PathBuf,
        directory: PathBuf,
        session: Uuid,
        project: Uuid,
        selection: Uuid,
        source_hash: String,
        turn: usize,
    }
    impl Conversation {
        async fn say(
            &mut self,
            message: &str,
        ) -> Result<(String, BTreeSet<String>), Box<dyn std::error::Error>> {
            self.turn += 1;
            println!("{}", json!({"stage":"model_turn_started","turn":self.turn}));
            let bridge = ToolBridge::start_project(self.host.clone(), &self.helper, self.project)?;
            let config = bridge.config()?;
            let configuration = self.directory.join("mcp.json");
            fs::write(
                &configuration,
                serde_json::to_vec(
                    &json!({"mcpServers":{"hazlomexa":{"type":"stdio","command":config.command,"env":config.env}}}),
                )?,
            )?;
            let view = self.workflow.view()?;
            let prompt = json!({"user_request":message,"desktop_context":{
                "project_id":self.project,"program":"KiCad / Konnect","schematic_source":{"selection":self.selection,"sha256":self.source_hash},
                "current_job":view.job.map(|job|json!({"id":job.snapshot.input.job_id(),"phase":job.snapshot.phase})),
                "document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true
            }}).to_string();
            let mut updates = Vec::new();
            let mut tools = BTreeSet::new();
            let response = claude::run(
                claude::TurnRequest {
                    executable: self.runtime.executable.clone(),
                    sha256: self.runtime.sha256.clone(),
                    directory: self.directory.clone(),
                    configuration: configuration.clone(),
                    session: self.session,
                    resume: self.turn > 1,
                    prompt,
                    environment: claude::environment(),
                    cancelled: Arc::new(AtomicBool::new(false)),
                },
                |update| {
                    if let Some(tool) = &update.active_tool {
                        tools.insert(tool.clone());
                    }
                    updates.push(update);
                },
            )
            .await;
            fs::remove_file(configuration)?;
            bridge.close().await?;
            fs::write(
                self.directory.join(format!("turn-{}.json", self.turn)),
                serde_json::to_vec_pretty(&json!({"updates":updates,"tools":tools}))?,
            )?;
            let response = response?;
            if let Some(error) = response.error {
                return Err(error.into());
            }
            assert!(response.initialized && response.complete && !response.text.trim().is_empty());
            println!(
                "{}",
                json!({"stage":"model_turn_completed","turn":self.turn,"tools_used":tools})
            );
            Ok((response.text, tools))
        }
        async fn approve_fixture(&self) -> Result<Uuid, Box<dyn std::error::Error>> {
            let pending = self
                .workflow
                .view()?
                .job
                .ok_or("No proposal was prepared")?;
            assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
            let dispatch = self
                .workflow
                .begin(pending.approval_id.ok_or("Missing fixture approval")?)?;
            let workflow = self.workflow.clone();
            let completed =
                tokio::task::spawn_blocking(move || workflow.execute(dispatch)).await??;
            assert_eq!(
                completed.snapshot.phase,
                Phase::Completed,
                "Native execution failed"
            );
            Ok(completed.snapshot.input.job_id())
        }
    }
    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
        let [client, helper, konnect, kicad, root] = args.as_slice() else {
            return Err("Pass configured Claude executable, helper, Konnect, KiCad CLI and fresh owned test root".into());
        };
        fs::create_dir(root)?;
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
        let runtime = claude::identify(client.clone())?;
        let project = Project::create(&root.join("project"))?;
        let id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_konnect(konnect.clone())?;
        workflow.set_kicad(kicad.clone())?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selected = host.select_schematic(
            id,
            root.join("source"),
            root.join("source/basic_test.kicad_sch"),
        )?;
        let resistor = selected
            .components
            .iter()
            .find(|component| component.reference.starts_with('R'))
            .ok_or("Fixture has no resistor")?
            .clone();
        let mut chat = Conversation {
            runtime,
            workflow: workflow.clone(),
            host: host.clone(),
            helper: helper.clone(),
            directory: root.join("conversation"),
            session: Uuid::new_v4(),
            project: id,
            selection: selected.id,
            source_hash: selected.sha256,
            turn: 0,
        };
        let (question,tools)=chat.say("Quiero cambiar una resistencia de este esquema, pero todavía no elegí cuál ni su valor. Inspecciona el esquema seleccionado, explícame las resistencias que tiene y pregúntame cuál cambiar. No prepares una propuesta hasta que responda.").await?;
        assert!(
            tools.contains("mcp__hazlomexa__inspect_selected_schematic"),
            "No actual source inspection occurred"
        );
        assert!(
            question.contains('?') && question.contains(&resistor.reference),
            "No source-grounded clarification"
        );
        assert!(
            workflow.view()?.job.is_none(),
            "The clarification prepared an unrequested operation"
        );
        let (_,tools)=chat.say(&format!("La resistencia {} que me acabas de mostrar: cámbiala a 4.7k. Prepara la propuesta para que pueda revisarla; todavía no está aprobada.",resistor.reference)).await?;
        assert!(tools.contains("mcp__hazlomexa__propose_schematic_value"));
        let pending = workflow.view()?.job.ok_or("Missing first proposal")?;
        let OperationInput::Konnect(input) = &pending.snapshot.input else {
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
        assert!(tools.contains("mcp__hazlomexa__read_verified_result"));
        assert!(text.contains("4.7") || text.contains("4,7"));
        let (_,tools)=chat.say("Ahora deja esa misma resistencia en 10k. Parte de la copia que acabamos de editar, no del original. Lee su resultado verificado antes de preparar otra propuesta para mi revisión.").await?;
        assert!(
            tools.contains("mcp__hazlomexa__read_verified_result")
                && tools.contains("mcp__hazlomexa__propose_schematic_value")
        );
        let pending = workflow.view()?.job.ok_or("Missing follow-up proposal")?;
        let OperationInput::Konnect(input) = &pending.snapshot.input else {
            return Err("Wrong follow-up integration".into());
        };
        assert_eq!(input.edit.reference, resistor.reference);
        assert_eq!(input.edit.expected_value, "4.7k");
        assert_eq!(input.edit.value, "10k");
        let second = chat.approve_fixture().await?;
        let (text,tools)=chat.say("Lee el último resultado verificado y dime el valor final. No prepares más cambios.").await?;
        assert!(tools.contains("mcp__hazlomexa__read_verified_result"));
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
                .find(|component| component.reference == resistor.reference)
                .ok_or("Missing final resistor")?
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
        let recovered = Project::open(&root.join("project"))?;
        assert_eq!(
            recovered
                .recover()?
                .iter()
                .filter(|run| run.phase == Phase::Completed)
                .count(),
            2
        );
        let summary = json!({"pass":true,"real_configured_claude":true,"claude_version":"2.1.285","model_override":false,"conversation_turns":5,"completed_native_jobs":2,"inspection_and_clarification":true,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","native_ui_tested":false,"installed_main_ui_tested":false});
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
    panic!("Windows probe required");
}
