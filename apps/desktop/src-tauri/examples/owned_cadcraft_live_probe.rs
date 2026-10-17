//! Actual pinned CADCraft GUI protocol acceptance; never connects to a user's editor.
use hazlomexa::process::run_owned;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
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

struct Client {
    stream: BufReader<TcpStream>,
    id: u64,
    calls: Vec<Value>,
}
impl Client {
    fn call(&mut self, method: &str, params: Value) -> Value {
        assert!(unlocked(), "Stop native work while Windows is locked");
        self.id += 1;
        let request = json!({"id":self.id,"method":method,"params":params});
        writeln!(self.stream.get_mut(), "{request}").unwrap();
        let mut reply = String::new();
        self.stream
            .by_ref()
            .take(2 * 1024 * 1024)
            .read_line(&mut reply)
            .unwrap();
        assert!(
            reply.ends_with('\n'),
            "Native reply missing or oversized; do not retry"
        );
        let reply: Value = serde_json::from_str(&reply).unwrap();
        self.calls.push(json!({"request":request,"reply":reply}));
        assert_eq!(reply["id"], self.id);
        assert_eq!(
            reply["ok"], true,
            "Native request refused; private evidence retained"
        );
        reply["result"].clone()
    }
    fn execute(&mut self, command: &str, params: Value) -> Value {
        self.call("engine.execute", json!({"command":command,"params":params}))
    }
    fn drawing(&mut self) -> Value {
        self.call("drawing.inspect", json!({"entities":true,"limit":128}))
    }
}

fn lisp_binding_probe(client: &mut Client, root: &Path, before: &Value) -> Value {
    let token=uuid::Uuid::new_v4().to_string();
    let capture=format!("(progn (setq hazlomexa-token \"{token}\" hazlomexa-before (entget (handent \"100\"))) \"captured\")");
    let guard=|expected:&str,x:f64|format!("(if (and (equal hazlomexa-token \"{expected}\") (equal hazlomexa-before (entget (handent \"100\")))) (progn (entmod (subst (cons 10 (list {x} 0.0 0.0)) (assoc 10 hazlomexa-before) hazlomexa-before)) \"applied\") \"refused\")");
    let eval=|client:&mut Client,code:String| {
        let result=client.execute("lisp",json!({"code":code}));
        assert!(result.get("error").is_none(),"Native LISP failed; retain private evidence");
        result["value"].clone()
    };
    assert_eq!(eval(client,capture.clone()),json!("\"captured\""));
    assert_eq!(client.drawing()["entities"],before["entities"],"Binding must not change geometry");
    assert_eq!(eval(client,guard(&uuid::Uuid::new_v4().to_string(),999.)),json!("\"refused\""));
    assert_eq!(client.drawing()["entities"],before["entities"]);
    assert_eq!(eval(client,guard(&token,1.)),json!("\"applied\""));
    let first=client.drawing();
    let mut expected=before["entities"].clone();
    expected[0]["geometry"]["center"]["x"]=json!(1.);
    assert_eq!(first["entities"],expected);
    assert_eq!(eval(client,guard(&token,999.)),json!("\"refused\""),"Changed target must refuse the old snapshot");
    assert_eq!(client.drawing()["entities"],first["entities"]);
    assert_eq!(eval(client,capture),json!("\"captured\""));
    assert_eq!(eval(client,guard(&token,1.+0.1*25.4)),json!("\"applied\""));
    let moved=client.drawing();
    expected[0]["geometry"]["center"]["x"]=json!(3.54);
    assert_eq!(moved["entities"],expected);
    assert_eq!(moved["counts"],before["counts"]);
    assert_eq!(moved["layers"],before["layers"]);
    assert_eq!(moved["dirty"],true);
    for previous in [&first,before] {
        client.execute("undo",json!({"count":1}));
        assert_eq!(client.drawing()["entities"],previous["entities"],"One native Undo per LISP edit");
    }
    client.execute("close",json!({}));
    client.call("app.open",json!({"path":root.join("OwnedDrawing.dxf")}));
    assert_eq!(client.drawing()["entities"],before["entities"]);
    assert_eq!(eval(client,guard(&token,999.)),json!("\"refused\""),"Reopened identical source must not inherit the old binding");
    assert_eq!(client.drawing()["entities"],before["entities"]);
    json!({"passed":true,"cadcraft_version":"0.7.0","native_lisp":true,"document_scoped_binding":true,"same_call_target_guard":true,
        "wrong_token_refused":true,"stale_target_refused":true,"reopened_document_refused":true,
        "metric_and_inch_moves":true,"native_undo":true,"saved_bytes_preserved":true,
        "whole_drawing_guard":false,"production_writer_enabled":false,"hazlo_approval":false,"real_provider":false})
}

