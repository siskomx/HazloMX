//! Exercise the Rust host against the real companion transport in an owned test buffer.
use hazlomexa::agent_editor::{self, Connection, Failure};
use std::{path::PathBuf, sync::atomic::AtomicBool};
use uuid::Uuid;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let root = PathBuf::from(
        std::env::var_os("HAZLOMEXA_NATIVE_EDITOR_TEST_ROOT").expect("Owned fixture root required"),
    );
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .join("target")
        .canonicalize()
        .unwrap();
    let canonical = root.canonicalize().unwrap();
    assert_eq!(canonical.parent(), Some(target.as_path()));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("native-arduino-transport-"));
    assert_eq!(
        std::fs::read(root.join("owned-host-probe")).unwrap(),
        b"owned"
    );
    let records = std::fs::read_dir(&root)
        .unwrap()
        .map(Result::unwrap)
        .filter(|item| item.path().extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    let connection = Connection::load(&records[0].path()).unwrap();
    let cancel = AtomicBool::new(false);
    let before = agent_editor::inspect(&connection, &cancel).await.unwrap();
    assert_eq!(before.name, "Sketch.ino");
    assert!(before.source.starts_with("// unsaved user note\n") && before.dirty);
    assert_eq!(before.source.matches("delay(1000)").count(), 1);
    let first_source = before.source.replace("delay(1000)", "delay(500)");
    let first = agent_editor::apply(&connection, Uuid::new_v4(), &before, &first_source, &cancel)
        .await
        .unwrap();
    assert_eq!(
        agent_editor::apply(&connection, Uuid::new_v4(), &before, &first_source, &cancel)
            .await
            .unwrap_err(),
        Failure::Refused
    );
    let current = agent_editor::inspect(&connection, &cancel).await.unwrap();
    assert_eq!(current.state_sha256, first.after.state_sha256);
    let final_source = current.source.replace("delay(500)", "delay(250)");
    let second = agent_editor::apply(
        &connection,
        Uuid::new_v4(),
        &current,
        &final_source,
        &cancel,
    )
    .await
    .unwrap();
    let final_state = agent_editor::inspect(&connection, &cancel).await.unwrap();
    assert_eq!(final_state.state_sha256, second.after.state_sha256);
    assert!(final_state.source.starts_with("// unsaved user note\n") && final_state.dirty);
    println!("Rust host and companion protocol: edit/readback/follow-up/stale refusal passed");
}
