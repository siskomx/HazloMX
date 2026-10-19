//! Checked edits in the explicitly selected patched CADCraft window.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{File, OpenOptions},
    io::{Read, Write},
    mem::{offset_of, size_of},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, WAIT_TIMEOUT},
    NetworkManagement::IpHelper::{GetTcpTable2, MIB_TCPROW2, MIB_TCPTABLE2},
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    },
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    },
};

pub const RUNTIME_SHA256: &str = "fda54c840f0ec0cb63eb9263f386975cb485d3082b81769e6edf5c643b059044";
const LEGACY_RUNTIME_SHA256:&str="bea64cc9f40935947bde7c37221b8ddf6e24e71d6bdf2a54b6156e4e04551bf5";
const INVALID: &str = "CADCraft no confirmó el estado esperado. Relee el dibujo antes de continuar; no repitas una escritura incierta.";
const FRAME_LIMIT: usize = 256 * 1024;
const EXPORT_LIMIT: usize = 128 * 1024;
const VERIFIER_SHA256: &str = "5cc67bb2265a8eae4918379ff139888e3b1b317692798982cf5d109534ba14a7";

// Socket, native binding and runtime handles never enter a model response or saved job.
#[derive(Clone)]
pub struct Connection(Arc<Mutex<Session>>);
struct Session {
    stream: TcpStream,
    process: OwnedHandle,
    runtime_directory: PathBuf,
    _files: Vec<File>,
    instance: Uuid,
    next: u64,
    failed: bool,
    binding: Option<(Value, Observation)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub instance_id: Uuid,
    pub binding_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_sha256: Option<String>,
    pub title: String,
    pub dirty: bool,
    pub insunits: u64,
    pub entities: Vec<Value>,
}
impl Observation {
    fn validate(&self) -> Result<(), String> {
        let mut handles = BTreeSet::new();
        if self.instance_id.is_nil()
            || !crate::kicad_project::valid_hash(&self.binding_sha256)
            || self
                .document_sha256
                .as_ref()
                .is_some_and(|h| !crate::kicad_project::valid_hash(h))
            || self.title.is_empty()
            || self.title.len() > 1024
            || self.title.chars().any(char::is_control)
            || !matches!(self.insunits, 1 | 4)
            || self.entities.len() > 128
            || serde_json::to_vec(self).map_err(|_| INVALID)?.len() > 25000
            || self.entities.iter().any(|e| {
                !e["handle"].as_str().is_some_and(|h| {
                    !h.is_empty()
                        && h.len() <= 32
                        && h.bytes().all(|b| b.is_ascii_hexdigit())
                        && handles.insert(h.to_ascii_uppercase())
                }) || !e["type"].is_string()
                    || !e["geometry"].is_object()
                    || !e["layer"].is_string()
            })
        {
            return Err(INVALID.into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    // shortcut: enable more native commands only with typed inputs and checked geometry readback.
    Circle { center: [f64; 2], diameter: f64 },
    MoveCircle { handle: String, delta: [f64; 2] },
    ExportDxf {},
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub live_drawing_version: u32,
    pub job_id: Uuid,
    pub description: String,
    pub units: crate::cadcraft::Units,
    pub operation: Operation,
    pub before: Observation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sha256: Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub agreement:Option<crate::native_tools::Agreement>,
}
impl Input {
    fn native(&self, value: f64) -> f64 {
        match (self.units, self.before.insunits) {
            (crate::cadcraft::Units::In, 4) => value * 25.4,
            (crate::cadcraft::Units::Mm, 1) => value / 25.4,
            _ => value,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(agreement)=&self.agreement{agreement.validate()?;}
        self.before.validate()?;
        if self.schema_version != 1
            || self.live_drawing_version != 1
            || self.job_id.is_nil()
            || self.description.trim().is_empty()
            || self.description.len() > 1024
            || self.description.chars().any(char::is_control)
            || self
                .receipt_sha256
                .as_ref()
                .is_some_and(|h| !crate::kicad_project::valid_hash(h))
        {
            return Err(INVALID.into());
        }
        let bounded = |n: f64| n.is_finite() && self.native(n).abs() <= 10000.;
        match &self.operation {
            Operation::ExportDxf {} => {
                if self.before.document_sha256.is_none()
                    || !matches!(
                        (self.units, self.before.insunits),
                        (crate::cadcraft::Units::Mm, 4) | (crate::cadcraft::Units::In, 1)
                    )
                {
                    return Err(
                        "Exporta con las unidades actuales del dibujo, sin cambiar su escala."
                            .into(),
                    );
                }
            }
            Operation::Circle { center, diameter } => {
                if self.before.entities.len() >= 128
                    || !center.iter().copied().all(bounded)
                    || !bounded(*diameter)
                    || self.native(*diameter) < 0.001
                {
                    return Err(
                        "Confirma el centro y el diámetro del círculo con sus unidades.".into(),
                    );
                }
            }
            Operation::MoveCircle { handle, delta } => {
                if !delta.iter().copied().all(bounded) || *delta == [0., 0.] {
                    return Err(INVALID.into());
                }
                let e = self
                    .before
                    .entities
                    .iter()
                    .find(|e| e["handle"].as_str() == Some(handle))
                    .ok_or(INVALID)?;
                circle_geometry(e)?;
                let c = &e["geometry"]["center"];
                if (c["x"].as_f64().ok_or(INVALID)? + self.native(delta[0])).abs() > 10000.
                    || (c["y"].as_f64().ok_or(INVALID)? + self.native(delta[1])).abs() > 10000.
                {
                    return Err(INVALID.into());
                }
            }
        }
        Ok(())
    }
    fn command(&self) -> (&str, Value) {
        match &self.operation {
            Operation::ExportDxf {} => ("document.bytes", json!({"format":"dxf"})),
            Operation::Circle { center, diameter } => (
                "circle",
                json!({"center":[self.native(center[0]),self.native(center[1])],"diameter":self.native(*diameter)}),
            ),
            Operation::MoveCircle { handle, delta } => (
                "move",
                json!({"handles":[handle],"delta":[self.native(delta[0]),self.native(delta[1])]}),
            ),
        }
    }
}

fn circle_geometry(e: &Value) -> Result<(), String> {
    if e["type"] != "Circle"
        || e["geometry"]["type"] != "circle"
        || !e["geometry"]["radius"]
            .as_f64()
            .is_some_and(|v| v.is_finite() && v > 0.)
        || ["x", "y", "z"].iter().any(|k| {
            !e["geometry"]["center"][k]
                .as_f64()
                .is_some_and(f64::is_finite)
        })
    {
        return Err("Esta operación requiere un círculo de la lectura actual.".into());
    }
    Ok(())
}

fn pin_runtime(
    executable: &Path,
    expected: &str,
    cancel: &AtomicBool,
) -> Result<Vec<File>, String> {
    crate::project::valid_location(executable).map_err(|_| INVALID)?;
    let mut files =
        crate::project::pin_lineage(executable.parent().ok_or(INVALID)?).map_err(|_| INVALID)?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(executable)
        .map_err(|_| INVALID)?;
    let metadata = file.metadata().map_err(|_| INVALID)?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() > 512 * 1024 * 1024
    {
        return Err(INVALID.into());
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        stop(cancel)?;
        let n = file.read(&mut buffer).map_err(|_| INVALID)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let actual=format!("{:x}", hasher.finalize());
    if actual != expected && !(expected==RUNTIME_SHA256 && actual==LEGACY_RUNTIME_SHA256) {
        return Err(
            "Selecciona el runtime CADCraft parcheado y probado; el instalado no se reemplaza."
                .into(),
        );
    }
    files.push(file);
    Ok(files)
}

/// Explicit user-selected executable and port; no scanning or arbitrary host addresses.
pub fn connect(
    executable: &Path,
    port: u16,
    cancel: &AtomicBool,
) -> Result<(Connection, Observation), String> {
    if port == 0 {
        return Err("Indica el puerto local configurado en CADCraft.".into());
    }
    let files = pin_runtime(executable, RUNTIME_SHA256, cancel)?;
    stop(cancel)?;
    let stream = TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        Duration::from_secs(1),
    )
    .map_err(|_| INVALID)?;
    let process = server_process(&stream, executable)?;
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|_| INVALID)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|_| INVALID)?;
    let connection = Connection(Arc::new(Mutex::new(Session {
        stream,
        process,
        runtime_directory: executable.parent().ok_or(INVALID)?.to_owned(),
        _files: files,
        instance: Uuid::new_v4(),
        next: 0,
        failed: false,
        binding: None,
    })));
    let observation = connection.inspect(cancel)?;
    Ok((connection, observation))
}

fn row_matches(row: &MIB_TCPROW2, local: SocketAddr, peer: SocketAddr) -> bool {
    let (SocketAddr::V4(local), SocketAddr::V4(peer)) = (local, peer) else {
        return false;
    };
    row.dwState == 5
        && row.dwLocalAddr == u32::from_ne_bytes(peer.ip().octets())
        && row.dwRemoteAddr == u32::from_ne_bytes(local.ip().octets())
        && u16::from_be(row.dwLocalPort as u16) == peer.port()
        && u16::from_be(row.dwRemotePort as u16) == local.port()
}
fn server_process(stream: &TcpStream, executable: &Path) -> Result<OwnedHandle, String> {
    let local = stream.local_addr().map_err(|_| INVALID)?;
    let peer = stream.peer_addr().map_err(|_| INVALID)?;
    let mut bytes = 0;
    // SAFETY: the first call only requests the required byte count.
    if unsafe { GetTcpTable2(std::ptr::null_mut(), &mut bytes, 0) } != ERROR_INSUFFICIENT_BUFFER
        || !(4..=4 * 1024 * 1024).contains(&bytes)
    {
        return Err(INVALID.into());
    }
    let mut buffer = vec![0u32; (bytes as usize).div_ceil(size_of::<u32>())];
    // SAFETY: the u32 buffer has the ABI alignment and at least the reported size.
    if unsafe { GetTcpTable2(buffer.as_mut_ptr().cast(), &mut bytes, 0) } != 0 {
        return Err(INVALID.into());
    }
    let offset = offset_of!(MIB_TCPTABLE2, table);
    let count = buffer[0] as usize;
    if offset + count.checked_mul(size_of::<MIB_TCPROW2>()).ok_or(INVALID)? > bytes as usize
        || bytes as usize > buffer.len() * size_of::<u32>()
    {
        return Err(INVALID.into());
    }
    // SAFETY: count, byte bounds and alignment were verified above.
    let rows = unsafe {
        std::slice::from_raw_parts(
            buffer
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<MIB_TCPROW2>(),
            count,
        )
    };
    let mut matches = rows.iter().filter(|r| row_matches(r, local, peer));
    let pid = matches.next().ok_or(INVALID)?.dwOwningPid;
    if matches.next().is_some() {
        return Err(INVALID.into());
    }
    // SAFETY: query-only owned process handle, retained until this connection is dropped.
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    if raw.is_null() {
        return Err(INVALID.into());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    // SAFETY: valid handle and writable UTF-16 buffer with its actual length.
    if unsafe {
        QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut length)
    } == 0
        || unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } != WAIT_TIMEOUT
    {
        return Err(INVALID.into());
    }
    let actual = std::path::PathBuf::from(
        String::from_utf16(&path[..length as usize]).map_err(|_| INVALID)?,
    );
    if actual.canonicalize().map_err(|_| INVALID)?
        != executable.canonicalize().map_err(|_| INVALID)?
    {
        return Err(INVALID.into());
    }
    Ok(process)
}
fn stop(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("Espera cancelada; relee CADCraft antes de continuar. Cancelar no deshace una edición nativa.".into())
    } else {
        Ok(())
    }
}
impl Session {
    fn call(&mut self, method: &str, params: Value, cancel: &AtomicBool) -> Result<Value, String> {
        self.call_with_dispatch(method,params,cancel,None)
    }
    fn call_with_dispatch(&mut self,method:&str,params:Value,cancel:&AtomicBool,dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>)->Result<Value,String>{
        if self.failed {
            return Err("La conexión terminó. Vuelve a conectar el dibujo desde Conexiones y relee su estado. No repitas una escritura incierta.".into());
        }
        let result = self.call_inner(method, params, cancel,dispatch);
        if result.is_err() && dispatch.is_none_or(|d|d.was_dispatched()) {
            self.failed = true;
            self.binding = None;
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
        result
    }
    fn call_inner(
        &mut self,
        method: &str,
        params: Value,
        cancel: &AtomicBool,
        dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>,
    ) -> Result<Value, String> {
        stop(cancel)?;
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } != WAIT_TIMEOUT {
            return Err(INVALID.into());
        }
        self.next = self.next.checked_add(1).ok_or(INVALID)?;
        let mut request =
            serde_json::to_vec(&json!({"id":self.next,"method":method,"params":params}))
                .map_err(|_| INVALID)?;
        if request.len() > 16384 {
            return Err(INVALID.into());
        }
        request.push(b'\n');
        {
            let _permit=dispatch.map(|d|d.enter()).transpose()?;
            self.stream.write_all(&request).map_err(|_| INVALID)?;
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut frame = Vec::new();
        loop {
            stop(cancel)?;
            if Instant::now() >= deadline || frame.len() >= FRAME_LIMIT {
                return Err(INVALID.into());
            }
            let mut byte = [0];
            match self.stream.read(&mut byte) {
                Ok(1) if byte[0] == b'\n' => break,
                Ok(1) => frame.push(byte[0]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                _ => return Err(INVALID.into()),
            }
        }
        let reply: Value = serde_json::from_slice(&frame).map_err(|_| INVALID)?;
        if reply["id"] != self.next || reply["ok"] != true || reply.get("error").is_some() {
            return Err(INVALID.into());
        }
        reply.get("result").cloned().ok_or_else(|| INVALID.into())
    }
    fn read(&mut self, cancel: &AtomicBool) -> Result<Observation, String> {
        self.binding = None;
        let native = self.call("document.observe", json!({}), cancel)?;
        let (binding,observation)=observation_from_native(&native,self.instance)?;
        self.binding=Some((binding,observation.clone()));
        Ok(observation)
    }
}
fn observation_from_native(native:&Value,instance:Uuid)->Result<(Value,Observation),String>{
    let token = &native["binding"];
    let document_id = native["document_id"].as_str().ok_or(INVALID)?;
    if document_id.is_empty()
        || document_id.len() > 32
        || !document_id.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(INVALID.into());
    }
    if !token["token"].as_str().is_some_and(|s| {
        !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_digit())
    }) {
        return Err(INVALID.into());
    }
    let drawing = &native["drawing"];
    let units = &native["units"];
    let entities = drawing["entities"].as_array().ok_or(INVALID)?.clone();
    if drawing["space"] != "Model"
        || !drawing["blockEditor"].is_null()
        || drawing["entityCount"].as_u64() != Some(entities.len() as u64)
        || drawing["skipped"]["count"] != 0
    {
        return Err("Esta conexión requiere un dibujo completo en Modelo, sin bloques en edición ni geometría omitida.".into());
    }
    let observation = Observation {
        instance_id: instance,
        binding_sha256: crate::kicad_project::digest(
            &serde_json::to_vec(&json!([instance, token])).map_err(|_| INVALID)?,
        ),
        document_sha256: Some(crate::kicad_project::digest(
            &serde_json::to_vec(&json!([instance, document_id])).map_err(|_| INVALID)?,
        )),
        title: drawing["title"].as_str().ok_or(INVALID)?.into(),
        dirty: drawing["dirty"].as_bool().ok_or(INVALID)?,
        insunits: units["insunits"].as_u64().ok_or(INVALID)?,
        entities,
    };
    observation.validate()?;
    Ok((json!({"token":token["token"]}),observation))
}
impl Connection {
    pub(crate) fn native_mcp_config(&self)->Result<crate::native_tools::McpConfig,String> {
        let session=self.0.lock().map_err(|_|INVALID)?;
        if session.failed || unsafe {WaitForSingleObject(session.process.as_raw_handle() as _,0)}!=WAIT_TIMEOUT {return Err(INVALID.into());}
        let address=session.stream.peer_addr().map_err(|_|INVALID)?;
        if !address.ip().is_loopback(){return Err(INVALID.into());}
        Ok(crate::native_tools::McpConfig{executable:session.runtime_directory.join("cadcraft-cli.exe"),args:vec!["mcp".into(),"--connect".into(),address.to_string()],env:BTreeMap::new(),url:None,authorization:None})
    }
    pub fn inspect(&self, cancel: &AtomicBool) -> Result<Observation, String> {
        self.0.lock().map_err(|_| INVALID)?.read(cancel)
    }
    pub fn validate_for(&self, before: &Observation) -> Result<(), String> {
        if self
            .0
            .lock()
            .map_err(|_| INVALID)?
            .binding
            .as_ref()
            .is_none_or(|(_, b)| b != before)
        {
            return Err(INVALID.into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub native_drawing_version: u32,
    pub job_id: Uuid,
    pub before_sha256: String,
    pub after: Observation,
    pub saved: bool,
    pub native_undo_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_document: Option<crate::kicad_project::CapturedFile>,
}
pub fn validate_report(input: &Input, report: &Receipt) -> Result<(), String> {
    input.validate()?;
    report.after.validate()?;
    let export = matches!(input.operation, Operation::ExportDxf {});
    if export {
        let file = report.exported_document.as_ref().ok_or(INVALID)?;
        if file.path != "Drawing.dxf"
            || file.bytes == 0
            || file.bytes > EXPORT_LIMIT as u64
            || !crate::kicad_project::valid_hash(&file.sha256)
        {
            return Err(INVALID.into());
        }
    } else if report.exported_document.is_some() {
        return Err(INVALID.into());
    }
    if report.native_drawing_version != 1
        || report.job_id != input.job_id
        || report.before_sha256 != input.before.binding_sha256
        || report.after.instance_id != input.before.instance_id
        || report.after.document_sha256 != input.before.document_sha256
        || report.after.title != input.before.title
        || report.after.binding_sha256 == input.before.binding_sha256
        || report.after.insunits != input.before.insunits
        || report.saved
        || report.native_undo_available == export
        || (!export && !report.after.dirty)
        || (export && report.after.dirty != input.before.dirty)
    {
        return Err(INVALID.into());
    }
    let mut expected = input.before.entities.clone();
    let mut actual = report.after.entities.clone();
    match &input.operation {
        Operation::ExportDxf {} => (),
        Operation::MoveCircle { handle, delta } => {
            let e = expected
                .iter_mut()
                .find(|e| e["handle"].as_str() == Some(handle))
                .ok_or(INVALID)?;
            for (axis, d) in ["x", "y"].into_iter().zip(delta) {
                e["geometry"]["center"][axis] = json!(
                    e["geometry"]["center"][axis].as_f64().ok_or(INVALID)? + input.native(*d)
                );
            }
        }
        Operation::Circle { center, diameter } => {
            let added: Vec<_> = actual
                .iter()
                .filter(|a| !expected.iter().any(|e| e["handle"] == a["handle"]))
                .collect();
            if added.len() != 1 {
                return Err(INVALID.into());
            }
            let added = added[0];
            circle_geometry(added)?;
            let c = &added["geometry"]["center"];
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-8;
            if !close(c["x"].as_f64().ok_or(INVALID)?, input.native(center[0]))
                || !close(c["y"].as_f64().ok_or(INVALID)?, input.native(center[1]))
                || c["z"] != 0.
                || !close(
                    added["geometry"]["radius"].as_f64().ok_or(INVALID)?,
                    input.native(*diameter) / 2.,
                )
            {
                return Err(INVALID.into());
            }
            expected.push(added.clone());
        }
    }
    expected.sort_by_key(|e| e["handle"].to_string());
    actual.sort_by_key(|e| e["handle"].to_string());
    if actual != expected {
        return Err(INVALID.into());
    }
    Ok(())
}
pub fn run(
    input: &Input,
    connection: &Connection,
    stage: &Path,
    profile: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    run_impl(input,connection,stage,profile,cancel,None)
}
pub(crate) fn run_gated(input:&Input,connection:&Connection,stage:&Path,profile:&Path,cancel:Arc<AtomicBool>,dispatch:&Arc<crate::live_dispatch::Dispatch>)->Result<(),String>{
    run_impl(input,connection,stage,profile,cancel,Some(dispatch))
}
fn run_impl(input:&Input,connection:&Connection,stage:&Path,profile:&Path,cancel:Arc<AtomicBool>,dispatch:Option<&Arc<crate::live_dispatch::Dispatch>>)->Result<(),String>{
    input.validate()?;
    let mut session = connection.0.lock().map_err(|_| INVALID)?;
    let (binding, before) = session.binding.take().ok_or(INVALID)?;
    if before != input.before {
        return Err(INVALID.into());
    }
    let (command, params) = input.command();
    let methods=session.call("control.methods",json!({}),&cancel)?;
    let recorded=methods.as_array().ok_or(INVALID)?.iter().any(|method|method["method"]=="engine.execute_recorded");
    if recorded {
        if matches!(input.operation,Operation::ExportDxf{}){
            let executable=session.runtime_directory.join("cadcraft-cli.exe");let _pin=pin_runtime(&executable,VERIFIER_SHA256,&cancel)?;
            let location=json!({"version":1,"input_sha256":completion_identity(input)?,"executable":executable});
            crate::kicad_project::write_new(&stage.join("drawing-verifier.json"),&serde_json::to_vec(&location).map_err(|_|INVALID)?)?;
        }
        let result=session.call_with_dispatch("engine.execute_recorded",json!({"expected":binding,"command":command,"params":params,"input_sha256":completion_identity(input)?,"receipt_path":stage.join("drawing-completion.json")}),&cancel,dispatch)?;
        let proof=read_completion(input,stage)?;
        if result!=proof{return Err("El recibo nativo cambió; se conserva la pausa sin repetir la edición.".into());}
        if proof["outcome"]=="not_dispatched"{
            crate::live_dispatch::recover_refused(stage,&crate::project::OperationInput::LiveDrawing(input.clone()))?;
            session.failed=true;let _=session.stream.shutdown(std::net::Shutdown::Both);
            return Err("CADCraft confirmó que no inició la edición porque su precondición cambió. Conserva el trabajo y relee el dibujo antes de preparar otra propuesta.".into());
        }
        return finish_completion(input,&proof,stage,Some(&session.runtime_directory),profile,cancel);
    }
    let result = session.call_with_dispatch(
        "engine.execute_checked",
        json!({"expected":binding,"command":command,"params":params}),
        &cancel,
        dispatch,
    )?;
    let exported_document = if matches!(input.operation, Operation::ExportDxf {}) {
        Some(export_dxf(
            &result,
            &before,
            &session.runtime_directory,
            stage,
            profile,
            cancel.clone(),
        )?)
    } else {
        None
    };
    let after = session.read(&cancel)?;
    let report = Receipt {
        native_drawing_version: 1,
        job_id: input.job_id,
        before_sha256: input.before.binding_sha256.clone(),
        after,
        saved: false,
        native_undo_available: exported_document.is_none(),
        exported_document,
    };
    validate_report(input, &report)?;
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| INVALID)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("result.json"))
        .map_err(|_| INVALID)?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| INVALID.to_string())
}

