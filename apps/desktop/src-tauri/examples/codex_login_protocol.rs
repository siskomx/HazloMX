//! Credential-free account/login/cancellation gate; never opens the browser or starts inference.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::codex;
    use serde_json::json;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Instant,
    };
    let runtime = codex::identify(PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass installed codex.exe")?,
    ))?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let base = workspace
        .join("target/codex-login-probe")
        .join(uuid::Uuid::new_v4().to_string());
    let home = base.join("home");
    let directory = base.join("workspace");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&directory)?;
    fs::write(home.join("config.toml"), codex::BASE_CONFIG)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let mut observed = false;
    let mut callback_port = None;
    let mut began = None;
    let result = codex::authenticate(
        runtime.clone(),
        home.clone(),
        directory.clone(),
        cancelled.clone(),
        |view| {
            assert!(
                !view.connected,
                "Fresh private home unexpectedly has a signed-in account"
            );
            if let Some(url) = view.auth_url {
                assert!(codex::login_url(&url).is_ok());
                let url = reqwest::Url::parse(&url).unwrap();
                let redirect = url
                    .query_pairs()
                    .find(|(key, _)| key == "redirect_uri")
                    .unwrap()
                    .1
                    .to_string();
                let redirect = reqwest::Url::parse(&redirect).unwrap();
                assert_eq!(redirect.scheme(), "http");
                assert!(matches!(
                    redirect.host_str(),
                    Some("localhost" | "127.0.0.1")
                ));
                callback_port = redirect.port();
                observed = true;
                began = Some(Instant::now());
                cancelled.store(true, Ordering::SeqCst);
            }
        },
    )
    .await;
    let cancelled_ok = result
        .err()
        .is_some_and(|error| error == "Conexión detenida.");
    if !observed || !cancelled_ok {
        return Err("Login URL/cancellation boundary failed".into());
    }
    let latency = began.unwrap().elapsed().as_millis();
    let port = callback_port.ok_or("Missing native callback port")?;
    if tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
    {
        return Err("Owned callback server remains live".into());
    }
    let denied = codex::run(
        codex::TurnRequest {
            image: None,
                expected_tools: hazlomexa::host_tools::catalog().iter().map(|tool|tool.name.to_string()).collect(),
            runtime: runtime.clone(),
            home: home.clone(),
            directory,
            resume: None,
            prompt: "Protocol fixture only; never start inference.".into(),
            cancelled: Arc::new(AtomicBool::new(false)),
        },
        |_| panic!("Missing-login turn emitted a reply"),
    )
    .await;
    if denied
        .err()
        .is_none_or(|error| !error.starts_with("Inicia sesión en el cliente de Codex"))
    {
        return Err("Missing login was not rejected before thread/tool dispatch".into());
    }
    if home.join("auth.json").exists() {
        return Err("Cancelled credential-free login retained an auth file".into());
    }
    fs::write(
        base.join("status.json"),
        serde_json::to_vec_pretty(
            &json!({"version":runtime.version,"sha256":runtime.sha256,"official_auth_url":true,"browser_opened":false,"account_connected":false,"credential_file_created":false,"login_cancelled":true,"cancel_latency_ms":latency,"native_callback_port_closed":true,"missing_login_refused_before_turn":true,"model_turn_started":false,"global_configuration_changed":false}),
        )?,
    )?;
    // The URL's OAuth state/query is deliberately not retained.
    println!(
        "PASS: owned Codex login URL/cancel/missing-login boundary; evidence {}",
        base.display()
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows-only login gate");
}