fn main() {
    assert!(unlocked(), "An active unlocked desktop is required");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let lisp=std::env::var("HAZLOMEXA_CADCRAFT_LISP_PROBE").as_deref()==Ok("1");
    let runtime = workspace.join(if lisp {"target/cadcraft-lisp-probe/runtime/cadcraft-0.7.0-windows-x64-portable/cadcraft.exe"}
        else {"target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft.exe"});
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&runtime).unwrap())),
        if lisp {"d92dd49cfc81521480d65c30d0abbc91b12b28be1fe95e0586c63d8d6c8544f5"}
        else {"339373dde0fa465f4bb3da23a7da48096329214587b4e363558f9931369d0982"}
    );
    let root = workspace.join(format!(
        "target/cadcraft-native-ui-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&root).unwrap();
    assert_eq!(
        root.canonicalize().unwrap().parent(),
        Some(workspace.join("target").canonicalize().unwrap().as_path())
    );
    let source =
        fs::read(workspace.join("docs/research/cadcraft-runtime-2026-10-11/metric.dxf")).unwrap();
    let drawing = root.join("OwnedDrawing.dxf");
    fs::write(&drawing, &source).unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    fs::write(
        workspace.join(if lisp {"target/current-cadcraft-lisp-ui.json"} else {"target/current-cadcraft-live-ui.json"}),
        serde_json::to_vec_pretty(&json!({
            "root":root,"port":address.port(),"owned":true,"production_writer_enabled":false
        }))
        .unwrap(),
    )
    .unwrap();
    let mut env = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATH"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for (key, path) in [
        ("CADCRAFT_CONFIG_DIR", root.join("config")),
        ("APPDATA", root.join("appdata")),
        ("LOCALAPPDATA", root.join("local")),
        ("USERPROFILE", root.join("profile")),
        ("TEMP", root.join("temp")),
        ("TMP", root.join("temp")),
    ] {
        fs::create_dir_all(&path).unwrap();
        env.insert(key.into(), path.into_os_string());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let process_root = root.clone();
    let process = thread::spawn(move || {
        run_owned(
            &runtime,
            &[
                "--control".into(),
                address.port().to_string().into(),
                drawing.into_os_string(),
            ],
            &process_root,
            &env,
            signal,
            Duration::from_secs(120),
        )
    });
    let mut calls = Vec::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let deadline = Instant::now();
        let stream = loop {
            assert!(unlocked());
            if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
                break stream;
            }
            assert!(
                !process.is_finished() && deadline.elapsed() < Duration::from_secs(30),
                "Owned GUI did not become ready"
            );
            thread::sleep(Duration::from_millis(100));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut client = Client {
            stream: BufReader::new(stream),
            id: 0,
            calls: Vec::new(),
        };
        let evidence = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let before = client.drawing();
            fs::write(
                root.join("before.json"),
                serde_json::to_vec_pretty(&before).unwrap(),
            )
            .unwrap();
            let ui = client.call("ui.inspect", json!({}));
            fs::write(
                root.join("ui-before.json"),
                serde_json::to_vec_pretty(&ui).unwrap(),
            )
            .unwrap();
            let catalog = client.call("engine.commands", json!({}));
            assert!(catalog
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "move"));
            let units = client.execute("units", json!({}));
            fs::write(
                root.join("units.json"),
                serde_json::to_vec_pretty(&units).unwrap(),
            )
            .unwrap();
            assert_eq!(units["insunits"], 4);
            assert_eq!(before["counts"], json!({"Circle":1}));
            assert_eq!(before["entities"][0]["handle"], "100");
            assert_eq!(before["entities"][0]["geometry"]["center"]["x"], 0.);
            assert_eq!(before["entities"][0]["geometry"]["radius"], 10.);
            if lisp {
                let active=ui["session"]["documents"].as_array().unwrap().iter().filter(|d|d["active"]==true).collect::<Vec<_>>();
                assert_eq!(active.len(),1);
                assert_eq!(active[0]["path"],json!(root.join("OwnedDrawing.dxf")));
            } else {assert_eq!(ui["session"]["documents"].as_array().unwrap().len(), 1);}
            assert_eq!(ui["closePrompt"], Value::Null);
            if lisp {return lisp_binding_probe(&mut client,&root,&before);}
            let mut states = Vec::new();
            for delta in [1., 0.1 * 25.4] {
                client.execute("move", json!({"handles":["100"],"delta":[delta,0.]}));
                states.push(client.drawing());
            }
            let moved = states.last().unwrap();
            assert!(
                (moved["entities"][0]["geometry"]["center"]["x"]
                    .as_f64()
                    .unwrap()
                    - 3.54)
                    .abs()
                    < 1e-12
            );
            assert_eq!(moved["dirty"], true);
            let mut expected = before["entities"].clone();
            expected[0]["geometry"]["center"]["x"] =
                moved["entities"][0]["geometry"]["center"]["x"].clone();
            assert_eq!(
                moved["entities"], expected,
                "Only the owned circle's X coordinate may change"
            );
            fs::write(
                root.join("ready-for-observation.json"),
                b"{\"moved_x_mm\":3.54}",
            )
            .unwrap();
            let waiting = Instant::now();
            while !root.join("continue-native-undo").exists()
                && waiting.elapsed() < Duration::from_secs(30)
            {
                assert!(unlocked());
                thread::sleep(Duration::from_millis(100));
            }
            for previous in [&states[0], &before] {
                client.execute("undo", json!({"count":1}));
                let restored = client.drawing();
                assert_eq!(restored["entities"], previous["entities"]);
                assert_eq!(restored["counts"], previous["counts"]);
                assert_eq!(restored["layers"], previous["layers"]);
            }
            client.execute("close", json!({}));
            client.call("app.open", json!({"path":root.join("OwnedDrawing.dxf")}));
            let reopened = client.call("ui.inspect", json!({}));
            assert_eq!(reopened["session"]["documents"],ui["session"]["documents"],
                "This release's public document summary cannot distinguish a close/reopen of the same source");
            assert_eq!(client.drawing()["entities"], before["entities"]);
            json!({"passed":true,"native_read":true,"metric_and_inch_moves":true,"native_undo":true,
                "reopened_document_summary_collision":true,"saved_bytes_preserved":true,
                "production_writer_enabled":false,"hazlo_approval":false,"real_provider":false})
        }));
        calls = client.calls;
        evidence.unwrap()
    }));
    cancel.store(true, Ordering::SeqCst);
    let stopped = process.join().unwrap().unwrap();
    fs::write(root.join("stdout.log"), stopped.stdout).unwrap();
    fs::write(root.join("stderr.log"), stopped.stderr).unwrap();
    fs::write(
        root.join("calls.json"),
        serde_json::to_vec_pretty(&calls).unwrap(),
    )
    .unwrap();
    assert_eq!(fs::read(root.join("OwnedDrawing.dxf")).unwrap(), source);
    fs::write(
        root.join("status.json"),
        serde_json::to_vec_pretty(&json!({"passed":result.is_ok(),"process_stopped":true}))
            .unwrap(),
    )
    .unwrap();
    let evidence = result.unwrap();
    fs::write(
        root.join("native-result.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!("Owned CADCraft native move/undo/reopen probe passed; no product writer enabled");
}
