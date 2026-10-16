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
                let reason = if !unlocked() {
                    Some("desktop_locked")
                } else if !memory_available() {
                    Some("memory_floor")
                } else {
                    None
                };
                if let Some(reason) = reason {
                    fs::write(
                        observed.join("editor-monitor-stop.json"),
                        serde_json::to_vec(&json!({"reason":reason})).unwrap(),
                    )
                    .unwrap();
                    cancel.store(true, Ordering::SeqCst);
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
    assert_eq!(input.movement.reference, "R1");
    assert_eq!((input.movement.x_mm, input.movement.y_mm), (x, y));
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
    let project = root.join("project");
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
            serde_json::from_value(result["observation"].clone())
                .map_err(|_| "Missing native PCB state".into())
        })
    };
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
    drop(chat);
    drop(host);
    drop(workflow);
    let history = Project::open(&project)?.recover()?;
    assert_eq!(history.len(), 2);
    assert!(history
        .iter()
        .all(|s| s.phase == Phase::Completed && s.schema_version == 14));
    drop(guard);
    assert!(root.join("editor-stopped.json").is_file());
    let summary = json!({"pass":true,"program":"pcb-live","conversation_turns":5,"completed_native_jobs":2,
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
