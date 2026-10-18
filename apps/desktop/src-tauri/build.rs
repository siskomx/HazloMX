fn main() {
    #[cfg(feature = "desktop")]
    verify_native_pcb_worker();
    #[cfg(feature = "desktop")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_snapshot",
            "conversation_history",
            "detect_clients",
            "connect_native_tools",
            "disconnect_native_tools",
            "cancel_source_inspection",
            "select_codex",
            "connect_codex",
            "cancel_codex_login",
            "open_codex_login",
            "start_codex_chat",
            "answer_codex_question",
            "select_claude",
            "start_claude_chat",
            "start_ollama_chat",
            "cancel_chat",
            "set_agent_tools",
            "select_project",
            "select_freecad",
            "select_arduino",
            "select_cadcraft",
            "prepare_drawing",
            "discover_drawing",
            "prepare_selected_drawing",
            "select_kicad",
            "select_konnect",
            "prepare_konnect",
            "discover_schematic",
            "discover_pcb",
            "discover_live_pcb",
            "discover_live_drawing",
            "prepare_pcb_move",
            "prepare_discovered_schematic",
            "select_orca",
            "prepare_slice",
            "discover_slice",
            "prepare_cadpilot",
            "discover_cad_document",
            "read_cadpilot_result",
            "prepare_cadpilot_followup",
            "prepare_kicad",
            "read_kicad_summary",
            "start_compile",
            "edit_compile",
            "install_agent_companion",
            "connect_agent",
            "select_agent_sketch",
            "start_agent_edit",
            "edit_agent_proposal",
            "start_proposal",
            "edit_proposal",
            "approve_operation",
            "cancel_job",
            "open_artifact",
            "start_inference",
            "cancel_inference",
        ]),
    ))
    .expect("Tauri application permissions/build failed");
}

#[cfg(feature = "desktop")]
fn verify_native_pcb_worker() {
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap();
    let directory = root.join("target/native-pcb-worker");
    let receipt = directory.join("build-receipt.json");
    let executable = directory.join("hazlomexa-kicad.exe");
    println!("cargo:rerun-if-changed={}", receipt.display());
    println!("cargo:rerun-if-changed={}", executable.display());
    let message = "Build the native PCB worker first: python tests/kicad/build_worker.py --python <Windows x64 Python 3.11>";
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(receipt).expect(message)).expect(message);
    assert_eq!(receipt["self_check"], true, "{message}");
    let hash = |path:&Path| format!("{:x}", Sha256::digest(fs::read(path).expect(message)));
    assert_eq!(receipt["sha256"], hash(&executable), "{message}");
    for name in [
        "apps/desktop/src-tauri/resources/kicad/live.py",
        "tests/kicad/build_worker.py", "tests/kicad/ipc-client-requirements.txt",
        "tests/kicad/worker-build-requirements.txt",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(name).display());
        assert_eq!(receipt["inputs"][name], hash(&root.join(name)), "{message}");
    }
}
