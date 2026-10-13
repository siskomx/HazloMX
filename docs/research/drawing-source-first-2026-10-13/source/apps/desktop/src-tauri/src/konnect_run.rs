//! Identified Konnect SDK operations on captured project copies.
use crate::{
    kicad_project::{self as source, KicadInput},
    konnect::{verify_value_edit, ValueEdit},
};
use rmcp::{
    model::CallToolRequestParams, service::RunningService, transport::TokioChildProcess,
    RoleClient, ServiceExt,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::Threading::{
        GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE,
    },
};

pub const VERSION: &str = "0.13.0";
const EXE_HASH: &str = "09969ec5f05600039e206acedb0b6b785528f1bced2dbe2c7f426483013e4f2c";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KonnectInput {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub konnect_version: String,
    pub source: KicadInput,
    pub edit: ValueEdit,
}
impl KonnectInput {
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        self.edit.validate()?;
        if !matches!(self.schema_version, 1 | 2)
            || self.job_id.is_nil()
            || self.job_id != self.source.job_id
            || self.konnect_version != VERSION
            || self.source.is_board()
        {
            return Err("La propuesta requiere un esquema capturado y Konnect 0.13.0.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KonnectResult {
    pub schema_version: u32,
    pub job_id: Uuid,
    pub konnect_version: String,
    pub protocol_version: String,
    pub files: Vec<crate::kicad_run::OutputFile>,
}

pub(crate) fn runtime(executable: &Path) -> Result<Vec<File>, String> {
    crate::project::valid_location(executable).map_err(|e| e.to_string())?;
    if !executable
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("konnect.exe"))
    {
        return Err("Selecciona konnect.exe del paquete Windows x64 0.13.0.".into());
    }
    let mut pins =
        crate::project::pin_lineage(executable.parent().ok_or("Falta la carpeta de Konnect.")?)
            .map_err(|e| e.to_string())?;
    let mut file = source::open_regular(executable, 64 * 1024 * 1024)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != EXE_HASH {
        return Err("Konnect no coincide con el paquete identificado 0.13.0.".into());
    }
    pins.push(file);
    Ok(pins)
}
pub fn verify_runtime(executable: &Path) -> Result<(), String> {
    runtime(executable).map(|_| ())
}

fn native_readback(
    cli: &Path,
    edited: &Path,
    stage: &Path,
    profile: &Path,
    input: &KonnectInput,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    use std::{collections::BTreeMap, ffi::OsString};
    let mut env: BTreeMap<OsString, OsString> = BTreeMap::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            env.insert(key.into(), value);
        }
    }
    for key in [
        "APPDATA",
        "LOCALAPPDATA",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "KICAD_CONFIG_HOME",
    ] {
        env.insert(key.into(), profile.as_os_str().to_owned());
    }
    let root = cli
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Falta la carpeta KiCad.")?;
    for (key, name) in [
        ("KICAD10_SYMBOL_DIR", "symbols"),
        ("KICAD10_FOOTPRINT_DIR", "footprints"),
    ] {
        env.insert(
            key.into(),
            root.join("share/kicad").join(name).into_os_string(),
        );
    }
    let args = vec![
        "sch".into(),
        "export".into(),
        "netlist".into(),
        "--format".into(),
        "kicadxml".into(),
        "--output".into(),
        stage.join("Netlist.xml").into_os_string(),
        edited
            .join(source::relative(&input.source.entry)?)
            .into_os_string(),
    ];
    let output =
        crate::process::run_owned(cli, &args, edited, &env, cancel, Duration::from_secs(120))
            .map_err(|e| e.to_string())?;
    match output.reason {
        crate::process::StopReason::Cancelled => {
            Err("Validación KiCad cancelada; la copia se conserva sin publicar.".into())
        }
        crate::process::StopReason::TimedOut => {
            Err("KiCad no confirmó la copia a tiempo; no se publicará.".into())
        }
        _ if output.exit_code != Some(0) => Err(
            "KiCad no pudo leer la copia editada; revisa el proyecto y sus dependencias.".into(),
        ),
        _ => verify_netlist(
            &source::read_regular(&stage.join("Netlist.xml"), source::FILE_LIMIT)?,
            &input.edit,
        ),
    }
}

