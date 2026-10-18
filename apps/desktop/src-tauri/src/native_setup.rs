//! Program-owned tools, with app-local configuration and no personal agent registration.
use crate::{host_tools::Program, native_tools::McpConfig};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub enum Setup {
    Mcp(McpConfig),
    Cli(PathBuf),
}

fn registry_folders() -> Vec<PathBuf> {
    use windows_sys::Win32::System::Registry::*;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain([0]).collect()
    }
    fn string(key: HKEY, subkey: &str, name: &str) -> Option<String> {
        let mut data = [0u16; 4096];
        let mut size = std::mem::size_of_val(&data) as u32;
        let status = unsafe {
            RegGetValueW(
                key,
                wide(subkey).as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 || size < 2 || size as usize > std::mem::size_of_val(&data) {
            return None;
        }
        String::from_utf16(&data[..(size as usize / 2) - 1]).ok()
    }
    let mut folders = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for subkey in [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let mut key = std::ptr::null_mut();
            if unsafe { RegOpenKeyExW(root, wide(subkey).as_ptr(), 0, KEY_READ, &mut key) } != 0 {
                continue;
            }
            for index in 0..2048 {
                let mut name = [0u16; 256];
                let mut length = name.len() as u32;
                if unsafe {
                    RegEnumKeyExW(
                        key,
                        index,
                        name.as_mut_ptr(),
                        &mut length,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    )
                } != 0
                {
                    break;
                }
                let Ok(name) = String::from_utf16(&name[..length as usize]) else {
                    continue;
                };
                let Some(display) = string(key, &name, "DisplayName") else {
                    continue;
                };
                if ![
                    "freecad", "cadcraft", "arduino", "kicad", "orca", "python", "konnect",
                ]
                .iter()
                .any(|prefix| display.to_lowercase().starts_with(prefix))
                {
                    continue;
                }
                if let Some(location) = string(key, &name, "InstallLocation") {
                    let path = PathBuf::from(location.trim_matches('"'));
                    if path.is_absolute() {
                        folders.extend([
                            path.clone(),
                            path.join("bin"),
                            path.join("Scripts"),
                            path.join("resources/app/lib/backend/resources"),
                        ]);
                    }
                }
            }
            unsafe {
                RegCloseKey(key);
            }
        }
    }
    folders
}

