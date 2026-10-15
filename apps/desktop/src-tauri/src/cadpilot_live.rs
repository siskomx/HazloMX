//! Bound native GUI transactions through CADPilot; saved files never substitute for open state.
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const INVALID: &str = "CADPilot devolvió una lectura nativa no compatible.";
const LIMIT: usize = 2 * 1024 * 1024;
const SCRIPT: &str = include_str!("../resources/freecad/live.py");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub live_cad_version: u32,
    pub job_id: uuid::Uuid,
    pub document_name: String,
    pub document_token: uuid::Uuid,
    pub state_sha256: String,
    pub description: String,
    pub operations: Vec<Value>,
    pub before: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sha256: Option<String>,
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        name(&json!(self.document_name))?;
        if self.schema_version != 1
            || self.live_cad_version != 1
            || self.job_id.is_nil()
            || self.document_token.is_nil()
            || !crate::kicad_project::valid_hash(&self.state_sha256)
            || self.description.trim().is_empty()
            || self.description.len() > 1024
            || !(1..=16).contains(&self.operations.len())
            || self.before.as_array().is_none_or(|a| a.len() > 256)
            || self
                .receipt_sha256
                .as_ref()
                .is_some_and(|s| !crate::kicad_project::valid_hash(s))
        {
            return Err("La propuesta nativa no es compatible.".into());
        }
        for operation in &self.operations {
            let object = operation.as_object().ok_or(INVALID)?;
            if object.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "action" | "obj_type" | "obj_name" | "obj_properties"
                )
            }) || !matches!(
                operation["action"].as_str(),
                Some(
                    "create_object"
                        | "edit_object"
                        | "delete_object"
                        | "boolean"
                        | "fillet"
                        | "chamfer"
                        | "loft"
                        | "sweep"
                        | "mirror"
                        | "pattern"
                        | "move"
                        | "color"
                        | "variables"
                        | "sketch"
                        | "pad"
                        | "pocket"
                        | "revolution"
                        | "groove"
                        | "thickness"
                        | "draft"
                        | "datum_plane"
                        | "hull"
                )
            ) {
                return Err("Usa una operación de geometría nativa de CADPilot.".into());
            }
            if operation["action"] == "create_object"
                && !matches!(
                    operation["obj_type"].as_str(),
                    Some(
                        "Part::Box"
                            | "Part::Cylinder"
                            | "Part::Cone"
                            | "Part::Sphere"
                            | "Part::Torus"
                            | "Part::Wedge"
                            | "PartDesign::Body"
                            | "Sketcher::SketchObject"
                            | "App::Part"
                            | "Spreadsheet::Sheet"
                    )
                )
            {
                return Err("El tipo de objeto no es una geometría nativa admitida.".into());
            }
            if let Some(value) = operation.get("obj_name") {
                name(value)?;
            }
            if let Some(value) = operation.get("obj_properties") {
                if !value.is_object() {
                    return Err(INVALID.into());
                }
            }
            validate_data(operation, 0)?;
        }
        if serde_json::to_vec(self).map_err(|_| INVALID)?.len()
            > if self.receipt_sha256.is_some() {
                60128
            } else {
                60000
            }
        {
            return Err("La propuesta nativa supera el límite del chat.".into());
        }
        Ok(())
    }
}

