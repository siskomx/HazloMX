//! Optional, pinned upstream packages. Only the desktop UI can request setup.
use crate::host_tools::Program;
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

struct Package {
    name: &'static str,
    url: &'static str,
    sha256: &'static str,
    max: usize,
}
fn package(program: Program) -> Result<Package, String> {
    Ok(match program {
        Program::Tshark => Package { name:"Wireshark-4.6.9-x64.exe",url:"https://2.na.dl.wireshark.org/win64/Wireshark-4.6.9-x64.exe",sha256:"bf9b5ce8a89f244c376a9b1a946276eaa06463dde3e33069a34d7f102f5878cf",max:110*1024*1024 },
        Program::Usbpcap => Package { name:"USBPcapSetup-1.5.4.0.exe",url:"https://github.com/desowin/usbpcap/releases/download/1.5.4.0/USBPcapSetup-1.5.4.0.exe",sha256:"87a7edf9bbbcf07b5f4373d9a192a6770d2ff3add7aa1e276e82e38582ccb622",max:1024*1024 },
        Program::Serial => Package { name:"pyserial-3.5-py2.py3-none-any.whl",url:"https://files.pythonhosted.org/packages/07/bc/587a445451b253b285629263eb51c2d8e9bcea4fc97826266d186f96f558/pyserial-3.5-py2.py3-none-any.whl",sha256:"c4451db6ba391ca6ca299fb3ec7bae67a5c55dde170964c7a14ceefec02f2cf0",max:1024*1024 },
        _ => return Err("Este programa no tiene una descarga opcional de diagnóstico.".into()),
    })
}
fn valid(bytes: &[u8], package: &Package) -> bool {
    bytes.len() <= package.max && format!("{:x}", Sha256::digest(bytes)) == package.sha256
}

pub async fn prepare(
    program: Program,
    data: &Path,
    cancelled: Arc<AtomicBool>,
) -> Result<Option<PathBuf>, String> {
    let package = package(program)?;
    let directory = data.join("diagnostic-tools/downloads");
    std::fs::create_dir_all(&directory).map_err(|_| "No se pudo crear la carpeta de descargas.")?;
    let path = directory.join(package.name);
    if path.exists() {
        let bytes = crate::kicad_project::read_regular(&path, package.max as u64)?;
        if !valid(&bytes, &package) {
            return Err(
                "La descarga guardada no coincide con el paquete verificado; no se abrió.".into(),
            );
        }
    } else {
        let download = async {
            let client = reqwest::Client::builder()
                .https_only(true)
                .timeout(Duration::from_secs(180))
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .map_err(|_| "No se pudo iniciar la descarga.")?;
            let mut response = client
                .get(package.url)
                .send()
                .await
                .map_err(|_| "La descarga no está disponible.")?
                .error_for_status()
                .map_err(|_| "El servidor rechazó la descarga.")?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "La descarga quedó incompleta.")?
            {
                if bytes.len() + chunk.len() > package.max {
                    return Err("El paquete supera el tamaño permitido.".to_string());
                }
                bytes.extend_from_slice(&chunk);
            }
            if !valid(&bytes, &package) {
                return Err(
                    "El paquete no coincide con su SHA-256; no se instaló ni abrió.".into(),
                );
            }
            Ok::<_, String>(bytes)
        };
        tokio::pin!(download);
        let bytes = loop {
            if cancelled.load(Ordering::SeqCst) {
                return Err("Descarga detenida.".into());
            }
            tokio::select! { result=&mut download => break result?, _=tokio::time::sleep(Duration::from_millis(50))=>{} }
        };
        use std::io::Write;
        // Only complete verified downloads get a public filename; interrupted downloads stay private.
        let temp = directory.join(format!("{}.partial", uuid::Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|_| "No se pudo conservar la descarga.")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "No se pudo conservar la descarga.")?;
        drop(file);
        std::fs::rename(temp, &path).map_err(|_| "No se pudo publicar la descarga.")?;
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err("Preparación detenida.".into());
    }
    if program != Program::Serial {
        return Ok(Some(path));
    }
    let data = data.to_owned();
    tokio::task::spawn_blocking(move || {
        let python=crate::native_setup::python().ok_or("Instala Python para usar pySerial; Hazlo no encontró un intérprete compatible.")?;
        let root=data.join("diagnostic-tools/pyserial");
        std::fs::create_dir_all(&root).map_err(|_|"No se pudo preparar pySerial.")?;
        let run=|executable:&Path,args:Vec<OsString>| -> Result<(),String> {
            let out=crate::process::run_owned(executable,&args,&root,&crate::native_tools::environment(),cancelled.clone(),Duration::from_secs(90))
                .map_err(|_|"No se pudo preparar el entorno privado de pySerial.")?;
            if out.reason != crate::process::StopReason::Completed || out.exit_code != Some(0) || out.truncated {return Err("La preparación de pySerial no terminó; puedes reintentarlo sin cambiar tu Python.".into());}
            Ok(())
        };
        let managed=root.join("Scripts/python.exe");
        // Also repairs an environment interrupted while ensurepip was running.
        run(&python,vec!["-I".into(),"-m".into(),"venv".into(),root.as_os_str().to_owned()])?;
        run(&managed,vec!["-I".into(),"-m".into(),"pip".into(),"install".into(),"--no-index".into(),"--no-deps".into(),"--disable-pip-version-check".into(),path.into_os_string()])?;
        if !crate::serial_diagnostics::available(&managed,&root) {return Err("El entorno no confirmó pySerial.".into());}
        Ok(None)
    }).await.map_err(|_|"No se pudo completar la preparación.")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn downloads_are_fixed_and_fail_closed_on_changed_bytes() {
        for program in [Program::Tshark, Program::Usbpcap, Program::Serial] {
            let p = package(program).unwrap();
            assert!(p.url.starts_with("https://"));
            assert!(crate::kicad_project::valid_hash(p.sha256));
            assert!(!valid(b"substituted package", &p));
        }
        assert!(package(Program::Freecad).is_err());
    }
}
