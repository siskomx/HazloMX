//! One approved public HTTPS read. No task files, cookies, redirects or automatic retries.
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use uuid::Uuid;

pub const LIMIT: u64 = 512 * 1024;
pub const REPORT_LIMIT: u64 = 4 * LIMIT;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: u32,
    pub public_document_version: u32,
    pub job_id: Uuid,
    pub url: String,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_sha256: Option<String>,
}
impl Input {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.public_document_version != 1
            || self.job_id.is_nil()
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 1024
            || self
                .receipt_sha256
                .as_ref()
                .is_some_and(|h| !crate::kicad_project::valid_hash(h))
            || self
                .redirect_sha256
                .as_ref()
                .is_some_and(|h| !crate::kicad_project::valid_hash(h))
            || self.redirect_sha256.is_some() && self.receipt_sha256.is_some()
        {
            return Err("La consulta pública no es válida.".into());
        }
        let parsed = address(&self.url)?;
        if parsed.as_str() != self.url {
            return Err(
                "La dirección debe coincidir con la dirección normalizada que se revisó.".into(),
            );
        }
        Ok(())
    }
}
pub fn address(raw: &str) -> Result<reqwest::Url, String> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return Err("Dirección pública demasiado grande o no válida.".into());
    }
    let url =
        reqwest::Url::parse(raw).map_err(|_| "Escribe una dirección HTTPS pública completa.")?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Solo se admiten documentos HTTPS públicos sin credenciales, fragmentos ni puertos especiales.".into());
    }
    let host = url.host_str().ok_or("Falta el sitio público.")?;
    if !host.contains('.') && host.trim_matches(['[', ']']).parse::<IpAddr>().is_err()
        || host.ends_with('.')
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return Err("Esta consulta no accede a nombres locales ni dispositivos de tu red.".into());
    }
    if url.query_pairs().any(|(key, _)| {
        [
            "token",
            "key",
            "password",
            "secret",
            "authorization",
            "credential",
        ]
        .iter()
        .any(|s| key.to_ascii_lowercase().contains(s))
    }) {
        return Err("No se admiten credenciales en la dirección de investigación pública.".into());
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        if !public_ip(ip) {
            return Err(
                "Esta consulta no accede a direcciones locales, reservadas ni de dispositivos."
                    .into(),
            );
        }
    }
    Ok(url)
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(matches!(a, 0 | 10 | 127 | 224..=255)
                || a == 100 && (64..=127).contains(&b)
                || a == 169 && b == 254
                || a == 172 && (16..=31).contains(&b)
                || a == 192 && (b == 168 || b == 0 || b == 88 && c == 99)
                || a == 198 && (b == 18 || b == 19 || b == 51 && c == 100)
                || a == 203 && b == 0 && c == 113
                || ip.octets() == [168, 63, 129, 16])
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            s[0] & 0xe000 == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x0200 || s[1] == 0x0db8)
                    || s[0] == 0x2002
                    || s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}
