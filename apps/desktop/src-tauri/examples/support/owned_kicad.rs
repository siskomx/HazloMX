//! Shared supervisor for disposable native KiCad test windows.
use hazlomexa::process::run_owned;
use serde_json::json;
use sha2::{Digest,Sha256};
use std::{collections::BTreeMap,ffi::OsString,fs,path::Path,sync::{Arc,atomic::{AtomicBool,Ordering}},thread,time::Duration};
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
pub struct Editor {
    pub(super) stop: Arc<AtomicBool>,
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
pub fn editor(workspace: &Path, owned: &Path, board: &Path, logs: &Path) -> Editor {
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
