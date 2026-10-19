//! Disposable protocol fixture; not an engineering program or acceptance substitute.
#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::{
        model::*, service::RequestContext, ErrorData, RoleServer, ServerHandler, ServiceExt,
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Fixture(Arc<Mutex<(i64, bool, uuid::Uuid, u64)>>);
    impl Default for Fixture {fn default()->Self {Self(Arc::new(Mutex::new((0,false,uuid::Uuid::new_v4(),0))))}}
    fn tools(changed: bool) -> Vec<Tool> {
        ["observe_native_state","program_owned_parameter_edit","change_catalog","fail_after_effect","wait_after_effect"]
            .into_iter().chain(changed.then_some("new_native_tool")).map(|name|serde_json::from_value(json!({
                "name":name,"description":"Native fixture schema, not a Hazlo operation.",
                "annotations":{"readOnlyHint":name=="observe_native_state","destructiveHint":false,"openWorldHint":false},
                "inputSchema":{"type":"object","properties":{"value":{"type":"integer"}},"additionalProperties":false}})).unwrap()).collect()
    }
    impl ServerHandler for Fixture {
        fn get_info(&self) -> ServerConfig {
            let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
            info.server_info = Implementation::new("owned-native-fixture", "1.0");
            info
        }
        async fn list_tools(
            &self,
            request: Option<PaginatedRequestParams>,
            _: RequestContext<RoleServer>,
        ) -> Result<ListToolsResult, ErrorData> {
            let all = tools(self.0.lock().unwrap().1);
            let index = request
                .and_then(|r| r.cursor)
                .map(|c| c.parse::<usize>())
                .transpose()
                .map_err(|_| ErrorData::invalid_params("Bad cursor", None))?
                .unwrap_or(0);
            let tool = all
                .get(index)
                .ok_or_else(|| ErrorData::invalid_params("Bad cursor", None))?
                .clone();
            Ok(ListToolsResult {
                tools: vec![tool],
                next_cursor: (index + 1 < all.len()).then(|| (index + 1).to_string()),
                ..Default::default()
            })
        }
        async fn call_tool(
            &self,
            request: CallToolRequestParams,
            _: RequestContext<RoleServer>,
        ) -> Result<CallToolResponse, ErrorData> {
            let args = request.arguments.unwrap_or_default();
            if args.keys().any(|key| key != "value") {
                return Ok(
                    CallToolResult::error(vec![ContentBlock::text("Unexpected arguments")]).into(),
                );
            }
            let value = args.get("value").and_then(|v| v.as_i64()).unwrap_or(0);
            match request.name.as_ref() {
                "observe_native_state" => {}
                "program_owned_parameter_edit" => {let mut state=self.0.lock().unwrap();state.0=value;state.3+=1;},
                "change_catalog" => self.0.lock().unwrap().1 = true,
                "fail_after_effect" => {
                    self.0.lock().unwrap().0 = value;
                    self.0.lock().unwrap().3 += 1;
                    return Ok(
                        CallToolResult::error(vec![ContentBlock::text("Uncertain effect")]).into(),
                    );
                }
                "wait_after_effect" => {
                    self.0.lock().unwrap().0 = value;
                    self.0.lock().unwrap().3 += 1;
                    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                }
                _ => return Err(ErrorData::invalid_params("Unknown native tool", None)),
            }
            let (state,_,identity,writes) = *self.0.lock().unwrap();
            if request.name == "observe_native_state" && value > 0 {
                return Ok(CallToolResult::structured(json!({"native_value":state,"native_id":identity,"native_writes":writes,"evidence":"á".repeat(value.min(100000) as usize)})).into());
            }
            Ok(CallToolResult::structured(json!({"native_value":state,"native_id":identity,"native_writes":writes,"secret_echo":std::env::var("NATIVE_TEST_TOKEN").unwrap_or_default()})).into())
        }
    }
    match std::env::args().nth(1).as_deref() {
        Some("--help") => {
            if std::env::var_os("NATIVE_EMPTY_HELP").is_some() {
                println!("\x1b[?25h");
                return Ok(());
            }
            println!("Owned fixture CLI: show reads native state; set VALUE edits native state");
            return Ok(());
        }
        Some("--console-output") => {
            use std::io::Write;
            let mut console = std::fs::OpenOptions::new().write(true).open("CONOUT$")?;
            writeln!(console, "native console output")?;
            return Ok(());
        }
        Some("show") => {
            println!(
                "{}",
                std::fs::read_to_string("native-fixture-state.txt").unwrap_or("0".into())
            );
            return Ok(());
        }
        Some("set") => {
            let value = std::env::args().nth(2).ok_or("Missing value")?;
            std::fs::write("native-fixture-state.txt", value)?;
            println!("changed");
            return Ok(());
        }
        _ => {}
    }
    let service = Fixture::default()
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    service.waiting().await?;
    Ok(())
}
#[cfg(not(windows))]
fn main() {}
