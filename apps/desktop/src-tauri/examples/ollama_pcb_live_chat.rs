//! Real existing-cloud conversation operating an owned open KiCad PCB, with fixture approvals.
#[path = "support/ollama_conversation.rs"]
mod conversation;
use conversation::Conversation;
use hazlomexa::{
    host_tools::{HostTools, Program},
    jobs::Workflow,
    kicad_live::{Connection, Observation},
    process::run_owned,
    project::{ArtifactFormat, OperationInput, Phase, Project},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::System::RemoteDesktop::{
    WTSActive, WTSFreeMemory, WTSQuerySessionInformationW, WTSSessionInfoEx, WTSINFOEXW,
    WTS_CURRENT_SESSION, WTS_SESSIONSTATE_UNLOCK,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

fn memory_available() -> bool {
    let mut memory = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    unsafe {
        GlobalMemoryStatusEx(&mut memory) != 0
            && memory.ullAvailPhys >= 1024 * 1024 * 1024
            && memory.ullAvailPageFile >= 1024 * 1024 * 1024
    }
}

fn unlocked() -> bool {
    let mut buffer = std::ptr::null_mut();
    let mut bytes = 0;
    unsafe {
        let queried = WTSQuerySessionInformationW(
            std::ptr::null_mut(),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        ) != 0;
        let ready = queried
            && !buffer.is_null()
            && bytes as usize >= std::mem::size_of::<WTSINFOEXW>()
            && {
                let info = &*buffer.cast::<WTSINFOEXW>();
                let session = info.Data.WTSInfoExLevel1;
                info.Level == 1
                    && session.SessionState == WTSActive
                    && session.SessionFlags == WTS_SESSIONSTATE_UNLOCK as i32
            };
        if !buffer.is_null() {
            WTSFreeMemory(buffer.cast());
        }
        ready
    }
}
struct Editor {
    stop: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
fn editor(workspace: &Path, owned: &Path, board: &Path, logs: &Path) -> Editor {
    assert!(unlocked(), "An unlocked Windows desktop is required");
    let executable = workspace.join("target/kicad-probe/runtime/bin/pcbnew.exe");
    assert!(
        memory_available(),
        "The owned editor requires at least 1 GB free physical and commit memory"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&executable).unwrap())),
        "8e75349270ebf6fba1709a82e75ede6bc395db25bd0ebc3fb5006547e8b51616"
    );
    let mut env = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATH"] {
        if let Some(v) = std::env::var_os(key) {
            env.insert(key.into(), v);
        }
    }
    // Reuse the already configured owned profile without changing its preferences.
    for (key, value) in [
        ("KICAD_CONFIG_HOME", owned.join("profile")),
        ("TEMP", owned.join("temp")),
        ("TMP", owned.join("temp")),
    ] {
        assert!(value.is_dir());
        env.insert(key.into(), value.into_os_string());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    let directory = logs.to_path_buf();
    let drawing = board.to_path_buf();
    let task = thread::spawn(move || {
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        let cancel = signal.clone();
        let observed = directory.clone();
        let monitor = thread::spawn(move || {
            while !finished.load(Ordering::SeqCst) {
                let reason = if observed.join("stop-owned-editors").exists() {
                    Some("explicit_stop")
                } else if !unlocked() {
                    Some("desktop_locked")
                } else if !memory_available() {
                    Some("memory_floor")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    cancel.store(true, Ordering::SeqCst);
                    let _ = fs::write(
                        observed.join("editor-monitor-stop.json"),
                        serde_json::to_vec(&json!({"reason":reason})).unwrap(),
                    );
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        });
        let result = run_owned(
            &executable,
            &[drawing.into_os_string()],
            &directory,
            &env,
            signal,
            Duration::from_secs(600),
        );
        done.store(true, Ordering::SeqCst);
        monitor.join().unwrap();
        match result {
            Ok(output) => {
                fs::write(directory.join("editor-stdout.log"), output.stdout).unwrap();
                fs::write(directory.join("editor-stderr.log"), output.stderr).unwrap();
                fs::write(
                    directory.join("editor-stopped.json"),
                    serde_json::to_vec(
                        &json!({"stopped":true,"reason":format!("{:?}",output.reason)}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            Err(_) => {
                fs::write(
                    directory.join("editor-stopped.json"),
                    b"{\"stopped\":true,\"launch_failed\":true}",
                )
                .unwrap();
            }
        }
    });
    Editor {
        stop,
        task: Some(task),
    }
}
fn proposal(
    workflow: &Workflow,
    before: &Observation,
    x: f64,
    y: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let pending = workflow.view()?.job.ok_or("Missing native PCB proposal")?;
    assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
    let OperationInput::LivePcb(input) = pending.snapshot.input else {
        panic!("A saved copy cannot satisfy this native gate")
    };
    assert_eq!(input.before.instance_sha256, before.instance_sha256);
    assert_eq!(input.before.document_sha256, before.document_sha256);
    assert_eq!(input.before.state_sha256, before.state_sha256);
    assert_eq!(input.movement.as_ref().unwrap().reference, "R1");
    assert_eq!(
        (
            input.movement.as_ref().unwrap().x_mm,
            input.movement.as_ref().unwrap().y_mm
        ),
        (x, y)
    );
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(code) = hazlomexa::konnect::movement_validator_cli() {
        std::process::exit(code);
    }
    let mut args = std::env::args().skip(1);
    let port: u16 = args.next().ok_or("Pass owned recording port")?.parse()?;
    let root = PathBuf::from(args.next().ok_or("Pass fresh conversation root")?);
    let mode = args.next();
    assert!(matches!(
        mode.as_deref(),
        None | Some("--export-only" | "--with-export" | "--hazlo-ui")
    ));
    let export_only = mode.as_deref() == Some("--export-only");
    let with_export = mode.as_deref() == Some("--with-export");
    let hazlo_ui = mode.as_deref() == Some("--hazlo-ui");
    assert!(args.next().is_none());
    let owned = PathBuf::from(
        std::env::var_os("HAZLOMEXA_REAL_PCB_ROOT").ok_or("Pass owned KiCad profile root")?,
    );
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    assert_eq!(
        owned.canonicalize()?.parent(),
        Some(workspace.join("target").canonicalize()?.as_path())
    );
    assert!(owned
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("kicad-live-ui-"));
    assert!(root
        .parent()
        .ok_or("Missing owned root parent")?
        .canonicalize()?
        .starts_with(workspace.join("target").canonicalize()?));
    fs::create_dir(&root)?;
    for name in ["project", "conversation"] {
        fs::create_dir(root.join(name))?;
    }
    let native = owned.join(format!("native-chat-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&native)?;
    let board = native.join("OwnedConversationBoard.kicad_pcb");
    let source = fs::read(
        workspace.join("docs/research/konnect-pcb-2026-10-12/Original/basic_test.kicad_pcb"),
    )?;
    fs::write(&board, &source)?;
    let guard = editor(workspace, &owned, &board, &root);
    let project = root.join(if hazlo_ui {
        "observer-project"
    } else {
        "project"
    });
    if hazlo_ui {
        fs::create_dir(&project)?;
    }
    let selected_project = Project::create(&project)?;
    let id = selected_project.id;
    let workflow = Arc::new(Workflow::new(
        root.join("FreeCADCmd.exe"),
        workspace.join("target/pcb-chat-profiles"),
    )?);
    workflow.select_project(selected_project)?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let connection = Connection {
        endpoint: format!("ipc://{}", owned.join("temp/kicad/api.sock").display()),
        token: String::new(),
    };
    let starting = Instant::now();
    let selected = loop {
        let result = tokio::task::block_in_place(|| {
            host.select_live_pcb(
                id,
                board.clone(),
                connection.clone(),
                Arc::new(AtomicBool::new(false)),
            )
        });
        match result {
            Ok(value) => break value,
            Err(_)
                if starting.elapsed() < Duration::from_secs(30)
                    && !root.join("editor-stopped.json").exists() =>
            {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(_) => {
                return Err(
                    "Owned native PCB did not become ready; private evidence retained".into(),
                )
            }
        }
    };
    let initial = selected.observation.clone();
    let read = || -> Result<Observation, String> {
        tokio::task::block_in_place(|| {
            let result = host.call(
                "inspect_selected_live_pcb",
                json!({"selection":selected.id}),
            )?;
            assert_eq!(result["coordinate_system"]["positive_y"], "down");
            assert_eq!(result["coordinate_system"]["topmost"], "smallest_y_mm");
            serde_json::from_value(result["observation"].clone())
                .map_err(|_| "Missing native PCB state".into())
        })
    };
    if hazlo_ui {
        let executable = workspace.join("target/debug/hazlomexa.exe");
        for name in [
            "profile/AppData/Local",
            "profile/AppData/Roaming",
            "profile/Desktop",
            "profile/Documents",
        ] {
            fs::create_dir_all(root.join(name))?;
        }
        let mut environment = BTreeMap::<OsString, OsString>::new();
        for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATH"] {
            if let Some(value) = std::env::var_os(key) {
                environment.insert(key.into(), value);
            }
        }
        for (key, name) in [
            ("USERPROFILE", "profile"),
            ("APPDATA", "appdata"),
            ("LOCALAPPDATA", "local"),
            ("WEBVIEW2_USER_DATA_FOLDER", "webview"),
            ("HAZLOMEXA_SMOKE_PROJECT", "project"),
            ("TEMP", "temp"),
            ("TMP", "temp"),
        ] {
            fs::create_dir_all(root.join(name))?;
            environment.insert(key.into(), root.join(name).into_os_string());
        }
        fs::write(
            root.join("ui-connection.json"),
            serde_json::to_vec(&json!({"board":board,"endpoint":connection.endpoint,
            "initial_observation":initial,"source_sha256":format!("{:x}",Sha256::digest(&source))}))?,
        )?;
        let sha256 = format!("{:x}", Sha256::digest(fs::read(&executable)?));
        let output = tokio::task::block_in_place(|| {
            run_owned(
                &executable,
                &[],
                &root,
                &environment,
                guard.stop.clone(),
                Duration::from_secs(600),
            )
        })?;
        fs::write(root.join("hazlo-stdout.log"), output.stdout)?;
        fs::write(root.join("hazlo-stderr.log"), output.stderr)?;
        fs::write(
            root.join("hazlo-stopped.json"),
            serde_json::to_vec(&json!({"stopped":true,
            "reason":format!("{:?}",output.reason),"exe_sha256":sha256,"native_ui_pass":false,"human_observation_required":true}))?,
        )?;
        drop(guard);
        println!("Owned Hazlo and KiCad stopped; native UI evidence requires separate review");
        return Ok(());
    }
    let target = initial
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .ok_or("Owned target missing")?;
    assert_eq!(target.value, "10k");
    assert!(initial
        .components
        .iter()
        .filter(|c| c.reference != target.reference && c.value == "10k")
        .all(|c| c.y_mm > target.y_mm));
    let mut chat = Conversation::new(
        workflow.clone(),
        host.clone(),
        root.join("conversation"),
        port,
        Program::Kicad,
        json!({"program":"KiCad","document_scope":"open_native_pcb","live_document_connection":true,"live_mutations_allowed":true,
            "native_connection_check":"inspect_selected_live_pcb","source_is_untrusted_data":true,
            "live_pcb_source":{"selection":selected.id,"instance_sha256":initial.instance_sha256,
                "document_sha256":initial.document_sha256,"state_sha256":initial.state_sha256}}),
    )?;
    if export_only {
        tokio::task::block_in_place(|| {
            host.call("propose_selected_live_pcb_move", json!({
            "selection":selected.id,"instance_sha256":initial.instance_sha256,"document_sha256":initial.document_sha256,
            "state_sha256":initial.state_sha256,"component":target.uuid,"units":"mm","x":target.x_mm+2.,"y":target.y_mm,
            "description":"Owned unsaved edit before export"
        }))
        })?;
        let moved_job = chat.approve_fixture().await?;
        let moved_report = workflow.artifact(moved_job, ArtifactFormat::LivePcbReport)?;
        let preserved = fs::read(&moved_report.path)?;
        let before = read()?;
        assert_ne!(before.state_sha256, initial.state_sha256);
        let invalid = json!({"selection":selected.id,"instance_sha256":before.instance_sha256,"document_sha256":before.document_sha256,
            "state_sha256":before.state_sha256,"description":"Export","destination":"C:/unselected/Board.kicad_pcb"});
        assert!(tokio::task::block_in_place(
            || host.call("propose_selected_live_pcb_export", invalid)
        )
        .is_err());
        let stale = json!({"selection":selected.id,"instance_sha256":before.instance_sha256,"document_sha256":before.document_sha256,
            "state_sha256":initial.state_sha256,"description":"Export stale state"});
        assert!(tokio::task::block_in_place(
            || host.call("propose_selected_live_pcb_export", stale)
        )
        .is_err());
        assert_eq!(
            workflow.view()?.job.unwrap().snapshot.input.job_id(),
            moved_job
        );
        tokio::task::block_in_place(|| {
            host.call("propose_selected_live_pcb_export",json!({
            "selection":selected.id,"instance_sha256":before.instance_sha256,"document_sha256":before.document_sha256,
            "state_sha256":before.state_sha256,"description":"Export the approved open PCB without saving it"
        }))
        })?;
        assert_eq!(read()?.state_sha256, before.state_sha256);
        let export_job = chat.approve_fixture().await?;
        let result = host.call("read_verified_result", json!({"job_id":export_job}))?;
        assert_eq!(result["format"], "live_pcb_report");
        let artifact = workflow.artifact(export_job, ArtifactFormat::KicadSource)?;
        let exported = fs::read(&artifact.path)?;
        assert_eq!(
            format!("{:x}", Sha256::digest(&exported)),
            before.state_sha256
        );
        let components = hazlomexa::konnect::footprints(&exported)?;
        let exported_target = components.iter().find(|c| c.uuid == target.uuid).unwrap();
        assert_eq!(
            (exported_target.x_mm, exported_target.y_mm),
            (target.x_mm + 2., target.y_mm)
        );
        assert_eq!(read()?.state_sha256, before.state_sha256);
        assert_eq!(fs::read(&board)?, source);
        assert_eq!(fs::read(&moved_report.path)?, preserved);
        drop(artifact);
        drop(moved_report);
        drop(chat);
        drop(host);
        drop(workflow);
        let history = Project::open(&project)?.recover()?;
        assert_eq!(history.len(), 2);
        assert!(history
            .iter()
            .all(|s| s.phase == Phase::Completed && s.schema_version == 14));
        drop(guard);
        let summary = json!({"pass":true,"native_export_copy":true,"unsaved_edit_exported":true,
            "original_and_native_state_preserved":true,"recovery_verified":true,"editor_stopped":true,
            "approval":"backend_fixture_only","provider_tested":false,"chat_export_tool_registered":true});
        fs::write(
            root.join("export-summary.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        println!("{summary}");
        return Ok(());
    }
    let (reply,tools)=chat.say("Quiero recolocar una resistencia de la PCB abierta, pero no conozco sus nombres técnicos ni las posiciones. Inspecciona la placa y ayúdame a identificar cuál y definir el movimiento. Pregunta lo que falte; no prepares el cambio todavía.").await?;
    assert!(tools.contains("inspect_selected_live_pcb") && reply.contains('?'));
    assert!(workflow.view()?.job.is_none());
    let (_,tools)=chat.say("Es la resistencia de 10k que está más arriba. Muévela dos milímetros a la derecha desde donde está. Prepara ese cambio para revisar, sin guardarlo ni volver a trazar pistas. Aún no está aprobado.").await?;
    assert!(tools.contains("propose_selected_live_pcb_move"));
    let first_x = target.x_mm + 2.;
    proposal(&workflow, &initial, first_x, target.y_mm)?;
    assert_eq!(read()?.state_sha256, initial.state_sha256);
    let first = chat.approve_fixture().await?;
    let first_report = workflow.artifact(first, ArtifactFormat::LivePcbReport)?;
    let preserved = fs::read(&first_report.path)?;
    let first_state = read()?;
    assert_ne!(first_state.state_sha256, initial.state_sha256);
    let (_,tools)=chat.say("Ya aprobé el cambio en Hazlo. Lee el resultado verificado y el estado de la PCB abierta antes de explicarlo. No prepares otro cambio todavía.").await?;
    assert!(tools.contains("read_verified_result") && tools.contains("inspect_selected_live_pcb"));
    let (_,tools)=chat.say("Ahora mueve esa misma resistencia un décimo de pulgada hacia abajo desde su posición actual. Conserva su posición horizontal y lo demás. Prepara solo ese movimiento para revisión; no guardes ni vuelvas a trazar pistas.").await?;
    assert!(
        tools.contains("inspect_selected_live_pcb")
            && tools.contains("propose_selected_live_pcb_move")
    );
    let second_y = ((target.y_mm + 2.54) * 1e6).round() / 1e6;
    proposal(&workflow, &first_state, first_x, second_y)?;
    assert_eq!(read()?.state_sha256, first_state.state_sha256);
    let second = chat.approve_fixture().await?;
    let (_,tools)=chat.say("Ya aprobé el segundo cambio. Lee el resultado verificado y la PCB abierta y explica la posición final en lenguaje cotidiano. No prepares más cambios ni afirmes haber guardado o reconectado pistas.").await?;
    assert!(tools.contains("read_verified_result") && tools.contains("inspect_selected_live_pcb"));
    let final_state = read()?;
    let final_target = final_state
        .components
        .iter()
        .find(|c| c.uuid == target.uuid)
        .unwrap();
    assert_eq!((final_target.x_mm, final_target.y_mm), (first_x, second_y));
    assert_eq!(final_state.instance_sha256, initial.instance_sha256);
    assert_eq!(final_state.document_sha256, initial.document_sha256);
    assert_eq!(fs::read(&board)?, source);
    assert_eq!(fs::read(&first_report.path)?, preserved);
    for job in [first, second] {
        assert_eq!(
            host.call("read_verified_result", json!({"job_id":job}))?["format"],
            "live_pcb_report"
        );
    }
    let completed_jobs = if with_export {
        let (_,tools)=chat.say("Para finalizar, quiero una copia editable de la PCB tal como está abierta ahora, con nuestros cambios sin guardar. Prepara la exportación para revisar; conserva el original y no guardes el documento abierto ni cambies las pistas.").await?;
        assert!(
            tools.contains("inspect_selected_live_pcb")
                && tools.contains("propose_selected_live_pcb_export")
        );
        let pending = workflow.view()?.job.unwrap();
        let OperationInput::LivePcb(input) = &pending.snapshot.input else {
            panic!("Missing native export")
        };
        assert!(input.export_copy && input.movement.is_none());
        assert_eq!(input.before.state_sha256, final_state.state_sha256);
        assert!(workflow
            .artifact(input.job_id, ArtifactFormat::KicadSource)
            .is_err());
        assert_eq!(read()?.state_sha256, final_state.state_sha256);
        let exported = chat.approve_fixture().await?;
        let artifact = workflow.artifact(exported, ArtifactFormat::KicadSource)?;
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&artifact.path)?)),
            final_state.state_sha256
        );
        let (reply,tools)=chat.say("Ya aprobé la exportación de la copia. Lee el resultado verificado y el estado de la PCB abierta, y explica qué archivo se conservó y qué sigue sin guardar. No prepares otro cambio.").await?;
        assert!(
            tools.contains("read_verified_result") && tools.contains("inspect_selected_live_pcb")
        );
        assert!(reply.to_lowercase().contains("copia"));
        assert_eq!(read()?.state_sha256, final_state.state_sha256);
        assert_eq!(fs::read(&board)?, source);
        assert_eq!(fs::read(&first_report.path)?, preserved);
        3
    } else {
        2
    };
    drop(chat);
    drop(host);
    drop(workflow);
    let history = Project::open(&project)?.recover()?;
    assert_eq!(history.len(), completed_jobs);
    assert!(history
        .iter()
        .all(|s| s.phase == Phase::Completed && s.schema_version == 14));
    drop(guard);
    assert!(root.join("editor-stopped.json").is_file());
    let summary = json!({"pass":true,"program":"pcb-live","conversation_turns":if with_export {7}else{5},"completed_native_jobs":completed_jobs,
        "export_copy_verified":with_export,
        "natural_component_resolution":true,"metric_and_imperial_moves":true,"fresh_native_readback":true,
        "source_and_first_receipt_preserved":true,"recovery_verified":true,"editor_stopped":true,
        "approval":"backend_fixture_only","native_ui_approval":false,"installed_main_ui_tested":false});
    fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{summary}");
    Ok(())
}