pub fn remember(profile: &Path, program: Program, kind: &str, path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() || !["mcp_config", "cli"].contains(&kind) {
        return Err("La ubicación seleccionada no es válida.".into());
    }
    std::fs::create_dir_all(profile)
        .map_err(|_| "No se pudo conservar la ubicación del programa.")?;
    let name = serde_json::to_value(program)
        .map_err(|_| "Programa no válido.")?
        .as_str()
        .unwrap()
        .to_owned();
    let temp = profile.join(format!("{}.json", uuid::Uuid::new_v4()));
    write_new(&temp, &json!({"kind":kind,"path":path}))?;
    std::fs::rename(temp, profile.join(format!("{name}.json")))
        .map_err(|_| "No se pudo conservar la ubicación del programa.".into())
}
pub fn remembered(profile: &Path, program: Program) -> Result<Option<Setup>, String> {
    let name = serde_json::to_value(program)
        .map_err(|_| "Programa no válido.")?
        .as_str()
        .unwrap()
        .to_owned();
    let path = profile.join(format!("{name}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let bytes = crate::kicad_project::read_regular(&path, 65536)?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "La ubicación guardada no es válida.")?;
    let path = PathBuf::from(
        value["path"]
            .as_str()
            .ok_or("La ubicación guardada no es válida.")?,
    );
    if !path.is_absolute() || !path.is_file() {
        return Ok(None);
    }
    match value["kind"].as_str() {
        Some("mcp_config") => Ok(Some(Setup::Mcp(McpConfig::from_file(&path)?))),
        Some("cli") => Ok(Some(Setup::Cli(path))),
        _ => Err("La conexión guardada no es válida.".into()),
    }
}

fn folders() -> Vec<PathBuf> {
    let mut roots = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|p| p.is_absolute())
        .take(128)
        .collect::<Vec<_>>();
    let mut parents = Vec::new();
    for name in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "LOCALAPPDATA",
        "APPDATA",
        "USERPROFILE",
    ] {
        if let Some(path) = std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            roots.extend([
                path.clone(),
                path.join("Programs"),
                path.join(".local/bin"),
                path.join("npm"),
            ]);
            parents.extend([path.clone(), path.join("Programs"), path.join("Python")]);
        }
    }
    if let Some(mut drive) = std::env::var_os("SystemDrive") {
        drive.push("\\");
        let root = PathBuf::from(drive);
        if root.is_absolute() {
            parents.push(root);
        }
    }
    for parent in parents {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten().take(256) {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if [
                    "freecad", "cadcraft", "arduino", "kicad", "orca", "python", "konnect",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
                    && entry.file_type().is_ok_and(|t| t.is_dir())
                {
                    let path = entry.path();
                    roots.extend([
                        path.clone(),
                        path.join("bin"),
                        path.join("Scripts"),
                        path.join("resources/app/lib/backend/resources"),
                    ]);
                    if name == "kicad" {
                        if let Ok(versions) = std::fs::read_dir(&path) {
                            for version in versions.flatten().take(32) {
                                roots.push(version.path().join("bin"));
                            }
                        }
                    }
                }
            }
        }
    }
    roots.extend(registry_folders());
    roots
}
fn find_in(roots: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    roots
        .iter()
        .filter(|p| p.is_absolute())
        .flat_map(|folder| names.iter().map(move |name| folder.join(name)))
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
}
pub fn cli(program: Program) -> Option<PathBuf> {
    let names: &[&str] = match program {
        Program::Drawing => &["cadcraft-cli.exe"],
        Program::Arduino => &["arduino-cli.exe"],
        Program::Kicad => &["kicad-cli.exe"],
        Program::Slice => &["orca-slicer.exe", "OrcaSlicer.exe"],
        _ => return None,
    };
    find_in(&folders(), names)
}
fn mcp(executable: PathBuf, args: Vec<String>) -> McpConfig {
    McpConfig {
        executable,
        args,
        env: BTreeMap::new(),
        url: None,
        authorization: None,
    }
}
fn orca(roots: &[PathBuf]) -> Result<Option<McpConfig>, String> {
    let Some(server) = find_in(roots, &["orcaslicer-mcp.exe"]) else {
        return Ok(None);
    };
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or("No se encontró la configuración de OrcaSlicer.")?
        .join("OrcaSlicer/OrcaSlicer.conf");
    if !path.is_file() {
        return Err("OrcaSlicer MCP está instalado. Abre su edición MCP y activa Remote API en Preferencias, solo para localhost.".into());
    }
    let bytes = crate::kicad_project::read_regular(&path, 1024 * 1024)?;
    let text =
        String::from_utf8(bytes).map_err(|_| "La configuración de OrcaSlicer no es válida.")?;
    let settings = orca_settings(&text)?;
    let mut config = mcp(server, vec![]);
    config.env.insert("ORCA_API_URL".into(), settings.0);
    config.env.insert("ORCA_API_TOKEN".into(), settings.1);
    Ok(Some(config))
}
fn orca_settings(text: &str) -> Result<(String, String), String> {
    let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let get = |name: &str| -> Option<String> {
        let value = json
            .get(name)
            .or_else(|| json.get("app").and_then(|app| app.get(name)));
        value
            .and_then(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .or_else(|| v.as_bool().map(|b| b.to_string()))
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .or_else(|| {
                text.lines()
                    .filter_map(|line| line.split_once('='))
                    .find(|(key, _)| key.trim() == name)
                    .map(|(_, v)| v.trim().to_owned())
            })
    };
    if !get("remote_api_enabled").is_some_and(|v| v == "true" || v == "1")
        || get("remote_api_bind_lan").is_some_and(|v| v == "true" || v == "1")
    {
        return Err("Activa Remote API en OrcaSlicer y deja desactivado el acceso LAN. Hazlo conectará después sin pedir un archivo de configuración.".into());
    }
    let port = get("remote_api_port")
        .unwrap_or_else(|| "13130".into())
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or("El puerto de OrcaSlicer no es válido.")?;
    let token = get("remote_api_token")
        .filter(|v| !v.is_empty() && v.len() <= 8192 && !v.chars().any(char::is_control))
        .ok_or("OrcaSlicer aún no tiene un token de Remote API válido.")?;
    Ok((format!("http://127.0.0.1:{port}"), token))
}
pub fn discover(
    program: Program,
    resources: &Path,
    project: &Path,
    profile: &Path,
) -> Result<Option<Setup>, String> {
    std::fs::create_dir_all(profile)
        .map_err(|_| "No se pudo crear la configuración local de herramientas.")?;
    let roots = folders();
    let config = match program {
        Program::Rea => {
            let rea = resources.join("rea");
            let node = rea.join("node.exe");
            let entry = rea.join("node_modules/rea-agents/scripts/rea.mjs");
            if !node.is_file() || !entry.is_file() {
                return Err(
                    "Falta el componente Rea de Hazlo. Reinstala el paquete completo de Hazlo."
                        .into(),
                );
            }
            let mut config = mcp(
                node,
                vec![entry.to_string_lossy().into_owned(), "mcp".into()],
            );
            for name in ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
                config
                    .env
                    .insert(name.into(), profile.to_string_lossy().into_owned());
            }
            config
                .env
                .insert("REA_MCP_INPUT_SCHEMA_PROFILE".into(), "compact".into());
            config
                .env
                .insert("REA_MCP_OUTPUT_SCHEMA_PROFILE".into(), "off".into());
            Some(config)
        }
        Program::Freecad => find_in(&roots, &["cadpilot.exe"]).map(|exe| {
            mcp(
                exe,
                vec!["--only-text-feedback".into(), "--no-auto-audit".into()],
            )
        }),
        Program::Drawing => {
            find_in(&roots, &["cadcraft-cli.exe"]).map(|exe| mcp(exe, vec!["mcp".into()]))
        }
        Program::Agent => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "No se pudo comprobar Arduino Agent.")?;
            Some(
                runtime
                    .block_on(crate::agent::connect())?
                    .native_mcp_config()?,
            )
        }
        Program::Kicad => {
            if let (Some(server), Some(cli)) = (
                find_in(&roots, &["konnect.exe"]),
                find_in(&roots, &["kicad-cli.exe"]),
            ) {
                let path = profile.join("konnect.json");
                write_new(
                    &path,
                    &json!({"transport":"stdio","project_dir":project,"kicad_cli":cli,"kicad_binary":cli.with_file_name("kicad.exe"),"log_level":"warn","auto_load_toolsets":false,"eager_toolsets":false}),
                )?;
                let mut config = mcp(
                    server,
                    vec!["--config".into(), path.to_string_lossy().into_owned()],
                );
                config.env.insert(
                    "KONNECT_STATE_DIR".into(),
                    profile.to_string_lossy().into_owned(),
                );
                Some(config)
            } else {
                None
            }
        }
        Program::Slice => orca(&roots)?,
        _ => None,
    };
    if let Some(config) = config {
        // Credentials stay in memory; generated files contain only paths and non-secret arguments.
        write_new(
            &profile.join("connection.json"),
            &json!({"command":config.executable,"args":config.args,"url":config.url,"env":config.env.iter().filter(|(key,_)|!["TOKEN","KEY","SECRET","PASSWORD"].iter().any(|word|key.to_uppercase().contains(word))).collect::<BTreeMap<_,_>>()}),
        )?;
        Ok(Some(Setup::Mcp(config)))
    } else {
        let executable = cli(program);
        if let Some(executable) = &executable {
            write_new(
                &profile.join("connection.json"),
                &json!({"program":program,"transport":"cli","command":executable}),
            )?;
        }
        Ok(executable.map(Setup::Cli))
    }
}
fn write_new(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "Configuración nativa no válida.")?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| "No se pudo crear la configuración nativa sin sobrescribir archivos.")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "No se pudo conservar la configuración nativa.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembers_portable_locations_without_copying_connection_secrets() {
        let root =
            std::env::temp_dir().join(format!("hazlo-native-location-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let config = root.join("portable.json");
        std::fs::write(
            &config,
            br#"{"url":"http://127.0.0.1:3847/mcp","authorization":"private-fixture-token"}"#,
        )
        .unwrap();
        let profile = root.join("locations");
        remember(&profile, Program::Agent, "mcp_config", &config).unwrap();
        let bytes = std::fs::read_to_string(profile.join("agent.json")).unwrap();
        assert!(!bytes.contains("private-fixture-token"));
        assert!(
            matches!(remembered(&profile,Program::Agent).unwrap(),Some(Setup::Mcp(c)) if c.authorization.as_deref()==Some("private-fixture-token"))
        );
        let cli = root.join("portable.exe");
        std::fs::write(&cli, b"fixture").unwrap();
        remember(&profile, Program::Drawing, "cli", &cli).unwrap();
        remember(&profile, Program::Drawing, "cli", &cli).unwrap();
        assert!(
            matches!(remembered(&profile,Program::Drawing).unwrap(),Some(Setup::Cli(p)) if p==cli)
        );
        std::fs::remove_file(cli).unwrap();
        assert!(remembered(&profile, Program::Drawing).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "Explicit bundled Rea discovery and real MCP catalog smoke check"]
    fn actual_bundled_rea_connects_without_a_personal_installation() {
        use std::sync::{atomic::AtomicBool, Arc};
        let resources = PathBuf::from(
            std::env::var_os("HAZLOMEXA_BUNDLED_RESOURCES_TEST")
                .expect("Select the isolated packaged resource fixture"),
        );
        let root = std::env::temp_dir().join(format!("hazlo-bundled-rea-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let Setup::Mcp(config) = discover(Program::Rea, &resources, &root, &root.join("profile"))
            .unwrap()
            .unwrap()
        else {
            panic!("Bundled Rea missing");
        };
        assert_eq!(config.executable, resources.join("rea/node.exe"));
        assert!(config.env.contains_key("REA_MCP_INPUT_SCHEMA_PROFILE"));
        let file = McpConfig::from_file(&root.join("profile/connection.json")).unwrap();
        assert_eq!(file.args, config.args);
        let (connection, catalog) = crate::native_tools::Connection::mcp(
            Program::Rea,
            config,
            root.clone(),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(catalog
            .tools
            .iter()
            .any(|tool| tool.name == "get_evidence_bundle"));
        assert!(catalog.tools.len() > 100);
        connection.close();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match std::fs::remove_dir_all(&root) {
                Ok(()) => break,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(100))
                }
                Err(e) => panic!("Owned Rea profile still busy: {e}"),
            }
        }
    }
    #[test]
    fn orca_auto_config_requires_enabled_localhost_api_and_valid_port() {
        let valid="remote_api_enabled = true\nremote_api_bind_lan = false\nremote_api_token = private-fixture-token\nremote_api_port = 13130";
        assert_eq!(
            orca_settings(valid).unwrap(),
            (
                "http://127.0.0.1:13130".into(),
                "private-fixture-token".into()
            )
        );
        for invalid in [
            valid.replace("enabled = true", "enabled = false"),
            valid.replace("bind_lan = false", "bind_lan = true"),
            valid.replace("13130", "65536"),
            valid.replace("13130", "0"),
        ] {
            assert!(orca_settings(&invalid).is_err());
        }
        assert!(orca_settings(r#"{"remote_api_enabled":true,"remote_api_bind_lan":false,"remote_api_token":"test","remote_api_port":13130}"#).is_ok());
    }
    #[test]
    fn discovery_ignores_relative_folders_and_generated_configs_never_overwrite() {
        let root = std::env::temp_dir().join(format!("hazlo-native-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let exe = root.join("owned.exe");
        std::fs::write(&exe, b"fixture").unwrap();
        assert!(find_in(&[PathBuf::from("relative")], &["owned.exe"]).is_none());
        assert_eq!(
            find_in(std::slice::from_ref(&root), &["owned.exe"]),
            Some(exe.canonicalize().unwrap())
        );
        let config = root.join("connection.json");
        write_new(&config, &json!({"command":exe})).unwrap();
        let before = std::fs::read(&config).unwrap();
        assert!(write_new(&config, &json!({"command":"other"})).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }
}