fn completion_identity(input:&Input)->Result<String,String>{
    let mut input=input.clone();input.receipt_sha256=None;
    serde_json::to_vec(&input).map(|bytes|crate::kicad_project::digest(&bytes)).map_err(|_|INVALID.into())
}
fn read_completion(input:&Input,directory:&Path)->Result<Value,String>{
    let bytes=crate::kicad_project::read_regular(&directory.join("drawing-completion.json"),FRAME_LIMIT as u64).map_err(|_|"Falta un recibo original completo de CADCraft; se conserva la pausa sin repetir la operación.")?;
    let proof:Value=serde_json::from_slice(&bytes).map_err(|_|INVALID)?;
    let completed=proof["outcome"]=="completed" && proof.as_object().map(|p|p.len())==Some(5) && proof.get("command_result").is_some() && proof.get("after").is_some();
    let refused=proof["outcome"]=="not_dispatched" && proof.as_object().map(|p|p.len())==Some(3);
    if (!completed && !refused) || proof["version"]!=1 || proof["input_sha256"]!=completion_identity(input)?{return Err("El recibo CADCraft no coincide con la operación aprobada; se conserva la pausa.".into());}
    Ok(proof)
}
pub(crate) fn refusal_proof(input:&Input,directory:&Path)->Result<bool,String>{Ok(read_completion(input,directory)?["outcome"]=="not_dispatched")}
fn finish_completion(input:&Input,proof:&Value,directory:&Path,runtime:Option<&Path>,profile:&Path,cancel:Arc<AtomicBool>)->Result<(),String>{
    let (_,after)=observation_from_native(&proof["after"],input.before.instance_id)?;
    let exported_document=if matches!(input.operation,Operation::ExportDxf{}){Some(export_dxf(&proof["command_result"],&input.before,runtime.ok_or("Falta el verificador CADCraft seleccionado; el recibo sigue local y no se repitió la exportación.")?,directory,profile,cancel)?)}else{None};
    let report=Receipt{native_drawing_version:1,job_id:input.job_id,before_sha256:input.before.binding_sha256.clone(),after,saved:false,native_undo_available:exported_document.is_none(),exported_document};
    validate_artifacts(directory,input,&report)?;
    let bytes=serde_json::to_vec_pretty(&report).map_err(|_|INVALID)?;let path=directory.join("result.json");
    if path.try_exists().map_err(|_|INVALID)?{
        let saved:Value=serde_json::from_slice(&crate::kicad_project::read_regular(&path,65536)?).map_err(|_|INVALID)?;
        if saved!=serde_json::to_value(&report).map_err(|_|INVALID)?{return Err("El informe CADCraft cambió; se conserva sin reemplazarlo.".into());}
    }else{crate::kicad_project::write_new(&path,&bytes).map_err(|_|"El recibo sigue local; no se pudo conservar su informe. No repitas la operación.")?;}
    Ok(())
}
pub(crate) fn recover_result(input:&Input,directory:&Path,profile:&Path,cancel:Arc<AtomicBool>)->Result<bool,String>{
    input.validate()?;stop(&cancel)?;
    let _pins=crate::project::pin_lineage(directory).map_err(|_|INVALID)?;
    let proof=read_completion(input,directory)?;
    if proof["outcome"]!="completed"{return Err("CADCraft no inició esta edición; no existe un resultado completado que publicar.".into());}
    let runtime=if matches!(input.operation,Operation::ExportDxf{}){
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Location{version:u32,input_sha256:String,executable:PathBuf}
        let location:Location=serde_json::from_slice(&crate::kicad_project::read_regular(&directory.join("drawing-verifier.json"),8192)?).map_err(|_|"Falta la ubicación verificada del exportador; el recibo sigue local.")?;
        crate::project::valid_location(&location.executable).map_err(|_|INVALID)?;
        if location.version!=1 || location.input_sha256!=completion_identity(input)? || location.executable.file_name().and_then(|s|s.to_str())!=Some("cadcraft-cli.exe"){return Err("La ubicación del verificador no coincide; se conserva el resultado sin repetir la exportación.".into());}
        Some(location.executable.parent().ok_or(INVALID)?.to_owned())
    }else{None};
    finish_completion(input,&proof,directory,runtime.as_deref(),profile,cancel)?;Ok(true)
}