fn validate_data(value: &Value, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err(INVALID.into());
    }
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "code" | "script" | "proxy" | "filename" | "path" | "command" | "module"
                ) {
                    return Err("La propuesta solo admite geometría; no código ni rutas.".into());
                }
                validate_data(value, depth + 1)?;
            }
        }
        Value::Array(list) => {
            for value in list {
                validate_data(value, depth + 1)?;
            }
        }
        Value::String(s)
            if s.len() > 4096
                || s.contains("://")
                || s.starts_with("\\\\")
                || s.as_bytes().get(1) == Some(&b':') =>
        {
            return Err("La propuesta solo admite datos nativos sin rutas externas.".into())
        }
        _ => (),
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    params: Params,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Params {
    param: Param,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Param {
    value: RpcValue,
}
#[derive(Deserialize)]
struct RpcValue {
    #[serde(rename = "$value")]
    kind: Kind,
}
#[derive(Deserialize)]
enum Kind {
    #[serde(rename = "string")]
    Text(String),
    #[serde(rename = "boolean")]
    Boolean(String),
    #[serde(rename = "int", alias = "i4", alias = "i8")]
    Integer(i64),
    #[serde(rename = "double")]
    Double(f64),
    #[serde(rename = "nil")]
    Nil,
    #[serde(rename = "struct")]
    Struct {
        #[serde(default)]
        member: Vec<Member>,
    },
    #[serde(rename = "array")]
    Array { data: Data },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    name: String,
    value: RpcValue,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    #[serde(default)]
    value: Vec<RpcValue>,
}

impl RpcValue {
    fn json(self) -> Result<Value, String> {
        Ok(match self.kind {
            Kind::Text(s) => json!(s),
            Kind::Boolean(s) => match s.as_str() {
                "1" => json!(true),
                "0" => json!(false),
                _ => return Err(INVALID.into()),
            },
            Kind::Integer(n) => json!(n),
            Kind::Double(n) if n.is_finite() => json!(n),
            Kind::Double(_) => return Err(INVALID.into()),
            Kind::Nil => Value::Null,
            Kind::Array { data } => Value::Array(
                data.value
                    .into_iter()
                    .map(Self::json)
                    .collect::<Result<_, _>>()?,
            ),
            Kind::Struct { member } => {
                let mut map = Map::new();
                for m in member {
                    if map.insert(m.name, m.value.json()?).is_some() {
                        return Err(INVALID.into());
                    }
                }
                Value::Object(map)
            }
        })
    }
}

fn decode(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > LIMIT {
        return Err(INVALID.into());
    }
    // Bound nesting before serde's recursive XML decoding; DTDs are never needed by XML-RPC.
    let mut reader = Reader::from_reader(bytes);
    let mut depth = 0usize;
    loop {
        match reader.read_event().map_err(|_| INVALID)? {
            Event::Start(_) => {
                depth += 1;
                if depth > 32 {
                    return Err(INVALID.into());
                }
            }
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or(INVALID)?;
            }
            Event::DocType(_) => return Err(INVALID.into()),
            Event::Eof => break,
            _ => (),
        }
    }
    if depth != 0 {
        return Err(INVALID.into());
    }
    let response: Response = quick_xml::de::from_reader(bytes).map_err(|_| INVALID)?;
    let value = response.params.param.value.json()?;
    if value["success"] != true {
        return Err("FreeCAD no confirmó la lectura. Revisa que el documento siga abierto y CADPilot esté conectado.".into());
    }
    Ok(value)
}

