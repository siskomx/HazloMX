//! Ollama's chat protocol uses the same bounded host tools and human approval as installed agents.
use super::{OllamaClient, MAX_RESPONSE, MAX_WIRE};
use crate::{
    host_tools::{HostTools, McpTools},
    jobs::Workflow,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

struct PendingProposals<'a> {
    workflow: &'a Workflow,
    cancelled: &'a AtomicBool,
    jobs: Vec<Uuid>,
}
impl Drop for PendingProposals<'_> {
    fn drop(&mut self) {
        if self.cancelled.load(Ordering::SeqCst) {
            for job in &self.jobs {
                let _ = self.workflow.cancel_unapproved(*job);
            }
        }
    }
}

#[derive(Deserialize, Serialize)]
struct Function {
    name: String,
    arguments: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    index: Option<u32>,
}
#[derive(Deserialize, Serialize)]
struct Call {
    function: Function,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct Message {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    thinking: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<Call>,
}

fn message(client: &OllamaClient, record: Value) -> Result<Message, String> {
    let direct_cloud = client
        .remote_model
        .as_deref()
        .is_some_and(|remote| record["model"].as_str() == Some(remote))
        && record.get("remote_model").is_none()
        && record.get("remote_host").is_none();
    if record.get("error").is_some()
        || record["done"] != true
        || record["done_reason"] != "stop"
        || record["model"] != client.model && !direct_cloud
        || !direct_cloud && !super::metadata_routing_matches(&record, &client.remote_model)
    {
        return Err("Ollama no completó una respuesta del perfil seleccionado.".into());
    }
    let message: Message = serde_json::from_value(record["message"].clone())
        .map_err(|_| "La respuesta de conversación de Ollama no es válida.")?;
    if message.role != "assistant"
        || message.content.len() > MAX_RESPONSE
        || message.thinking.len() > 64 * 1024
        || message.tool_calls.len() > 8
        || message.content.trim().is_empty() && message.tool_calls.is_empty()
    {
        return Err("La respuesta de conversación está vacía o supera el límite permitido.".into());
    }
    Ok(message)
}

pub async fn run(
    client: OllamaClient,
    workflow: Arc<Workflow>,
    host: Arc<HostTools>,
    project: (Uuid, PathBuf),
    prompt: String,
    cancelled: Arc<AtomicBool>,
    emit: impl FnMut(String),
) -> Result<String, String> {
    let tools = McpTools::for_project(host, project.0)?;
    if tools.project() != Some(project) {
        return Err("El proyecto cambió o la conversación se detuvo.".into());
    }
    run_scoped(client, workflow, tools, prompt, cancelled, emit).await
}
pub async fn run_scoped(
    client: OllamaClient,
    workflow: Arc<Workflow>,
    tools: McpTools,
    prompt: String,
    cancelled: Arc<AtomicBool>,
    mut emit: impl FnMut(String),
) -> Result<String, String> {
    if prompt.trim().is_empty() || prompt.len() > 8192 {
        return Err("Escribe una solicitud de hasta 8 KiB.".into());
    }
    let check = || -> Result<(), String> {
        if cancelled.load(Ordering::SeqCst) || tools.check_scope().is_err() {
            return Err("El proyecto cambió o la conversación se detuvo.".into());
        }
        Ok(())
    };
    check()?;
    client.check_capabilities(&cancelled, true).await?;
    let catalog = tools.catalog();
    let allowed: BTreeSet<_> = catalog.iter().map(|tool| tool.name.to_string()).collect();
    let read_only: BTreeSet<_> = catalog
        .iter()
        .filter(|tool| {
            tool.annotations
                .as_ref()
                .and_then(|annotations| annotations.read_only_hint)
                == Some(true)
        })
        .map(|tool| tool.name.to_string())
        .collect();
    let functions: Vec<_> = catalog
        .iter()
        .map(|tool| {
            json!({"type":"function","function":{
                "name":tool.name,"description":tool.description,"parameters":tool.input_schema
            }})
        })
        .collect();
    let mut messages = vec![
        json!({"role":"system","content":crate::claude::INSTRUCTIONS}),
        json!({"role":"user","content":prompt}),
    ];
    let mut text = String::new();
    let mut seen = BTreeSet::new();
    let mut pending = PendingProposals {
        workflow: &workflow,
        cancelled: &cancelled,
        jobs: Vec::new(),
    };
    // Bound autonomous tool rounds; a human turn starts a fresh inspection rather than retrying a call.
    for _ in 0..8 {
        check()?;
        let request =
            json!({"model":client.model,"messages":messages,"tools":functions,"stream":false});
        if serde_json::to_vec(&request)
            .map_err(|_| "No se pudo preparar la conversación.")?
            .len()
            > 2 * MAX_WIRE
        {
            return Err(
                "El contexto de herramientas supera el límite; inicia otra conversación.".into(),
            );
        }
        let record = client
            .metadata("/api/chat", Some(request), &cancelled)
            .await?;
        let response = message(&client, record)?;
        check()?;
        for call in &response.tool_calls {
            if !allowed.contains(&call.function.name)
                || call.id.as_ref().is_some_and(|id| id.len() > 256)
                || !read_only.contains(&call.function.name)
                    && !seen.insert(
                        serde_json::to_string(&json!([
                            call.function.name,
                            call.function.arguments
                        ]))
                        .map_err(|_| "Llamada no válida.")?,
                    )
            {
                return Err("Ollama solicitó una herramienta no autorizada o repetida; revisa el estado antes de continuar.".into());
            }
        }
        if !response.content.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&response.content);
            if text.len() > 2 * MAX_RESPONSE {
                return Err("La conversación supera el límite de respuesta.".into());
            }
            emit(text.clone());
        }
        if response.tool_calls.is_empty() {
            return Ok(text);
        }
        messages.push(serde_json::to_value(&response).map_err(|_| "Respuesta no válida.")?);
        for call in response.tool_calls {
            check()?;
            let name = call.function.name;
            let result = tools
                .call_in_project(
                    name.clone(),
                    Value::Object(call.function.arguments),
                    cancelled.clone(),
                )
                .await;
            if let Ok(value) = &result {
                if let Some(job) = value
                    .get("job_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                {
                    if value["human_approval_required"] == true {
                        pending.jobs.push(job);
                    }
                }
            }
            check()?;
            let output = match result {
                Ok(value) => json!({"result":value,"source_is_untrusted_data":true}),
                Err(error) => json!({"error":error}),
            };
            let content = serde_json::to_string(&output)
                .map_err(|_| "Resultado de herramienta no válido.")?;
            if content.len() > MAX_WIRE {
                return Err("El resultado de la herramienta supera el límite permitido.".into());
            }
            let mut message = json!({"role":"tool","tool_name":name,"content":content});
            if let Some(id) = call.id {
                message["tool_call_id"] = json!(id);
            }
            messages.push(message);
        }
    }
    Err("La IA alcanzó el límite de herramientas de este turno. Revisa el estado antes de continuar.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_tools::catalog;
    use crate::{
        ollama::tests::{metadata_replies, reply, server, DIGEST},
        project::{Phase, Project},
    };
    use std::{fs, time::Duration};

    fn setup() -> (PathBuf, Arc<Workflow>, Arc<HostTools>, (Uuid, PathBuf)) {
        let root = std::env::temp_dir().join(format!("hazlomexa-ollama-chat-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("project")).unwrap();
        let project = Project::create(&root.join("project")).unwrap();
        let identity = (project.id, project.root.clone());
        let workflow =
            Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
        workflow.select_project(project).unwrap();
        let host = Arc::new(HostTools::new(workflow.clone()));
        (root, workflow, host, identity)
    }
    fn metadata() -> Vec<String> {
        let mut responses = metadata_replies();
        responses[2] = reply(
            r#"{"capabilities":["completion","tools"]}"#,
            "application/json",
        );
        responses
    }
    fn record(content: &str, calls: Vec<Value>) -> Value {
        json!({"model":"fixture","done":true,"done_reason":"stop","message":{
            "role":"assistant","content":content,"thinking":"Reasoning is protocol data, not displayed instructions.","tool_calls":calls
        }})
    }
    fn call(name: &str, arguments: Value) -> Value {
        json!({"function":{"name":name,"arguments":arguments}})
    }
    fn proposal(project: Uuid) -> Value {
        call(
            "propose_rectangular_solid",
            json!({"project_id":project,"units":"in","length":1,"width":1,"height":1}),
        )
    }
    fn body(request: &str) -> Value {
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
    }

    #[tokio::test]
    async fn scoped_chat_refuses_a_cross_program_batch_before_any_proposal() {
        use crate::host_tools::Program;
        for program in [Program::Planning, Program::Freecad, Program::Drawing, Program::Arduino, Program::Agent, Program::Kicad, Program::Slice] {
            let (root, workflow, host, project) = setup();
            let tools = McpTools::for_program(host, project.clone(), program).unwrap();
            let wrong = if program == Program::Freecad {
                call("propose_arduino_compile", json!({"project_id":project.0,"fqbn":"arduino:avr:uno","source":"void setup() {}\nvoid loop() {}"}))
            } else { proposal(project.0) };
            let mut replies = metadata();
            replies.push(reply(&record("", vec![call("get_workflow_status", json!({})), wrong]).to_string(), "application/json"));
            let (port, fixture, _) = server(replies);
            let error = run_scoped(OllamaClient::new(port, "fixture", DIGEST).unwrap(), workflow.clone(), tools, "Inspecciona el programa seleccionado.".into(), Arc::new(AtomicBool::new(false)), |_| {}).await.unwrap_err();
            assert!(error.contains("no autorizada o repetida"));
            assert!(workflow.view().unwrap().job.is_none());
            assert!(workflow.view().unwrap().history.is_empty());
            let requests = fixture.join().unwrap();
            let advertised = body(requests.last().unwrap());
            let names: Vec<_> = advertised["tools"].as_array().unwrap().iter().map(|tool| tool["function"]["name"].as_str().unwrap()).collect();
            assert_eq!(names, program.catalog().iter().map(|tool| tool.name.as_ref()).collect::<Vec<_>>());
            drop(workflow);
            assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn invalid_tool_arguments_return_schema_and_allow_a_corrected_proposal() {
        let (root, workflow, host, project) = setup();
        let mut replies = metadata();
        for response in [
            record(
                "",
                vec![call(
                    "propose_rectangular_solid",
                    json!({"private-field":"private-value"}),
                )],
            ),
            record("", vec![proposal(project.0)]),
            record("Revisa la propuesta antes de aprobar.", vec![]),
        ] {
            replies.push(reply(&response.to_string(), "application/json"));
        }
        let (port, fixture, _) = server(replies);
        run(
            OllamaClient::new(port, "fixture", DIGEST).unwrap(),
            workflow.clone(),
            host,
            project,
            "Un cubo de una pulgada por lado, pendiente de revisión.".into(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await
        .unwrap();
        let requests = fixture.join().unwrap();
        let correction = body(&requests[4]);
        let messages = correction["messages"].as_array().unwrap();
        let result: Value =
            serde_json::from_str(messages.last().unwrap()["content"].as_str().unwrap()).unwrap();
        let error = result["error"].as_str().unwrap();
        let schema: Value = serde_json::from_str(error.split_once("faltan: ").unwrap().1).unwrap();
        assert_eq!(
            schema,
            Value::Object(
                (*catalog()
                    .into_iter()
                    .find(|tool| tool.name == "propose_rectangular_solid")
                    .unwrap()
                    .input_schema)
                    .clone()
            )
        );
        assert!(!error.contains("private-field") && !error.contains("private-value"));
        let pending = workflow.view().unwrap().job.unwrap();
        assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
        assert_eq!(
            pending.snapshot.input.solid().unwrap().dimensions_mm.height,
            25.4
        );
        assert_eq!(workflow.view().unwrap().history.len(), 1);
        assert!(!requests
            .join(" ")
            .contains(&pending.approval_id.unwrap().to_string()));
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn actual_http_chat_uses_host_tools_and_leaves_approval_to_the_human() {
        let (root, workflow, host, project) = setup();
        let mut replies = metadata();
        for response in [
            record(
                "Voy a inspeccionar el proyecto.",
                vec![call("get_workflow_status", json!({}))],
            ),
            record("", vec![proposal(project.0)]),
            record("", vec![call("get_workflow_status", json!({}))]),
            record(
                "Revisa el cambio propuesto en Hazlo antes de aprobarlo.",
                vec![],
            ),
        ] {
            replies.push(reply(&response.to_string(), "application/json"));
        }
        let (port, fixture, _) = server(replies);
        let mut updates = Vec::new();
        let result = run(
            OllamaClient::new(port, "fixture", DIGEST).unwrap(),
            workflow.clone(),
            host,
            project.clone(),
            "Quiero un cubo de una pulgada por lado; prepara una propuesta.".into(),
            Arc::new(AtomicBool::new(false)),
            |text| updates.push(text),
        )
        .await
        .unwrap();
        assert!(result.contains("antes de aprobarlo"));
        assert_eq!(updates.last(), Some(&result));
        let view = workflow.view().unwrap();
        let job = view.job.unwrap();
        assert_eq!(job.snapshot.phase, Phase::AwaitingApproval);
        assert!(!job.busy);
        let input = job.snapshot.input.solid().unwrap();
        assert_eq!(input.dimensions_mm.length, 25.4);
        let requests = fixture.join().unwrap();
        assert_eq!(requests.len(), 7);
        let first = body(&requests[3]);
        assert_eq!(first["stream"], false);
        assert_eq!(first["messages"][0]["content"], crate::claude::INSTRUCTIONS);
        assert_eq!(first["tools"].as_array().unwrap().len(), catalog().len());
        assert!(!first["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["function"]["name"] == "approve_operation"));
        let final_request = body(requests.last().unwrap());
        let messages = final_request["messages"].as_array().unwrap();
        assert!(messages
            .iter()
            .any(|message| message["role"] == "assistant" && message.get("thinking").is_some()));
        let tool_results: Vec<Value> = messages
            .iter()
            .filter(|message| message["role"] == "tool")
            .map(|message| serde_json::from_str(message["content"].as_str().unwrap()).unwrap())
            .collect();
        assert_eq!(tool_results.len(), 3);
        assert!(tool_results
            .iter()
            .all(|result| result["source_is_untrusted_data"] == true));
        assert_eq!(tool_results[1]["result"]["human_approval_required"], true);
        assert!(!requests
            .join(" ")
            .contains(&job.approval_id.unwrap().to_string()));
        assert!(workflow
            .view()
            .unwrap()
            .history
            .iter()
            .all(|job| job.phase != Phase::Completed));
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn unknown_or_duplicate_proposal_batch_never_dispatches_a_prefix() {
        for unknown in [true, false] {
            let (root, workflow, host, project) = setup();
            let mut replies = metadata();
            let calls = vec![
                proposal(project.0),
                if unknown {
                    call("approve_operation", json!({}))
                } else {
                    proposal(project.0)
                },
            ];
            replies.push(reply(&record("", calls).to_string(), "application/json"));
            let (port, fixture, _) = server(replies);
            let error = run(
                OllamaClient::new(port, "fixture", DIGEST).unwrap(),
                workflow.clone(),
                host,
                project,
                "Revisa mi idea.".into(),
                Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .await
            .unwrap_err();
            assert!(error.contains("no autorizada o repetida"));
            assert!(workflow.view().unwrap().job.is_none());
            fixture.join().unwrap();
            drop(workflow);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn missing_tool_capability_never_receives_the_user_prompt() {
        let (root, workflow, host, project) = setup();
        let (port, fixture, _) = server(metadata_replies());
        assert!(run(
            OllamaClient::new(port, "fixture", DIGEST).unwrap(),
            workflow.clone(),
            host,
            project,
            "PRIVATE TEST PROMPT".into(),
            Arc::new(AtomicBool::new(false)),
            |_| {}
        )
        .await
        .unwrap_err()
        .contains("no admite herramientas"));
        assert!(!fixture
            .join()
            .unwrap()
            .join(" ")
            .contains("PRIVATE TEST PROMPT"));
        assert!(workflow.view().unwrap().job.is_none());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn explicit_cloud_chat_keeps_the_pinned_route_and_shared_tool_catalog() {
        let (root, workflow, host, project) = setup();
        let replies = vec![
            reply(r#"{"version":"0.35.1"}"#, "application/json"),
            reply(&json!({"models":[{"name":"fixture:cloud","digest":DIGEST,"size":0,"remote_model":"fixture","remote_host":"https://ollama.com:443"}]}).to_string(), "application/json"),
            reply(r#"{"capabilities":["completion","tools"]}"#, "application/json"),
            reply(&record("¿Qué quieres crear y para qué lo necesitas?",vec![]).to_string(), "application/json"),
        ];
        let (port, fixture, _) = server(replies);
        let result = run(
            OllamaClient::cloud(port, "fixture:cloud", DIGEST).unwrap(),
            workflow.clone(),
            host,
            project,
            "Quiero crear una pieza.".into(),
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .await
        .unwrap();
        assert!(result.contains("para qué"));
        let requests = fixture.join().unwrap();
        let request = body(&requests[3]);
        assert_eq!(request["model"], "fixture:cloud");
        assert_eq!(request["tools"].as_array().unwrap().len(), catalog().len());
        assert!(workflow.view().unwrap().job.is_none());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cancelling_a_stalled_followup_closes_http_and_cancels_only_its_unapproved_job() {
        let (root, workflow, host, project) = setup();
        let mut replies = metadata();
        replies.push(reply(
            &record("", vec![proposal(project.0)]).to_string(),
            "application/json",
        ));
        replies.push("stall".into());
        let (port, fixture, ready) = server(replies);
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let cancel = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(3), async {
                while !ready.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            signal.store(true, Ordering::SeqCst);
        });
        assert!(run(
            OllamaClient::new(port, "fixture", DIGEST).unwrap(),
            workflow.clone(),
            host,
            project,
            "Un cubo de una pulgada por lado.".into(),
            cancelled,
            |_| {}
        )
        .await
        .is_err());
        cancel.await.unwrap();
        assert_eq!(
            tokio::task::spawn_blocking(move || fixture.join().unwrap())
                .await
                .unwrap()
                .len(),
            5
        );
        assert_eq!(
            workflow.view().unwrap().job.unwrap().snapshot.phase,
            Phase::Cancelled
        );
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn copied_project_folder_is_refused_before_http_or_tool_dispatch() {
        let (root, workflow, host, mut project) = setup();
        project.1 = root.join("other-folder");
        assert!(run(
            OllamaClient::new(9, "fixture", DIGEST).unwrap(),
            workflow.clone(),
            host.clone(),
            project.clone(),
            "No request should leave.".into(),
            Arc::new(AtomicBool::new(false)),
            |_| {}
        )
        .await
        .unwrap_err()
        .contains("proyecto cambió"));
        assert!(host
            .call_in_project(
                project,
                "get_workflow_status".into(),
                json!({}),
                Arc::new(AtomicBool::new(false))
            )
            .await
            .is_err());
        assert!(workflow.view().unwrap().job.is_none());
        drop(workflow);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chat_requires_a_complete_routed_assistant_record() {
        let client = OllamaClient::new(9, "fixture", DIGEST).unwrap();
        let valid = record("¿Qué quieres crear?", vec![]);
        assert!(message(&client, valid.clone()).is_ok());
        for (key, value) in [
            ("done", json!(false)),
            ("done_reason", json!("length")),
            ("model", json!("other")),
            ("error", json!("private provider failure")),
            ("remote_host", Value::Null),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(message(&client, invalid).is_err(), "{key}");
        }
        let mut invalid = valid.clone();
        invalid["message"]["role"] = json!("tool");
        assert!(message(&client, invalid).is_err());
        let mut invalid = valid.clone();
        invalid.as_object_mut().unwrap().remove("model");
        assert!(message(&client, invalid).is_err());
        let cloud = OllamaClient::cloud(9, "fixture:cloud", DIGEST).unwrap();
        assert!(message(&cloud, valid).is_ok());
    }

    #[tokio::test]
    #[ignore = "Actual headless CADCraft metric/inch inspection, fixture clarification/approval and verified readback through Ollama HTTP"]
    async fn actual_native_drawing_from_ollama_protocol_and_verified_followup() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        for (source, units, length) in [("metric.dxf", "mm", 25.4), ("imperial.dxf", "in", 1.)] {
            let (root, workflow, host, project) = setup();
            workflow.set_cadcraft(workspace.join("target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable/cadcraft-cli.exe")).unwrap();
            let original = fs::read(
                workspace
                    .join("docs/research/cadcraft-runtime-2026-10-11")
                    .join(source),
            )
            .unwrap();
            let entry = root.join(source);
            fs::write(&entry, &original).unwrap();
            let selected = host
                .select_drawing(project.0, entry.clone(), Arc::new(AtomicBool::new(false)))
                .unwrap();
            let mut replies = metadata();
            replies.push(reply(
                &record(
                    "",
                    vec![call(
                        "inspect_drawing_native_tools",
                        json!({"selection":selected.id}),
                    )],
                )
                .to_string(),
                "application/json",
            ));
            replies.push(reply(
                &record(
                    "¿Quieres que la línea mida una pulgada? Confirma la escala del dibujo.",
                    vec![],
                )
                .to_string(),
                "application/json",
            ));
            let (port, inspection, _) = server(replies);
            let question = run(OllamaClient::new(port,"fixture",DIGEST).unwrap(),workflow.clone(),host.clone(),project.clone(),
                "Quiero añadir una línea, pero aún no he confirmado su longitud. Inspecciona el dibujo.".into(),Arc::new(AtomicBool::new(false)),|_|{}).await.unwrap();
            assert!(question.contains("Confirma la escala"));
            assert!(workflow.view().unwrap().job.is_none());
            let requests = inspection.join().unwrap();
            let context = body(&requests[4]);
            let inspected: Value = serde_json::from_str(
                context["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            let before_count = inspected["result"]["context"]["native"]["entities"]
                .as_array()
                .unwrap()
                .len();
            let mut replies = metadata();
            replies.push(reply(
                &record(
                    "",
                    vec![call(
                        "propose_drawing_native_tools",
                        json!({"selection":selected.id,"units":units,"allow_loss":false,"plan":{
                            "intent":"Añadir una línea horizontal de una pulgada desde el origen",
                            "calls":[{"command":"line","params":{"points":[[0,0],[length,0]]}}]
                        }}),
                    )],
                )
                .to_string(),
                "application/json",
            ));
            replies.push(reply(
                &record("Revisa y aprueba la línea propuesta en Hazlo.", vec![]).to_string(),
                "application/json",
            ));
            let (port, proposal, _) = server(replies);
            run(OllamaClient::new(port,"fixture",DIGEST).unwrap(),workflow.clone(),host.clone(),project.clone(),
                format!("Confirmo escala {units}; añade una línea horizontal de una pulgada desde el origen."),Arc::new(AtomicBool::new(false)),|_|{}).await.unwrap();
            let pending = workflow.view().unwrap().job.unwrap();
            assert_eq!(pending.snapshot.phase, Phase::AwaitingApproval);
            let requests = proposal.join().unwrap();
            assert!(!requests
                .join(" ")
                .contains(&pending.approval_id.unwrap().to_string()));
            // Approval is supplied by this backend fixture, never by the model or a tool.
            let executing = workflow.clone();
            let completed = tokio::task::spawn_blocking(move || {
                executing
                    .execute(executing.begin(pending.approval_id.unwrap()).unwrap())
                    .unwrap()
            })
            .await
            .unwrap();
            assert_eq!(
                completed.snapshot.phase,
                Phase::Completed,
                "{:?}",
                completed.error
            );
            let job = completed.snapshot.input.job_id();
            let report = workflow
                .artifact(job, crate::project::ArtifactFormat::DrawingReport)
                .unwrap()
                .read_text()
                .unwrap();
            let native: Value = serde_json::from_str(&report).unwrap();
            assert_eq!(
                native["native_readback"]["entities"]
                    .as_array()
                    .unwrap()
                    .len(),
                before_count + 1
            );
            let mut replies = metadata();
            replies.push(reply(
                &record(
                    "",
                    vec![call("read_verified_result", json!({"job_id":job}))],
                )
                .to_string(),
                "application/json",
            ));
            replies.push(reply(
                &record(
                    "La copia guardada contiene la línea y el resultado se volvió a verificar.",
                    vec![],
                )
                .to_string(),
                "application/json",
            ));
            let (port, readback, _) = server(replies);
            run(
                OllamaClient::new(port, "fixture", DIGEST).unwrap(),
                workflow.clone(),
                host.clone(),
                project,
                "Lee el resultado verificado antes de describir el cambio.".into(),
                Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .await
            .unwrap();
            let requests = readback.join().unwrap();
            let context = body(&requests[4]);
            let result: Value = serde_json::from_str(
                context["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                result["result"]["sha256"],
                crate::kicad_project::digest(report.as_bytes())
            );
            assert_eq!(result["result"]["content"], report);
            assert_eq!(fs::read(entry).unwrap(), original);
            assert_eq!(workflow.view().unwrap().history.len(), 1);
            drop(host);
            drop(workflow);
            fs::remove_dir_all(root).unwrap();
        }
        println!("Two actual headless native drawing jobs passed through the Ollama HTTP tool loop with fixture clarification/approval, metric/inch scales, unchanged originals and verified-result follow-ups; no model or GUI ran.");
    }
}
