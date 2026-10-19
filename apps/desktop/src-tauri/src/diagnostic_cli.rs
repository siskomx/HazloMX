//! Capture scope checks supplement native help; they do not replace the program's CLI.
use crate::host_tools::Program;
use std::path::{Component, Path};

pub fn validate(program: Program, args: &[String], cwd: &Path) -> Result<(), String> {
    if !matches!(program, Program::Tshark | Program::Usbpcap) {
        return Ok(());
    }
    if args.is_empty() {
        return Err(
            "Usa la ayuda nativa o identifica una interfaz; no se abre un modo interactivo.".into(),
        );
    }
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
    };
    for arg in args {
        let option = arg.split('=').next().unwrap_or(arg);
        if option.starts_with("--") && option.len()>2 && ["--output","--write-file","--fifo","--init-non-standard-hwids","--export-objects","--export-tls-session-keys","--log-file","--ring-buffer"].iter().any(|full|full.starts_with(option) && *full!=option) {
            return Err("Usa nombres completos de opciones; no se aceptan abreviaturas de salidas o acciones.".into());
        }
        if arg.starts_with("--log-file") || arg.starts_with("--ring-buffer") || program==Program::Tshark && arg.starts_with("-b") {
            return Err("Usa una captura acotada en un solo archivo del proyecto, sin salidas auxiliares.".into());
        }
        if program==Program::Usbpcap && arg.starts_with('-') && !arg.starts_with("--") && !["-h","-?","-d","-o","-s","-b"].contains(&arg.as_str()) {
            return Err("Usa opciones USB completas y separadas, sin inicialización ni captura global.".into());
        }
    }
    if args.iter().any(|a| {
        a.starts_with("-X")
            || a.contains("lua_script")
            || a.starts_with("--export-")
            || a.starts_with("-I") && program == Program::Usbpcap
            || a.starts_with("--init-non-standard-hwids")
    }) {
        return Err(
            "Esta conexión no ejecuta scripts ni cambia controladores o el registro.".into(),
        );
    }
    if program == Program::Tshark {
        let query = args.iter().any(|a| {
            matches!(
                a.as_str(),
                "--help"
                    | "-h"
                    | "--version"
                    | "-v"
                    | "-D"
                    | "--list-interfaces"
                    | "-G"
                    | "-r"
                    | "--read-file"
            )
        });
        if !query {
            let duration = args
                .windows(2)
                .filter(|a| a[0] == "-a" || a[0] == "--autostop")
                .filter_map(|a| a[1].strip_prefix("duration:"))
                .filter_map(|s| s.parse::<f64>().ok())
                .any(|n| n.is_finite() && n > 0.0 && n <= 30.0);
            if value("-i").or_else(|| value("--interface")).is_none() || !duration {
                return Err("La captura requiere una interfaz explícita y autostop duration de hasta 30 segundos.".into());
            }
        }
    } else {
        let query = args.iter().any(|a| {
            matches!(
                a.as_str(),
                "--help"
                    | "-h"
                    | "--extcap-version"
                    | "--extcap-interfaces"
                    | "--extcap-config"
                    | "--extcap-dlts"
            )
        });
        if !query
            && (value("-d").or_else(|| value("--device")).is_none()
                || value("--devices").is_none()
                || args.iter().any(|a| {
                    matches!(
                        a.as_str(),
                        "-A" | "--capture-from-all-devices" | "--capture-from-new-devices"
                    )
                }))
        {
            return Err("La captura USB requiere un root hub y direcciones --devices explícitas; no se captura todo el hub.".into());
        }
        // USBPcap can otherwise spawn an elevated worker outside our owned process job.
        if !query && unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() } == 0 {
            return Err("USBPcap requiere permisos de captura que esta sesión no tiene. No se inició una elevación ni un proceso fuera del control de cancelación. Puedes analizar una captura obtenida externamente con TShark.".into());
        }
    }
    let flag = if program == Program::Tshark {
        "-w"
    } else {
        "-o"
    };
    for (i, arg) in args.iter().enumerate() {
        // Accept only separate output arguments so validation cannot miss a short-form alias.
        if (arg.starts_with(flag) && arg != flag)
            || arg.starts_with("--output=")
            || arg.starts_with("--write-file=")
            || arg.starts_with("--fifo")
        {
            return Err(
                "Usa una salida nueva del proyecto con -w o -o como argumento separado.".into(),
            );
        }
        if arg == flag || arg == "--output" || arg == "--write-file" {
            let target = Path::new(args.get(i + 1).ok_or("Falta la salida de captura.")?);
            if target
                .components()
                .any(|c| matches!(c, Component::ParentDir))
                || target == Path::new("-")
            {
                return Err("La captura debe guardarse en un archivo nuevo del proyecto.".into());
            }
            let path = if target.is_absolute() {
                target.to_owned()
            } else {
                cwd.join(target)
            };
            let parent = path
                .parent()
                .ok_or("Salida no válida.")?
                .canonicalize()
                .map_err(|_| "La carpeta de captura no existe.")?;
            if !parent.starts_with(
                cwd.canonicalize()
                    .map_err(|_| "El proyecto no está disponible.")?,
            ) || path.exists()
            {
                return Err("La salida debe ser un archivo nuevo dentro del proyecto; no se sobrescriben capturas.".into());
            }
        }
    }
    Ok(())
}

pub fn output(args: &[String], cwd: &Path) -> Option<std::path::PathBuf> {
    args.windows(2)
        .find(|a| matches!(a[0].as_str(), "-w" | "-o" | "--output" | "--write-file"))
        .map(|a| cwd.join(&a[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_bounded_targeted_capture_and_preserves_existing_files() {
        let root = std::env::temp_dir().join(format!("hazlo-diagnostics-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let check = |p, args: &[&str]| {
            validate(
                p,
                &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                &root,
            )
        };
        assert!(check(Program::Tshark, &["-D"]).is_ok());
        assert!(check(Program::Tshark, &["-i", "1"]).is_err());
        assert!(check(
            Program::Tshark,
            &["-i", "1", "-a", "duration:2", "-w", "new.pcapng"]
        )
        .is_ok());
        std::fs::write(root.join("old.pcapng"), b"original").unwrap();
        assert!(check(Program::Tshark, &["-r", "old.pcapng", "-w", "old.pcapng"]).is_err());
        assert!(check(Program::Tshark, &["-Xlua_script:bad.lua"]).is_err());
        assert!(check(Program::Usbpcap, &["--extcap-interfaces"]).is_ok());
        assert!(check(Program::Usbpcap, &["-I"]).is_err());
        assert!(check(Program::Usbpcap, &["--ini"]).is_err());
        assert!(check(Program::Usbpcap, &["--out=outside.pcap"]).is_err());
        assert!(check(Program::Usbpcap, &["--extcap-interfaces","-AI"]).is_err());
        assert!(check(Program::Tshark, &["-r","old.pcapng","--write=outside.pcap"]).is_err());
        assert!(check(Program::Usbpcap, &["-d", "hub", "-A"]).is_err());
        if unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() } == 0 {
            assert!(check(
                Program::Usbpcap,
                &["-d", "hub", "--devices", "3", "-o", "new.pcap"]
            )
            .is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