pub(crate) fn validate_artifacts(
    directory: &Path,
    input: &Input,
    report: &Receipt,
) -> Result<(), String> {
    validate_report(input, report)?;
    if let Some(file) = &report.exported_document {
        let bytes =
            crate::kicad_project::read_regular(&directory.join(&file.path), EXPORT_LIMIT as u64)?;
        if bytes.len() as u64 != file.bytes || crate::kicad_project::digest(&bytes) != file.sha256 {
            return Err(INVALID.into());
        }
    }
    Ok(())
}

fn decode_export(result: &Value) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let data = result["data"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= EXPORT_LIMIT * 4 / 3 + 4)
        .ok_or(INVALID)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| INVALID)?;
    if bytes.is_empty()
        || bytes.len() > EXPORT_LIMIT
        || result["bytes"].as_u64() != Some(bytes.len() as u64)
        || result.get("notSaved").is_some()
    {
        return Err(INVALID.into());
    }
    Ok(bytes)
}

fn verify_export_output(stdout: &[u8], before: &Observation) -> Result<(), String> {
    let lines: Vec<_> = stdout
        .split(|b| *b == b'\n')
        .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
        .collect();
    if lines.len() != 2 {
        return Err(INVALID.into());
    }
    let drawing: Value = serde_json::from_slice(lines[0]).map_err(|_| INVALID)?;
    let units: Value = serde_json::from_slice(lines[1]).map_err(|_| INVALID)?;
    let mut expected = before.entities.clone();
    let mut actual = drawing["entities"].as_array().ok_or(INVALID)?.clone();
    expected.sort_by_key(|e| e["handle"].to_string());
    actual.sort_by_key(|e| e["handle"].to_string());
    if drawing["entityCount"].as_u64() != Some(expected.len() as u64)
        || drawing["skipped"]["count"] != 0
        || drawing["space"] != "Model"
        || !drawing["blockEditor"].is_null()
        || units["insunits"].as_u64() != Some(before.insunits)
        || actual != expected
    {
        return Err(INVALID.into());
    }
    Ok(())
}

