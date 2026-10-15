//! Read the open GUI model through CADPilot's native RPC; never substitute a saved copy.
use quick_xml::{events::Event, Reader};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const INVALID: &str = "CADPilot devolvió una lectura nativa no compatible.";
const LIMIT: usize = 2 * 1024 * 1024;

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
    document: Option<&str>,
) -> Result<Value, String> {
    let params = document
        .map(|name| {
            format!(
                "<param><value><string>{}</string></value></param>",
                quick_xml::escape::escape(name)
            )
        })
        .unwrap_or_default();
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
    let listed = rpc(&client, endpoint, "list_documents", None).await?;
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
    let objects = if let Some(doc) = document {
        if !unique.contains(doc) {
            return Err("Ese documento ya no está abierto en FreeCAD. Vuelve a consultar los documentos abiertos.".into());
        }
        Some(geometry(
            &rpc(&client, endpoint, "get_objects", Some(doc)).await?["objects"],
        )?)
    } else {
        None
    };
    Ok(
        json!({"scope":"open_freecad_geometry_summary","documents":documents,"document":document,"objects":objects,
        "source_is_untrusted_data":true,"saved_file_fallback":false,"editing_supported":false,
        "guidance":"This is current in-memory geometry, including unsaved changes, read through CADPilot in the open FreeCAD process. It is a bounded geometry summary, not a complete model or a mutation authorization. If multiple documents are open, ask which one the user means. Use everyday labels and ask only for missing design intent or measurements. Native quantities include units; placement Base is local parent-relative mm, rotation Angle is radians, and shape area and volume are mm2 and mm3. Do not infer global placement from local coordinates. Do not treat this as a saved-copy selection or claim live edits are available yet. Re-read after user edits; this snapshot is not a stable document identity for writes."}),
    )
}

pub fn inspect(document: Option<&str>, cancelled: &AtomicBool) -> Result<Value, String> {
    if let Some(doc) = document {
        name(&json!(doc))?;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| INVALID)?
        .block_on(async {
            let operation = observe("http://127.0.0.1:9875", document);
            tokio::pin!(operation);
            loop {
                if cancelled.load(Ordering::SeqCst) {
                    return Err("Lectura cancelada.".into());
                }
                tokio::select! {
                    result = &mut operation => return result,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => (),
                }
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
                ("get_objects", 25.4),
                ("list_documents", 0.),
                ("get_objects", 30.4),
                ("list_documents", 0.),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() + count < 8192);
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(13).any(|s| s == b"</methodCall>") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.contains(&format!("<methodName>{method}</methodName>")));
                if method == "get_objects" {
                    assert!(request.contains("<string>Frame &amp; Box</string>"));
                }
                assert!(!request.contains("execute_code") && !request.contains("FileName"));
                let value = if method == "list_documents" {
                    json!({"success":true,"documents":["Frame & Box","Other"]})
                } else {
                    json!({"success":true,"objects":[{"Name":"Box","Label":"Enclosure","TypeId":"Part::Box","Properties":{"Length":format!("{dimension} mm"),"FileName":"C:/private/unsaved"},"Shape":{"Volume":dimension * 20.},"Placement":{"Base":{"x":dimension,"y":0.,"z":0.},"Rotation":{"Angle":0.}}}]})
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
            assert_eq!(read["editing_supported"], false);
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