fn verify_netlist(bytes: &[u8], edit: &ValueEdit) -> Result<(), String> {
    use quick_xml::{events::Event, Reader, XmlVersion};
    let mut reader = Reader::from_reader(bytes);
    let mut open: Vec<String> = Vec::new();
    let mut root_seen = false;
    let mut selected = false;
    let mut selected_count = 0;
    let mut value_seen = false;
    let mut tool_seen = false;
    let mut count = 0;
    loop {
        count += 1;
        if count > 500_000 {
            return Err("El netlist supera el límite de elementos.".into());
        }
        let event = reader
            .read_event()
            .map_err(|_| "Netlist KiCad no válido.")?;
        let start = matches!(&event, Event::Start(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name().as_ref().to_owned();
                if open.is_empty() {
                    if root_seen || name != "export" || !start {
                        return Err("Raíz del netlist no válida.".into());
                    }
                    root_seen = true;
                }
                if open == ["export".to_owned(), "components".to_owned()] && name == "comp" {
                    selected = false;
                    for a in e.attributes() {
                        let a = a.map_err(|_| "Atributos del netlist no válidos.")?;
                        if a.key.as_ref() == "ref"
                            && a.normalized_value(XmlVersion::Explicit1_0)
                                .map_err(|_| "Referencia del netlist no válida.")?
                                .as_ref()
                                == edit.reference
                        {
                            selected = true;
                            selected_count += 1;
                            if selected_count > 1 {
                                return Err("El netlist contiene referencias duplicadas.".into());
                            }
                        }
                    }
                }
                let value = selected
                    && open
                        == [
                            "export".to_owned(),
                            "components".to_owned(),
                            "comp".to_owned(),
                        ]
                    && name == "value";
                let tool = open == ["export".to_owned(), "design".to_owned()] && name == "tool";
                if value || tool {
                    if !start {
                        return Err("Campo vacío del netlist.".into());
                    }
                    let raw = reader
                        .read_text(e.name())
                        .map_err(|_| "Campo del netlist incompleto.")?;
                    let raw = raw.xml_content(XmlVersion::Explicit1_0);
                    if raw.contains('<') {
                        return Err("Campo del netlist con estructura inesperada.".into());
                    }
                    let text = quick_xml::escape::unescape(&raw)
                        .map_err(|_| "Texto del netlist no válido.")?;
                    if value {
                        if value_seen || text.as_ref() != edit.value {
                            return Err("KiCad no confirmó el valor aprobado.".into());
                        }
                        value_seen = true;
                    } else {
                        if tool_seen || text != "Eeschema 10.0.7" {
                            return Err("El netlist no identifica KiCad 10.0.7.".into());
                        }
                        tool_seen = true;
                    }
                } else if start {
                    open.push(name);
                    if open.len() > 128 {
                        return Err("Netlist demasiado anidado.".into());
                    }
                } else if selected && name == "comp" {
                    return Err("Componente vacío del netlist.".into());
                }
            }
            Event::End(_) => {
                open.pop().ok_or("Netlist sin raíz.")?;
                if open.len() < 3 {
                    selected = false;
                }
            }
            Event::DocType(_) => return Err("Declaración DTD del netlist no permitida.".into()),
            Event::Eof => break,
            Event::Text(e)
                if open.is_empty() && !e.xml_content(XmlVersion::Explicit1_0).trim().is_empty() =>
            {
                return Err("Texto fuera del netlist.".into())
            }
            Event::CData(_) | Event::GeneralRef(_) if open.is_empty() => {
                return Err("Contenido fuera de la raíz del netlist.".into())
            }
            _ => {}
        }
    }
    if !open.is_empty() || !root_seen || selected_count != 1 || !value_seen || !tool_seen {
        return Err("KiCad no confirmó el componente editado.".into());
    }
    Ok(())
}