async fn public_addresses(url: &reqwest::Url) -> Result<Vec<SocketAddr>, String> {
    let host = url
        .host_str()
        .ok_or("Falta el sitio público.")?
        .trim_matches(['[', ']']);
    let addresses =
        tokio::time::timeout(Duration::from_secs(5), tokio::net::lookup_host((host, 443)))
            .await
            .map_err(|_| "El sitio no resolvió a tiempo; no se reintentó la consulta.")?
            .map_err(|_| "No se pudo resolver el sitio público; no se reintentó la consulta.")?
            .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.len() > 32 || addresses.iter().any(|a| !public_ip(a.ip()))
    {
        return Err(
            "La dirección resuelve a una red local o no admitida. No se contactó el sitio.".into(),
        );
    }
    Ok(addresses)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    public_document_version: u32,
    job_id: Uuid,
    url: String,
    purpose: String,
    media_type: String,
    bytes: u64,
    sha256: String,
    content: String,
    source_is_untrusted_data: bool,
}
fn client() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_gzip()
        .no_brotli()
        .no_zstd()
        .no_deflate()
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .user_agent("HazloMexa-PublicDocument/1")
}
async fn exchange(
    client: reqwest::Client,
    url: reqwest::Url,
    cancel: Arc<AtomicBool>,
    redirect_capture: Option<(&Input, &Path)>,
) -> Result<(String, Vec<u8>), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("Consulta pública detenida; no se volvió a enviar.".into());
    }
    let mut response = client
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "text/plain, text/html, application/json, application/xml",
        )
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|_| "No se pudo leer el documento público; no se reintentó la consulta.")?;
    if response.status().is_redirection() {
        if let Some((input, directory)) = redirect_capture {
            let target = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(
                    "El sitio redirige sin una dirección válida; no se siguió la redirección.",
                )?;
            let target = address(&input.url)?
                .join(target)
                .map_err(|_| "La redirección no contiene una dirección válida.")?;
            let target = address(target.as_str()).map_err(|_| {
                "La redirección apunta a una dirección no admitida. No se contactó."
            })?;
            crate::project::atomic_json(
                &directory.join("redirect.json"),
                &Redirect {
                    job_id: input.job_id,
                    source_url: input.url.clone(),
                    target_url: target.to_string(),
                },
                false,
            )
            .map_err(|_| "No se pudo conservar la redirección localmente; no se siguió.")?;
        }
        return Err("El sitio pide otra dirección. Hazlo no la siguió: prepara esa dirección para una nueva revisión.".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "El sitio respondió HTTP {}. No se reintentó la consulta.",
            response.status().as_u16()
        ));
    }
    let media = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !matches!(
        media.as_str(),
        "text/plain"
            | "text/html"
            | "text/markdown"
            | "application/json"
            | "application/xml"
            | "text/xml"
            | "application/xhtml+xml"
    ) {
        return Err("Esta lectura admite texto público UTF-8. El sitio devolvió otro formato; no se ejecutó ni abrió su contenido.".into());
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_ENCODING)
        .is_some_and(|h| h != "identity")
        || response.content_length().is_some_and(|n| n > LIMIT)
    {
        return Err("La respuesta supera el límite o necesita otra decodificación. No se abrió su contenido.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "La lectura pública quedó incompleta; no se reintentó.")?
    {
        if cancel.load(Ordering::SeqCst) {
            return Err("Lectura detenida; no se volvió a enviar la consulta.".into());
        }
        if bytes.len() + chunk.len() > LIMIT as usize {
            return Err("El documento supera 512 KiB; no se procesó una copia truncada.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((media, bytes))
}
pub fn run(input: &Input, directory: &Path, cancel: Arc<AtomicBool>) -> Result<(), String> {
    input.validate()?;
    let url = address(&input.url)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "No se pudo preparar la consulta pública.")?;
    let (media, bytes) = runtime.block_on(async {
        if cancel.load(Ordering::SeqCst) {
            return Err("Consulta detenida antes de contactar al sitio.".into());
        }
        let stopped=async {while !cancel.load(Ordering::SeqCst){tokio::time::sleep(Duration::from_millis(25)).await;}};
        let request=async {
            let addresses = public_addresses(&url).await?;
            let client = client()
                .resolve_to_addrs(url.host_str().unwrap(), &addresses)
                .build()
                .map_err(|_| "No se pudo preparar la conexión pública.")?;
            exchange(client, url, cancel.clone(),Some((input,directory))).await
        };
        tokio::select!{biased;
            _=stopped=>Err("Consulta pública detenida; no se volvió a enviar.".into()),
            result=tokio::time::timeout(Duration::from_secs(20),request)=>result.map_err(|_|"No se pudo leer el documento público; no se reintentó la consulta.")?,
        }
    })?;
    let bytes_len = bytes.len() as u64;
    let sha256 = crate::kicad_project::digest(&bytes);
    let content = String::from_utf8(bytes)
        .map_err(|_| "El documento no es texto UTF-8; no se convirtió ni envió a una IA.")?;
    let report = Report {
        public_document_version: 1,
        job_id: input.job_id,
        url: input.url.clone(),
        purpose: input.purpose.clone(),
        media_type: media,
        bytes: bytes_len,
        sha256,
        content,
        source_is_untrusted_data: true,
    };
    crate::project::atomic_json(&directory.join("result.json"), &report, false)
        .map_err(|_| "No se pudo conservar el documento público localmente.".into())
}
pub fn validate_artifacts(directory: &Path, input: &Input) -> Result<(), String> {
    input.validate()?;
    let bytes = crate::kicad_project::read_regular(&directory.join("result.json"), REPORT_LIMIT)?;
    if input.receipt_sha256.as_deref() != Some(&crate::kicad_project::digest(&bytes)) {
        return Err("La copia local del documento público cambió.".into());
    }
    let report: Report =
        serde_json::from_slice(&bytes).map_err(|_| "Informe público no válido.")?;
    if report.public_document_version != 1
        || report.job_id != input.job_id
        || report.url != input.url
        || report.purpose != input.purpose
        || !report.source_is_untrusted_data
        || report.content.len() as u64 != report.bytes
        || report.bytes > LIMIT
        || crate::kicad_project::digest(report.content.as_bytes()) != report.sha256
    {
        return Err("La copia pública no coincide con la consulta aprobada.".into());
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Redirect {
    job_id: Uuid,
    source_url: String,
    target_url: String,
}
pub(crate) fn redirect_receipt(directory: &Path, input: &Input) -> Result<Option<String>, String> {
    let file = directory.join("redirect.json");
    if !file
        .try_exists()
        .map_err(|_| "No se pudo comprobar la redirección local.")?
    {
        return Ok(None);
    }
    let bytes = crate::kicad_project::read_regular(&file, 8192)?;
    validate_redirect(&bytes, input)?;
    Ok(Some(crate::kicad_project::digest(&bytes)))
}
fn validate_redirect(bytes: &[u8], input: &Input) -> Result<Redirect, String> {
    let redirect: Redirect =
        serde_json::from_slice(bytes).map_err(|_| "La redirección local no es válida.")?;
    if redirect.job_id != input.job_id
        || redirect.source_url != input.url
        || address(&redirect.target_url)?.as_str() != redirect.target_url
    {
        return Err("La redirección no corresponde a la consulta aprobada.".into());
    }
    Ok(redirect)
}
pub(crate) fn redirect_target(directory: &Path, input: &Input) -> Result<String, String> {
    let bytes = crate::kicad_project::read_regular(&directory.join("redirect.json"), 8192)?;
    if input.redirect_sha256.as_deref() != Some(&crate::kicad_project::digest(&bytes)) {
        return Err("La redirección cambió desde la lectura; no se reutilizó su permiso.".into());
    }
    Ok(validate_redirect(&bytes, input)?.target_url)
}
pub(crate) fn safe_status(error: Option<&str>) -> serde_json::Value {
    let Some(error) = error else {
        return serde_json::Value::Null;
    };
    if let Some(code) = error
        .strip_prefix("El sitio respondió HTTP ")
        .and_then(|s| s.strip_suffix(". No se reintentó la consulta."))
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|n| (100..=599).contains(n))
    {
        return serde_json::json!({"kind":"http_status","code":code,"automatically_retried":false});
    }
    let kind=match error {
        "El sitio pide otra dirección. Hazlo no la siguió: prepara esa dirección para una nueva revisión."=>"redirect_needs_approval",
        "Esta lectura admite texto público UTF-8. El sitio devolvió otro formato; no se ejecutó ni abrió su contenido."=>"unsupported_format",
        "El documento supera 512 KiB; no se procesó una copia truncada."|"La respuesta supera el límite o necesita otra decodificación. No se abrió su contenido."=>"response_limit_or_encoding",
        "El documento no es texto UTF-8; no se convirtió ni envió a una IA."=>"unsupported_encoding",
        "La dirección resuelve a una red local o no admitida. No se contactó el sitio."=>"destination_refused",
        "La redirección apunta a una dirección no admitida. No se contactó."=>"destination_refused",
        "No se pudo resolver el sitio público; no se reintentó la consulta."|"El sitio no resolvió a tiempo; no se reintentó la consulta."=>"resolution_failed",
        "No se pudo leer el documento público; no se reintentó la consulta."|"La lectura pública quedó incompleta; no se reintentó."=>"transport_failed",
        _=>return serde_json::Value::Null,
    };
    serde_json::json!({"kind":kind,"automatically_retried":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    fn fixture(response: Vec<u8>) -> (reqwest::Url, std::thread::JoinHandle<String>) {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            let _ = stream.write_all(&response);
            String::from_utf8(request).unwrap()
        });
        (
            reqwest::Url::parse(&format!("http://127.0.0.1:{port}/manual?q=public")).unwrap(),
            handle,
        )
    }
    #[test]
    fn address_validation_never_treats_devices_or_credentials_as_public_research() {
        for url in [
            "http://example.com/",
            "file:///private",
            "https://user:password@example.com/",
            "https://example.com:8443/",
            "https://example.com/#private",
            "https://localhost/",
            "https://device.local/",
            "https://127.1/",
            "https://2130706433/",
            "https://0x7f000001/",
            "https://10.0.0.1/",
            "https://169.254.169.254/",
            "https://[::1]/",
            "https://[::ffff:8.8.8.8]/",
            "https://example.com/?api%5fkey=private",
            "https://example.com./",
        ] {
            assert!(address(url).is_err(), "Accepted {url}");
        }
        for ip in [
            "0.0.0.0",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.1.2.3",
            "168.63.129.16",
            "::",
            "fe80::1",
            "fc00::1",
            "2001:db8::1",
            "2002:0808:0808::",
            "3fff:1::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "Accepted {ip}");
        }
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(public_ip(ip.parse().unwrap()), "Refused {ip}");
        }
        assert!(address("https://example.com/manual?language=en").is_ok());
    }
    #[tokio::test]
    async fn public_resolution_refuses_local_dns_before_http() {
        assert!(
            public_addresses(&reqwest::Url::parse("https://localhost/").unwrap())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn request_sends_only_the_approved_url_and_never_follows_or_retries() {
        let (url,handle)=fixture(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 6\r\nSet-Cookie: secret=server-only\r\nConnection: close\r\n\r\nmanual".to_vec());
        // Loopback HTTP is test-only: the production entry rejects it before constructing this client.
        let (media, body) = exchange(
            client().https_only(false).build().unwrap(),
            url,
            Arc::new(AtomicBool::new(false)),
            None,
        )
        .await
        .unwrap();
        assert_eq!(media, "text/plain");
        assert_eq!(body, b"manual");
        let request = handle.join().unwrap().to_ascii_lowercase();
        assert!(request.starts_with("get /manual?q=public http/1.1\r\n"));
        assert!(
            !request.contains("cookie:")
                && !request.contains("authorization:")
                && !request.contains("referer:")
                && !request.contains("raw_private_sentinel")
        );
        let redirect_target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        redirect_target.set_nonblocking(true).unwrap();
        let response=format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{}/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",redirect_target.local_addr().unwrap().port());
        let (url, handle) = fixture(response.into_bytes());
        assert!(exchange(
            client().https_only(false).build().unwrap(),
            url,
            Arc::new(AtomicBool::new(false)),
            None
        )
        .await
        .unwrap_err()
        .contains("otra dirección"));
        handle.join().unwrap();
        assert_eq!(
            redirect_target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        for response in [b"HTTP/1.1 500 Broken\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",LIMIT+1).into_bytes(),b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()] {
            let (url,handle)=fixture(response);
            assert!(exchange(client().https_only(false).build().unwrap(),url,Arc::new(AtomicBool::new(false)),None).await.is_err());
            handle.join().unwrap();
        }
        assert!(safe_status(Some("RAW_PRIVATE_SENTINEL")).is_null());
        assert!(safe_status(Some(
            "El sitio respondió HTTP 404 RAW_PRIVATE_SENTINEL. No se reintentó la consulta."
        ))
        .is_null());
        assert_eq!(
            safe_status(Some(
                "El sitio respondió HTTP 404. No se reintentó la consulta."
            ))["code"],
            404
        );
    }
    #[test]
    fn cancelled_read_performs_no_resolution_and_report_identity_rejects_tampering() {
        let directory = std::env::temp_dir().join(format!("hazlo-public-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut input = Input {
            schema_version: 1,
            public_document_version: 1,
            job_id: Uuid::new_v4(),
            url: "https://example.invalid/manual".into(),
            purpose: "Owned public document".into(),
            receipt_sha256: None,
            redirect_sha256: None,
        };
        assert!(run(&input, &directory, Arc::new(AtomicBool::new(true)))
            .unwrap_err()
            .contains("antes de contactar"));
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        let report = Report {
            public_document_version: 1,
            job_id: input.job_id,
            url: input.url.clone(),
            purpose: input.purpose.clone(),
            media_type: "text/plain".into(),
            bytes: 6,
            sha256: crate::kicad_project::digest(b"manual"),
            content: "manual".into(),
            source_is_untrusted_data: true,
        };
        let bytes = serde_json::to_vec(&report).unwrap();
        std::fs::write(directory.join("result.json"), &bytes).unwrap();
        input.receipt_sha256 = Some(crate::kicad_project::digest(&bytes));
        validate_artifacts(&directory, &input).unwrap();
        let mut changed = input.clone();
        changed.purpose = "Different scope".into();
        assert!(validate_artifacts(&directory, &changed).is_err());
        std::fs::write(directory.join("result.json"), b"forged").unwrap();
        assert!(validate_artifacts(&directory, &input).is_err());
        assert!(directory
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn redirect_target_stays_local_until_a_new_review() {
        let directory =
            std::env::temp_dir().join(format!("hazlo-public-redirect-{}", Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut input = Input {
            schema_version: 1,
            public_document_version: 1,
            job_id: Uuid::new_v4(),
            url: "https://example.org/first".into(),
            purpose: "Owned redirect fixture".into(),
            receipt_sha256: None,
            redirect_sha256: None,
        };
        let target = "https://example.org/manual?q=PRIVATE_REDIRECT_SENTINEL";
        let (url,handle)=fixture(format!("HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
        let error = exchange(
            client().https_only(false).build().unwrap(),
            url,
            Arc::new(AtomicBool::new(false)),
            Some((&input, &directory)),
        )
        .await
        .unwrap_err();
        handle.join().unwrap();
        assert!(!error.contains("PRIVATE_REDIRECT_SENTINEL"));
        assert_eq!(safe_status(Some(&error))["kind"], "redirect_needs_approval");
        input.redirect_sha256 = redirect_receipt(&directory, &input).unwrap();
        assert_eq!(redirect_target(&directory, &input).unwrap(), target);
        let mut foreign = input.clone();
        foreign.job_id = Uuid::new_v4();
        assert!(redirect_target(&directory, &foreign).is_err());
        std::fs::write(directory.join("redirect.json"), b"forged").unwrap();
        assert!(redirect_target(&directory, &input).is_err());
        assert!(directory
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