async fn rpc(
    client: &reqwest::Client,
    endpoint: &str,
    method: &str,
    arguments: &[&str],
) -> Result<Value, String> {
    let params: String = arguments
        .iter()
        .map(|name| {
            format!(
                "<param><value><string>{}</string></value></param>",
                quick_xml::escape::escape(*name)
            )
        })
        .collect();
    let body = format!("<?xml version=\"1.0\"?><methodCall><methodName>{method}</methodName><params>{params}</params></methodCall>");
    let mut response = client
        .post(endpoint)
        .header("Content-Type", "text/xml")
        .body(body)
        .send()
        .await
        .map_err(|_| {
            "Abre FreeCAD y activa CADPilot para leer el documento abierto.".to_string()
        })?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > LIMIT as u64)
    {
        return Err(INVALID.into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| INVALID)? {
        if chunk.len() > LIMIT.saturating_sub(bytes.len()) {
            return Err(INVALID.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    decode(&bytes)
}

fn name(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .ok_or_else(|| INVALID.into())
}

fn geometry(objects: &Value) -> Result<Value, String> {
    let objects = objects
        .as_array()
        .filter(|a| a.len() <= 256)
        .ok_or(INVALID)?;
    let mut names = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for object in objects {
        let id = name(&object["Name"])?;
        if !names.insert(id) {
            return Err(INVALID.into());
        }
        let mut properties = Map::new();
        // Do not forward arbitrary native string properties, which can include private file paths.
        for key in [
            "Length", "Width", "Height", "Radius", "Radius1", "Radius2", "Angle", "Angle1",
            "Angle2",
        ] {
            let value = &object["Properties"][key];
            let quantity = value.as_str().is_some_and(|s| {
                let parts: Vec<_> = s.split_whitespace().collect();
                parts.len() == 2
                    && parts[0].parse::<f64>().is_ok_and(f64::is_finite)
                    && matches!(
                        parts[1],
                        "mm" | "cm" | "m" | "in" | "ft" | "deg" | "rad" | "°"
                    )
            });
            if value.is_number() || quantity {
                properties.insert(key.into(), value.clone());
            }
        }
        let mut shape = Map::new();
        for key in ["Volume", "Area", "VertexCount", "EdgeCount", "FaceCount"] {
            if let Some(n) = object["Shape"][key]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.)
            {
                shape.insert(key.into(), json!(n));
            }
        }
        let mut placement = Map::new();
        for (group, axes) in [("Base", &["x", "y", "z"][..]), ("Rotation", &["Angle"][..])] {
            let mut coordinates = Map::new();
            for axis in axes {
                if let Some(n) = object["Placement"][group][axis]
                    .as_f64()
                    .filter(|n| n.is_finite())
                {
                    coordinates.insert((*axis).into(), json!(n));
                }
            }
            placement.insert(group.into(), json!(coordinates));
        }
        out.push(json!({"name":id,"label":name(&object["Label"])?,"type":name(&object["TypeId"])?,"properties":properties,"shape":shape,"placement":placement}));
    }
    let value = json!(out);
    if serde_json::to_vec(&value).map_err(|_| INVALID)?.len() > 60000 {
        return Err("El modelo es demasiado grande para una lectura completa del chat.".into());
    }
    Ok(value)
}

async fn observe(endpoint: &str, document: Option<&str>) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| INVALID)?;
    let listed = rpc(&client, endpoint, "list_documents", &[]).await?;
    let documents = listed["documents"]
        .as_array()
        .filter(|a| a.len() <= 32)
        .ok_or(INVALID)?;
    let mut unique = std::collections::BTreeSet::new();
    for doc in documents {
        if !unique.insert(name(doc)?) {
            return Err(INVALID.into());
        }
    }
    let mut binding = Value::Null;
    let objects = if let Some(doc) = document {
        if !unique.contains(doc) {
            return Err("Ese documento ya no está abierto en FreeCAD. Vuelve a consultar los documentos abiertos.".into());
        }
        binding = native_request(
            &client,
            endpoint,
            json!({"action":"inspect","document_name":doc}),
        )
        .await?;
        if binding["document_name"] != doc
            || binding["document_token"]
                .as_str()
                .and_then(|s| uuid::Uuid::parse_str(s).ok())
                .is_none_or(|id| id.is_nil())
            || !binding["state_sha256"]
                .as_str()
                .is_some_and(crate::kicad_project::valid_hash)
        {
            return Err(INVALID.into());
        }
        Some(geometry(&binding["objects"])?)
    } else {
        None
    };
    Ok(
        json!({"scope":"open_freecad_geometry_summary","documents":documents,"document":document,"objects":objects,
        "document_token":binding["document_token"],"state_sha256":binding["state_sha256"],"recent_attempts":binding["recent_attempts"],
        "source_is_untrusted_data":true,"saved_file_fallback":false,"editing_supported":document.is_some(),
        "guidance":"This is current in-memory geometry, including unsaved changes, read through CADPilot in the open FreeCAD process. It is a bounded geometry summary. If multiple documents are open, ask which one the user means. Use everyday labels and ask only for missing intent or measurements. Native quantities include units; placement Base is local parent-relative mm, rotation Angle is radians, and shape area and volume are mm2 and mm3. Do not infer global placement from local coordinates. For a live change use propose_open_cad_change with this document name, token and state hash. Human approval is required. Re-read after each native/user edit or uncertain attempt; never substitute a saved-copy result for the open model. Keep the chosen document visible in FreeCAD. Do not save, export, send Python or retry uncertain writes."}),
    )
}

