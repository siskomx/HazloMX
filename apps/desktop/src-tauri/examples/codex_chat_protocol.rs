//! Actual Codex tool-surface/failure gate against an owned loopback HTTP fixture, not a model.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use hazlomexa::{
        codex, host_tools::HostTools, jobs::Workflow, project::Project, tool_bridge::ToolBridge,
    };
    use serde_json::{json, Value};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    let executable = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass installed codex.exe")?,
    );
    let mode = std::env::args().nth(2).unwrap_or_else(|| "failure".into());
    let mut program: Option<hazlomexa::host_tools::Program> = if mode == "switch" { Some(hazlomexa::host_tools::Program::Freecad) } else { mode.strip_prefix("program-").map(|name| serde_json::from_value(json!(name))).transpose()? };
    if ![
        "success",
        "failure",
        "cancel",
        "resume",
        "switch",
        "questions",
        "question-cancel",
    ]
    .contains(&mode.as_str()) && program.is_none()
    {
        return Err("Use success, failure, cancel, resume, questions, question-cancel or program-<program>".into());
    }
    let question_cancel = mode == "question-cancel";
    let questions = mode == "questions" || question_cancel;
    let resume = mode == "resume" || mode == "switch";
    let success = mode == "success" || resume || questions || program.is_some();
    let cancel = mode == "cancel" || question_cancel;
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fixture_cancelled = cancelled.clone();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let base = workspace
        .join("target/codex-chat-probe")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(base.join("home"))?;
    fs::create_dir_all(base.join("project"))?;
    let workflow = Arc::new(Workflow::new(
        base.join("FreeCADCmd.exe"),
        base.join("profiles"),
    )?);
    let project = Project::create(&base.join("project"))?;
    let project_id = project.id;
    workflow.select_project(project)?;
    let project_scope = workflow.view()?.project.ok_or("Missing selected project")?;
    let host = Arc::new(HostTools::new(workflow.clone()));
    let helper = workspace.join("target/release/hazlomexa-tools.exe");
    let mut bridge = match program {
        Some(program) => ToolBridge::start_program(host.clone(), &helper, project_scope.clone(), program)?,
        None => ToolBridge::start(host.clone(), &helper)?,
    };
    let mut config = bridge.config()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let fixture = tokio::spawn(async move {
        let mut requests = Vec::new();
        for sequence in 0..if resume {
            3
        } else if questions && !question_cancel {
            2
        } else {
            1
        } {
            let request=async {
        let (socket, _) = listener.accept().await?;
        let mut socket = BufReader::new(socket);
        let mut header = Vec::new();
        loop {
            let mut line = Vec::new();
            (&mut socket)
                .take(8193)
                .read_until(b'\n', &mut line)
                .await?;
            if line.is_empty() || line.len() > 8192 || header.len() + line.len() > 65536 {
                return Err(std::io::Error::other("Invalid fixture HTTP headers"));
            }
            let end = line == b"\r\n";
            header.extend(line);
            if end {
                break;
            }
        }
        let header = String::from_utf8(header).map_err(std::io::Error::other)?;
        if !header.starts_with("POST /v1/responses ")
            || header.to_ascii_lowercase().contains("authorization:")
        {
            return Err(std::io::Error::other(
                "Unexpected fixture request or credentials",
            ));
        }
        let length = header
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>())
            })
            .ok_or_else(|| std::io::Error::other("Missing fixture content length"))?
            .map_err(std::io::Error::other)?;
        if length > 1024 * 1024 {
            return Err(std::io::Error::other("Fixture request too large"));
        }
        let mut body = vec![0; length];
        socket.read_exact(&mut body).await?;
        let request: Value = serde_json::from_slice(&body).map_err(std::io::Error::other)?;
        if cancel && !questions {
            let started = std::time::Instant::now();
            fixture_cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
            let mut byte = [0];
            let closed = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut byte))
                .await
                .map_err(|_| std::io::Error::other("Cancelled Codex kept the HTTP request open"))?;
            let closed = match closed {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => 0,
                Err(error) => return Err(error),
            };
            if closed != 0 {
                return Err(std::io::Error::other("Cancelled client sent more input"));
            }
            let mut request = request;
            request["fixture_cancel_to_socket_close_ms"] = json!(started.elapsed().as_millis());
            return Ok(request);
        }
        let error = br#"{"error":{"message":"Hazlo protocol fixture intentionally refuses inference","type":"invalid_request_error","code":"fixture_no_inference"}}"#;
        if (resume && sequence==1) || (questions && sequence==0) {
            let item=json!({"id":"fixture_call","type":"function_call","call_id":"fixture_call","name":"get_workflow_status","namespace":"mcp__hazlomexa","arguments":"{}"});
            let item = if questions { json!({"id":"fixture_question","type":"function_call","call_id":"fixture_question","name":"request_user_input","arguments":serde_json::to_string(&json!({"questions":[{"id":"units","header":"Units","question":"Which units should we use?","options":[{"label":"mm","description":"Metric"},{"label":"in","description":"Inches"}]}]})).unwrap()}) } else {item};
            let events=[
                json!({"type":"response.created","response":{"id":"fixture_tool_response","object":"response","status":"in_progress","output":[]}}),
                json!({"type":"response.output_item.added","output_index":0,"item":item}),
                json!({"type":"response.output_item.done","output_index":0,"item":item}),
                json!({"type":"response.completed","response":{"id":"fixture_tool_response","object":"response","status":"completed","output":[item],"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}),
            ];
            let body=events.iter().map(|event|format!("event: {}\ndata: {event}\n\n",event["type"].as_str().unwrap())).collect::<String>();
            socket.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await?;
            socket.get_mut().write_all(body.as_bytes()).await?;
            return Ok(request);
        }
        if success {
            let item = json!({"id":"fixture_message","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"¿Qué quieres cambiar?","annotations":[]}]});
            let events = [
                json!({"type":"response.created","response":{"id":"fixture_response","object":"response","status":"in_progress","output":[]}}),
                json!({"type":"response.output_item.added","output_index":0,"item":{"id":"fixture_message","type":"message","role":"assistant","status":"in_progress","content":[]}}),
                json!({"type":"response.output_text.delta","item_id":"fixture_message","output_index":0,"content_index":0,"delta":"¿Qué quieres "}),
                json!({"type":"response.output_text.delta","item_id":"fixture_message","output_index":0,"content_index":0,"delta":"cambiar?"}),
                json!({"type":"response.output_item.done","output_index":0,"item":item}),
                json!({"type":"response.completed","response":{"id":"fixture_response","object":"response","status":"completed","output":[item],"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}),
            ];
            let body = events
                .iter()
                .map(|event| {
                    format!(
                        "event: {}\ndata: {event}\n\n",
                        event["type"].as_str().unwrap()
                    )
                })
                .collect::<String>();
            socket.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await?;
            socket.get_mut().write_all(body.as_bytes()).await?;
            return Ok(request);
        }
        socket.get_mut().write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", error.len()).as_bytes()).await?;
        socket.get_mut().write_all(error).await?;
        Ok::<_, std::io::Error>(request)
        }.await?;
            requests.push(request);
        }
        Ok::<_, std::io::Error>(requests)
    });
    let fixture_settings =
        |config: &hazlomexa::tool_bridge::BridgeConfig| -> Result<String, String> {
            Ok(format!(
                r#"model = "hazlomexa-protocol-fixture"
model_provider = "hazlo_fixture"
{}
[model_providers.hazlo_fixture]
name = "Hazlo local protocol fixture"
base_url = "http://{address}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
"#,
                codex::configuration(config)?
            ))
        };
    let settings = fixture_settings(&config)?;
    fs::write(base.join("home/config.toml"), settings)?;
    let runtime = codex::identify(executable)?;
    let mut updates = Vec::new();
    let mut asked = Vec::new();
    let answer_cancelled = cancelled.clone();
    let mut result = codex::run_with_questions(
        codex::TurnRequest {
            image: None,
                expected_tools: bridge.tool_names().to_vec(),
            runtime: runtime.clone(),
            home: base.join("home"),
            directory: base.join("project"),
            resume: None,
            prompt: "Protocol fixture only; do not invoke a tool.".into(),
            cancelled,
        },
        |update| updates.push(update),
        |question| {
            asked.push(serde_json::to_value(&question.questions).unwrap());
            let answer_cancelled = answer_cancelled.clone();
            async move {
                if question_cancel {
                    answer_cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
                    return std::future::pending().await;
                }
                Ok(std::collections::BTreeMap::from([(
                    "units".into(),
                    "mm".into(),
                )]))
            }
        },
    )
    .await;
    let mut resumed_thread = None;
    if resume {
        let prior = result
            .as_ref()
            .map_err(|error| std::io::Error::other(error.clone()))?;
        if prior.view.error.is_some() {
            return Err("First fixture turn failed".into());
        }
        let thread_id = prior.thread_id.clone();
        bridge.close().await?;
        if hazlomexa::tool_bridge::connect(
            &config.env["HAZLOMEXA_PIPE"],
            &config.env["HAZLOMEXA_TOKEN"],
        )
        .await
        .is_ok()
        {
            return Err("Revoked connection still accepts calls".into());
        }
        bridge = if mode == "switch" {
            program = Some(hazlomexa::host_tools::Program::Arduino);
            ToolBridge::start_program(host, &helper, project_scope, program.unwrap())?
        } else { ToolBridge::start(host, &helper)? };
        let fresh = bridge.config()?;
        if fresh.env == config.env {
            return Err("Bridge capability was reused".into());
        }
        config = fresh;
        fs::write(base.join("home/config.toml"), fixture_settings(&config)?)?;
        result = codex::run(
            codex::TurnRequest {
            image: None,
                expected_tools: bridge.tool_names().to_vec(),
                runtime,
                home: base.join("home"),
                directory: base.join("project"),
                resume: Some(thread_id.clone()),
                prompt:
                    "Follow-up fixture: preserve the earlier conversation and read Hazlo status."
                        .into(),
                cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
            |update| updates.push(update),
        )
        .await;
        resumed_thread = Some(thread_id);
    }
    let checked = if cancel {
        match result {
            Err(error) if error == "Solicitud detenida." => Ok(()),
            _ => Err("Cancellation did not stop the owned turn".into()),
        }
    } else {
        result.and_then(|result| {
            if resumed_thread
                .as_ref()
                .is_some_and(|id| id != &result.thread_id)
            {
                return Err("Resume changed the thread identity".into());
            }
            let view = result.view;
            if success {
                if view.error.is_some()
                    || view.cancelled
                    || view.text != "¿Qué quieres cambiar?"
                    || !updates
                        .iter()
                        .any(|update| !update.complete && !update.text.is_empty())
                {
                    return Err(
                        "Actual streaming fixture did not preserve streamed/final text".into(),
                    );
                }
            } else if view.error.is_none() || view.cancelled || !view.text.is_empty() {
                return Err("HTTP fixture failure was reported as a successful reply".into());
            }
            Ok(())
        })
    };
    fs::write(
        base.join("updates.json"),
        serde_json::to_vec_pretty(&updates)?,
    )?;
    bridge.close().await?;
    fs::remove_file(base.join("home/config.toml"))?;
    fs::write(base.join("checked.txt"), format!("{checked:?}"))?;
    if let Err(error) = &checked {
        fixture.abort();
        return Err(std::io::Error::other(error.clone()).into());
    }
    let requests = tokio::time::timeout(Duration::from_secs(5), fixture).await???;
    let captured = requests.last().ok_or("Missing fixture request")?;
    if questions && !question_cancel {
        fs::write(
            base.join("questions.json"),
            serde_json::to_vec_pretty(&asked)?,
        )?;
        let output = requests[1]["input"]
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == "fixture_question"
                })
            })
            .and_then(|item| item["output"].as_str())
            .ok_or("Missing human answer output")?;
        let answer: Value = serde_json::from_str(output)?;
        if asked.len() != 1 || answer["answers"]["units"]["answers"] != json!(["mm"]) {
            return Err("Native question round trip failed".into());
        }
    }

    if question_cancel && asked.len() != 1 {
        return Err("Cancellation did not reach a pending native question".into());
    }
    let mut context_preserved = false;
    let mut refreshed_tool_result = false;
    if resume {
        let input = serde_json::to_string(&requests[1]["input"])?;
        context_preserved = input.contains("Protocol fixture only")
            && input.contains("¿Qué quieres cambiar?")
            && input.contains("Follow-up fixture");
        let output = requests[2]["input"]
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == "fixture_call"
                })
            })
            .and_then(|item| item["output"].as_str())
            .and_then(|output| output.split_once("\nOutput:\n").map(|(_, json)| json))
            .ok_or("Missing actual resumed tool output")?;
        let status: Value = serde_json::from_str(output)?;
        refreshed_tool_result = status["project_id"] == json!(project_id)
            && status["job"].is_null()
            && status["history"].as_array().is_some_and(Vec::is_empty);
        if !context_preserved || !refreshed_tool_result {
            return Err("Resume lost context or the refreshed Hazlo tool result".into());
        }
        if workflow.view()?.job.is_some() || !workflow.view()?.history.is_empty() {
            return Err("Status read changed engineering jobs".into());
        }
    }
    fs::write(
        base.join("request-inputs.json"),
        serde_json::to_vec_pretty(
            &requests
                .iter()
                .map(|request| &request["input"])
                .collect::<Vec<_>>(),
        )?,
    )?;
    let tools = captured["tools"]
        .as_array()
        .ok_or("Missing model-facing tool catalog")?;
    let names: Vec<_> = tools
        .iter()
        .map(|tool| json!({"type":tool["type"],"name":tool["name"]}))
        .collect();
    fs::write(base.join("tools.json"), serde_json::to_vec_pretty(tools)?)?;
    let allowed = [
        "list_mcp_resources",
        "list_mcp_resource_templates",
        "read_mcp_resource",
        "request_user_input",
        "mcp__hazlomexa",
    ];
    let exact_outer = tools.len() == allowed.len()
        && allowed
            .iter()
            .all(|name| tools.iter().filter(|tool| tool["name"] == *name).count() == 1);
    let namespace = tools
        .iter()
        .find(|tool| tool["name"] == "mcp__hazlomexa")
        .ok_or("Missing Hazlo namespace")?;
    let expected = program.map_or_else(hazlomexa::host_tools::catalog, hazlomexa::host_tools::Program::catalog);
    let exact_inner = namespace["type"] == "namespace"
        && namespace["tools"].as_array().is_some_and(|tools| {
            tools.len() == expected.len()
                && expected.iter().all(|entry| {
                    tools
                        .iter()
                        .filter(|tool| tool["name"] == entry.name.as_ref())
                        .count()
                        == 1
                })
        });
    fs::write(
        base.join("tool-surface.json"),
        serde_json::to_vec_pretty(&names)?,
    )?;
    fs::write(
        base.join("status.json"),
        serde_json::to_vec_pretty(
            &json!({"loopback_only":true,"cloud_inference":false,"credentials_present":false,"turn_decoder_passed":checked.is_ok(),"success_fixture":success,"resume_fixture":resume,"question_fixture":questions,"question_cancel":question_cancel,"questions_received":asked.len(),"context_preserved":context_preserved,"refreshed_tool_result":refreshed_tool_result,"cancellation_fixture":cancel,"cancel_to_socket_close_ms":captured["fixture_cancel_to_socket_close_ms"],"tool_count":tools.len(),"exact_model_tool_surface":exact_outer && exact_inner}),
        )?,
    )?;
    println!("Evidence {}", base.display());
    if !exact_outer || !exact_inner {
        return Err("Codex exposed an unexpected tool surface".into());
    }
    checked.map_err(std::io::Error::other)?;
    println!("PASS: actual Codex turn decoding against a local HTTP fixture");
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    panic!("Windows-only protocol gate");
}
