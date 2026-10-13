use hazlomexa::{
    ollama::{generation_request, Decoder, OllamaClient, Proposal},
    process::run_owned,
    Dimensions,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Server {
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

async fn metadata(
    client: &reqwest::Client,
    url: String,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut response = client.get(url).send().await?.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("metadata limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 3 {
        return Err(
            "usage: ollama_benchmark <ollama.exe> <private-runtime-root> <new-output-dir> [--pull | --cloud model:cloud | --local model [--pull] | --case fixture-id | --cancel-only]"
                .into(),
        );
    }
    let executable = PathBuf::from(&args[0]);
    let runtime = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let cloud = args.get(3).is_some_and(|value| value == "--cloud");
    let cloud_name = if cloud {
        Some(
            args.get(4)
                .ok_or("missing cloud model")?
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };
    let local_name = if args.get(3).is_some_and(|value| value == "--local") {
        Some(
            args.get(4)
                .ok_or("missing local model")?
                .to_string_lossy()
                .into_owned(),
        )
    } else {
        None
    };
    let model = cloud_name
        .as_deref()
        .or(local_name.as_deref())
        .unwrap_or("qwen3:4b");
    if cloud && !model.ends_with(":cloud") {
        return Err("explicit cloud tag required".into());
    }
    if !executable.is_absolute() || !runtime.is_absolute() || !output.is_absolute() {
        return Err("absolute paths required".into());
    }
    fs::create_dir_all(&runtime)?;
    fs::create_dir(&output)?;
    for name in ["home", "models", "temp"] {
        fs::create_dir_all(runtime.join(name))?;
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let base = format!("http://127.0.0.1:{port}");
    let mut env = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(OsString::from(key), value);
        }
    }
    for (key, value) in [
        ("USERPROFILE", runtime.join("home").into_os_string()),
        ("HOME", runtime.join("home").into_os_string()),
        ("OLLAMA_MODELS", runtime.join("models").into_os_string()),
        ("TEMP", runtime.join("temp").into_os_string()),
        ("TMP", runtime.join("temp").into_os_string()),
        ("OLLAMA_HOST", base.clone().into()),
        ("OLLAMA_NO_CLOUD", "1".into()),
        ("OLLAMA_VULKAN", "0".into()),
        ("OLLAMA_NUM_PARALLEL", "1".into()),
    ] {
        env.insert(key.into(), value);
    }
    if cloud {
        let home =
            PathBuf::from(std::env::var_os("USERPROFILE").ok_or("missing Windows user profile")?);
        if !home.join(".ollama/id_ed25519").is_file() {
            return Err("Sign in to Ollama before running the explicit cloud benchmark.".into());
        }
        env.insert("USERPROFILE".into(), home.clone().into_os_string());
        env.insert("HOME".into(), home.clone().into_os_string());
        env.insert(
            "OLLAMA_MODELS".into(),
            home.join(".ollama/models").into_os_string(),
        );
        env.insert("OLLAMA_NO_CLOUD".into(), "0".into());
        env.insert("OLLAMA_NOPRUNE".into(), "1".into());
        env.insert("OLLAMA_REMOTES".into(), "ollama.com".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let log_dir = output.clone();
    let worker = thread::spawn(move || {
        match run_owned(
            &executable,
            &["serve".into()],
            &runtime,
            &env,
            flag,
            Duration::from_secs(7200),
        ) {
            Ok(result) => {
                let _ = write_new(&log_dir.join("server.stdout.log"), &result.stdout);
                let _ = write_new(&log_dir.join("server.stderr.log"), &result.stderr);
            }
            Err(error) => {
                let _ = write_new(
                    &log_dir.join("server.error.txt"),
                    error.to_string().as_bytes(),
                );
            }
        }
    });
    let server = Server {
        cancel,
        worker: Some(worker),
    };
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .read_timeout(Duration::from_secs(120))
        .timeout(Duration::from_secs(7200))
        .build()?;
    let started = Instant::now();
    let version = loop {
        match metadata(&client, format!("{base}/api/version")).await {
            Ok(value) => break value,
            Err(_) if started.elapsed() < Duration::from_secs(15) => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(error) => return Err(error),
        }
    };
    println!("Owned Ollama ready: {version}");
    if version["version"] != "0.35.1" {
        return Err("untested Ollama version".into());
    }
    if args.iter().any(|value| value == "--pull") {
        let mut pull = client
            .post(format!("{base}/api/pull"))
            .json(&json!({"model":model,"stream":true}))
            .send()
            .await?
            .error_for_status()?;
        let mut pending = Vec::new();
        let mut last_percent = 0;
        let mut last_digest = String::new();
        let mut success = false;
        while let Some(chunk) = pull.chunk().await? {
            pending.extend_from_slice(&chunk);
            if pending.len() > 65536 {
                return Err("pull record limit".into());
            }
            while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
                let value: Value = serde_json::from_slice(&pending[..end])?;
                pending.drain(..=end);
                if value.get("error").is_some() {
                    return Err("model pull failed; inspect owned server log".into());
                }
                success |= value["status"] == "success";
                if let Some(digest) = value["digest"].as_str() {
                    if digest != last_digest {
                        last_digest = digest.into();
                        last_percent = 0;
                    }
                }
                if let (Some(done), Some(total)) =
                    (value["completed"].as_u64(), value["total"].as_u64())
                {
                    let percent = done.saturating_mul(100) / total.max(1);
                    if percent >= last_percent + 10 {
                        println!("Model download: {percent}%");
                        last_percent = percent;
                    }
                }
            }
        }
        if !success {
            return Err("model pull did not finish".into());
        }
    }
    let tags = metadata(&client, format!("{base}/api/tags")).await?;
    write_new(
        &output.join("version.json"),
        &serde_json::to_vec_pretty(&version)?,
    )?;
    write_new(
        &output.join("tags.json"),
        &serde_json::to_vec_pretty(&tags)?,
    )?;
    let entry = tags["models"]
        .as_array()
        .ok_or("missing model list")?
        .iter()
        .find(|entry| entry["name"] == model)
        .ok_or("local model absent; use --pull explicitly")?;
    let digest = entry["digest"].as_str().ok_or("missing digest")?;
    let local = if cloud {
        OllamaClient::cloud(port, model, digest)?
    } else {
        OllamaClient::new(port, model, digest)?
    };
    let show_response = client
        .post(format!("{base}/api/show"))
        .json(&json!({"model":model}))
        .send()
        .await?;
    let show_status = show_response.status().as_u16();
    let show: Value = show_response.json().await?;
    write_new(
        &output.join("show.json"),
        &serde_json::to_vec_pretty(&json!({"status":show_status,"metadata":show}))?,
    )?;
    if let Err(error) = local.check(&AtomicBool::new(false)).await {
        write_new(
            &output.join("preflight-error.json"),
            &serde_json::to_vec_pretty(&json!({"error":error}))?,
        )?;
        return Err(error.into());
    }
    if args.get(3).is_some_and(|value| value == "--cancel-only") {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut cancel_time = None;
        let mut delta_bytes = 0;
        let result = local
            .propose(
                "Un bloque sólido rectangular de largo 10 mm, ancho 20 mm y alto 30 mm.",
                cancelled.clone(),
                |delta| {
                    delta_bytes += delta.len();
                    if cancel_time.is_none() {
                        cancel_time = Some(Instant::now());
                        cancelled.store(true, Ordering::SeqCst);
                    }
                },
            )
            .await;
        let elapsed_ms = cancel_time
            .ok_or("No streamed delta; cancellation was not exercised")?
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        if result.as_ref().err().map(String::as_str) != Some("Generación cancelada.")
            || elapsed_ms >= 1000.0
        {
            return Err(format!("Cancellation failed: {result:?}, {elapsed_ms} ms").into());
        }
        write_new(
            &output.join("cancellation-client.json"),
            &serde_json::to_vec_pretty(&json!({
                "model":model,"digest":digest,"runtime":version,"pass":true,"delta_bytes":delta_bytes,
                "client_cancel_ms":elapsed_ms,"error":result.unwrap_err(),"engine_cessation":"requires independent runner observation"
            }))?,
        )?;
        println!("Client cancelled after a real delta in {elapsed_ms:.2} ms; observing live runner for 8 seconds",);
        tokio::time::sleep(Duration::from_secs(8)).await;
        metadata(&client, format!("{base}/api/version")).await?;
        drop(server);
        return Ok(());
    }
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/spanish-solid-proposals.json"
    ))?;
    let design_fixtures: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/design-conversations.json"
    ))?;
    let mut all_cases = fixtures["cases"].as_array().ok_or("missing cases")?.clone();
    all_cases.extend(
        design_fixtures["cases"]
            .as_array()
            .ok_or("missing design cases")?
            .iter()
            .cloned(),
    );
    let selected = if args.get(3).is_some_and(|value| value == "--case") {
        Some(args.get(4).ok_or("missing case id")?.to_string_lossy())
    } else {
        None
    };
    let cases: Vec<_> = all_cases
        .iter()
        .filter(|case| {
            selected
                .as_ref()
                .is_none_or(|id| case["id"].as_str() == Some(id.as_ref()))
        })
        .collect();
    if cases.is_empty() {
        return Err("unknown fixture id".into());
    }
    let mut records = Vec::new();
    let mut warm_times = Vec::new();
    let mut passed = 0;
    for (index, case) in cases.iter().enumerate() {
        let start = Instant::now();
        let mut generated_text = String::new();
        let result = local
            .propose(
                case["prompt"].as_str().unwrap(),
                Arc::new(AtomicBool::new(false)),
                |delta| generated_text.push_str(&delta),
            )
            .await;
        if let Err(error) = &result {
            write_new(
                &output.join(format!("generation-error-{index:02}.json")),
                &serde_json::to_vec_pretty(
                    &json!({"id":case["id"],"error":error,"generated_text":generated_text}),
                )?,
            )?;
        }
        let seconds = start.elapsed().as_secs_f64();
        if index > 0 {
            warm_times.push(seconds);
        }
        let acceptable = match &result {
            Ok(Proposal::Clarification { .. }) => case["expected"] == "clarification",
            Ok(Proposal::Proposal { dimensions_mm }) => {
                serde_json::from_value::<Dimensions>(case["expected"].clone()).ok()
                    == Some(*dimensions_mm)
            }
            Err(_) => false,
        };
        passed += usize::from(acceptable);
        println!(
            "{}: {} in {:.2}s",
            case["id"],
            if acceptable { "PASS" } else { "FAIL" },
            seconds
        );
        let result = match result {
            Ok(value) => json!({"proposal":value}),
            Err(error) => json!({"error":error}),
        };
        let record = json!({"id":case["id"],"expected":case["expected"],"result":result,"acceptable":acceptable,"seconds":seconds,"cold":index==0});
        write_new(
            &output.join(format!("case-{index:02}.json")),
            &serde_json::to_vec_pretty(&record)?,
        )?;
        records.push(record);
    }
    let process_status = metadata(&client, format!("{base}/api/ps")).await?;
    for index in 0..3 {
        let start = Instant::now();
        local
            .propose(
                cases[0]["prompt"].as_str().unwrap(),
                Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .await?;
        warm_times.push(start.elapsed().as_secs_f64());
        println!(
            "Warm repeat {}: {:.2}s",
            index + 1,
            warm_times.last().unwrap()
        );
    }
    let mut request = generation_request(model, cases[0]["prompt"].as_str().unwrap())?;
    if cloud {
        request.as_object_mut().unwrap().remove("format");
    }
    let mut response = client
        .post(format!("{base}/api/generate"))
        .json(&request)
        .send()
        .await?
        .error_for_status()?;
    let mut wire = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if wire.len() + chunk.len() > 1024 * 1024 {
            return Err("capture limit".into());
        }
        wire.extend_from_slice(&chunk);
    }
    let mut decoder = if cloud {
        Decoder::cloud(model, model.strip_suffix(":cloud").unwrap())
    } else {
        Decoder::new(model)
    };
    decoder.push(&wire)?;
    let captured = decoder.finish()?;
    write_new(&output.join("explicit.ndjson"), &wire)?;
    warm_times.sort_by(f64::total_cmp);
    let p95 = warm_times[(warm_times.len() * 95).div_ceil(100) - 1];
    let median = (warm_times[(warm_times.len() - 1) / 2] + warm_times[warm_times.len() / 2]) / 2.0;
    let original_passed = records
        .iter()
        .take(fixtures["cases"].as_array().unwrap().len())
        .filter(|record| record["acceptable"] == true)
        .count();
    let report = json!({"schema_version":1,"model":entry,"runtime":version,"model_source":format!("https://ollama.com/library/{model}"),"license":if cloud { "cloud service; no bundled weights" } else if matches!(model, "qwen3:4b" | "qwen3.5:2b" | "qwen2.5:7b") { "Apache-2.0" } else { "not verified; check the selected model's license" },
        "server_profile":{"cloud_disabled":!cloud,"vulkan_enabled":false,"parallel":1,"model_storage":if cloud { "existing user cloud manifests; pruning disabled" } else { "separate HazloMexa proof directory" }},
        "request":request,"cases":records,"acceptable":passed,"total":cases.len(),"warm_seconds":warm_times,
        "warm_median_seconds":median,"warm_p95_seconds":p95,"ps":process_status,"capture_result":captured,
        "full_fixture_run":selected.is_none(),"quality_target_met":selected.is_none() && original_passed>=18 && passed*10>=cases.len()*9,"latency_target_met":p95<=30.0,"inference_cessation":"not yet measured","second_environment":"not yet tested"});
    write_new(
        &output.join("report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Result: {passed}/{}; warm p95 {p95:.2}s; evidence {}",
        cases.len(),
        output.display()
    );
    drop(server);
    Ok(())
}
