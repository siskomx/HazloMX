//! Arduino Agent's native editor companion. Internal until real IDE and approval acceptance pass.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs::OpenOptions,
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
};
use uuid::Uuid;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

const SOURCE_LIMIT: usize = 65536;
const FRAME_LIMIT: usize = 512 * 1024;

// Connection secrets are deliberately neither serializable nor printable.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    editor_version: u32,
    instance_id: Uuid,
    pipe: String,
    token: String,
}
impl Connection {
    fn validate(&self) -> Result<(), Failure> {
        if self.editor_version != 1
            || self.instance_id.is_nil()
            || self.pipe != format!(r"\\.\pipe\hazlomexa-arduino-{}", self.instance_id)
            || self.token.len() != 64
            || !self.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Failure::Refused);
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self, Failure> {
        crate::project::valid_location(path).map_err(|_| Failure::Refused)?;
        let _parents = crate::project::pin_lineage(path.parent().ok_or(Failure::Refused)?)
            .map_err(|_| Failure::Refused)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| Failure::Refused)?;
        let metadata = file.metadata().map_err(|_| Failure::Refused)?;
        if !metadata.is_file()
            || metadata.len() > 4096
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(Failure::Refused);
        }
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Refused)?;
        if bytes.len() > 4096 {
            return Err(Failure::Refused);
        }
        let connection: Self = serde_json::from_slice(&bytes).map_err(|_| Failure::Refused)?;
        connection.validate()?;
        Ok(connection)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Refused,
    Uncertain,
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Refused => "No se confirmó la conexión o el documento de Arduino Agent. Relee el sketch antes de proponer un cambio.",
            Self::Uncertain => "El cambio pudo llegar a Arduino Agent. Relee su documento antes de continuar; no repitas el cambio automáticamente.",
        }
    }
}