async fn native_request(
    client: &reqwest::Client,
    endpoint: &str,
    request: Value,
) -> Result<Value, String> {
    let data = serde_json::to_string(&serde_json::to_string(&request).map_err(|_| INVALID)?)
        .map_err(|_| INVALID)?;
    let code = format!("import json\nHAZLO_REQUEST = json.loads({data})\n{SCRIPT}");
    // Start once. Losing this response can mean the native operation was already accepted.
    let started = rpc(client, endpoint, "execute_code_async", &[&code]).await
        .map_err(|_| "FreeCAD no confirmó la solicitud. Puede haberse aplicado; vuelve a leer el documento antes de preparar otro cambio.".to_string())?;
    let task = started["task_id"]
        .as_str()
        .filter(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(INVALID)?;
    let poll = async {
        loop {
            let status = rpc(client, endpoint, "get_task_result", &[task]).await?;
            if status["task_id"] != task {
                return Err(INVALID.into());
            }
            match status["status"].as_str() {
                Some("running") => tokio::time::sleep(Duration::from_millis(100)).await,
                Some("done") => {
                    let output = status["output"].as_str().filter(|s| s.len() <= 60000).ok_or(INVALID)?;
                    let value: Value = serde_json::from_str(output).map_err(|_| INVALID)?;
                    if value["ok"] != true {
                        let message = match value["error"].as_str() {
                            Some("changed_document" | "stale_document" | "closed_document") => "El documento abierto cambió o se volvió a abrir. Léelo de nuevo antes de preparar otra propuesta.",
                            Some("native_undo_unavailable") => "Termina la edición nativa pendiente y activa Deshacer en FreeCAD antes de aprobar.",
                            Some("document_not_visible") => "Muestra el documento elegido en FreeCAD antes de aprobar.",
                            Some("native_operation_rolled_back") => "FreeCAD rechazó la operación y verificó que se revirtió la transacción completa.",
                            _ => "FreeCAD no confirmó un resultado válido. Puede haber cambios; vuelve a leer el documento y sus intentos recientes antes de continuar.",
                        };
                        return Err(message.into());
                    }
                    return Ok(value["result"].clone());
                }
                _ => return Err("La tarea nativa no confirmó su resultado. Relee el documento antes de continuar.".into()),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(90), poll).await
        .map_err(|_| "FreeCAD sigue sin confirmar. La operación puede terminar después; relee el documento antes de continuar.".to_string())?
}

fn wait(
    cancelled: &AtomicBool,
    operation: impl std::future::Future<Output = Result<Value, String>>,
) -> Result<Value, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| INVALID)?
        .block_on(async {
            tokio::pin!(operation);
            loop {
                if cancelled.load(Ordering::SeqCst) {
                    return Err("Espera cancelada. Una operación nativa aceptada puede terminar después; relee el documento antes de continuar.".into());
                }
                tokio::select! {
                    result = &mut operation => return result,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => (),
                }
            }
        })
}

pub fn inspect(document: Option<&str>, cancelled: &AtomicBool) -> Result<Value, String> {
    if let Some(doc) = document {
        name(&json!(doc))?;
    }
    wait(cancelled, observe("http://127.0.0.1:9875", document))
}

pub fn run(
    input: &Input,
    directory: &std::path::Path,
    cancelled: &AtomicBool,
) -> Result<(), String> {
    input.validate()?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| INVALID)?
        .as_secs()
        + 30;
    let request = json!({"action":"apply","document_token":input.document_token,"state_sha256":input.state_sha256,"job_id":input.job_id,"expires_at":expires,"operations":input.operations});
    let mut result = wait(cancelled, async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| INVALID)?;
        native_request(&client, "http://127.0.0.1:9875", request).await
    })?;
    result["after"]["objects"] = geometry(&result["after"]["objects"])?;
    result["live_cad_version"] = json!(1);
    validate_result(input, &result)?;
    std::fs::write(directory.join("result.json"), serde_json::to_vec_pretty(&result).map_err(|_| INVALID)?)
        .map_err(|_| "El cambio nativo pudo completarse, pero no se guardó su informe. Relee el documento antes de continuar.".into())
}

