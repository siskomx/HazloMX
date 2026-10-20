use std::{env, fs, io::{self, Read, Write}, process::Command, thread, time::Duration};

fn main() {
    let arguments: Vec<_> = env::args().skip(1).collect();
    match arguments[0].as_str() {
        "input" => {
            let mut bytes = Vec::new();
            io::stdin().lock().take(256 * 1024 + 1).read_to_end(&mut bytes).unwrap();
            io::stdout().lock().write_all(&bytes).unwrap();
        }
        "leaf" => {
            fs::write("leaf.pid", std::process::id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "tree" => {
            let child = Command::new(env::current_exe().unwrap()).arg("leaf").spawn().unwrap();
            fs::write("child.pid", child.id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "tree-exit" => {
            let child = Command::new(env::current_exe().unwrap()).arg("leaf")
                .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null()).spawn().unwrap();
            fs::write("child.pid", child.id().to_string()).unwrap();
        }
        "noisy" => {
            let encoded = arguments[1..].iter().map(|argument| {
                let mut value = String::from("\"");
                for ch in argument.chars() {
                    if ch == '\\' || ch == '"' { value.push('\\'); }
                    value.push(ch);
                }
                value.push('"'); value
            }).collect::<Vec<_>>().join(",");
            fs::write("arguments.json", format!("[{encoded}]")).unwrap();
            let chunk = vec![b'x'; 8192];
            let mut stdout = io::stdout().lock();
            let mut stderr = io::stderr().lock();
            for _ in 0..128 {
                stdout.write_all(&chunk).unwrap();
                stderr.write_all(&chunk).unwrap();
            }
        }
        _ => panic!("Unknown proof mode"),
    }
}
