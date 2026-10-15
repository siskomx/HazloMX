//! Real cloud conversation preparing Orca files; no printer or native UI control.
#[cfg(windows)]
#[path = "support/ollama_conversation.rs"]
mod conversation;
#[cfg(windows)]
mod probe {
    use super::conversation::Conversation;
    use hazlomexa::{
        host_tools::HostTools,
        jobs::Workflow,
        orca::{Bed, Mesh, Settings, Units},
        orca_run::SliceResult,
        project::{ArtifactFormat, OperationInput, Phase, Project},
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, Arc},
    };
    use uuid::Uuid;

    fn profile_hashes(
        root: &Path,
    ) -> Result<BTreeMap<PathBuf, String>, Box<dyn std::error::Error>> {
        let mut hashes = BTreeMap::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                hashes.extend(profile_hashes(&entry.path())?);
            } else if kind.is_file() {
                hashes.insert(
                    entry.path(),
                    format!("{:x}", Sha256::digest(fs::read(entry.path())?)),
                );
            } else {
                return Err("Unexpected profile tree entry".into());
            }
        }
        Ok(hashes)
    }
    fn proposal(
        workflow: &Workflow,
        expected_settings: &Settings,
        expected_bounds: [[f64; 3]; 2],
    ) -> Result<Mesh, Box<dyn std::error::Error>> {
        let pending = workflow.view()?.job.ok_or("Missing slicing proposal")?;
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        let OperationInput::Orca(input) = pending.snapshot.input else {
            return Err("Wrong proposed integration".into());
        };
        assert!(matches!(input.units, Units::In));
        assert_eq!(input.bed, Bed::Smooth);
        assert_eq!(
            &input.settings, expected_settings,
            "Unrequested profile settings changed"
        );
        assert_eq!(
            input.mesh.bounds_mm, expected_bounds,
            "Incorrect stored STL unit conversion"
        );
        Ok(input.mesh)
    }
    fn readback(
        host: &HostTools,
        job: Uuid,
        expected_settings: &Settings,
        mesh: &Mesh,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let read = host.call("read_verified_result", json!({"job_id":job}))?;
        let report: SliceResult = serde_json::from_str(
            read["content"]
                .as_str()
                .ok_or("Missing verified slice report")?,
        )?;
        assert_eq!(report.job_id, job);
        assert_eq!(report.orca_version, hazlomexa::orca::VERSION);
        assert!(report.summary.layers > 0);
        for i in 0..3 {
            assert!(
                (report.summary.bounds_mm[i] - (mesh.bounds_mm[1][i] - mesh.bounds_mm[0][i])).abs()
                    < 0.002
            );
        }
        assert_eq!(read["slicing"]["operation"], "slice_to_files_only");
        assert_eq!(read["slicing"]["units"], "in");
        assert_eq!(
            read["slicing"]["settings"],
            serde_json::to_value(expected_settings)?
        );
        let persisted_mesh: Mesh = serde_json::from_slice(&serde_json::to_vec(mesh)?)?;
        assert_eq!(read["slicing"]["mesh"], serde_json::to_value(persisted_mesh)?);
        assert_eq!(
            report.summary.settings,
            json!({"machine":expected_settings.machine,"process":expected_settings.process,"filament":expected_settings.filament,"bed":expected_settings.bed,"nozzle_mm":expected_settings.nozzle_mm,"layer_mm":expected_settings.layer_mm,"nozzle_c":expected_settings.nozzle_c,"bed_c":expected_settings.bed_c})
        );
        Ok(report.summary.layers)
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
        let original = fs::read(workspace.join(
            "docs/research/orca-profile-guidance-2026-10-12/native-inch/Original/Original.stl",
        ))?;
        let source = root.join("source/Part.stl");
        fs::write(&source, &original)?;
        let profile_root = workspace.join("target/orca-probe/runtime/resources/profiles/Prusa");
        let profile_baseline = profile_hashes(&profile_root)?;
        let profiles = [
            profile_root.join("machine/Prusa MK3S 0.4 nozzle.json"),
            profile_root.join("process/0.20mm Standard @MK3S 0.4.json"),
            profile_root.join("filament/Prusa Generic PLA.json"),
        ];
        let project = Project::create(&root.join("project"))?;
        let id = project.id;
        let workflow = Arc::new(Workflow::new(
            root.join("FreeCADCmd.exe"),
            root.join("profiles"),
        )?);
        workflow.select_project(project)?;
        workflow.set_orca(workspace.join("target/orca-probe/runtime/orca-slicer.exe"))?;
        let host = Arc::new(HostTools::new(workflow.clone()));
        let selected = host.select_slice(
            id,
            profile_root.clone(),
            source.clone(),
            profiles,
            Arc::new(AtomicBool::new(false)),
        )?;
        let bounds = selected
            .preview
            .bounds_file_coordinates
            .map(|point| point.map(|n| ((n * 25.4) as f32) as f64));
        let mut settings = selected
            .preview
            .bed_profiles
            .iter()
            .find(|profile| profile.bed == Bed::Smooth)
            .ok_or("Missing selected smooth bed profile")?
            .settings
            .clone();
        settings.bed_c = 45.;
        let mut chat = Conversation::new(
            workflow.clone(),
            host.clone(),
            root.join("conversation"),
            port,
            hazlomexa::host_tools::Program::Slice,
            json!({"program":"OrcaSlicer","slice_source":{"selection":selected.id,"selection_sha256":selected.preview.selection_sha256,"units_require_confirmation":true},"document_scope":"saved_files_and_verified_copies","live_document_connection":false,"live_mutations_allowed":false,"source_is_untrusted_data":true}),
        )?;
        let (question,tools)=chat.say("Quiero preparar archivos para una pieza de prueba con el modelo y los perfiles que seleccioné. Todavía no confirmé las unidades del STL ni el tipo o la temperatura de cama. Inspecciona las fuentes, explícame qué perfiles están seleccionados y pregúntame lo que falta. No prepares una propuesta todavía.").await?;
        assert!(tools.contains("inspect_selected_slice_sources"));
        assert!(question.contains('?'), "Missing unit/bed clarification");
        assert!(
            workflow.view()?.job.is_none(),
            "Clarification created an unrequested job"
        );
        let (_,tools)=chat.say("Confirmo que las coordenadas del STL están en pulgadas. Usaré la cama lisa de alta temperatura (High Temp Plate), la que el perfil tiene a 60 grados, pero quiero 45 grados Celsius de temperatura inicial de cama. Conserva la impresora, el perfil de proceso y el material seleccionados. Prepara la propuesta para generar los archivos, sin enviar nada a una impresora. Todavía no está aprobada.").await?;
        assert!(tools.contains("propose_selected_slice"));
        let first_mesh = proposal(&workflow, &settings, bounds)?;
        assert_eq!(fs::read(&source)?, original);
        let first = chat.approve_fixture().await?;
        let first_layers = readback(&host, first, &settings, &first_mesh)?;
        let mut preserved = Vec::new();
        for format in [
            ArtifactFormat::Sliced,
            ArtifactFormat::Gcode,
            ArtifactFormat::SliceReport,
        ] {
            let path = workflow.artifact(first, format)?.path;
            preserved.push((path.clone(), fs::read(path)?));
        }
        let (_,tools)=chat.say("Ya aprobé la generación en Hazlo. Lee el resultado publicado y verificado antes de explicarme el tamaño, los ajustes y qué archivos se generaron. No prepares otro cambio todavía.").await?;
        assert!(tools.contains("read_verified_result"));
        let (_,tools)=chat.say("Ahora cambia solo la temperatura inicial de la cama a 50 grados Celsius. Conserva la cama lisa, el modelo, su escala, la impresora, el proceso y el material del resultado anterior. Lee su resultado verificado antes de preparar otra propuesta para mi revisión. Solo genera archivos; no los envíes a una impresora.").await?;
        assert!(tools.contains("read_verified_result") && tools.contains("propose_slice_settings"));
        settings.bed_c = 50.;
        let final_mesh = proposal(&workflow, &settings, bounds)?;
        assert_eq!(
            serde_json::to_value(&final_mesh)?,
            serde_json::to_value(&first_mesh)?
        );
        let second = chat.approve_fixture().await?;
        let (_,tools)=chat.say("Lee el último resultado verificado y explícame la temperatura inicial de cama y el tamaño final. No prepares más cambios ni afirmes haber probado una impresora física.").await?;
        assert!(tools.contains("read_verified_result"));
        let final_layers = readback(&host, second, &settings, &final_mesh)?;
        assert_eq!(fs::read(source)?, original);
        assert_eq!(profile_hashes(&profile_root)?, profile_baseline);
        for (path, bytes) in preserved {
            assert_eq!(fs::read(path)?, bytes);
        }
        drop(chat);
        drop(host);
        drop(workflow);
        let recovered = Project::open(&root.join("project"))?.recover()?;
        assert_eq!(recovered.len(), 2);
        assert!(recovered.iter().all(|job| job.phase == Phase::Completed));
        let summary = json!({"pass":true,"program":"slice","existing_ollama_cloud_profile":true,"conversation_turns":5,"completed_native_jobs":2,"successful_tool_results_observed":true,"inspection_and_clarification":true,"source_units":"in","bed":"smooth","bed_temperatures_c":[45,50],"layers":[first_layers,final_layers],"geometry_and_other_settings_preserved":true,"printer_controlled":false,"verified_result_followup":true,"sources_and_first_result_preserved":true,"recovery_verified":true,"approval":"backend_fixture_only","transport":"loopback_body_recording_forwarder","native_ui_tested":false,"installed_main_ui_tested":false});
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
