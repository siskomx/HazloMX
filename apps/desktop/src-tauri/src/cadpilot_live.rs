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
const STEP_POLICY: &str = include_str!("../resources/freecad/cadpilot_policy.py");
const SCRIPT: &str = include_str!("../resources/freecad/live.py");

#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCompletion{version:u32,job_id:uuid::Uuid,input_sha256:String,result_sha256:String,result:Value}
fn completion_input(input:&Input)->Result<String,String>{
    let mut value=serde_json::to_value(input).map_err(|_|INVALID)?;value.as_object_mut().ok_or(INVALID)?.remove("receipt_sha256");Ok(crate::kicad_project::digest(&serde_json::to_vec(&value).map_err(|_|INVALID)?))
}
pub(crate) fn read_native_completion(directory:&std::path::Path,input:&Input,expected_hash:&str)->Result<Option<Value>,String>{
    let path=directory.join("native-completion.json");if !path.try_exists().map_err(|_|"No se pudo comprobar el recibo nativo local.")?{return Ok(None);}
    let bytes=crate::kicad_project::read_regular(&path,256*1024)?;if crate::kicad_project::digest(&bytes)!=expected_hash{return Err("El recibo original cambió; no se adoptó otro contenido.".into());}let saved:NativeCompletion=serde_json::from_slice(&bytes).map_err(|_|"El recibo local no es válido; no se reemplazó.".to_string())?;
    validate_result(input,&saved.result)?;let result=serde_json::to_vec_pretty(&saved.result).map_err(|_|INVALID)?;
    if saved.version!=1 || saved.job_id!=input.job_id || saved.input_sha256!=completion_input(input)? || saved.result_sha256!=crate::kicad_project::digest(&result) || input.receipt_sha256.as_ref().is_some_and(|hash|hash!=&saved.result_sha256) || result.len()>65536{return Err("El recibo no coincide con la entrada y respuesta originales; se conserva la pausa.".into());}Ok(Some(saved.result))
}
pub(crate) fn write_native_completion(directory:&std::path::Path,input:&Input,result:&Value)->Result<String,String>{
    validate_result(input,result)?;let bytes=serde_json::to_vec_pretty(result).map_err(|_|INVALID)?;if bytes.len()>65536{return Err("El resultado nativo supera el límite; sigue local y no se repetirá la edición.".into());}
    let path=directory.join("native-completion.json");
    let saved=NativeCompletion{version:1,job_id:input.job_id,input_sha256:completion_input(input)?,result_sha256:crate::kicad_project::digest(&bytes),result:result.clone()};let expected=serde_json::to_vec_pretty(&saved).map_err(|_|INVALID)?;
    if path.try_exists().map_err(|_|"No se pudo comprobar el recibo local.")?{if crate::kicad_project::read_regular(&path,256*1024)?!=expected{return Err("El recibo original cambió; no se reemplazó ni se repitió la edición.".into());}}
    else{crate::project::atomic_json(&path,&saved,false).map_err(|_|"La edición pudo terminar, pero no se pudo conservar su recibo local. No la repitas.".to_string())?;}
    if crate::kicad_project::read_regular(&path,256*1024)?!=expected{return Err("El recibo cambió antes de vincularse; no se adoptó otro contenido.".into());}Ok(crate::kicad_project::digest(&expected))
}
pub(crate) fn recover_recorded_result(directory:&std::path::Path,input:&Input)->Result<bool,String>{
    let operation=crate::project::OperationInput::LiveCad(input.clone());let Some(record)=crate::live_dispatch::read(directory,&operation)? else{return Ok(false);};
    let Some(hash)=record.native_completion_sha256.as_deref() else{return Ok(false);};
    let result=read_native_completion(directory,input,hash)?.ok_or("Falta el recibo nativo original.")?;
    let bytes=serde_json::to_vec_pretty(&result).map_err(|_|INVALID)?;let hash=crate::kicad_project::digest(&bytes);
    if input.receipt_sha256.as_ref().is_some_and(|expected|expected!=&hash) || record.receipt_sha256.as_ref().is_some_and(|expected|expected!=&hash){return Err("El informe esperado no coincide; no se reconstruyó otro resultado.".into());}
    let path=directory.join("result.json");if path.try_exists().map_err(|_|"No se pudo comprobar el informe local.")?{if crate::kicad_project::read_regular(&path,65536)?!=bytes{return Err("El informe local cambió; se conserva sin reemplazarlo.".into());}}
    else{crate::project::atomic_json(&path,&result,false).map_err(|_|"El recibo sigue local, pero no se pudo guardar el informe. No repitas la edición.".to_string())?;}
    Ok(true)
}

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
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agreement:Option<crate::native_tools::Agreement>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub working_version:Option<crate::native_working_version::Plan>,
}
impl Input {
    // New creation rules apply before effects; existing verified receipts remain readable.
    pub(crate) fn validate_for_dispatch(&self) -> Result<(), String> {
        self.validate()?;
        for operation in &self.operations {
            if operation["action"] != "create_object" { continue; }
            if ["x","y","z"].iter().any(|axis| operation["obj_properties"].get(*axis).is_some()) {
                return Err("Las coordenadas no son propiedades de una pieza nueva. Usa Placement.Base con coordenadas numéricas en mm; no pongas x, y o z directamente en obj_properties.".into());
            }
            let required:&[&str]=match operation["obj_type"].as_str() {
                Some("Part::Box")=>&["Length","Width","Height"],
                Some("Part::Cylinder")=>&["Radius","Height"],
                Some("Part::Cone")=>&["Radius1","Radius2","Height"],
                Some("Part::Sphere")=>&["Radius"],
                Some("Part::Torus")=>&["Radius1","Radius2"],
                Some("Part::Wedge")=>&["Xmin","Ymin","Zmin","X2min","Z2min","Xmax","Ymax","Zmax","X2max","Z2max"],
                _=>&[],
            };
            if required.iter().any(|key| !operation["obj_properties"].get(*key).is_some_and(|value|
                value.is_number() || value.as_str().is_some_and(|text|!text.trim().is_empty()))) {
                return Err("Faltan medidas explícitas para crear la pieza. Confírmalas en lenguaje natural e incluye sus propiedades dimensionales; no uses las medidas predeterminadas de FreeCAD.".into());
            }
        }
        Ok(())
    }