fn export_dxf(
    result: &Value,
    before: &Observation,
    runtime: &Path,
    stage: &Path,
    profile: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<crate::kicad_project::CapturedFile, String> {
    let bytes = decode_export(result)?;
    stop(&cancel)?;
    let path = stage.join("Drawing.dxf");
    if path.try_exists().map_err(|_|INVALID)? {
        if crate::kicad_project::read_regular(&path,EXPORT_LIMIT as u64)?!=bytes{return Err("La copia DXF cambió; se conserva sin reemplazarla.".into());}
    }else{let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| INVALID)?;
    output
        .write_all(&bytes)
        .and_then(|_| output.sync_all())
        .map_err(|_| INVALID)?;
    drop(output);}
    let _drawing_pin = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)
        .map_err(|_| INVALID)?;
    let metadata = _drawing_pin.metadata().map_err(|_| INVALID)?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(INVALID.into());
    }
    let executable = runtime.join("cadcraft-cli.exe");
    let _runtime_pin = pin_runtime(&executable, VERIFIER_SHA256, &cancel)?;
    std::fs::create_dir(profile).map_err(|_| INVALID)?;
    let mut env = BTreeMap::<OsString, OsString>::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for key in [
        "TEMP",
        "TMP",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "CADCRAFT_CONFIG_DIR",
    ] {
        env.insert(key.into(), profile.as_os_str().to_owned());
    }
    let args = [
        "run".into(),
        path.as_os_str().to_owned(),
        "--cmd".into(),
        "drawing.inspect {\"entities\":true,\"limit\":128}".into(),
        "--cmd".into(),
        "units {}".into(),
    ];
    let reply = crate::process::run_owned(
        &executable,
        &args,
        profile,
        &env,
        cancel.clone(),
        Duration::from_secs(60),
    )
    .map_err(|_| INVALID)?;
    stop(&cancel)?;
    if reply.reason != crate::process::StopReason::Completed
        || reply.exit_code != Some(0)
        || reply.truncated
        || !reply.stderr.iter().all(u8::is_ascii_whitespace)
    {
        return Err(INVALID.into());
    }
    verify_export_output(&reply.stdout, before)?;
    Ok(crate::kicad_project::CapturedFile {
        path: "Drawing.dxf".into(),
        sha256: crate::kicad_project::digest(&bytes),
        bytes: bytes.len() as u64,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires explicitly selected owned CLI export fixtures and the maintained native verifier"]
    fn verifies_real_native_exports_with_the_owned_process_runner() {
        let root = PathBuf::from(std::env::var_os("HAZLOMEXA_OWNED_DXF_FIXTURES").unwrap());
        let runtime = PathBuf::from(std::env::var_os("HAZLOMEXA_OWNED_DXF_RUNTIME").unwrap());
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(workspace.join("target").canonicalize().unwrap().as_path())
        );
        assert!(root
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("cadcraft-export-roundtrip-"));
        for name in ["metric", "imperial"] {
            let bytes = crate::kicad_project::read_regular(
                &root.join(format!("{name}-initial.stdout")),
                FRAME_LIMIT as u64,
            )
            .unwrap();
            let records: Vec<Value> = serde_json::Deserializer::from_slice(&bytes)
                .into_iter()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(records.len(), 3);
            let before = Observation {
                instance_id: Uuid::new_v4(),
                binding_sha256: "a".repeat(64),
                document_sha256: Some("c".repeat(64)),
                title: records[0]["title"].as_str().unwrap().into(),
                dirty: records[0]["dirty"].as_bool().unwrap(),
                insunits: records[1]["insunits"].as_u64().unwrap(),
                entities: records[0]["entities"].as_array().unwrap().clone(),
            };
            before.validate().unwrap();
            let stage = root.join(format!("production-{name}-{}", Uuid::new_v4()));
            std::fs::create_dir(&stage).unwrap();
            let file = export_dxf(
                &records[2],
                &before,
                &runtime,
                &stage,
                &stage.join("profile"),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(
                std::fs::read(stage.join("Drawing.dxf")).unwrap(),
                decode_export(&records[2]).unwrap()
            );
            let mut after = before.clone();
            after.binding_sha256 = "b".repeat(64);
            let input = Input {agreement:Some(crate::native_tools::tests::agreement_fixture()),
                schema_version: 1,
                live_drawing_version: 1,
                job_id: Uuid::new_v4(),
                description: "Owned native CLI export verification".into(),
                units: if before.insunits == 4 {
                    crate::cadcraft::Units::Mm
                } else {
                    crate::cadcraft::Units::In
                },
                operation: Operation::ExportDxf {},
                before,
                receipt_sha256: None,
            };
            let report = Receipt {
                native_drawing_version: 1,
                job_id: input.job_id,
                before_sha256: input.before.binding_sha256.clone(),
                after,
                saved: false,
                native_undo_available: false,
                exported_document: Some(file),
            };
            validate_artifacts(&stage, &input, &report).unwrap();
        }
    }
    pub(crate) fn fixture_input() -> Input {
        Input {agreement:Some(crate::native_tools::tests::agreement_fixture()),
            schema_version: 1,
            live_drawing_version: 1,
            job_id: Uuid::new_v4(),
            description: "Move right by one tenth of an inch".into(),
            units: crate::cadcraft::Units::In,
            operation: Operation::MoveCircle {
                handle: "100".into(),
                delta: [0.1, 0.],
            },
            receipt_sha256: None,
            before: Observation {
                instance_id: Uuid::new_v4(),
                binding_sha256: "a".repeat(64),
                document_sha256: Some("c".repeat(64)),
                title: "Owned.dxf".into(),
                dirty: true,
                insunits: 4,
                entities: vec![
                    json!({"handle":"100","type":"Circle","layer":"0","geometry":{"type":"circle","center":{"x":0.,"y":0.,"z":0.},"radius":10.}}),
                ],
            },
        }
    }
    pub(crate) fn receipt(input: &Input) -> Receipt {
        let mut after = input.before.clone();
        after.binding_sha256 = "b".repeat(64);
        after.entities[0]["geometry"]["center"]["x"] = json!(2.54);
        Receipt {
            native_drawing_version: 1,
            job_id: input.job_id,
            before_sha256: input.before.binding_sha256.clone(),
            after,
            saved: false,
            native_undo_available: true,
            exported_document: None,
        }
    }
    pub(crate) fn recorded_fixture()->(Input,Value){
        let mut input=fixture_input();input.before.document_sha256=Some(crate::kicad_project::digest(&serde_json::to_vec(&json!([input.before.instance_id,"77"])).unwrap()));let report=receipt(&input);
        let native=json!({"binding":{"token":"99"},"document_id":"77","drawing":{"title":report.after.title,"dirty":report.after.dirty,"entities":report.after.entities,"entityCount":report.after.entities.len(),"skipped":{"count":0},"space":"Model","blockEditor":null},"units":{"insunits":report.after.insunits}});
        let proof=json!({"version":1,"input_sha256":completion_identity(&input).unwrap(),"outcome":"completed","command_result":{},"after":native});(input,proof)
    }
    #[test]
    fn recorded_recovery_requires_original_proof_and_preserves_changed_reports(){
        for mode in ["missing","pending","wrong_input","wrong_document","changed_report","success"]{
            let root=std::env::temp_dir().join(format!("hazlo-drawing-proof-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();let (input,mut proof)=recorded_fixture();
            if mode=="wrong_input"{proof["input_sha256"]=json!("f".repeat(64));}else if mode=="wrong_document"{proof["after"]["document_id"]=json!("78");}
            if mode!="missing"{std::fs::write(root.join(if mode=="pending"{"drawing-completion.json.pending"}else{"drawing-completion.json"}),serde_json::to_vec(&proof).unwrap()).unwrap();}
            if mode=="changed_report"{std::fs::write(root.join("result.json"),b"preserve manual report").unwrap();}
            let result=recover_result(&input,&root,&root.join("unused-profile"),Arc::new(AtomicBool::new(false)));
            if mode=="success"{assert!(result.unwrap());let saved=std::fs::read(root.join("result.json")).unwrap();assert!(recover_result(&input,&root,&root.join("unused-profile"),Arc::new(AtomicBool::new(false))).unwrap());assert_eq!(std::fs::read(root.join("result.json")).unwrap(),saved);let report:Receipt=serde_json::from_slice(&saved).unwrap();validate_report(&input,&report).unwrap();}
            else{assert!(result.is_err(),"{mode}");if mode=="changed_report"{assert_eq!(std::fs::read(root.join("result.json")).unwrap(),b"preserve manual report");}else{assert!(!root.join("result.json").exists());}}
            assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn export_requires_native_scale_and_preserves_the_open_document() {
        let mut input = fixture_input();
        input.units = crate::cadcraft::Units::Mm;
        input.operation = Operation::ExportDxf {};
        assert_eq!(input.command(), ("document.bytes", json!({"format":"dxf"})));
        assert!(serde_json::from_value::<Operation>(
            json!({"action":"export_dxf","path":"elsewhere"})
        )
        .is_err());
        for dirty in [false, true] {
            input.before.dirty = dirty;
            input.validate().unwrap();
            let mut after = input.before.clone();
            after.binding_sha256 = "b".repeat(64);
            let mut report = Receipt {
                native_drawing_version: 1,
                job_id: input.job_id,
                before_sha256: input.before.binding_sha256.clone(),
                after,
                saved: false,
                native_undo_available: false,
                exported_document: Some(crate::kicad_project::CapturedFile {
                    path: "Drawing.dxf".into(),
                    sha256: "e".repeat(64),
                    bytes: 100,
                }),
            };
            validate_report(&input, &report).unwrap();
            report.after.dirty = !dirty;
            assert!(validate_report(&input, &report).is_err());
            report.after.dirty = dirty;
            report.after.entities[0]["geometry"]["radius"] = json!(99.);
            assert!(validate_report(&input, &report).is_err());
            report.after.entities = input.before.entities.clone();
            report.native_undo_available = true;
            assert!(validate_report(&input, &report).is_err());
            report.native_undo_available = false;
            report.exported_document.as_mut().unwrap().path = "../Drawing.dxf".into();
            assert!(validate_report(&input, &report).is_err());
        }
        input.units = crate::cadcraft::Units::In;
        assert!(input.validate().is_err());
        input.before.insunits = 1;
        input.validate().unwrap();
        input.before.document_sha256 = None;
        assert!(input.validate().is_err());
    }
    #[test]
    fn exported_bytes_and_reopened_geometry_must_be_complete_and_exact() {
        assert_eq!(
            decode_export(&json!({"data":"RFhG","bytes":3})).unwrap(),
            b"DXF"
        );
        for result in [
            json!({"data":"***","bytes":3}),
            json!({"data":"RFhG","bytes":4}),
            json!({"data":"RFhG","bytes":3,"notSaved":{"CIRCLE":1}}),
            json!({"data":"A".repeat(EXPORT_LIMIT*4/3+8),"bytes":1}),
        ] {
            assert!(decode_export(&result).is_err());
        }
        let input = fixture_input();
        let mut drawing = json!({"space":"Model","blockEditor":null,"entityCount":1,"skipped":{"count":0},"entities":input.before.entities});
        let output =
            |d: &Value, units: u64| format!("{d}\n{}\n", json!({"insunits":units})).into_bytes();
        verify_export_output(&output(&drawing, 4), &input.before).unwrap();
        assert!(verify_export_output(&output(&drawing, 1), &input.before).is_err());
        drawing["entities"][0]["geometry"]["radius"] = json!(99.);
        assert!(verify_export_output(&output(&drawing, 4), &input.before).is_err());
        drawing["entities"] = json!(input.before.entities);
        drawing["skipped"]["count"] = json!(1);
        assert!(verify_export_output(&output(&drawing, 4), &input.before).is_err());
        assert!(verify_export_output(b"{}\n{}\n{}\n", &input.before).is_err());
    }
    #[test]
    fn exported_artifact_hash_is_checked_on_recovery() {
        let root =
            std::env::temp_dir().join(format!("hazlomexa-drawing-export-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let mut input = fixture_input();
        input.units = crate::cadcraft::Units::Mm;
        input.operation = Operation::ExportDxf {};
        let mut after = input.before.clone();
        after.binding_sha256 = "b".repeat(64);
        let report = Receipt {
            native_drawing_version: 1,
            job_id: input.job_id,
            before_sha256: input.before.binding_sha256.clone(),
            after,
            saved: false,
            native_undo_available: false,
            exported_document: Some(crate::kicad_project::CapturedFile {
                path: "Drawing.dxf".into(),
                sha256: crate::kicad_project::digest(b"DXF"),
                bytes: 3,
            }),
        };
        std::fs::write(root.join("Drawing.dxf"), b"DXF").unwrap();
        validate_artifacts(&root, &input, &report).unwrap();
        std::fs::write(root.join("Drawing.dxf"), b"BAD").unwrap();
        assert!(validate_artifacts(&root, &input, &report).is_err());
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn converts_requested_units_once_and_checks_complete_readback() {
        let mut input = fixture_input();
        input.validate().unwrap();
        assert_eq!(
            input.command(),
            ("move", json!({"handles":["100"],"delta":[2.54,0.]}))
        );
        let mut report = receipt(&input);
        validate_report(&input, &report).unwrap();
        report.after.entities[0]["geometry"]["radius"] = json!(11.);
        assert!(
            validate_report(&input, &report).is_err(),
            "Unrequested geometry must not pass"
        );
        report = receipt(&input);
        report.after.instance_id = Uuid::new_v4();
        assert!(validate_report(&input, &report).is_err());
        report = receipt(&input);
        report.saved = true;
        assert!(validate_report(&input, &report).is_err());
        input.before.insunits = 1;
        input.units = crate::cadcraft::Units::Mm;
        input.operation = Operation::Circle {
            center: [25.4, 0.],
            diameter: 50.8,
        };
        assert_eq!(
            input.command(),
            ("circle", json!({"center":[1.,0.],"diameter":2.}))
        );
    }
    #[test]
    fn report_rejects_reopened_identical_document_and_reads_legacy_receipts() {
        let mut input = fixture_input();
        let mut report = receipt(&input);
        report.after.document_sha256 = Some("d".repeat(64));
        assert!(validate_report(&input, &report).is_err());
        report.after.document_sha256 = None;
        assert!(validate_report(&input, &report).is_err());
        input.before.document_sha256 = None;
        let legacy = serde_json::to_value(&report).unwrap();
        assert!(legacy["after"].get("document_sha256").is_none());
        let recovered: Receipt = serde_json::from_value(legacy).unwrap();
        validate_report(&input, &recovered).unwrap();
        report.after.document_sha256 = Some("invalid".into());
        assert!(validate_report(&input, &report).is_err());
    }
    #[test]
    fn reads_one_atomic_native_observation_without_exposing_its_identity() {
        let reply = json!({"id":1,"ok":true,"result":{
            "binding":{"token":"22"},"document_id":"123456789",
            "drawing":{"title":"Owned.dxf","dirty":false,"space":"Model","blockEditor":null,
                "entityCount":0,"skipped":{"count":0},"entities":[]},"units":{"insunits":4}
        }});
        let mut bytes = serde_json::to_vec(&reply).unwrap();
        bytes.push(b'\n');
        let (mut session, task) = mock(bytes);
        let observation = session.read(&AtomicBool::new(false)).unwrap();
        assert_eq!(session.next, 1);
        assert!(observation
            .document_sha256
            .as_ref()
            .is_some_and(|h| crate::kicad_project::valid_hash(h)));
        assert!(!serde_json::to_string(&observation)
            .unwrap()
            .contains("123456789"));
        assert_eq!(session.binding.as_ref().unwrap().0, json!({"token":"22"}));
        drop(session);
        task.join().unwrap();
    }
    #[test]
    fn refuses_unknown_commands_nonfinite_ambiguous_and_missing_targets() {
        let mut input = fixture_input();
        assert!(serde_json::from_value::<Operation>(json!({"action":"qsave"})).is_err());
        assert!(serde_json::from_value::<Operation>(
            json!({"action":"circle","center":[0,0],"diameter":2,"path":"ignored"})
        )
        .is_err());
        input.operation = Operation::Circle {
            center: [f64::NAN, 0.],
            diameter: 1.,
        };
        assert!(input.validate().is_err());
        input.operation = Operation::Circle {
            center: [0., 0.],
            diameter: 0.,
        };
        assert!(input.validate().is_err());
        input.operation = Operation::MoveCircle {
            handle: "MISSING".into(),
            delta: [1., 0.],
        };
        assert!(input.validate().is_err());
        input = fixture_input();
        input.before.entities.push(input.before.entities[0].clone());
        assert!(input.validate().is_err());
    }

    #[test]
    fn native_journal_round_trip_uses_its_own_mcp_schema() {
        use crate::project::{OperationInput, Phase, Project, RunSource};
        let root =
            std::env::temp_dir().join(format!("hazlomexa-drawing-journal-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let project = Project::create(&root).unwrap();
        let mut snapshot = project
            .snapshot(
                OperationInput::LiveDrawing(fixture_input()),
                Phase::AwaitingApproval,
            )
            .unwrap();
        assert_eq!(snapshot.schema_version, 16);
        snapshot.audit.as_mut().unwrap().source = RunSource::Mcp;
        project.save(&snapshot).unwrap();
        assert_eq!(project.recover().unwrap()[0].schema_version, 16);
        snapshot.schema_version = 10;
        assert!(project.save(&snapshot).is_err());
        drop(project);
        assert_eq!(
            root.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn tcp_owner_uses_server_side_established_tuple_and_pins_process() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let process = server_process(&client, &std::env::current_exe().unwrap()).unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(process.as_raw_handle(), 0) },
            WAIT_TIMEOUT
        );
        assert!(server_process(&client, Path::new("C:/not-the-selected-runtime.exe")).is_err());
        drop(server);
    }
    fn mock(reply: Vec<u8>) -> (Session, std::thread::JoinHandle<()>) {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let process = server_process(&client, &std::env::current_exe().unwrap()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let task = std::thread::spawn(move || {
            let mut stream = std::io::BufReader::new(server);
            let mut request = String::new();
            stream.read_line(&mut request).unwrap();
            assert_eq!(serde_json::from_str::<Value>(&request).unwrap()["id"], 1);
            let _ = stream.get_mut().write_all(&reply);
            request.clear();
            assert_eq!(
                stream.read_line(&mut request).unwrap_or(0),
                0,
                "Failed connection sent another request"
            );
        });
        (
            Session {
                stream: client,
                process,
                runtime_directory: PathBuf::new(),
                _files: vec![],
                instance: Uuid::new_v4(),
                next: 0,
                failed: false,
                binding: None,
            },
            task,
        )
    }
    #[test]
    fn bad_frames_close_connection_and_never_retry() {
        for reply in [
            b"{\"id\":2,\"ok\":true,\"result\":{}}\n".to_vec(),
            b"{invalid}\n".to_vec(),
            vec![b'x'; FRAME_LIMIT],
            b"{\"id\":1,\"ok\":false,\"error\":\"private path\"}\n".to_vec(),
        ] {
            let (mut session, task) = mock(reply);
            let error = session
                .call("drawing.inspect", json!({}), &AtomicBool::new(false))
                .unwrap_err();
            assert!(!error.contains("private path"));
            assert!(session
                .call("drawing.inspect", json!({}), &AtomicBool::new(false))
                .unwrap_err()
                .contains("Conexiones"));
            assert_eq!(session.next, 1);
            task.join().unwrap();
        }
    }

    #[test]
    fn cancelled_inflight_read_closes_instead_of_reusing_a_late_reply() {
        let (mut session, task) = mock(Vec::new());
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let cancelling = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            flag.store(true, Ordering::SeqCst);
        });
        assert!(session.call("drawing.inspect", json!({}), &cancel).is_err());
        cancelling.join().unwrap();
        assert!(session.failed && session.next == 1);
        assert!(session
            .call("drawing.inspect", json!({}), &AtomicBool::new(false))
            .is_err());
        task.join().unwrap();
    }
    #[test]
    fn socket_dispatch_fence_keeps_unsent_connection_and_releases_during_reply_wait() {
        use std::io::BufRead;
        let root=std::env::temp_dir().join(format!("hazlo-drawing-fence-{}",Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
        assert!(root.canonicalize().unwrap().starts_with(std::env::temp_dir().canonicalize().unwrap()));
        let listener=std::net::TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();let client=TcpStream::connect(listener.local_addr().unwrap()).unwrap();let (mut server,_)=listener.accept().unwrap();
        let process=server_process(&client,&std::env::current_exe().unwrap()).unwrap();client.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let mut session=Session{stream:client,process,runtime_directory:PathBuf::new(),_files:vec![],instance:Uuid::new_v4(),next:0,failed:false,binding:None};
        let input=crate::project::OperationInput::LiveDrawing(fixture_input());let gate=Arc::new(Mutex::new(()));
        let first=crate::live_dispatch::Dispatch::start(&root,&input,gate.clone(),Arc::new(AtomicBool::new(true)),Arc::new(AtomicBool::new(false))).unwrap();
        assert!(session.call_with_dispatch("engine.execute_checked",json!({}),&AtomicBool::new(false),Some(&first)).is_err());assert!(!session.failed);
        server.set_nonblocking(true).unwrap();assert_eq!(server.peek(&mut [0]).unwrap_err().kind(),std::io::ErrorKind::WouldBlock);server.set_nonblocking(false).unwrap();
        let next=root.join("next");std::fs::create_dir(&next).unwrap();let paused=Arc::new(AtomicBool::new(false));
        let dispatch=crate::live_dispatch::Dispatch::start(&next,&input,gate.clone(),paused.clone(),Arc::new(AtomicBool::new(false))).unwrap();let selected=dispatch.clone();
        let (sent,received)=std::sync::mpsc::channel();let (release,wait)=std::sync::mpsc::channel();
        let server_task=std::thread::spawn(move||{let mut reader=std::io::BufReader::new(&mut server);let mut frame=String::new();reader.read_line(&mut frame).unwrap();let request:Value=serde_json::from_str(&frame).unwrap();sent.send(()).unwrap();wait.recv().unwrap();let reply=json!({"id":request["id"],"ok":true,"result":{"accepted":true}});reader.get_mut().write_all(format!("{reply}\n").as_bytes()).unwrap();});
        let worker=std::thread::spawn(move||{let value=session.call_with_dispatch("engine.execute_checked",json!({}),&AtomicBool::new(false),Some(&selected)).unwrap();assert!(!session.failed);value});
        received.recv_timeout(Duration::from_secs(5)).unwrap();let permit=gate.try_lock().expect("Correction must not wait for the native reply");paused.store(true,Ordering::SeqCst);drop(permit);
        release.send(()).unwrap();assert_eq!(worker.join().unwrap()["accepted"],true);server_task.join().unwrap();
        assert!(crate::live_dispatch::read(&next,&input).unwrap().unwrap().summary(&input).needs_review,"An acknowledgment alone is not a checked native result");
        std::fs::remove_dir_all(root).unwrap();
    }
}
