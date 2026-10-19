//! pySerial API worker; the model supplies bounded data, never Python code.
use crate::native_tools::Call;
use rmcp::model::Tool;
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

pub const WORKER: &str = include_str!("../resources/serial/worker.py");

pub fn available(python: &Path, cwd: &Path) -> bool {
    let args: Vec<OsString> = ["-I", "-c", WORKER, "--help"]
        .iter()
        .map(Into::into)
        .collect();
    crate::process::run_owned(
        python,
        &args,
        cwd,
        &crate::native_tools::environment(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_secs(5),
    )
    .is_ok_and(|out| {
        out.reason == crate::process::StopReason::Completed
            && out.exit_code == Some(0)
            && !out.truncated
            && serde_json::from_slice::<Value>(&out.stdout)
                .is_ok_and(|value| value["library"] == "pySerial" && value["version"].is_string())
    })
}

pub fn tools() -> Vec<Tool> {
    [
        json!({"name":"get_info","description":"Read the installed pySerial version and diagnostic limits.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"list_ports","description":"Discover local serial ports with descriptions and USB VID/PID. Does not open them.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"capture","description":"Read bytes from a currently enumerated local port. No writes. Opening the port can affect the connected device, even with DTR/RTS disabled. Requires reviewed device, baudrate and duration; binary data is returned as hex.","inputSchema":{"type":"object","properties":{"port":{"type":"string","maxLength":256},"baudrate":{"type":"integer","minimum":50,"maximum":4000000},"seconds":{"type":"number","exclusiveMinimum":0,"maximum":30},"max_bytes":{"type":"integer","minimum":1,"maximum":32768}},"required":["port","baudrate","seconds","max_bytes"],"additionalProperties":false}}),
    ].into_iter().map(|tool| serde_json::from_value(tool).expect("Static diagnostic schema")).collect()
}

pub fn arguments(call: &Call) -> Result<Vec<String>, String> {
    let operation = match call.tool.as_str() {
        "get_info" => "info",
        "list_ports" => "list_ports",
        "capture" => "capture",
        _ => {
            return Err(
                "Usa una herramienta del catálogo pySerial; no se ejecuta código Python.".into(),
            )
        }
    };
    let mut request = call.arguments.clone();
    if request.contains_key("operation") {
        return Err("Argumentos pySerial no válidos.".into());
    }
    request.insert("operation".into(), json!(operation));
    Ok(vec![Value::Object(request).to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_accepts_data_not_code_or_another_operation() {
        assert_eq!(tools().len(), 3);
        let mut call = Call {
            tool: "list_ports".into(),
            arguments: Default::default(),
        };
        assert_eq!(
            serde_json::from_str::<Value>(&arguments(&call).unwrap()[0]).unwrap(),
            json!({"operation":"list_ports"})
        );
        call.arguments.insert("operation".into(), json!("exec"));
        assert!(arguments(&call).is_err());
        call.arguments.clear();
        call.tool = "cli".into();
        assert!(arguments(&call).is_err());
    }
}