    pub fn is_save_as(&self) -> bool {
        self.operations == [json!({"action":"save_as"})]
    }

    pub fn is_export_step(&self) -> bool {
        self.operations.len() == 1 && self.operations[0]["action"] == "export_step"
    }

    pub fn uses_file_dialog(&self) -> bool {
        self.is_save_as() || self.is_export_step()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(agreement)=&self.agreement{agreement.validate()?;}
        if let Some(plan)=&self.working_version{plan.validate()?;if self.uses_file_dialog(){return Err("La versión de trabajo acompaña las ediciones; Guardar como y STEP conservan su alcance separado.".into());}}
        name(&json!(self.document_name))?;
        if self.schema_version != 1
            || self.live_cad_version != 1
            || self.job_id.is_nil()
            || self.document_token.is_nil()
            || !crate::kicad_project::valid_hash(&self.state_sha256)
            || self.description.trim().is_empty()
            || self.description.len() > 1024
            || self.operations.len()>16 || (self.operations.is_empty() && self.working_version.is_none())
            || self.before.as_array().is_none_or(|a| a.len() > 256)
            || self
                .receipt_sha256
                .as_ref()
                .is_some_and(|s| !crate::kicad_project::valid_hash(s))
        {
            return Err("La propuesta nativa no es compatible.".into());
        }
        if self.operations.iter().any(|op| op["action"] == "save_as") && !self.is_save_as() {
            return Err("Guardar como debe ser la única acción, sin rutas ni parámetros; FreeCAD pedirá el destino.".into());
        }
        if self.operations.iter().any(|op| op["action"] == "export_step")
            && (!self.is_export_step()
                || self.operations[0].as_object().is_none_or(|op| op.len() != 2)
                || name(&self.operations[0]["obj_name"]).is_err())
        {
            return Err("Exportar STEP debe ser una sola acción con obj_name de la lectura, sin rutas ni parámetros.".into());
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
                        | "save_as"
                        | "export_step"
                )
            ) {
                return Err("Usa el esquema operations de propose_open_cad_change. Para cambiar una propiedad: {\"action\":\"edit_object\",\"obj_name\":\"nombre_de_la_lectura\",\"obj_properties\":{\"Length\":\"1.2 in\"}}. edit_box pertenece a copias guardadas; no pidas a la persona nombres internos ni formatos de herramientas.".into());
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
    // A lone open document needs no extra discovery round or user-supplied technical name.
    let document = document.or_else(|| {
        (documents.len() == 1).then(|| documents[0].as_str()).flatten()
    });
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
        "document_token":binding["document_token"],"state_sha256":binding["state_sha256"],"recent_attempts":binding["recent_attempts"],"has_saved_file":binding["has_saved_file"],"modified":binding["modified"],
        "source_is_untrusted_data":true,"saved_file_fallback":false,"editing_supported":document.map(|_| true),"inspection_required":document.is_none(),
        "guidance":"When inspection_required is true this is only a document list, not a failed or unsupported connection: call inspect_open_cad_documents with the intended document_name to read its geometry and editing binding. This is current in-memory geometry, including unsaved changes, read through CADPilot in the open FreeCAD process. It is a bounded geometry summary. If multiple documents are open, ask which one the user means. Use everyday labels and ask only for missing intent or measurements. Native quantities include units; placement Base is local parent-relative mm, rotation Angle is radians, and shape area and volume are mm2 and mm3. Do not infer global placement from local coordinates. For a live change use propose_open_cad_change with this document name, token and state hash. Human approval is required. Re-read after each native/user edit or uncertain attempt; never substitute a saved-copy result for the open model. Keep the chosen document visible in FreeCAD. For native Save As, propose a separate single operation {action:save_as}, without paths or parameters; FreeCAD asks the person for the destination after approval. Never combine saving with geometry edits. Save As is not a geometry undo transaction. For STEP export, use a separate single operation {action:export_step,obj_name:name_from_inspection}; confirm the intended part in ordinary language. FreeCAD asks for a new destination and refuses existing files. The host checks solid count, volume, world bounds and center of mass after native export. Export does not save or modify the open document. Do not send Python or retry uncertain writes."}),
    )
}