async fn cancelled(flag: &AtomicBool) {
    while !flag.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
async fn ceased(process: &OwnedHandle) -> Result<(), String> {
    let deadline = std::time::Instant::now();
    while unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } != WAIT_OBJECT_0 {
        if deadline.elapsed() >= Duration::from_secs(5) {
            return Err("Konnect no confirmó su cierre; conserva la copia sin publicar.".into());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}
type Session = RunningService<RoleClient, ()>;
async fn tool(
    service: &Session,
    name: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let reply = service
        .call_tool(
            CallToolRequestParams::new(name.to_owned()).with_arguments(
                args.as_object()
                    .ok_or("Argumentos de Konnect no válidos.")?
                    .clone(),
            ),
        )
        .await
        .map_err(|_| {
            "Konnect no confirmó la respuesta. Revisa la copia antes de repetir la operación."
                .to_string()
        })?;
    if reply.is_error.unwrap_or(false) || reply.content.len() != 1 {
        return Err("Konnect rechazó la operación sobre la copia; revisa la referencia y los bloqueos del editor.".into());
    }
    let text = &reply.content[0]
        .as_text()
        .ok_or("Konnect devolvió un resultado no compatible.")?
        .text;
    if text.len() > 65536 {
        return Err("La respuesta de Konnect supera el límite.".into());
    }
    serde_json::from_str(text).map_err(|_| "Konnect devolvió un resultado no válido.".into())
}

pub(crate) enum Edit<'a> {
    Value(&'a KonnectInput),
    Move(&'a crate::konnect_pcb::Input),
}
fn startup_error(error: String, exit_code: u32) -> String {
    if exit_code == 0xc0000135 {
        "Faltan dependencias de Windows para iniciar Konnect. Revisa el runtime de Microsoft Visual C++ y vuelve a revisar la operación.".into()
    } else {
        error
    }
}
pub(crate) async fn dispatch(
    executable: &Path,
    cli: &Path,
    edited: &Path,
    profile: &Path,
    input: Edit<'_>,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
    let config = profile.join("konnect.json");
    source::write_new(&config,&serde_json::to_vec(&serde_json::json!({"transport":"stdio","project_dir":edited,"ipc_address":format!("ipc://{}",profile.join("no-editor-socket").display()),"kicad_cli":cli,"kicad_binary":cli.parent().ok_or("Falta KiCad.")?.join("kicad.exe"),"log_level":"warn","auto_load_toolsets":false,"eager_toolsets":false})).map_err(|e|e.to_string())?)?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("--config")
        .arg(&config)
        .current_dir(profile)
        .env_clear()
        .kill_on_drop(true);
    for key in ["SystemRoot", "WINDIR", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env(
        "PATH",
        format!(
            "{}\\System32",
            std::env::var("SystemRoot").map_err(|_| "Falta SystemRoot.")?
        ),
    );
    for key in [
        "APPDATA",
        "LOCALAPPDATA",
        "USERPROFILE",
        "TEMP",
        "TMP",
        "KONNECT_STATE_DIR",
        "KICAD_CONFIG_HOME",
    ] {
        command.env(key, profile);
    }
    let mut wrapped = CommandWrap::from(command);
    let mut flags = CreationFlags(Default::default());
    flags.0 .0 = 0x08000000;
    wrapped.wrap(flags).wrap(KillOnDrop).wrap(JobObject);
    if cancel.load(Ordering::SeqCst) {
        return Err("Edición cancelada antes de iniciar Konnect.".into());
    }
    let (transport, _) = TokioChildProcess::builder(wrapped)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "No se pudo iniciar el servidor Konnect identificado.".to_string())?;
    let pid = transport.id().ok_or("Konnect no confirmó su proceso.")?;
    let raw = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if raw.is_null() {
        return Err("No se pudo observar el cierre de Konnect.".into());
    }
    // SAFETY: OpenProcess returned a new owned Windows handle.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let started = tokio::select! {
        result=tokio::time::timeout(Duration::from_secs(20),().serve(transport))=>result.map_err(|_|"Konnect no respondió a tiempo.".to_string()).and_then(|r|r.map_err(|_|"No se pudo conectar con Konnect.".to_string())),
        _=cancelled(&cancel)=>Err("Edición cancelada durante la conexión.".into())
    };
    let service = match started {
        Ok(service) => service,
        Err(error) => {
            ceased(&process).await?;
            let mut exit_code = 0;
            // The owned process handle remains valid after its startup failure.
            unsafe { GetExitCodeProcess(process.as_raw_handle() as _, &mut exit_code) };
            return Err(startup_error(error, exit_code));
        }
    };
    let operation = async {
        let peer = service
            .peer_info()
            .ok_or("Falta la identidad del servidor Konnect.")?;
        if peer.protocol_version.as_str() != "2025-06-18"
            || !peer
                .server_info
                .as_ref()
                .is_some_and(|i| i.name == "konnect" && i.version == VERSION)
        {
            return Err("Servidor o protocolo Konnect no compatible.".into());
        }
        match input {
            Edit::Value(input) => {
                tool(
                    &service,
                    "load_toolset",
                    serde_json::json!({"name":["sch_components"]}),
                )
                .await?;
                let actual = service
                    .list_all_tools()
                    .await
                    .map_err(|_| "No se pudo verificar el catálogo de Konnect.".to_string())?;
                let expected: serde_json::Value = serde_json::from_str(include_str!(
                    "../../../../docs/research/konnect-runtime-2026-10-11/raw/expanded-tools.json"
                ))
                .map_err(|e| e.to_string())?;
                for name in ["get_schematic_component", "edit_schematic_component"] {
                    let identified = expected["tools"]
                        .as_array()
                        .and_then(|a| a.iter().find(|t| t["name"] == name))
                        .ok_or("Falta el esquema de herramienta identificado.")?;
                    let tool = actual
                        .iter()
                        .find(|t| t.name == name)
                        .ok_or("El catálogo de Konnect cambió.")?;
                    if serde_json::to_value(tool).map_err(|e| e.to_string())?["inputSchema"]
                        != identified["inputSchema"]
                    {
                        return Err(
                            "Los argumentos de Konnect cambiaron; no se editará la copia.".into(),
                        );
                    }
                }
                let path = edited.join(source::relative(&input.source.entry)?);
                let before = tool(
                    &service,
                    "get_schematic_component",
                    serde_json::json!({"schematic":path,"reference":input.edit.reference}),
                )
                .await?;
                if before["value"].as_str() != Some(&input.edit.expected_value)
                    || !before["properties"].is_object()
                    || before["properties"]["Value"].as_str() != Some(&input.edit.expected_value)
                    || before["reference"].as_str() != Some(&input.edit.reference)
                {
                    return Err("El valor actual cambió. Inspecciona de nuevo la propuesta.".into());
                }
                tool(&service,"edit_schematic_component",serde_json::json!({"schematic":path,"reference":input.edit.reference,"value":input.edit.value})).await?;
                let after = tool(
                    &service,
                    "get_schematic_component",
                    serde_json::json!({"schematic":path,"reference":input.edit.reference}),
                )
                .await?;
                let mut expected = before;
                expected["value"] = input.edit.value.clone().into();
                expected["properties"]["Value"] = input.edit.value.clone().into();
                if after != expected {
                    return Err(
                        "Konnect no confirmó el cambio aprobado y la conservación del componente."
                            .into(),
                    );
                }
            }
            Edit::Move(input) => {
                tool(
                    &service,
                    "load_toolset",
                    serde_json::json!({"name":["pcb_board","pcb_components"]}),
                )
                .await?;
                let actual = service
                    .list_all_tools()
                    .await
                    .map_err(|_| "No se pudo verificar el catálogo PCB.")?;
                let expected: serde_json::Value = serde_json::from_str(include_str!(
                    "../../../../docs/research/konnect-pcb-2026-10-12/catalogs.json"
                ))
                .map_err(|e| e.to_string())?;
                for name in ["get_board_info", "move_component"] {
                    let identified = expected["expanded"]
                        .as_array()
                        .and_then(|tools| tools.iter().find(|tool| tool["name"] == name))
                        .ok_or("Falta el contrato PCB identificado.")?;
                    let current = actual
                        .iter()
                        .find(|tool| tool.name == name)
                        .ok_or("El catálogo PCB cambió.")?;
                    if serde_json::to_value(current).map_err(|e| e.to_string())?["inputSchema"]
                        != identified["inputSchema"]
                    {
                        return Err("Los argumentos PCB cambiaron; no se editará la copia.".into());
                    }
                }
                let path = edited.join(source::relative(&input.source.entry)?);
                let before_bytes = source::read_regular(&path, source::FILE_LIMIT)?;
                crate::konnect::validate_current_position(&before_bytes, &input.pcb_move)?;
                // The fixture holds the subsequent native read without blocking the host pre-read.
                #[cfg(test)]
                if profile.with_extension("test-board-hold").is_file() {
                    fs::write(profile.with_extension("test-board-ready"), b"ready")
                        .map_err(|e| e.to_string())?;
                    while !profile.with_extension("test-board-resume").is_file() {
                        tokio::time::sleep(Duration::from_millis(2)).await;
                    }
                }
                let before = tool(
                    &service,
                    "get_board_info",
                    serde_json::json!({"board":path}),
                )
                .await?;
                if before["source"] != "file" {
                    return Err("La copia PCB no está cerrada; no se editará.".into());
                }
                let applied=tool(&service,"move_component",serde_json::json!({"board":path,"reference":input.pcb_move.reference,"x":input.pcb_move.x_mm,"y":input.pcb_move.y_mm})).await?;
                if applied["source"] != "file" || applied["moved"] != input.pcb_move.reference {
                    return Err("Konnect no confirmó el movimiento en la copia cerrada.".into());
                }
                crate::konnect::verify_footprint_move(
                    &before_bytes,
                    &source::read_regular(&path, source::FILE_LIMIT)?,
                    &input.pcb_move,
                )?;
                if tool(
                    &service,
                    "get_board_info",
                    serde_json::json!({"board":path}),
                )
                .await?
                    != before
                {
                    return Err("Konnect cambió los metadatos de la PCB.".into());
                }
            }
        }
        Ok(())
    };
    let result = tokio::select! {
        value=tokio::time::timeout(Duration::from_secs(60),operation)=>value.unwrap_or_else(|_|Err("Konnect no confirmó la edición a tiempo; la copia se conserva sin publicar.".into())),
        _=cancelled(&cancel)=>Err("Edición cancelada; se conserva la copia sin publicar.".into())
    };
    let closed = service
        .cancel()
        .await
        .map_err(|_| "No se pudo cerrar la sesión Konnect.".to_string());
    ceased(&process).await?;
    closed?;
    result
}

pub fn run(
    executable: &Path,
    cli: &Path,
    imported: &Path,
    stage: &Path,
    profile: &Path,
    input: &KonnectInput,
    cancel: Arc<AtomicBool>,
) -> Result<KonnectResult, String> {
    input.validate()?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Edición cancelada antes de iniciar.".into());
    }
    if input.schema_version != 2 {
        return Err("Prepare una propuesta nueva para validar la copia con KiCad.".into());
    }
    let _runtime_pins = runtime(executable)?;
    let _kicad_runtime = crate::kicad_run::runtime(cli, &cancel)?;
    source::validate_capture(imported, &input.source)?;
    let _stage_pins = crate::project::pin_lineage(stage).map_err(|e| e.to_string())?;
    if fs::read_dir(stage)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("El directorio de edición no está vacío.".into());
    }
    fs::create_dir(profile).map_err(|e| e.to_string())?;
    let _profile_pins = crate::project::pin_lineage(profile).map_err(|e| e.to_string())?;
    source::write_new(
        &stage.join("input.json"),
        &serde_json::to_vec(input).map_err(|e| e.to_string())?,
    )?;
    for file in &input.source.files {
        let path = source::relative(&file.path)?;
        source::write_new(
            &stage.join("Original").join(&path),
            &source::read_regular(&imported.join(path), source::FILE_LIMIT)?,
        )?;
    }
    let edited = stage.join("Edited");
    source::working_copy(&stage.join("Original"), &edited, &input.source, &cancel)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(dispatch(
        executable,
        cli,
        &edited,
        profile,
        Edit::Value(input),
        cancel.clone(),
    ))?;
    native_readback(cli, &edited, stage, profile, input, cancel.clone())?;
    if cancel.load(Ordering::SeqCst) {
        return Err("Edición cancelada; no se publicarán resultados.".into());
    }
    let result = KonnectResult {
        schema_version: 2,
        job_id: input.job_id,
        konnect_version: VERSION.into(),
        protocol_version: "2025-06-18".into(),
        files: crate::kicad_run::files(stage)?
            .into_iter()
            .map(|p| {
                Ok(crate::kicad_run::OutputFile {
                    path: p
                        .strip_prefix(stage)
                        .map_err(|e| e.to_string())?
                        .to_str()
                        .ok_or("Ruta no válida.")?
                        .replace('\\', "/"),
                    sha256: source::digest(&source::read_regular(&p, source::FILE_LIMIT)?),
                })
            })
            .collect::<Result<_, String>>()?,
    };
    validate_artifacts(stage, input, &result)?;
    let metadata = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("El resultado Konnect supera el límite de recuperación.".into());
    }
    source::write_new(&stage.join("result.json"), &metadata)?;
    Ok(result)
}