pub fn validate_result(input: &Input, result: &Value) -> Result<(), String> {
    input.validate()?;
    if result["live_cad_version"] != 1
        || result["job_id"] != input.job_id.to_string()
        || result["document_token"] != input.document_token.to_string()
        || result["before_sha256"] != input.state_sha256
        || result["after"]["document_name"] != input.document_name
        || result["undo_available"] != true
        || !result["after"]["state_sha256"]
            .as_str()
            .is_some_and(crate::kicad_project::valid_hash)
        || !result["after"]["objects"].is_array()
        || !result["operations"].as_array().is_some_and(|ops| {
            ops.len() == input.operations.len() && ops.iter().all(|op| op["success"] == true)
        })
    {
        return Err(
            "No se pudo verificar la transacción nativa. Relee FreeCAD antes de continuar.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_proposals_reject_code_paths_and_scripted_types_before_execution() {
        let valid = Input {
            schema_version: 1,
            live_cad_version: 1,
            job_id: uuid::Uuid::new_v4(),
            document_name: "Enclosure".into(),
            document_token: uuid::Uuid::new_v4(),
            state_sha256: "a".repeat(64),
            description: "Make the enclosure longer".into(),
            operations: vec![
                json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"1.2 in"}}),
            ],
            before: json!([]),
            receipt_sha256: None,
        };
        valid.validate().unwrap();
        for operation in [
            json!({"action":"execute_code","code":"print(1)"}),
            json!({"action":"create_object","obj_name":"X","obj_type":"Part::FeaturePython"}),
            json!({"action":"edit_object","obj_name":"Box","obj_properties":{"FileName":"C:/private"}}),
            json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Label":"https://outside.invalid"}}),
            json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Proxy":"injected"}}),
            json!({"action":"edit_object","code":"injected"}),
        ] {
            let mut bad = valid.clone();
            bad.operations = vec![operation];
            assert!(bad.validate().is_err());
        }
        let mut bad = valid;
        bad.document_token = uuid::Uuid::nil();
        assert!(bad.validate().is_err());
    }

    fn response(value: &str) -> String {
        format!("<?xml version=\"1.0\"?><methodResponse><params><param><value><struct><member><name>success</name><value><boolean>1</boolean></value></member>{value}</struct></value></param></params></methodResponse>")
    }

    #[test]
    fn native_xml_is_bounded_and_rejects_faults_duplicates_and_nonfinite_numbers() {
        let text = response("<member><name>documents</name><value><array><data><value><string>Sketch &amp; Frame</string></value></data></array></value></member>");
        assert_eq!(
            decode(text.as_bytes()).unwrap()["documents"][0],
            "Sketch & Frame"
        );
        for bad in [response("<member><name>success</name><value><boolean>1</boolean></value></member>"), response("<member><name>n</name><value><double>NaN</double></value></member>"), text.replace("<boolean>1", "<boolean>2"), "<!DOCTYPE x><methodResponse/>".into(), "<methodResponse><fault><value><string>private detail</string></value></fault></methodResponse>".into(), "<value>".repeat(33)] { assert!(decode(bad.as_bytes()).is_err()); }
        assert!(decode(&vec![b'x'; LIMIT + 1]).is_err());
    }

    #[test]
    fn geometry_exposes_measurements_without_forwarding_native_paths_or_claiming_a_full_model() {
        let object = json!({"Name":"Box","Label":"Enclosure","TypeId":"Part::Box","Properties":{"Length":"25.4 mm","Width":12.7,"Height":"NaN mm","FileName":"C:/private/part","Radius":"C:/private/radius"},"Shape":{"Volume":100.,"Area":50.,"FaceCount":6,"private":"secret"},"FileName":"secret"});
        let result = geometry(&json!([object])).unwrap();
        assert_eq!(
            result[0]["properties"],
            json!({"Length":"25.4 mm","Width":12.7})
        );
        assert_eq!(result[0]["shape"]["FaceCount"], 6.);
        assert!(!result.to_string().contains("private"));
        assert!(geometry(&json!([object, object])).is_err());
        assert!(geometry(&json!([{"Name":"Box"}])).is_err());
    }

    fn xml(value: &Value) -> String {
        let inner = match value {
            Value::Object(map) => format!(
                "<struct>{}</struct>",
                map.iter()
                    .map(|(key, value)| format!(
                        "<member><name>{}</name>{}</member>",
                        quick_xml::escape::escape(key),
                        xml(value)
                    ))
                    .collect::<String>()
            ),
            Value::Array(list) => format!(
                "<array><data>{}</data></array>",
                list.iter().map(xml).collect::<String>()
            ),
            Value::String(s) => format!("<string>{}</string>", quick_xml::escape::escape(s)),
            Value::Bool(b) => format!("<boolean>{}</boolean>", usize::from(*b)),
            Value::Number(n) => format!("<double>{n}</double>"),
            Value::Null => "<nil/>".into(),
        };
        format!("<value>{inner}</value>")
    }

    #[tokio::test]
    async fn native_protocol_reads_fresh_unsaved_geometry_and_never_sends_a_mutation_or_file_path()
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            // First list, two successive model reads, then a request for a document that was closed.
            for (method, dimension) in [
                ("list_documents", 0.),
                ("list_documents", 0.),
                ("execute_code_async", 25.4),
                ("get_task_result", 25.4),
                ("list_documents", 0.),
                ("execute_code_async", 30.4),
                ("get_task_result", 30.4),
                ("list_documents", 0.),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() + count < 32768);
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(13).any(|s| s == b"</methodCall>") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.contains(&format!("<methodName>{method}</methodName>")));
                if method == "execute_code_async" {
                    assert!(request.contains("Frame &amp; Box"));
                    assert!(request.contains("inspect"));
                    assert!(request.contains("_run_one_operation"));
                }
                assert!(!request.contains("<methodName>execute_operations</methodName>"));
                let value = if method == "list_documents" {
                    json!({"success":true,"documents":["Frame & Box","Other"]})
                } else if method == "execute_code_async" {
                    json!({"success":true,"task_id":"abcd1234"})
                } else {
                    let output = json!({"ok":true,"result":{"document_name":"Frame & Box","document_token":"11111111-1111-4111-8111-111111111111","state_sha256":"a".repeat(64),"recent_attempts":{},"objects":[{"Name":"Box","Label":"Enclosure","TypeId":"Part::Box","Properties":{"Length":format!("{dimension} mm"),"FileName":"C:/private/unsaved"},"Shape":{"Volume":dimension * 20.},"Placement":{"Base":{"x":dimension,"y":0.,"z":0.},"Rotation":{"Angle":0.}}}]}});
                    json!({"success":true,"task_id":"abcd1234","status":"done","output":output.to_string(),"error":null})
                };
                let body = format!(
                    "<methodResponse><params><param>{}</param></params></methodResponse>",
                    xml(&value)
                );
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let listed = observe(&endpoint, None).await.unwrap();
        assert_eq!(listed["documents"].as_array().unwrap().len(), 2);
        assert!(listed["objects"].is_null());
        for length in ["25.4 mm", "30.4 mm"] {
            let read = observe(&endpoint, Some("Frame & Box")).await.unwrap();
            assert_eq!(read["objects"][0]["properties"]["Length"], length);
            assert_eq!(read["editing_supported"], true);
            assert_eq!(read["saved_file_fallback"], false);
            assert!(!read.to_string().contains("C:/private"));
        }
        assert!(observe(&endpoint, Some("Closed")).await.is_err());
        server.await.unwrap();
    }

    #[test]
    fn invalid_or_cancelled_requests_never_connect() {
        for doc in ["", "Box\n", &"x".repeat(257)] {
            assert!(inspect(Some(doc), &AtomicBool::new(false)).is_err());
        }
        assert!(inspect(None, &AtomicBool::new(true))
            .unwrap_err()
            .contains("cancelada"));
    }
}