async fn native_request(
    client: &reqwest::Client,
    endpoint: &str,
    request: Value,
) -> Result<Value, String> {
    let interactive_save = request["operations"].as_array().is_some_and(|ops| ops.iter().any(|op| matches!(op["action"].as_str(), Some("save_as" | "export_step"))));
    let data = serde_json::to_string(&serde_json::to_string(&request).map_err(|_| INVALID)?)
        .map_err(|_| INVALID)?;
    let copy_glue=if request["keep_working_version"]==true && request["action"]=="apply"{let core=serde_json::to_string(&crate::native_working_version::glue()).map_err(|_|INVALID)?;format!("import types\nHAZLO_COPY_CORE=types.ModuleType('hazlo_copy_core')\nHAZLO_COPY_CORE.__dict__.update({{'NATIVE_TYPES':NATIVE_TYPES,'PROPERTY_TYPES':PROPERTY_TYPES,'document':document}})\nexec({core},HAZLO_COPY_CORE.__dict__)\n")}else{String::new()};
    let code = format!("import json\nHAZLO_REQUEST = json.loads({data})\n{STEP_POLICY}\n{copy_glue}{SCRIPT}");
    // Start once. Losing this response can mean the native operation was already accepted.
    let started = rpc(client, endpoint, "execute_code_async", &[&code]).await
        .map_err(|_| "FreeCAD no confirmó la solicitud. Puede haberse aplicado; vuelve a leer el documento antes de preparar otro cambio.".to_string())?;
    let task = started["task_id"]
        .as_str()
        .filter(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(INVALID)?;
    let poll = async {
        loop {
            let status = rpc(client, endpoint, "get_task_result", &[task]).await.map_err(|_| {
                "FreeCAD no confirmó el resultado de la operación. Puede terminar después; relee el documento y sus intentos recientes antes de continuar. No repitas una escritura incierta.".to_string()
            })?;
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
                            Some("native_export_not_completed") => "Se canceló la exportación en FreeCAD. Relee el documento antes de continuar.",
                            Some("native_export_destination_exists") => "Elige un archivo nuevo para exportar; no se sobrescribió el destino existente.",
                            Some("native_export_unsupported") => "La pieza elegida no contiene sólidos válidos para exportar a STEP.",
                            Some("native_export_uncertain") => "FreeCAD pudo exportar, pero no confirmó la verificación. Comprueba el archivo y relee el documento; no repitas la escritura.",
                            Some("native_save_not_completed") => "FreeCAD no completó Guardar como. Relee el documento y revisa su ventana; puede haberse cancelado o fallado.",
                            Some("native_save_uncertain") => "FreeCAD pudo guardar el documento, pero no confirmó la verificación. Relee su estado y comprueba el archivo antes de continuar.",
                            Some("native_operation_pending") => "Hay una operación nativa pendiente en este documento. Termina o cancela el diálogo en FreeCAD y vuelve a leerlo.",
                            Some("native_operation_rolled_back") => "FreeCAD rechazó la operación y verificó que se revirtió la transacción completa.",
                            Some("native_result_unavailable") => "FreeCAD no conserva el resultado completo de ese intento. El trabajo y la pausa se mantienen; no repitas la edición. Hace falta una revisión nativa o una copia de trabajo verificada.",
                            Some("native_result_mismatch") => "El recibo nativo no corresponde a la operación aprobada. Se conserva el trabajo y la pausa; no se repitió la edición.",
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
    tokio::time::timeout(Duration::from_secs(if interactive_save { 600 } else { 90 }), poll).await
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
    run_impl(input,directory,cancelled,None,"http://127.0.0.1:9875")
}
async fn submit_native(client:reqwest::Client,endpoint:String,request:Value,dispatch:Option<&std::sync::Arc<crate::live_dispatch::Dispatch>>)->Result<Value,String>{
    let mut tasks=tokio::task::JoinSet::new();
    {
        let _permit=dispatch.map(|d|d.enter()).transpose()?;
        tasks.spawn(async move {native_request(&client,&endpoint,request).await});
    }
    tasks.join_next().await.ok_or(INVALID)?.map_err(|_|INVALID)?
}
pub(crate) fn run_gated(input:&Input,directory:&std::path::Path,cancelled:&AtomicBool,dispatch:&std::sync::Arc<crate::live_dispatch::Dispatch>)->Result<(),String>{
    run_impl(input,directory,cancelled,Some(dispatch),"http://127.0.0.1:9875")
}
fn run_impl(input:&Input,directory:&std::path::Path,cancelled:&AtomicBool,dispatch:Option<&std::sync::Arc<crate::live_dispatch::Dispatch>>,endpoint:&str)->Result<(),String>{
    input.validate_for_dispatch()?;
    let expires = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| INVALID)?
        .as_secs()
        + 30;
    let mut request = json!({"action":"apply","document_token":input.document_token,"state_sha256":input.state_sha256,"job_id":input.job_id,"expires_at":expires,"operations":input.operations});
    let _version_pins=if input.working_version.is_some(){let root=directory.join("NativeVersion");std::fs::create_dir(&root).map_err(|_|"No se pudo reservar una nueva versión; no se enviaron ediciones.")?;request["keep_working_version"]=json!(true);request["working_version_directory"]=json!(root);Some(crate::project::pin_lineage(&root).map_err(|_|"No se pudo fijar la carpeta de la nueva versión.")?)}else{None};
    let mut result = wait(cancelled, async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            // Native Save As may hold Python's GIL until the person closes its file dialog.
            .timeout(Duration::from_secs(if input.uses_file_dialog() { 600 } else { 15 }))
            .build()
            .map_err(|_| INVALID)?;
        submit_native(client,endpoint.into(),request,dispatch).await
    })?;
    normalize_result(input,&mut result)?;
    if let Some(dispatch)=dispatch{dispatch.retain_native_completion(input,&result)?;}
    write_checked_result(input,directory,&result)
}
fn write_checked_result(input:&Input,directory:&std::path::Path,result:&Value)->Result<(),String>{
    validate_result(input,result)?;
    crate::project::atomic_json(&directory.join("result.json"),result,false)
        .map_err(|_| "FreeCAD confirmó la operación, pero Hazlo no pudo guardar su informe. Conserva los cambios y recupera el recibo local sin repetir la edición.".into())
}
fn normalize_result(input:&Input,result:&mut Value)->Result<(),String>{
    result["after"]["objects"] = geometry(&result["after"]["objects"])?;
    result["live_cad_version"] = json!(1);
    validate_result(input, result)
}
pub(crate) fn recover_result(input:&Input,directory:&std::path::Path,cancelled:&AtomicBool)->Result<bool,String>{
    recover_result_at(input,directory,cancelled,"http://127.0.0.1:9875")
}
fn recover_result_at(input:&Input,directory:&std::path::Path,cancelled:&AtomicBool,endpoint:&str)->Result<bool,String>{
    input.validate()?;
    if recover_recorded_result(directory,input)?{return Ok(true);}
    let path=directory.join("result.json");
    let mut request=json!({"action":"recover","job_id":input.job_id,"document_token":input.document_token,"state_sha256":input.state_sha256,"operations":input.operations});if input.working_version.is_some(){request["keep_working_version"]=json!(true);}
    let mut result=wait(cancelled,async {
        let client=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).timeout(Duration::from_secs(15)).build().map_err(|_|INVALID)?;
        native_request(&client,endpoint,request).await
    })?;
    if result==json!({"recovery_status":"running"}){return Ok(false);}
    normalize_result(input,&mut result)?;
    if crate::live_dispatch::read(directory,&crate::project::OperationInput::LiveCad(input.clone()))?.is_some_and(|record|matches!(record.outcome,crate::native_journal::Outcome::Dispatched|crate::native_journal::Outcome::Uncertain)){crate::live_dispatch::retain_native_completion(directory,input,&result)?;}
    if path.try_exists().map_err(|_|"No se pudo comprobar el resultado local.")?{
        let saved:Value=crate::project::read_json(&path).map_err(|_|"El resultado local no es válido; se conservó sin reemplazarlo.")?;
        if saved!=result{return Err("El informe local cambió frente al recibo nativo; se conservó sin reemplazarlo ni liberar la pausa.".into());}
        return Ok(true);
    }
    write_checked_result(input,directory,&result)?;Ok(true)
}

