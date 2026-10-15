//! A disposable native editor fixture. Windows owns and stops the entire launched process tree.
use hazlomexa::process::run_owned;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use windows_sys::Win32::System::RemoteDesktop::{
    WTSActive, WTSFreeMemory, WTSQuerySessionInformationW, WTSSessionInfoEx, WTSINFOEXW,
    WTS_CURRENT_SESSION, WTS_SESSIONSTATE_UNLOCK,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

fn desktop_unlocked() -> bool {
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
fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let root = PathBuf::from(
        std::env::var_os("HAZLOMEXA_TEST_AGENT_ROOT").expect("Supply the owned fixture root"),
    );
    assert!(root
        .canonicalize()
        .unwrap()
        .starts_with(workspace.join("target").canonicalize().unwrap()));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("arduino-native-ui-"));
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("owned-native-probe.json")).unwrap()).unwrap();
    assert_eq!(marker["owned"], true);
    if !desktop_unlocked() {
        fs::write(
            root.join("owned-runner-status.json"),
            b"{\"reason\":\"desktop_not_unlocked\",\"launched\":false}",
        )
        .unwrap();
        println!("Owned editor test requires an active, unlocked Windows desktop; launch skipped");
        return;
    }
    assert!(
        memory_available(),
        "At least 1 GB of physical and commit memory is required"
    );
    let executable = root.join("runtime/Arduino Agent.exe");
    let mut file = fs::File::open(&executable).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    assert_eq!(
        format!("{:x}", hash.finalize()),
        "d7c6e956ceecf5214867ff063e0ecf6bf0276946950ddb1f34946de31d2c5cd9"
    );
    let profile = root.join("profile");
    let mut env = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATH"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for (key, path) in [
        ("USERPROFILE", profile.clone()),
        ("HOME", profile.clone()),
        ("APPDATA", profile.join("AppData/Roaming")),
        ("LOCALAPPDATA", profile.join("AppData/Local")),
        ("TEMP", root.join("temp")),
        ("TMP", root.join("temp")),
        ("HAZLOMEXA_AGENT_EDITOR_ROOT", root.join("connection")),
        ("HAZLOMEXA_AGENT_EDITOR_PROBE_ROOT", root.clone()),
    ] {
        env.insert(key.into(), path.into_os_string());
    }
    let cli = root.join("runtime/resources/app/lib/backend/resources/arduino-cli.exe");
    let preflight = run_owned(
        &cli,
        &[
            "--config-file".into(),
            profile
                .join(".arduinoIDE/arduino-cli.yaml")
                .into_os_string(),
            "config".into(),
            "dump".into(),
            "--format".into(),
            "json".into(),
        ],
        &root,
        &env,
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(15),
    )
    .unwrap();
    fs::write(root.join("cli-preflight.stdout"), preflight.stdout).unwrap();
    fs::write(root.join("cli-preflight.stderr"), preflight.stderr).unwrap();
    if preflight.exit_code != Some(0)
        || preflight.reason != hazlomexa::process::StopReason::Completed
    {
        fs::write(
            root.join("owned-runner-status.json"),
            b"{\"reason\":\"cli_preflight_failed\"}",
        )
        .unwrap();
        println!("Owned CLI profile preflight failed; editor launch skipped");
        return;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let finished = done.clone();
    let observed = root.clone();
    let monitor = thread::spawn(move || {
        while !finished.load(Ordering::SeqCst) {
            let result = fs::read(observed.join("native-api-result.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
            let reason = if observed.join("stop-owned-agent").exists() {
                Some("explicit_owned_stop")
            } else if !desktop_unlocked() {
                Some("desktop_not_unlocked")
            } else if !memory_available() {
                Some("memory_floor")
            } else if result.as_ref().is_some_and(|r| r.get("error").is_some()) {
                Some("native_probe_failed")
            } else {
                None
            };
            if let Some(reason) = reason {
                signal.store(true, Ordering::SeqCst);
                return reason;
            }
            thread::sleep(Duration::from_millis(100));
        }
        "process_finished"
    });
    let args = vec![
        "--electronUserData".into(),
        root.join("runtime/resources/app/data/app-data")
            .into_os_string(),
        "--config-dir".into(),
        profile.join(".arduinoIDE").into_os_string(),
        "--hostname=127.0.0.1".into(),
        root.join("sketchbook/OwnedSketch/OwnedSketch.ino")
            .into_os_string(),
    ];
    let output = run_owned(
        &executable,
        &args,
        &root,
        &env,
        cancel,
        Duration::from_secs(150),
    );
    done.store(true, Ordering::SeqCst);
    let reason = monitor.join().unwrap();
    match output {
        Ok(output) => {
            fs::write(root.join("stdout.log"), output.stdout).unwrap();
            fs::write(root.join("stderr.log"), output.stderr).unwrap();
            fs::write(root.join("owned-runner-status.json"),serde_json::to_vec(&serde_json::json!({
                "reason":reason,"stop_reason":format!("{:?}",output.reason),"exit_code":output.exit_code,"logs_truncated":output.truncated})).unwrap()).unwrap();
            println!("Owned native editor stopped: {reason}");
        }
        Err(_) => {
            fs::write(
                root.join("owned-runner-status.json"),
                b"{\"reason\":\"owned_process_failed\"}",
            )
            .unwrap();
            panic!("Owned native editor process failed; inspect its private fixture logs");
        }
    }
}
