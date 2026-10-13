use rmcp::{
    model::CallToolRequestParams,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
    },
    ServiceExt,
};
use serde_json::json;
use std::{fs, path::PathBuf, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        std::env::var("USERNAME")?,
        "WDAGUtilityAccount",
        "Owned guest required"
    );
    let profile = PathBuf::from(std::env::var_os("USERPROFILE").ok_or("Profile missing")?);
    let token = fs::read_to_string(profile.join(".arduinoIDE/mcp-token"))?;
    assert!(token.trim().len() >= 32);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .pool_max_idle_per_host(0)
        .timeout(Duration::from_secs(15))
        .build()?;
    let mut config = StreamableHttpClientTransportConfig::with_uri("http://127.0.0.1:3847/mcp");
    config.auth_header = Some(token.trim().into());
    config.allow_stateless = false;
    config.reinit_on_expired_session = false;
    config.max_sse_event_size = 1048576;
    config.max_concurrent_requests = 1;
    let service = tokio::time::timeout(
        Duration::from_secs(20),
        ().serve(StreamableHttpClientTransport::with_client(client, config)),
    )
    .await??;
    let tools = tokio::time::timeout(Duration::from_secs(15), service.list_all_tools()).await??;
    assert!(tools.iter().any(|tool| tool.name == "execute_tool"));
    let source_path = "C:\\AgentSketchbook\\ProbeSketch\\ProbeSketch.ino";
    let original = fs::read_to_string(source_path)?;
    let arguments =
        json!({"tool_name":"arduino_sketch","params":{"action":"get_content","path":source_path}})
            .as_object()
            .unwrap()
            .clone();
    let source = tokio::time::timeout(
        Duration::from_secs(15),
        service.call_tool(CallToolRequestParams::new("execute_tool").with_arguments(arguments)),
    )
    .await??;
    assert_ne!(source.is_error, Some(true));
    assert_eq!(source.content.len(), 1);
    let received: serde_json::Value = serde_json::from_str(
        &source.content[0]
            .as_text()
            .ok_or("Expected text content")?
            .text,
    )?;
    assert_eq!(received["content"].as_str(), Some(original.as_str()));
    assert_eq!(received["path"].as_str(), Some(source_path));
    assert_eq!(fs::read_to_string(source_path)?, original);
    fs::write(
        "C:\\AgentProbe\\output\\sdk-result.json",
        serde_json::to_vec_pretty(
            &json!({"pass":true,"sdk":"rmcp 3.5.1","server":service.peer_info(),"tools":tools,"source":source,"original_preserved":true,"automatic_post_retry":false,"hardware_mutations":false}),
        )?,
    )?;
    service.cancel().await?;
    println!("PASS: official Rust SDK negotiated Agent and read the fixture without modification.");
    Ok(())
}