pub fn validate_result(input: &Input, result: &Value) -> Result<(), String> {
    input.validate()?;
    if input.working_version.is_some(){if !matches!(result["working_version"]["state"].as_str(),Some("captured"|"blocked")){return Err("Falta el resultado de captura de la versión nativa.".into());}}else if !result["working_version"].is_null(){return Err(INVALID.into());}
    if result["live_cad_version"] != 1
        || result["job_id"] != input.job_id.to_string()
        || result["document_token"] != input.document_token.to_string()
        || result["before_sha256"] != input.state_sha256
        || result["after"]["document_name"] != input.document_name
        || result["undo_available"] != !input.uses_file_dialog()
        || !result["after"]["state_sha256"]
            .as_str()
            .is_some_and(crate::kicad_project::valid_hash)
        || !result["after"]["objects"].is_array()
        || !result["operations"].as_array().is_some_and(|ops| {
            ops.len() == input.operations.len() && ops.iter().zip(&input.operations).all(|(op, expected)| op["success"] == true && op["action"] == expected["action"])
        })
    {
        return Err(
            "No se pudo verificar la operación nativa. Relee FreeCAD antes de continuar.".into(),
        );
    }
    if input.is_save_as() {
        let saved = &result["saved_document"];
        let filename = name(&saved["name"])?;
        if !filename.to_ascii_lowercase().ends_with(".fcstd")
            || filename.contains(['/', '\\', ':'])
            || !saved["sha256"].as_str().is_some_and(crate::kicad_project::valid_hash)
            || !saved["bytes"].as_u64().is_some_and(|n| n > 0 && n <= 256 * 1024 * 1024)
            || result["after"]["has_saved_file"] != true
            || result["after"]["modified"] != false
        {
            return Err("FreeCAD no confirmó el archivo guardado. Relee su estado antes de continuar.".into());
        }
    } else if !result["saved_document"].is_null() {
        return Err(INVALID.into());
    }
    if input.is_export_step() {
        let exported = &result["exported_document"];
        let filename = name(&exported["name"])?;
        if !(filename.to_ascii_lowercase().ends_with(".step") || filename.to_ascii_lowercase().ends_with(".stp"))
            || filename.contains(['/', '\\', ':'])
            || !exported["sha256"].as_str().is_some_and(crate::kicad_project::valid_hash)
            || !exported["bytes"].as_u64().is_some_and(|n| n > 0 && n <= 256 * 1024 * 1024)
            || result["after"]["state_sha256"] != input.state_sha256
            || result["after"]["objects"] != input.before
        {
            return Err("No se verificó la exportación STEP con el documento sin cambios. Relee FreeCAD.".into());
        }
    } else if !result["exported_document"].is_null() {
        return Err(INVALID.into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn completed_fixture()->(Input,Value){
        let input=Input{working_version:None,schema_version:1,live_cad_version:1,job_id:uuid::Uuid::new_v4(),document_name:"Owned".into(),document_token:uuid::Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Preserve the native edit".into(),operations:vec![json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}})],before:json!([]),receipt_sha256:None,agreement:Some(crate::native_tools::tests::agreement_fixture())};
        let result=json!({"live_cad_version":1,"job_id":input.job_id,"document_token":input.document_token,"before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":"b".repeat(64),"objects":[{"name":"Box","label":"Preserved user label","properties":{"Length":"12 mm"}}]},"operations":[{"action":"edit_object","success":true}],"undo_available":true});(input,result)
    }
    #[test]
    fn original_native_completion_recovers_offline_and_never_replaces_changed_work(){
        use std::sync::{Arc,Mutex};use crate::{live_dispatch::Dispatch,project::OperationInput};
        for scenario in ["missing_report","acknowledged_missing_report","same_report","changed_report","changed_completion","rehashed_completion","changed_report_with_receipt","changed_agreement","not_admitted","unbound_copy","changed_expected_hash"]{
            let root=std::env::temp_dir().join(format!("hazlo-native-completion-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let (mut input,result)=completed_fixture();let operation=OperationInput::LiveCad(input.clone());let fence=Dispatch::start(&root,&operation,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();
            if scenario!="not_admitted"{drop(fence.enter().unwrap());}else{assert!(fence.retain_native_completion(&input,&result).is_err());}
            if scenario=="unbound_copy" || scenario=="not_admitted"{write_native_completion(&root,&input,&result).unwrap();assert!(!recover_recorded_result(&root,&input).unwrap());assert!(!root.join("result.json").exists());drop(fence);std::fs::remove_dir_all(root).unwrap();continue;}
            fence.retain_native_completion(&input,&result).unwrap();fence.retain_native_completion(&input,&result).unwrap();let summary=crate::live_dispatch::read(&root,&operation).unwrap().unwrap().summary(&operation);assert_eq!(summary.acknowledged,1);assert_eq!(summary.uncertain,0);assert!(summary.needs_review && !summary.finished);assert!(fence.enter().is_err());
            if scenario=="acknowledged_missing_report"{assert!(recover_recorded_result(&root,&input).unwrap());let mut checked=operation.clone();checked.retain_receipt(&root).unwrap();fence.complete(&checked).unwrap();input.receipt_sha256=checked.receipt_sha256().map(str::to_owned);std::fs::remove_file(root.join("result.json")).unwrap();}
            std::fs::write(root.join("manual.native"),b"preserve later manual edits").unwrap();
            if scenario=="same_report"{crate::project::atomic_json(&root.join("result.json"),&result,false).unwrap();}
            if scenario=="changed_report"{std::fs::write(root.join("result.json"),b"preserve changed report").unwrap();}
            if scenario=="changed_completion"{std::fs::write(root.join("native-completion.json"),b"preserve changed receipt").unwrap();}
            if scenario=="rehashed_completion"{let path=root.join("native-completion.json");let mut changed:NativeCompletion=crate::project::read_json(&path).unwrap();changed.result["after"]["state_sha256"]=json!("c".repeat(64));changed.result_sha256=crate::kicad_project::digest(&serde_json::to_vec_pretty(&changed.result).unwrap());crate::project::atomic_json(&path,&changed,true).unwrap();}
            if scenario=="changed_report_with_receipt"{let mut changed=result.clone();changed["after"]["state_sha256"]=json!("c".repeat(64));crate::project::atomic_json(&root.join("result.json"),&changed,false).unwrap();input.receipt_sha256=Some(crate::kicad_project::digest(&std::fs::read(root.join("result.json")).unwrap()));}
            if scenario=="changed_agreement"{input.agreement.as_mut().unwrap().outcome="Different outcome".into();}
            if scenario=="changed_expected_hash"{input.receipt_sha256=Some("c".repeat(64));}
            let journal=std::fs::read(root.join("live-dispatch.json")).unwrap();let recovered=recover_result_at(&input,&root,&AtomicBool::new(false),"http://127.0.0.1:9");
            if matches!(scenario,"missing_report"|"acknowledged_missing_report"|"same_report"){
                assert!(recovered.unwrap());let bytes=std::fs::read(root.join("result.json")).unwrap();assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(),result);assert!(recover_recorded_result(&root,&input).unwrap());assert_eq!(std::fs::read(root.join("result.json")).unwrap(),bytes);
            }else{assert!(recovered.is_err());if scenario=="changed_report"{assert_eq!(std::fs::read(root.join("result.json")).unwrap(),b"preserve changed report");}else if scenario=="changed_report_with_receipt"{assert_eq!(crate::project::read_json::<Value>(&root.join("result.json")).unwrap()["after"]["state_sha256"],"c".repeat(64));}else{assert!(!root.join("result.json").exists());}}
            assert_eq!(std::fs::read(root.join("live-dispatch.json")).unwrap(),journal);assert_eq!(std::fs::read(root.join("manual.native")).unwrap(),b"preserve later manual edits");drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn failed_native_completion_anchor_preserves_local_evidence_and_retries_only_bookkeeping(){
        use std::{os::windows::fs::OpenOptionsExt,sync::{Arc,Mutex}};use crate::{live_dispatch::Dispatch,project::OperationInput};
        let root=std::env::temp_dir().join(format!("hazlo-native-anchor-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let(input,result)=completed_fixture();let operation=OperationInput::LiveCad(input.clone());let fence=Dispatch::start(&root,&operation,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();drop(fence.enter().unwrap());let path=root.join("live-dispatch.json");let before=std::fs::read(&path).unwrap();
        let lock=std::fs::OpenOptions::new().read(true).share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ).open(&path).unwrap();assert!(fence.retain_native_completion(&input,&result).is_err());let receipt=std::fs::read(root.join("native-completion.json")).unwrap();assert_eq!(std::fs::read(&path).unwrap(),before);assert!(!recover_recorded_result(&root,&input).unwrap());assert!(!root.join("result.json").exists());assert!(fence.enter().is_err());drop(lock);
        fence.retain_native_completion(&input,&result).unwrap();assert_eq!(std::fs::read(root.join("native-completion.json")).unwrap(),receipt);assert!(recover_result_at(&input,&root,&AtomicBool::new(false),"http://127.0.0.1:9").unwrap());assert!(fence.enter().is_err());drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn admitted_native_reply_is_retained_before_an_occupied_final_report_can_fail(){
        use std::{io::{Read,Write},net::TcpListener,sync::{Arc,Mutex,atomic::AtomicUsize}};use crate::{live_dispatch::Dispatch,project::OperationInput};
        let root=std::env::temp_dir().join(format!("hazlo-native-final-write-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let(input,mut native)=completed_fixture();native["after"]["objects"]=json!([{"Name":"Box","Label":"Preserved user label","TypeId":"Part::Box","Properties":{"Length":"12 mm"}}]);let mut expected=native.clone();normalize_result(&input,&mut expected).unwrap();let operation=OperationInput::LiveCad(input.clone());let fence=Dispatch::start(&root,&operation,Arc::new(Mutex::new(())),Arc::new(AtomicBool::new(false)),Arc::new(AtomicBool::new(false))).unwrap();std::fs::write(root.join("result.json"),b"preserve the occupied report").unwrap();
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let endpoint=format!("http://{}",listener.local_addr().unwrap());let calls=Arc::new(AtomicUsize::new(0));let count=calls.clone();let server=std::thread::spawn(move||{
            for method in ["execute_code_async","get_task_result"]{
                let(mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut request=Vec::new();loop{let mut chunk=[0;4096];let size=stream.read(&mut chunk).unwrap();assert!(size>0 && request.len()+size<1_000_000);request.extend_from_slice(&chunk[..size]);if request.windows(13).any(|bytes|bytes==b"</methodCall>"){break;}}
                let request=String::from_utf8(request).unwrap();assert!(request.contains(&format!("<methodName>{method}</methodName>")));let value=if method=="execute_code_async"{count.fetch_add(1,Ordering::SeqCst);json!({"success":true,"task_id":"faceb00c"})}else{json!({"success":true,"task_id":"faceb00c","status":"done","output":json!({"ok":true,"result":native}).to_string()})};let body=format!("<methodResponse><params><param>{}</param></params></methodResponse>",xml(&value));stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
            }
        });
        let error=run_impl(&input,&root,&AtomicBool::new(false),Some(&fence),&endpoint).unwrap_err();assert!(error.contains("confirmó la operación"));server.join().unwrap();assert_eq!(calls.load(Ordering::SeqCst),1);assert_eq!(std::fs::read(root.join("result.json")).unwrap(),b"preserve the occupied report");let record=crate::live_dispatch::read(&root,&operation).unwrap().unwrap();assert_eq!(read_native_completion(&root,&input,record.native_completion_sha256.as_ref().unwrap()).unwrap().unwrap(),expected);assert!(recover_result_at(&input,&root,&AtomicBool::new(false),&endpoint).is_err());
        // Resolve only the fixture's file clash; recovering afterward must not contact its closed server.
        std::fs::rename(root.join("result.json"),root.join("preserved-report.txt")).unwrap();assert!(recover_result_at(&input,&root,&AtomicBool::new(false),&endpoint).unwrap());assert_eq!(std::fs::read(root.join("preserved-report.txt")).unwrap(),b"preserve the occupied report");assert_eq!(calls.load(Ordering::SeqCst),1);assert!(fence.enter().is_err());drop(fence);assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn working_version_checkpoint_binds_runtime_and_excludes_file_dialogs(){
        let(mut input,mut result)=completed_fixture();assert!(serde_json::to_value(&input).unwrap().get("working_version").is_none());
        input.operations.clear();assert!(input.validate_for_dispatch().is_err());input.working_version=Some(crate::native_working_version::Plan{verifier_sha256:"a".repeat(64)});input.validate_for_dispatch().unwrap();
        result["operations"]=json!([]);assert!(validate_result(&input,&result).is_err());result["working_version"]=json!({"state":"blocked","reason":"native_working_copy_unverified"});validate_result(&input,&result).unwrap();
        let original=crate::kicad_project::digest(&serde_json::to_vec(&input).unwrap());input.working_version.as_mut().unwrap().verifier_sha256="b".repeat(64);assert_ne!(original,crate::kicad_project::digest(&serde_json::to_vec(&input).unwrap()));
        for op in [json!({"action":"save_as"}),json!({"action":"export_step","obj_name":"Box"})]{input.operations=vec![op];assert!(input.validate_for_dispatch().is_err());}
    }
    #[test]
    fn primitive_creation_requires_explicit_sizes_without_changing_history_validation() {
        let mut input=Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:uuid::Uuid::new_v4(),
            document_name:"NewPart".into(),document_token:uuid::Uuid::new_v4(),state_sha256:"a".repeat(64),
            description:"Create the confirmed shape".into(),operations:vec![],before:json!([]),receipt_sha256:None};
        for (kind,properties) in [
            ("Part::Box",json!({"Length":"1 in","Width":"2 cm","Height":5})),
            ("Part::Cylinder",json!({"Radius":"0.5 in","Height":10})),
            ("Part::Cone",json!({"Radius1":10,"Radius2":0,"Height":"1 in"})),
            ("Part::Sphere",json!({"Radius":"3 cm"})),
            ("Part::Torus",json!({"Radius1":20,"Radius2":"2 mm"})),
            ("Part::Wedge",json!({"Xmin":0,"Ymin":0,"Zmin":0,"X2min":2,"Z2min":2,
                "Xmax":10,"Ymax":10,"Zmax":10,"X2max":8,"Z2max":8})),
        ] {
            let complete=json!({"action":"create_object","obj_type":kind,"obj_name":"NewShape","obj_properties":properties});
            input.operations=vec![complete.clone()];
            input.validate_for_dispatch().unwrap();
            for key in properties.as_object().unwrap().keys() {
                for invalid in [None,Some(json!(null)),Some(json!(false)),Some(json!("  ")),Some(json!([])),Some(json!({}))] {
                    input.operations=vec![complete.clone()];
                    if let Some(value)=invalid {input.operations[0]["obj_properties"][key]=value;}
                    else {input.operations[0]["obj_properties"].as_object_mut().unwrap().remove(key);}
                    assert!(input.validate_for_dispatch().is_err(),"{kind}: {key}");
                    input.validate().unwrap();
                    input.receipt_sha256=Some("b".repeat(64));
                    input.validate().unwrap();
                    assert!(input.validate_for_dispatch().is_err(),"A receipt must not bypass the effect guard");
                    input.receipt_sha256=None;
                }
            }
        }
        for kind in ["PartDesign::Body","Sketcher::SketchObject","App::Part","Spreadsheet::Sheet"] {
            input.operations=vec![json!({"action":"create_object","obj_type":kind,"obj_name":"Container"})];
            input.validate_for_dispatch().unwrap();
        }
        input.operations=vec![json!({"action":"edit_object","obj_name":"Existing","obj_properties":{"Height":"1 in"}})];
        input.validate_for_dispatch().unwrap();
    }

    #[test]
    fn creation_coordinates_use_native_placement_before_dispatch() {
        let mut input=Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:uuid::Uuid::new_v4(),
            document_name:"NewPart".into(),document_token:uuid::Uuid::new_v4(),state_sha256:"a".repeat(64),
            description:"Create a support".into(),before:json!([]),receipt_sha256:None,
            operations:vec![json!({"action":"create_object","obj_type":"Part::Box","obj_name":"Support",
                "obj_properties":{"Length":"0.5 in","Width":"2 cm","Height":"10 mm"}})]};
        for axis in ["x","y","z"] {
            input.operations[0]["obj_properties"][axis]=json!("0 mm");
            input.validate().unwrap();
            assert!(input.validate_for_dispatch().is_err(),"Invalid coordinate reached native dispatch: {axis}");
            input.receipt_sha256=Some("b".repeat(64));
            input.validate().unwrap();
            assert!(input.validate_for_dispatch().is_err());
            input.receipt_sha256=None;
            input.operations[0]["obj_properties"].as_object_mut().unwrap().remove(axis);
        }
        input.operations[0]["obj_properties"]["Placement"]=json!({"Base":{"x":0,"y":0,"z":0},
            "Rotation":{"Axis":{"x":0,"y":0,"z":1},"Angle":0}});
        input.validate_for_dispatch().unwrap();
    }

    #[test]
    fn native_proposals_reject_code_paths_and_scripted_types_before_execution() {
        let valid = Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),
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

    #[test]
    fn native_save_requires_one_pathless_action_and_a_verified_native_file_receipt() {
        let mut input = Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),
            schema_version: 1, live_cad_version: 1, job_id: uuid::Uuid::new_v4(),
            document_name: "Enclosure".into(), document_token: uuid::Uuid::new_v4(),
            state_sha256: "a".repeat(64), description: "Save the finished enclosure".into(),
            operations: vec![json!({"action":"save_as"})], before: json!([]), receipt_sha256: None,
        };
        input.validate().unwrap();
        let report = json!({"live_cad_version":1,"job_id":input.job_id,"document_token":input.document_token,
            "before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":"b".repeat(64),"objects":[],"has_saved_file":true,"modified":false},
            "undo_available":false,"operations":[{"action":"save_as","success":true}],
            "saved_document":{"name":"Enclosure.FCStd","bytes":100,"sha256":"c".repeat(64)}});
        validate_result(&input, &report).unwrap();
        for saved in [json!(null), json!({"name":"C:/private/Enclosure.FCStd","bytes":100,"sha256":"c".repeat(64)}),
            json!({"name":"Enclosure.FCStd","bytes":0,"sha256":"c".repeat(64)}),
            json!({"name":"Enclosure.FCStd","bytes":100,"sha256":"bad"})] {
            let mut bad = report.clone(); bad["saved_document"] = saved;
            assert!(validate_result(&input, &bad).is_err());
        }
        let mut bad = report.clone(); bad["after"]["modified"] = json!(true);
        assert!(validate_result(&input, &bad).is_err());
        bad = report.clone(); bad["operations"][0]["action"] = json!("edit_object");
        assert!(validate_result(&input, &bad).is_err());
        for operations in [
            json!([{"action":"save_as","obj_name":"Box"}]),
            json!([{"action":"save_as","obj_properties":{"filename":"outside.FCStd"}}]),
            json!([{"action":"save_as"},{"action":"edit_object","obj_name":"Box"}])
        ] {
            input.operations = serde_json::from_value(operations).unwrap();
            assert!(input.validate().is_err());
        }
    }

    #[test]
    fn native_step_export_is_separate_pathless_and_verifies_unchanged_document() {
        let mut input = Input {working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),
            schema_version: 1, live_cad_version: 1, job_id: uuid::Uuid::new_v4(),
            document_name: "Enclosure".into(), document_token: uuid::Uuid::new_v4(),
            state_sha256: "a".repeat(64), description: "Export the enclosure".into(),
            operations: vec![json!({"action":"export_step","obj_name":"Box"})], before: json!([]), receipt_sha256: None,
        };
        input.validate().unwrap();
        let report = json!({"live_cad_version":1,"job_id":input.job_id,"document_token":input.document_token,
            "before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":input.state_sha256,"objects":[]},
            "undo_available":false,"operations":[{"action":"export_step","success":true}],
            "exported_document":{"name":"Enclosure.step","bytes":100,"sha256":"c".repeat(64)}});
        validate_result(&input,&report).unwrap();
        for field in ["state_sha256","objects"] {
            let mut bad = report.clone(); bad["after"][field] = json!("changed");
            assert!(validate_result(&input,&bad).is_err());
        }
        for exported in [json!(null),json!({"name":"private/part.step","bytes":100,"sha256":"c".repeat(64)}),
            json!({"name":"part.step","bytes":0,"sha256":"c".repeat(64)}),
            json!({"name":"part.FCStd","bytes":100,"sha256":"c".repeat(64)})] {
            let mut bad = report.clone(); bad["exported_document"] = exported;
            assert!(validate_result(&input,&bad).is_err());
        }
        for operations in [json!([{"action":"export_step"}]),json!([{"action":"export_step","obj_name":"Box","obj_properties":{}}]),
            json!([{"action":"export_step","obj_name":"Box"},{"action":"save_as"}])] {
            input.operations = serde_json::from_value(operations).unwrap();
            assert!(input.validate().is_err());
        }
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
            // List multiple documents, read one explicitly, discover a lone document, then refuse a closed one.
            let mut lists = 0;
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
                    assert!(count > 0 && request.len() + count < 65536);
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
                    assert!(!request.contains("HAZLO_COPY_CORE=types.ModuleType"),"Inspection must not attach the copy implementation");
                }
                assert!(!request.contains("<methodName>execute_operations</methodName>"));
                let value = if method == "list_documents" {
                    lists += 1;
                    json!({"success":true,"documents":if lists == 3 {vec!["Frame & Box"]}else{vec!["Frame & Box","Other"]}})
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
        assert!(listed["editing_supported"].is_null());
        assert_eq!(listed["inspection_required"], true);
        for (length, document) in [("25.4 mm", Some("Frame & Box")), ("30.4 mm", None)] {
            let read = observe(&endpoint, document).await.unwrap();
            assert_eq!(read["document"], "Frame & Box");
            assert_eq!(read["inspection_required"], false);
            assert_eq!(read["objects"][0]["properties"]["Length"], length);
            assert_eq!(read["editing_supported"], true);
            assert_eq!(read["saved_file_fallback"], false);
            assert!(!read.to_string().contains("C:/private"));
        }
        assert!(observe(&endpoint, Some("Closed")).await.is_err());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn native_task_submission_is_fenced_and_keeps_polling_an_admitted_call_after_pause() {
        use std::sync::{Arc,Mutex,atomic::Ordering};
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let root=std::env::temp_dir().join(format!("hazlo-cad-task-fence-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input=crate::project::OperationInput::LiveCad(Input{working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:uuid::Uuid::new_v4(),document_name:"Owned".into(),document_token:uuid::Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Owned native submission fence".into(),operations:vec![json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}})],before:json!([]),receipt_sha256:None});
        let gate=Arc::new(Mutex::new(()));let paused=Arc::new(AtomicBool::new(true));
        let dispatch=crate::live_dispatch::Dispatch::start(&root,&input,gate.clone(),paused,Arc::new(AtomicBool::new(false))).unwrap();
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let endpoint=format!("http://{}",listener.local_addr().unwrap());
        let client=reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap();
        assert!(submit_native(client.clone(),endpoint.clone(),json!({"action":"apply"}),Some(&dispatch)).await.is_err());
        assert!(tokio::time::timeout(Duration::from_millis(50),listener.accept()).await.is_err());
        let next=root.join("admitted");std::fs::create_dir(&next).unwrap();let paused=Arc::new(AtomicBool::new(false));
        let dispatch=crate::live_dispatch::Dispatch::start(&next,&input,gate.clone(),paused.clone(),Arc::new(AtomicBool::new(false))).unwrap();
        let (seen,poll_seen)=tokio::sync::oneshot::channel();let (release,reply_ready)=tokio::sync::oneshot::channel();
        let server=tokio::spawn(async move {
            let mut seen=Some(seen);let mut reply_ready=Some(reply_ready);
            for method in ["execute_code_async","get_task_result"] {
                let (mut stream,_)=listener.accept().await.unwrap();let mut request=Vec::new();
                loop {let mut chunk=[0;4096];let count=stream.read(&mut chunk).await.unwrap();assert!(count>0&&request.len()+count<1_000_000);request.extend_from_slice(&chunk[..count]);if request.windows(13).any(|w|w==b"</methodCall>"){break;}}
                assert!(String::from_utf8(request).unwrap().contains(&format!("<methodName>{method}</methodName>")));
                let value=if method=="execute_code_async"{json!({"success":true,"task_id":"f00dbabe"})}else{
                    seen.take().unwrap().send(()).unwrap();reply_ready.take().unwrap().await.unwrap();
                    json!({"success":true,"task_id":"f00dbabe","status":"done","output":json!({"ok":true,"result":{"native_reply":true}}).to_string()})
                };
                let body=format!("<methodResponse><params><param>{}</param></params></methodResponse>",xml(&value));
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            }
        });
        let selected=dispatch.clone();let operation=tokio::spawn(async move {submit_native(client,endpoint,json!({"action":"apply"}),Some(&selected)).await});
        tokio::time::timeout(Duration::from_secs(5),poll_seen).await.unwrap().unwrap();
        let permit=gate.try_lock().expect("Correction waited for native task completion");paused.store(true,Ordering::SeqCst);drop(permit);release.send(()).unwrap();
        assert_eq!(operation.await.unwrap().unwrap()["native_reply"],true);server.await.unwrap();
        assert!(crate::live_dispatch::read(&next,&input).unwrap().unwrap().summary(&input).needs_review,"Transport success alone must not release the write hold");
        std::fs::remove_dir_all(root).unwrap();
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
    #[test]
    fn native_recovery_uses_only_the_original_receipt_and_preserves_changed_local_reports(){
        use std::{io::{Read,Write},net::TcpListener};
        let root=std::env::temp_dir().join(format!("hazlo-native-recovery-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let input=Input{working_version:None,agreement:Some(crate::native_tools::tests::agreement_fixture()),schema_version:1,live_cad_version:1,job_id:uuid::Uuid::new_v4(),document_name:"Owned".into(),document_token:uuid::Uuid::new_v4(),state_sha256:"a".repeat(64),description:"Change the approved part".into(),operations:vec![json!({"action":"edit_object","obj_name":"Box","obj_properties":{"Length":"12 mm"}})],before:json!([]),receipt_sha256:None};
        let result=json!({"job_id":input.job_id,"document_token":input.document_token,"before_sha256":input.state_sha256,"after":{"document_name":input.document_name,"state_sha256":"b".repeat(64),"objects":[]},"operations":[{"action":"edit_object","success":true}],"undo_available":true});
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let endpoint=format!("http://{}",listener.local_addr().unwrap());let expected=input.clone();let native=result.clone();
        let server=std::thread::spawn(move||{
            for pass in 0..5 {
                for method in ["execute_code_async","get_task_result"]{
                    let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut request=Vec::new();
                    loop {let mut chunk=[0;4096];let count=stream.read(&mut chunk).unwrap();assert!(count>0 && request.len()+count<1_000_000);request.extend_from_slice(&chunk[..count]);if request.windows(13).any(|w|w==b"</methodCall>"){break;}}
                    let request=String::from_utf8(request).unwrap();assert!(request.contains(&format!("<methodName>{method}</methodName>")));
                    let value=if method=="execute_code_async"{
                        let code=quick_xml::escape::unescape(request.split("<string>").nth(1).unwrap().split("</string>").next().unwrap()).unwrap();
                        let encoded=code.lines().nth(1).unwrap().strip_prefix("HAZLO_REQUEST = json.loads(").unwrap().strip_suffix(')').unwrap();let data:String=serde_json::from_str(encoded).unwrap();let query:Value=serde_json::from_str(&data).unwrap();
                        assert_eq!(query,json!({"action":"recover","job_id":expected.job_id,"document_token":expected.document_token,"state_sha256":expected.state_sha256,"operations":expected.operations}));json!({"success":true,"task_id":"faceb00c"})
                    }else{let response=if pass==0{json!({"recovery_status":"running"})}else if pass==4{let mut wrong=native.clone();wrong["job_id"]=json!(uuid::Uuid::new_v4());wrong}else{native.clone()};json!({"success":true,"task_id":"faceb00c","status":"done","output":json!({"ok":true,"result":response}).to_string()})};
                    let body=format!("<methodResponse><params><param>{}</param></params></methodResponse>",xml(&value));stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len()).as_bytes()).unwrap();
                }
            }
        });
        let cancel=AtomicBool::new(false);assert!(!recover_result_at(&input,&root,&cancel,&endpoint).unwrap());assert!(!root.join("result.json").exists());
        assert!(recover_result_at(&input,&root,&cancel,&endpoint).unwrap());let bytes=std::fs::read(root.join("result.json")).unwrap();
        assert!(recover_result_at(&input,&root,&cancel,&endpoint).unwrap());assert_eq!(std::fs::read(root.join("result.json")).unwrap(),bytes);
        let mut changed:Value=serde_json::from_slice(&bytes).unwrap();changed["after"]["state_sha256"]=json!("c".repeat(64));let manual=serde_json::to_vec_pretty(&changed).unwrap();std::fs::write(root.join("result.json"),&manual).unwrap();
        assert!(recover_result_at(&input,&root,&cancel,&endpoint).is_err());assert_eq!(std::fs::read(root.join("result.json")).unwrap(),manual);
        let other=root.join("wrong-reply");std::fs::create_dir(&other).unwrap();assert!(recover_result_at(&input,&other,&cancel,&endpoint).is_err());assert!(!other.join("result.json").exists());server.join().unwrap();std::fs::remove_dir_all(root).unwrap();
    }
}