fn source_valid(source: &str, eol: u32) -> bool {
    source.len() <= SOURCE_LIMIT
        && !source.contains('\0')
        && match eol {
            1 => !source.contains('\r'),
            2 => !source.replace("\r\n", "").contains(['\r', '\n']),
            _ => false,
        }
}
fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub editor_version: u32,
    pub instance_id: Uuid,
    pub document_token: Uuid,
    pub name: String,
    pub version: u64,
    pub state_sha256: String,
    pub source: String,
    pub eol: u32,
    pub dirty: bool,
    pub source_is_untrusted_data: bool,
}
impl Observation {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.editor_version != 1
            || self.instance_id.is_nil()
            || self.document_token.is_nil()
            || self.version == 0
            || self.version > 9007199254740991
            || !self.source_is_untrusted_data
            || self.name.len() > 1024
            || !self.name.to_ascii_lowercase().ends_with(".ino")
            || self.name.chars().any(|c| {
                c.is_control() || matches!(c, '/' | '\\' | ':' | '"' | '<' | '>' | '|' | '?' | '*')
            })
            || !source_valid(&self.source, self.eol)
            || digest(&self.source) != self.state_sha256
        {
            return Err(Failure::Refused);
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub editor_version: u32,
    pub job_id: Uuid,
    pub status: String,
    pub before_sha256: String,
    pub after: Observation,
    pub native_edit_confirmed: bool,
}
impl Receipt {
    pub fn validate(&self, job: Uuid, before: &Observation, source: &str) -> Result<(), Failure> {
        let valid = before.validate().is_ok()
            && self.after.validate().is_ok()
            && !job.is_nil()
            && self.editor_version == 1
            && self.job_id == job
            && self.status == "completed"
            && self.native_edit_confirmed
            && self.before_sha256 == before.state_sha256
            && source != before.source
            && self.after.instance_id == before.instance_id
            && self.after.document_token == before.document_token
            && self.after.name == before.name
            && self.after.eol == before.eol
            && self.after.version > before.version
            && self.after.source == source;
        if valid {
            Ok(())
        } else {
            Err(Failure::Uncertain)
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply<T> {
    ok: bool,
    result: Option<T>,
    outcome: Option<String>,
    error: Option<String>,
}

async fn exchange<T: DeserializeOwned>(
    connection: &Connection,
    request: serde_json::Value,
    mutation: bool,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<T, Failure> {
    connection.validate()?;
    if cancel.load(Ordering::SeqCst) {
        return Err(Failure::Refused);
    }
    let mut frame = serde_json::to_vec(&json!({"token":connection.token,"request":request}))
        .map_err(|_| Failure::Refused)?;
    frame.push(b'\n');
    if frame.len() > FRAME_LIMIT {
        return Err(Failure::Refused);
    }
    let sent = Cell::new(false);
    let attempt = async {
        let mut pipe = ClientOptions::new()
            .open(&connection.pipe)
            .map_err(|_| ())?;
        // Once a write is attempted, partial delivery or lost replies cannot prove nonexecution.
        sent.set(mutation);
        pipe.write_all(&frame).await.map_err(|_| ())?;
        let mut bytes = Vec::new();
        (&mut pipe)
            .take((FRAME_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| ())?;
        if bytes.len() > FRAME_LIMIT {
            return Err(());
        }
        serde_json::from_slice::<Reply<T>>(&bytes).map_err(|_| ())
    };
    let cancelled = async {
        while !cancel.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    let reply = tokio::select! {
        _ = cancelled => None,
        result = tokio::time::timeout(timeout, attempt) => result.ok().and_then(Result::ok),
    };
    let failure = if sent.get() {
        Failure::Uncertain
    } else {
        Failure::Refused
    };
    match reply {
        Some(Reply {
            ok: true,
            result: Some(value),
            outcome: None,
            error: None,
        }) => Ok(value),
        Some(Reply {
            ok: false,
            result: None,
            outcome: Some(outcome),
            error: Some(error),
        }) if outcome == "refused" && error == "native_editor_not_confirmed" => {
            Err(Failure::Refused)
        }
        _ => Err(failure),
    }
}

pub async fn inspect(connection: &Connection, cancel: &AtomicBool) -> Result<Observation, Failure> {
    let result: Observation = exchange(
        connection,
        json!({"action":"inspect"}),
        false,
        cancel,
        Duration::from_secs(15),
    )
    .await?;
    result.validate()?;
    if result.instance_id != connection.instance_id {
        return Err(Failure::Refused);
    }
    Ok(result)
}

// Callers must obtain human approval bound to this exact observation before calling apply.
pub async fn apply(
    connection: &Connection,
    job: Uuid,
    before: &Observation,
    source: &str,
    cancel: &AtomicBool,
) -> Result<Receipt, Failure> {
    before.validate()?;
    if job.is_nil()
        || before.instance_id != connection.instance_id
        || source == before.source
        || !source_valid(source, before.eol)
    {
        return Err(Failure::Refused);
    }
    let receipt:Receipt = exchange(connection, json!({"action":"apply","job_id":job,
        "instance_id":before.instance_id,"document_token":before.document_token,"version":before.version,
        "state_sha256":before.state_sha256,"source":source}), true, cancel, Duration::from_secs(15)).await?;
    receipt.validate(job, before, source)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    fn observation(instance_id: Uuid) -> Observation {
        let source = "void loop() { delay(1000); }\n// unsaved note\n".to_string();
        Observation {
            editor_version: 1,
            instance_id,
            document_token: Uuid::new_v4(),
            name: "OwnedSketch.ino".into(),
            version: 3,
            state_sha256: digest(&source),
            source,
            eol: 1,
            dirty: true,
            source_is_untrusted_data: true,
        }
    }
    fn fixture() -> (Connection, NamedPipeServer) {
        let instance_id = Uuid::new_v4();
        let connection = Connection {
            editor_version: 1,
            instance_id,
            pipe: format!(r"\\.\pipe\hazlomexa-arduino-{instance_id}"),
            token: "a".repeat(64),
        };
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&connection.pipe)
            .unwrap();
        (connection, server)
    }
    async fn receive(server: &mut NamedPipeServer) -> serde_json::Value {
        server.connect().await.unwrap();
        let mut bytes = Vec::new();
        while bytes.len() <= FRAME_LIMIT {
            let byte = server.read_u8().await.unwrap();
            if byte == b'\n' {
                return serde_json::from_slice(&bytes).unwrap();
            }
            bytes.push(byte);
        }
        panic!("Request must be bounded");
    }
    fn receipt(job: Uuid, before: &Observation, source: &str) -> serde_json::Value {
        let mut after = before.clone();
        after.version += 1;
        after.source = source.into();
        after.state_sha256 = digest(source);
        json!({"editor_version":1,"job_id":job,"status":"completed","before_sha256":before.state_sha256,
            "after":after,"native_edit_confirmed":true})
    }

    #[test]
    fn untrusted_native_observations_need_exact_source_identity_and_bounds() {
        let before = observation(Uuid::new_v4());
        before.validate().unwrap();
        let original = serde_json::to_value(&before).unwrap();
        for (field, value) in [
            ("editor_version", json!(2)),
            ("instance_id", json!(Uuid::nil())),
            ("document_token", json!(Uuid::nil())),
            ("name", json!("../foreign.ino")),
            ("version", json!(0)),
            ("version", json!(9007199254740992u64)),
            ("state_sha256", json!("0".repeat(64))),
            ("source_is_untrusted_data", json!(false)),
            ("source", json!("x".repeat(SOURCE_LIMIT + 1))),
            ("eol", json!(2)),
        ] {
            let mut value_copy = original.clone();
            value_copy[field] = value;
            let invalid: Observation = serde_json::from_value(value_copy).unwrap();
            assert_eq!(invalid.validate(), Err(Failure::Refused), "{field}");
        }
        assert!(!source_valid("bad\0", 1));
        assert!(!source_valid("a\rb", 1));
        assert!(source_valid("a\r\nb\r\n", 2));
        assert!(source_valid("", 1));
    }

    #[test]
    fn completed_receipts_bind_the_job_original_and_exact_native_result() {
        let before = observation(Uuid::new_v4());
        let job = Uuid::new_v4();
        let source = before.source.replace("1000", "500");
        let original = receipt(job, &before, &source);
        serde_json::from_value::<Receipt>(original.clone())
            .unwrap()
            .validate(job, &before, &source)
            .unwrap();
        for (field, value) in [
            ("job_id", json!(Uuid::new_v4())),
            ("before_sha256", json!("0".repeat(64))),
            ("status", json!("failed")),
            ("native_edit_confirmed", json!(false)),
            ("editor_version", json!(2)),
        ] {
            let mut invalid = original.clone();
            invalid[field] = value;
            assert_eq!(
                serde_json::from_value::<Receipt>(invalid)
                    .unwrap()
                    .validate(job, &before, &source),
                Err(Failure::Uncertain)
            );
        }
        for (field, value) in [
            ("instance_id", json!(Uuid::new_v4())),
            ("document_token", json!(Uuid::new_v4())),
            ("name", json!("Other.ino")),
            ("version", json!(before.version)),
            ("source", json!("changed by user\n")),
        ] {
            let mut invalid = original.clone();
            invalid["after"][field] = value;
            assert_eq!(
                serde_json::from_value::<Receipt>(invalid)
                    .unwrap()
                    .validate(job, &before, &source),
                Err(Failure::Uncertain)
            );
        }
    }

    #[tokio::test]
    async fn actual_pipe_inspection_and_apply_validate_the_full_native_contract() {
        let (connection, mut server) = fixture();
        let before = observation(connection.instance_id);
        let expected = before.clone();
        let token = connection.token.clone();
        let worker = tokio::spawn(async move {
            let request = receive(&mut server).await;
            assert_eq!(
                request,
                json!({"token":token,"request":{"action":"inspect"}})
            );
            server
                .write_all(&serde_json::to_vec(&json!({"ok":true,"result":expected})).unwrap())
                .await
                .unwrap();
        });
        let observed = inspect(&connection, &AtomicBool::new(false)).await.unwrap();
        worker.await.unwrap();
        assert_eq!(observed.state_sha256, before.state_sha256);
        assert!(observed.dirty);
        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&connection.pipe)
            .unwrap();
        let job = Uuid::new_v4();
        let source = before.source.replace("1000", "500");
        let result = receipt(job, &before, &source);
        let expected = before.clone();
        let edited = source.clone();
        let worker = tokio::spawn(async move {
            let request = receive(&mut server).await;
            assert_eq!(
                request["request"],
                json!({"action":"apply","job_id":job,"instance_id":expected.instance_id,
                "document_token":expected.document_token,"version":expected.version,"state_sha256":expected.state_sha256,"source":edited})
            );
            server
                .write_all(&serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap())
                .await
                .unwrap();
        });
        let result = apply(&connection, job, &before, &source, &AtomicBool::new(false))
            .await
            .unwrap();
        worker.await.unwrap();
        assert_eq!(result.after.source, source);
        assert!(result.after.source.contains("unsaved note"));
    }

    #[tokio::test]
    async fn refusal_is_distinct_from_lost_malformed_or_unconfirmed_edit_replies() {
        for (reply, expected) in [
            (
                Some(json!({"ok":false,"outcome":"refused","error":"native_editor_not_confirmed"})),
                Failure::Refused,
            ),
            (
                Some(
                    json!({"ok":false,"outcome":"uncertain","error":"native_editor_not_confirmed"}),
                ),
                Failure::Uncertain,
            ),
            (
                Some(json!({"ok":true,"result":{"unexpected":"reply"}})),
                Failure::Uncertain,
            ),
            (None, Failure::Uncertain),
        ] {
            let (connection, mut server) = fixture();
            let before = observation(connection.instance_id);
            let worker = tokio::spawn(async move {
                receive(&mut server).await;
                if let Some(reply) = reply {
                    server
                        .write_all(&serde_json::to_vec(&reply).unwrap())
                        .await
                        .unwrap();
                }
            });
            let result = apply(
                &connection,
                Uuid::new_v4(),
                &before,
                &before.source.replace("1000", "500"),
                &AtomicBool::new(false),
            )
            .await;
            assert_eq!(result.unwrap_err(), expected);
            worker.await.unwrap();
        }
    }

    #[tokio::test]
    async fn cancellation_before_dispatch_never_connects_but_after_dispatch_is_uncertain() {
        let (connection, mut server) = fixture();
        let before = observation(connection.instance_id);
        let source = before.source.replace("1000", "500");
        let cancel = std::sync::Arc::new(AtomicBool::new(true));
        assert_eq!(
            apply(&connection, Uuid::new_v4(), &before, &source, &cancel)
                .await
                .unwrap_err(),
            Failure::Refused
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(50), server.connect())
                .await
                .is_err()
        );
        cancel.store(false, Ordering::SeqCst);
        let signal = cancel.clone();
        let (stop, done) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            signal.store(true, Ordering::SeqCst);
            let _ = done.await;
        });
        assert_eq!(
            apply(&connection, Uuid::new_v4(), &before, &source, &cancel)
                .await
                .unwrap_err(),
            Failure::Uncertain
        );
        stop.send(()).unwrap();
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn held_and_oversized_replies_cannot_escape_the_transport_limits() {
        let (connection, mut server) = fixture();
        let (stop, done) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            let _ = done.await;
        });
        let result = exchange::<Receipt>(
            &connection,
            json!({"action":"apply"}),
            true,
            &AtomicBool::new(false),
            Duration::from_millis(100),
        )
        .await;
        assert_eq!(result.unwrap_err(), Failure::Uncertain);
        stop.send(()).unwrap();
        worker.await.unwrap();
        let (connection, mut server) = fixture();
        let worker = tokio::spawn(async move {
            receive(&mut server).await;
            let _ = server.write_all(&vec![b' '; FRAME_LIMIT + 1]).await;
        });
        assert_eq!(
            inspect(&connection, &AtomicBool::new(false))
                .await
                .unwrap_err(),
            Failure::Refused
        );
        worker.await.unwrap();
    }

    #[tokio::test]
    async fn discovery_refuses_remote_or_mismatched_pipes_and_oversized_files() {
        let (mut connection, _server) = fixture();
        connection.pipe = r"\\remote\pipe\other".into();
        assert_eq!(connection.validate(), Err(Failure::Refused));
        connection.pipe = format!(r"\\.\pipe\hazlomexa-arduino-{}", Uuid::new_v4());
        assert_eq!(connection.validate(), Err(Failure::Refused));
        let root =
            std::env::temp_dir().join(format!("hazlomexa-native-discovery-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("connection.json");
        let (connection, _server) = fixture();
        std::fs::write(&path,serde_json::to_vec(&json!({"editor_version":1,"instance_id":connection.instance_id,"pipe":connection.pipe,"token":connection.token})).unwrap()).unwrap();
        assert_eq!(
            Connection::load(&path).unwrap().instance_id,
            connection.instance_id
        );
        std::fs::write(&path, vec![b' '; 4097]).unwrap();
        assert!(Connection::load(&path).is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