pub fn validate_artifacts(
    directory: &Path,
    input: &KonnectInput,
    result: &KonnectResult,
) -> Result<(), String> {
    use std::collections::BTreeSet;
    input.validate()?;
    if result.schema_version != input.schema_version
        || result.job_id != input.job_id
        || result.konnect_version != VERSION
        || result.protocol_version != "2025-06-18"
    {
        return Err("El resultado Konnect no corresponde a la propuesta.".into());
    }
    if source::read_regular(&directory.join("input.json"), 1024 * 1024)?
        != serde_json::to_vec(input).map_err(|e| e.to_string())?
    {
        return Err("La solicitud Konnect cambió.".into());
    }
    source::portable_files(
        &directory.join("Original"),
        &input.source,
        &AtomicBool::new(false),
        |path, expected| {
            let actual =
                source::read_regular(&directory.join("Edited").join(path), source::FILE_LIMIT)?;
            if path == source::relative(&input.source.entry)? {
                verify_value_edit(expected, &actual, &input.edit)
            } else if actual == expected {
                Ok(())
            } else {
                Err("Konnect cambió un archivo fuera del esquema aprobado.".into())
            }
        },
    )?;
    let mut expected = BTreeSet::from(["input.json".to_owned()]);
    if input.schema_version == 2 {
        verify_netlist(
            &source::read_regular(&directory.join("Netlist.xml"), source::FILE_LIMIT)?,
            &input.edit,
        )?;
        expected.insert("Netlist.xml".to_owned());
    }
    for file in &input.source.files {
        expected.insert(format!("Original/{}", file.path));
        expected.insert(format!("Edited/{}", file.path));
    }
    if input.schema_version == 2 {
        // KiCad readback can create project-local view settings beside the edited sheet.
        let local = source::relative(&input.source.entry)?.with_extension("kicad_prl");
        let name = format!("Edited/{}", local.to_string_lossy().replace('\\', "/"));
        let path = directory.join(&name);
        if path.try_exists().map_err(|e| e.to_string())? {
            let settings: serde_json::Value =
                serde_json::from_slice(&source::read_regular(&path, 65536)?)
                    .map_err(|_| "Los ajustes locales de KiCad no son JSON válido.")?;
            if !settings.is_object()
                || settings["meta"]["version"] != 5
                || settings["meta"]["filename"].as_str()
                    != local.file_name().and_then(|name| name.to_str())
            {
                return Err(
                    "Los ajustes locales no corresponden al proyecto KiCad identificado.".into(),
                );
            }
            expected.insert(name);
        }
    }
    let mut observed = BTreeSet::new();
    for file in &result.files {
        if !observed.insert(file.path.clone())
            || !expected.contains(&file.path)
            || source::digest(&source::read_regular(
                &directory.join(source::relative(&file.path)?),
                source::FILE_LIMIT,
            )?) != file.sha256
        {
            return Err(
                "Los archivos de Konnect cambiaron o contienen una salida inesperada.".into(),
            );
        }
    }
    if expected != observed {
        return Err("Faltan archivos de la copia editada.".into());
    }
    let actual: BTreeSet<_> = crate::kicad_run::files(directory)?
        .into_iter()
        .map(|p| {
            p.strip_prefix(directory)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .filter(|p| p != "result.json")
        .collect();
    if actual != expected {
        return Err("El directorio Konnect contiene archivos inesperados.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(dead_code)]
    mod read_barrier {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/fixtures/publication_barrier.rs"
        ));
    }
    #[test]
    fn missing_windows_runtime_is_actionable_without_relabeling_other_failures() {
        let missing = startup_error("No se pudo conectar con Konnect.".into(), 0xc0000135);
        assert!(missing.contains("Microsoft Visual C++"));
        for code in [0, 1, 0xc0000005] {
            assert_eq!(
                startup_error("Edición cancelada.".into(), code),
                "Edición cancelada."
            );
            assert_eq!(
                startup_error("Konnect no respondió a tiempo.".into(), code),
                "Konnect no respondió a tiempo."
            );
        }
    }
    #[test]
    fn native_netlist_rejects_wrong_values_duplicates_and_malformed_xml() {
        let edit = ValueEdit {
            reference: "R1".into(),
            expected_value: "10k".into(),
            value: "12.34k".into(),
        };
        let xml = r#"<export><design><tool>Eeschema 10.0.7</tool></design><components><comp ref="R1"><value>12.34k</value></comp></components></export>"#;
        verify_netlist(xml.as_bytes(), &edit).unwrap();
        for invalid in [
            xml.replace("12.34k", "99k"),
            xml.replace("10.0.7", "10.0.6"),
            xml.replace("</components>", "<comp ref=\"R1\"/></components>"),
            xml.replace(
                "</components>",
                "<comp ref=\"R1\"><value>12.34k</value></comp></components>",
            ),
            xml.replace("12.34k", "<nested>12.34k</nested>"),
            xml.replace("</export>", ""),
            format!("<!DOCTYPE export SYSTEM 'file:///unknown'>{xml}"),
            format!("{xml}<export/>"),
        ] {
            assert!(
                verify_netlist(invalid.as_bytes(), &edit).is_err(),
                "{invalid}"
            );
        }
        let mut escaped = edit;
        escaped.value = "A&B<quoted>".into();
        verify_netlist(
            xml.replace("12.34k", "A&amp;B&lt;quoted&gt;").as_bytes(),
            &escaped,
        )
        .unwrap();
    }
    fn fixture() -> (std::path::PathBuf, KonnectInput) {
        let root =
            std::env::temp_dir().join(format!("hazlomexa-konnect-adapter-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let original = root.join("Sources");
        fs::create_dir(&original).unwrap();
        let bytes=include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Original/basic_test.kicad_sch");
        fs::write(original.join("basic_test.kicad_sch"), bytes).unwrap();
        let job_id = Uuid::new_v4();
        let source = source::capture(
            &original,
            &original.join("basic_test.kicad_sch"),
            &root.join("Capture"),
            job_id,
            crate::kicad::Units::In,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let input = KonnectInput {
            schema_version: 1,
            job_id,
            konnect_version: VERSION.into(),
            source,
            edit: ValueEdit {
                reference: "R1".into(),
                expected_value: "10k".into(),
                value: "12.34k".into(),
            },
        };
        (root, input)
    }
    #[test]
    fn verifies_captured_portable_copy_and_refuses_tampered_inventory() {
        let (root, input) = fixture();
        let stage = root.join("Stage");
        fs::create_dir(&stage).unwrap();
        let path = source::relative(&input.source.entry).unwrap();
        source::write_new(
            &stage.join("Original").join(&path),
            &fs::read(root.join("Capture").join(&path)).unwrap(),
        )
        .unwrap();
        source::write_new(
            &stage.join("input.json"),
            &serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        source::working_copy(
            &stage.join("Original"),
            &stage.join("Edited"),
            &input.source,
            &AtomicBool::new(false),
        )
        .unwrap();
        let edited = stage.join("Edited").join(&path);
        let bytes = include_bytes!(
            "../../../../docs/research/konnect-runtime-2026-10-11/raw/Edited/basic_test.kicad_sch"
        );
        fs::write(&edited, bytes).unwrap();
        let result = KonnectResult {
            schema_version: 1,
            job_id: input.job_id,
            konnect_version: VERSION.into(),
            protocol_version: "2025-06-18".into(),
            files: crate::kicad_run::files(&stage)
                .unwrap()
                .into_iter()
                .map(|p| crate::kicad_run::OutputFile {
                    path: p
                        .strip_prefix(&stage)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    sha256: source::digest(&fs::read(p).unwrap()),
                })
                .collect(),
        };
        validate_artifacts(&stage, &input, &result).unwrap();
        let mut current = input.clone();
        current.schema_version = 2;
        let netlist =
            include_bytes!("../../../../docs/research/konnect-runtime-2026-10-11/raw/Edited.net");
        source::write_new(&stage.join("Netlist.xml"), netlist).unwrap();
        fs::write(
            stage.join("input.json"),
            serde_json::to_vec(&current).unwrap(),
        )
        .unwrap();
        let mut native = KonnectResult {
            schema_version: 2,
            job_id: input.job_id,
            konnect_version: VERSION.into(),
            protocol_version: "2025-06-18".into(),
            files: result.files.clone(),
        };
        native
            .files
            .iter_mut()
            .find(|f| f.path == "input.json")
            .unwrap()
            .sha256 = source::digest(&serde_json::to_vec(&current).unwrap());
        native.files.push(crate::kicad_run::OutputFile {
            path: "Netlist.xml".into(),
            sha256: source::digest(netlist),
        });
        validate_artifacts(&stage, &current, &native).unwrap();
        let local_path = stage.join("Edited/basic_test.kicad_prl");
        let local =
            br#"{"meta":{"filename":"basic_test.kicad_prl","version":5},"project":{"files":[]}}"#;
        fs::write(&local_path, local).unwrap();
        native.files.push(crate::kicad_run::OutputFile {
            path: "Edited/basic_test.kicad_prl".into(),
            sha256: source::digest(local),
        });
        validate_artifacts(&stage, &current, &native).unwrap();
        for invalid in [
            br#"{"meta":{"filename":"Other.kicad_prl","version":5}}"#.as_slice(),
            br#"{"meta":{"filename":"basic_test.kicad_prl","version":99}}"#.as_slice(),
            b"not-json".as_slice(),
        ] {
            fs::write(&local_path, invalid).unwrap();
            native.files.last_mut().unwrap().sha256 = source::digest(invalid);
            assert!(validate_artifacts(&stage, &current, &native).is_err());
        }
        fs::remove_file(local_path).unwrap();
        native.files.pop();
        native.schema_version = 1;
        assert!(validate_artifacts(&stage, &current, &native).is_err());
        fs::remove_file(stage.join("Netlist.xml")).unwrap();
        fs::write(
            stage.join("input.json"),
            serde_json::to_vec(&input).unwrap(),
        )
        .unwrap();
        fs::write(stage.join("unexpected.txt"), b"unexpected").unwrap();
        assert!(validate_artifacts(&stage, &input, &result).is_err());
        fs::remove_file(stage.join("unexpected.txt")).unwrap();
        fs::write(
            &edited,
            String::from_utf8(bytes.to_vec())
                .unwrap()
                .replacen("127", "128", 1),
        )
        .unwrap();
        assert!(validate_artifacts(&stage, &input, &result).is_err());
        fs::write(&edited, bytes).unwrap();
        let mut stale = input.clone();
        stale.job_id = Uuid::new_v4();
        assert!(stale.validate().is_err());
        fs::write(stage.join("input.json"), b"{}").unwrap();
        assert!(validate_artifacts(&stage, &input, &result).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "actual Konnect 0.13.0 and KiCad 10.0.7 runtime required"]
    fn actual_konnect_captured_copy_adapter() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let (root, mut input) = fixture();
        input.schema_version = 2;
        let stage = root.join("Stage");
        fs::create_dir(&stage).unwrap();
        let result = run(
            &workspace.join("target/konnect-probe/runtime/konnect.exe"),
            &workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe"),
            &root.join("Capture"),
            &stage,
            &root.join("Profile"),
            &input,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        validate_artifacts(&stage, &input, &result).unwrap();
        assert_eq!(
            fs::read(root.join("Sources/basic_test.kicad_sch")).unwrap(),
            fs::read(root.join("Capture/basic_test.kicad_sch")).unwrap()
        );
        println!("Verified actual Konnect copy adapter: {}", root.display());
    }

    #[test]
    #[ignore = "Actual native Konnect with its full 60-second tool deadline; run alone"]
    fn actual_konnect_blocked_tool_deadline_and_shutdown() {
        blocked_native_tool(false);
    }

    #[test]
    #[ignore = "Actual native Konnect cancellation during a blocked component read; run alone"]
    fn actual_konnect_blocked_tool_cancellation_and_shutdown() {
        blocked_native_tool(true);
    }

    fn blocked_native_tool(cancel_on_read: bool) {
        use crate::process::tests::{child_processes, is_alive};
        use std::{sync::mpsc, time::Instant};
        assert!(child_processes("konnect.exe").is_empty());
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let executable = workspace.join("target/konnect-probe/runtime/konnect.exe");
        let cli = workspace.join("target/kicad-probe/runtime/bin/kicad-cli.exe");
        let _native_pins = runtime(&executable).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let _cli_pins = crate::kicad_run::runtime(&cli, &cancel).unwrap();
        let (root, mut input) = fixture();
        input.schema_version = 2;
        input.validate().unwrap();
        let edited = root.join("Capture");
        source::validate_capture(&edited, &input.source).unwrap();
        let path = edited.join(source::relative(&input.source.entry).unwrap());
        let original = fs::read(&path).unwrap();
        let profile = root.join("Profile");
        fs::create_dir(&profile).unwrap();
        let mut barrier = read_barrier::Barrier::request(&path);
        let started = Instant::now();
        let (sender, receiver) = mpsc::channel();
        let worker_cancel = cancel.clone();
        let worker = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(dispatch(
                &executable,
                &cli,
                &edited,
                &profile,
                Edit::Value(&input),
                worker_cancel,
            ));
            sender.send(result).unwrap();
        });
        let read_requested = barrier.wait(20000) == 0;
        let observed = child_processes("konnect.exe");
        let observed_alive = !observed.is_empty() && observed.iter().all(|pid| is_alive(*pid));
        let cancelled_at = if cancel_on_read {
            let now = Instant::now();
            cancel.store(true, Ordering::SeqCst);
            Some(now)
        } else {
            None
        };
        // Release the owned file even if the SDK shutdown misses the test watchdog.
        let outcome = receiver.recv_timeout(Duration::from_secs(100));
        let elapsed_ms = started.elapsed().as_millis();
        let cancellation_ms = cancelled_at.map(|time| time.elapsed().as_millis());
        let ceased_before_release =
            !observed.is_empty() && observed.iter().all(|pid| !is_alive(*pid));
        barrier.release();
        worker.join().unwrap();
        assert!(
            read_requested,
            "Native file access did not break the owned barrier"
        );
        let error = outcome
            .expect("Konnect did not return within the watchdog")
            .unwrap_err();
        assert!(
            observed_alive,
            "Owned server was not observed alive during the held read"
        );
        let mode = if cancel_on_read {
            "cancellation"
        } else {
            "deadline"
        };
        if cancel_on_read {
            assert!(error.contains("Edición cancelada"), "{error}");
            assert!(
                cancellation_ms.unwrap() < 10000,
                "Cancellation exceeded the shutdown bound"
            );
        } else {
            assert!(error.contains("no confirmó la edición a tiempo"), "{error}");
            assert!(
                (60000..90000).contains(&elapsed_ms),
                "Unexpected deadline: {elapsed_ms} ms"
            );
        }
        assert!(
            ceased_before_release,
            "Owned server remained alive until file release"
        );
        assert!(child_processes("konnect.exe").is_empty());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            fs::read(root.join("Sources/basic_test.kicad_sch")).unwrap(),
            original
        );
        assert!(!root.join("result.json").exists());
        fs::write(root.join(format!("{mode}-result.json")), serde_json::to_vec_pretty(&serde_json::json!({
            "pass":true,"scope":"actual native dispatcher blocked component read before mutation", "error":error,
            "mode":mode,"elapsed_ms":elapsed_ms,"cancellation_ms":cancellation_ms,"owned_pids":observed,"ceased_before_barrier_release":ceased_before_release,
            "source_and_copy_preserved":true,"native_ui":false,"model_inference":false
        })).unwrap()).unwrap();
        println!(
            "Verified actual Konnect blocked-read {mode}: {}",
            root.display()
        );
    }

    #[test]
    fn cancelled_request_never_starts_or_creates_private_files() {
        let (root, input) = fixture();
        let stage = root.join("Stage");
        fs::create_dir(&stage).unwrap();
        assert!(run(
            &root.join("missing-konnect.exe"),
            &root.join("missing-kicad-cli.exe"),
            &root.join("Capture"),
            &stage,
            &root.join("Profile"),
            &input,
            Arc::new(AtomicBool::new(true))
        )
        .unwrap_err()
        .contains("cancelada"));
        assert!(fs::read_dir(&stage).unwrap().next().is_none());
        assert!(!root.join("Profile").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
