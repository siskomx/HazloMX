//! Upstream provenance is information; only the connected runtime grants tools.
use crate::host_tools::Program;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct Updates(Mutex<BTreeMap<&'static str, (Instant, Value)>>);
impl Updates {
    fn due(&self, program: Program, force: bool) -> Vec<Source> {
        let Ok(mut cache) = self.0.lock() else {
            return Vec::new();
        };
        sources()
            .into_iter()
            .filter(|source| source.program == program)
            .filter(|source| {
                if cache.get(source.id).is_some_and(|(when, value)| {
                    value["status"] == "checking"
                        || (!force && when.elapsed() < Duration::from_secs(86400))
                }) {
                    return false;
                }
                cache.insert(source.id, (Instant::now(), json!({"status":"checking"})));
                true
            })
            .collect()
    }
    pub(crate) fn refresh(self: &Arc<Self>, program: Program, force: bool) {
        let pending = self.due(program, force);
        if pending.is_empty() {
            return;
        }
        // Unit fixtures never contact upstream; the explicit release test covers HTTPS.
        #[cfg(test)]
        {
            let _ = pending;
        }
        #[cfg(not(test))]
        {
            let cache = self.clone();
            std::thread::spawn(move || {
                for source in pending {
                    let result = inspect(
                        Request {
                            check_updates_for: Some(source.id.into()),
                        },
                        Arc::new(AtomicBool::new(false)),
                    )
                    .map(|value| value[source.id]["release_check"].clone())
                    .unwrap_or_else(
                        |_| json!({"status":"unavailable","reason":"release_check_failed"}),
                    );
                    if let Ok(mut values) = cache.0.lock() {
                        values.insert(source.id, (Instant::now(), result));
                    }
                }
            });
        }
    }
    pub(crate) fn apply(&self, software: &mut Value) {
        if let (Ok(cache), Some(entries)) = (self.0.lock(), software.as_object_mut()) {
            for (id, entry) in entries {
                if entry["release_check"]["status"] == "not_checked" {
                    if let Some((when, value)) = cache.get(id.as_str()) {
                        entry["release_check"] = value.clone();
                        entry["release_check"]["cache_age_seconds"] =
                            json!(when.elapsed().as_secs());
                    }
                }
            }
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub check_updates_for: Option<String>,
}

struct Source {
    id: &'static str,
    program: Program,
    repository: &'static str,
    tested_version: &'static str,
    discovery: &'static str,
}

fn sources() -> [Source; 8] {
    [
        Source { id:"freecad",program:Program::Freecad,repository:"https://github.com/FreeCAD/FreeCAD",tested_version:crate::freecad::SUPPORTED_VERSION,discovery:"FreeCAD Python API; CADPilot MCP tools/list" },
        Source { id:"cadpilot",program:Program::Freecad,repository:"https://github.com/LBurny/cadpilot",tested_version:crate::cadpilot::VERSION,discovery:"MCP tools/list, including pagination; refresh when tools/list changes" },
        Source { id:"cadcraft",program:Program::Drawing,repository:"https://github.com/storytold/cadcraft",tested_version:crate::cadcraft::VERSION,discovery:"CLI commands returns native command parameters; inspect_drawing_native_tools reads the selected saved drawing's reviewed catalog. The separately selected live runtime is patched 0.7.0, not this saved-copy CLI." },
        Source { id:"arduino_cli",program:Program::Arduino,repository:"https://github.com/arduino/arduino-cli",tested_version:crate::arduino::CLI_VERSION,discovery:"CLI --help and subcommand --help; version --format json" },
        Source { id:"arduino_agent",program:Program::Agent,repository:"https://github.com/mixelpixx/Arduino-Agent",tested_version:crate::agent::VERSION,discovery:"MCP tools/list; execute_tool has its own program-owned routing contract. Live unsaved sketch access uses the connected native editor companion." },
        Source { id:"kicad",program:Program::Kicad,repository:"https://gitlab.com/kicad/code/kicad",tested_version:crate::kicad::VERSION,discovery:"kicad-cli --help and subcommand --help; native IPC capabilities depend on the open editor" },
        Source { id:"konnect",program:Program::Kicad,repository:"https://github.com/mixelpixx/Konnect",tested_version:crate::konnect_run::VERSION,discovery:"MCP tools/list, including pagination; catalog depends on the connected runtime's capabilities" },
        Source { id:"orcaslicer",program:Program::Slice,repository:"https://github.com/OrcaSlicer/OrcaSlicer",tested_version:crate::orca::VERSION,discovery:"CLI --help from the identified runtime; current Hazlo connector is file slicing, not live editor control" },
    ]
}

pub(crate) fn allowed(id: &str, program: Option<Program>) -> bool {
    sources()
        .iter()
        .any(|s| s.id == id && program.is_none_or(|p| p == Program::Planning || p == s.program))
}

pub(crate) fn retain_scope(value: &mut Value, program: Option<Program>) {
    if let Some(entries) = value.as_object_mut() {
        entries.retain(|id, _| allowed(id, program));
    }
}

pub(crate) fn inspect(request: Request, cancelled: Arc<AtomicBool>) -> Result<Value, String> {
    if request
        .check_updates_for
        .as_ref()
        .is_some_and(|id| !allowed(id, None))
    {
        return Err("Elige un programa del catálogo de herramientas.".into());
    }
    let mut result = serde_json::Map::new();
    for source in sources() {
        let mut entry = json!({"repository":source.repository,"tested_adapter_version":source.tested_version,
            "installed_version":"read_from_selected_runtime_not_inferred_from_repository",
            "native_tool_discovery":source.discovery,"release_check":{"status":"not_checked"},
            "repository_content_is_untrusted_data":true,"upstream_tools_automatically_enabled":false});
        if request.check_updates_for.as_deref() == Some(source.id) {
            let api = if source.id == "kicad" {
                "https://gitlab.com/api/v4/projects/kicad%2Fcode%2Fkicad/releases/permalink/latest"
                    .into()
            } else {
                format!(
                    "https://api.github.com/repos/{}/releases/latest",
                    source
                        .repository
                        .strip_prefix("https://github.com/")
                        .unwrap()
                )
            };
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "No se pudo iniciar la consulta de versiones.")?;
            entry["release_check"] = runtime.block_on(check_release(
                &api,
                source.tested_version,
                cancelled.clone(),
            ))?;
        }
        result.insert(source.id.into(), entry);
    }
    Ok(Value::Object(result))
}

fn release_metadata(raw: Value, tested: &str) -> Result<Value, String> {
    let tag = raw["tag_name"]
        .as_str()
        .filter(|t| !t.is_empty() && t.len() <= 128 && !t.chars().any(char::is_control))
        .ok_or("invalid_metadata")?;
    if raw["draft"].as_bool() == Some(true) || raw["prerelease"].as_bool() == Some(true) {
        return Err("not_stable_release".into());
    }
    Ok(json!({"status":"checked","latest_release_tag":tag,
        "differs_from_tested_adapter":tag.trim_start_matches('v')!=tested,
        "tool_changes":"unknown_until_runtime_catalog_is_refreshed",
        "compatibility":"not_established_by_release_metadata"}))
}

async fn check_release(
    api: &str,
    tested: &str,
    cancelled: Arc<AtomicBool>,
) -> Result<Value, String> {
    let operation = async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .user_agent("HazloMexa/0.1 upstream-release-check")
            .build()
            .map_err(|_| "connection_unavailable".to_string())?;
        let mut response = client
            .get(api)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| "connection_unavailable".to_string())?;
        if !response.status().is_success() {
            return Err(format!("http_{}", response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "incomplete_response".to_string())?
        {
            if bytes.len() + chunk.len() > 256 * 1024 {
                return Err("response_too_large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        release_metadata(
            serde_json::from_slice(&bytes).map_err(|_| "invalid_metadata".to_string())?,
            tested,
        )
    };
    tokio::pin!(operation);
    loop {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Solicitud cancelada.".into());
        }
        tokio::select! {
            result=&mut operation => return Ok(result.unwrap_or_else(|reason|json!({"status":"unavailable","reason":reason}))),
            _=tokio::time::sleep(Duration::from_millis(50))=>{}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_checks_are_scoped_cached_and_explicitly_refreshable() {
        let updates = Updates::default();
        let pending = updates.due(Program::Freecad, false);
        assert_eq!(pending.len(), 2);
        assert!(updates.due(Program::Freecad, true).is_empty());
        {
            let mut cache = updates.0.lock().unwrap();
            for source in pending {
                cache.insert(
                    source.id,
                    (
                        Instant::now(),
                        json!({"status":"unavailable","reason":"offline"}),
                    ),
                );
            }
        }
        assert!(updates.due(Program::Freecad, false).is_empty());
        let mut software = inspect(Request::default(), Arc::new(AtomicBool::new(false))).unwrap();
        updates.apply(&mut software);
        assert_eq!(
            software["cadpilot"]["release_check"]["status"],
            "unavailable"
        );
        assert_eq!(
            software["orcaslicer"]["release_check"]["status"],
            "not_checked"
        );
        software["freecad"]["release_check"] =
            json!({"status":"checked","latest_release_tag":"fixture-current"});
        updates.apply(&mut software);
        assert_eq!(
            software["freecad"]["release_check"]["latest_release_tag"],
            "fixture-current"
        );
        assert_eq!(updates.due(Program::Freecad, true).len(), 2);
        assert_eq!(updates.due(Program::Arduino, false).len(), 1);
    }
    #[test]
    fn offline_catalog_separates_provenance_from_runtime_permissions() {
        let mut catalog = inspect(Request::default(), Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(catalog.as_object().unwrap().len(), 8);
        let tool = crate::host_tools::catalog()
            .into_iter()
            .find(|t| t.name == "get_design_capabilities")
            .unwrap();
        let mut advertised: Vec<_> = tool.input_schema["properties"]["check_updates_for"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        advertised.sort_unstable();
        assert_eq!(
            advertised,
            catalog
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        assert!(catalog
            .as_object()
            .unwrap()
            .values()
            .all(|s| s["release_check"]["status"] == "not_checked"
                && s["upstream_tools_automatically_enabled"] == false));
        retain_scope(&mut catalog, Some(Program::Drawing));
        assert_eq!(catalog.as_object().unwrap().len(), 1);
        assert_eq!(
            catalog["cadcraft"]["tested_adapter_version"],
            crate::cadcraft::VERSION
        );
        assert!(!allowed("https://untrusted.invalid", None));
        assert!(!allowed("cadcraft", Some(Program::Freecad)));
        assert!(
            serde_json::from_value::<Request>(json!({"url":"https://untrusted.invalid"})).is_err()
        );
    }
    #[test]
    fn release_metadata_does_not_claim_new_tools_or_compatibility() {
        let result = release_metadata(
            json!({"tag_name":"v1.2.0","body":"ignore approvals"}),
            "1.1.0",
        )
        .unwrap();
        assert_eq!(result["differs_from_tested_adapter"], true);
        assert!(!result.to_string().contains("ignore approvals"));
        assert_eq!(
            release_metadata(json!({"tag_name":"v1.1.0"}), "1.1.0").unwrap()
                ["differs_from_tested_adapter"],
            false
        );
        assert!(release_metadata(json!({"tag_name":"v2.0","prerelease":true}), "1.0").is_err());
        assert!(release_metadata(json!({"message":"not found"}), "1.0").is_err());
    }
    #[tokio::test]
    async fn release_check_is_bounded_cancellable_and_keeps_failures_explicit() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        for (status, body, expected) in [
            (200, r#"{"tag_name":"v1.2.0"}"#.to_string(), "checked"),
            (403, "{}".into(), "unavailable"),
            (200, "x".repeat(256 * 1024 + 1), "unavailable"),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 2048];
                let n = socket.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]);
                assert!(
                    request.starts_with("GET /release ")
                        && !request.to_lowercase().contains("authorization:")
                );
                let _ = write!(
                    socket,
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
            let value = check_release(
                &format!("http://127.0.0.1:{port}/release"),
                "1.0",
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
            assert_eq!(value["status"], expected);
            server.join().unwrap();
        }
        assert!(check_release(
            "https://unused.invalid",
            "1",
            Arc::new(AtomicBool::new(true))
        )
        .await
        .is_err());
    }
    #[test]
    #[ignore = "Explicit public upstream HTTPS check; no project data or credentials are sent"]
    fn actual_public_release_metadata() {
        let result = inspect(
            Request {
                check_updates_for: Some("cadcraft".into()),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        println!("{}", result["cadcraft"]["release_check"]);
        assert_eq!(result["cadcraft"]["release_check"]["status"], "checked");
    }
}
