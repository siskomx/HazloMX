//! Local authenticated transport to the desktop's existing tool host.
use crate::host_tools::{HostTools, McpTools};
use rmcp::ServiceExt;
use serde::Serialize;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, ServerOptions},
    sync::watch,
};
use uuid::Uuid;

#[derive(Clone, Serialize)]
pub struct BridgeConfig {
    pub command: String,
    pub env: std::collections::BTreeMap<String, String>,
}
pub struct ToolBridge {
    config: BridgeConfig,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl ToolBridge {
    pub fn start(host: Arc<HostTools>, helper: &Path) -> Result<Self, String> {
        let handler = McpTools::new(host)?;
        Self::start_handler(handler, helper)
    }
    pub fn start_project(
        host: Arc<HostTools>,
        helper: &Path,
        project: Uuid,
    ) -> Result<Self, String> {
        Self::start_handler(McpTools::for_project(host, project)?, helper)
    }
    fn start_handler(handler: McpTools, helper: &Path) -> Result<Self, String> {
        if !helper.is_absolute() || !helper.is_file() {
            return Err("Falta hazlomexa-tools.exe junto a la aplicación. Compila o reinstala el paquete completo.".into());
        }
        let pipe = format!(r"\\.\pipe\hazlomexa-{}", Uuid::new_v4());
        let secret = Uuid::new_v4().simple().to_string();
        let listener = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&pipe)
            .map_err(|e| e.to_string())?;
        let config = BridgeConfig {
            command: helper
                .to_str()
                .ok_or("Ruta del conector no válida.")?
                .into(),
            env: std::collections::BTreeMap::from([
                ("HAZLOMEXA_PIPE".into(), pipe.clone()),
                ("HAZLOMEXA_TOKEN".into(), secret.clone()),
            ]),
        };
        let (stop, mut stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let mut listener = listener;
            let mut sessions = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _=stopped.changed()=>break,
                    _=sessions.join_next(),if !sessions.is_empty()=>{},
                    connected=listener.connect()=>{
                        if connected.is_err(){break;}
                        let Ok(next)=ServerOptions::new().reject_remote_clients(true).create(&pipe) else {break};
                        let mut connection=std::mem::replace(&mut listener,next);
                        if sessions.len()>=4 {drop(connection);continue;}
                        let secret=secret.clone();let handler=handler.clone();let mut stop_session=stopped.clone();
                        sessions.spawn(async move {
                            let handshake=async {
                                let mut received=[0u8;32];connection.read_exact(&mut received).await?;
                                let mismatch=received.iter().zip(secret.as_bytes()).fold(0u8,|difference,(a,b)|difference | (a ^ b));
                                if mismatch!=0{return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,"Connection denied"));}
                                connection.write_all(&[1]).await
                            };
                            tokio::select! {
                                _=stop_session.changed()=>return,
                                result=tokio::time::timeout(Duration::from_secs(3),handshake)=>if !matches!(result,Ok(Ok(()))){return;},
                            }
                            let connecting=handler.serve(connection);
                            let service=tokio::select! {
                                _=stop_session.changed()=>return,
                                result=tokio::time::timeout(Duration::from_secs(10),connecting)=>match result {Ok(Ok(service))=>service,_=>return},
                            };
                            let cancel=service.cancellation_token();
                            let waiting=service.waiting();tokio::pin!(waiting);
                            tokio::select! {_=stop_session.changed()=>{cancel.cancel();let _=waiting.await;},_= &mut waiting=>{}}
                        });
                    }
                }
            }
            drop(listener);
            // Each session sees revocation and closes its protocol service.
            while sessions.join_next().await.is_some() {}
        });
        Ok(Self { config, stop, task })
    }
    pub async fn close(mut self) -> Result<(), String> {
        let _ = self.stop.send(true);
        (&mut self.task)
            .await
            .map_err(|_| "La conexión no pudo cerrarse correctamente.".to_string())
    }
    pub fn config(&self) -> Result<BridgeConfig, String> {
        if self.task.is_finished() {
            return Err("La conexión se cerró. Actívala de nuevo.".into());
        }
        Ok(self.config.clone())
    }
}
impl Drop for ToolBridge {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

pub async fn connect(pipe: &str, secret: &str) -> std::io::Result<NamedPipeClient> {
    if !pipe.starts_with(r"\\.\pipe\hazlomexa-")
        || Uuid::parse_str(&pipe[r"\\.\pipe\hazlomexa-".len()..]).is_err()
        || secret.len() != 32
        || !secret.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid Hazlo Mexa connection",
        ));
    }
    let attempt = async {
        loop {
            match ClientOptions::new().open(pipe) {
                Ok(mut connection) => {
                    connection.write_all(secret.as_bytes()).await?;
                    let mut accepted = [0u8; 1];
                    connection.read_exact(&mut accepted).await?;
                    if accepted != [1] {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::PermissionDenied,
                            "Connection denied",
                        ));
                    }
                    return Ok(connection);
                }
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(50)).await
                }
                Err(error) => return Err(error),
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), attempt)
        .await
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Hazlo Mexa did not accept the connection",
            )
        })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::Workflow;
    use rmcp::{model::CallToolRequestParams, ServiceExt};
    #[tokio::test]
    async fn local_auth_sdk_and_revocation() {
        let root = std::env::temp_dir().join(format!("hazlomexa-bridge-{}", Uuid::new_v4()));
        let workflow =
            Arc::new(Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap());
        let host = Arc::new(HostTools::new(workflow.clone()));
        let bridge = ToolBridge::start(host, &std::env::current_exe().unwrap()).unwrap();
        let config = bridge.config().unwrap();
        let pipe = &config.env["HAZLOMEXA_PIPE"];
        let secret = &config.env["HAZLOMEXA_TOKEN"];
        assert!(connect(pipe, &"0".repeat(32)).await.is_err());
        assert!(connect(r"\\server\pipe\hazlomexa-123", secret)
            .await
            .is_err());
        let connection = connect(pipe, secret).await.unwrap();
        let client = ().serve(connection).await.unwrap();
        assert_eq!(
            client.list_all_tools().await.unwrap().len(),
            crate::host_tools::catalog().len()
        );
        assert!(client
            .call_tool(CallToolRequestParams::new("get_workflow_status"))
            .await
            .unwrap()
            .structured_content
            .unwrap()["project_id"]
            .is_null());
        std::fs::create_dir_all(root.join("project")).unwrap();
        workflow
            .select_project(crate::project::Project::create(&root.join("project")).unwrap())
            .unwrap();
        let changed = client
            .call_tool(CallToolRequestParams::new("get_workflow_status"))
            .await
            .unwrap();
        assert_eq!(changed.is_error, Some(true));
        assert!(changed.structured_content.is_none());
        drop(bridge);
        let stopped = tokio::time::timeout(Duration::from_secs(5), client.waiting()).await;
        assert!(stopped.is_ok());
        let mut denied = false;
        for _ in 0..20 {
            if connect(pipe, secret).await.is_err() {
                denied = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(denied);
        drop(workflow);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    #[ignore = "Build release hazlomexa-tools.exe first"]
    async fn actual_stdio_helper_and_idle_revocation() {
        use rmcp::transport::TokioChildProcess;
        let root = std::env::temp_dir().join(format!("hazlomexa-helper-{}", Uuid::new_v4()));
        let host = Arc::new(HostTools::new(Arc::new(
            Workflow::new(root.join("FreeCADCmd.exe"), root.join("profiles")).unwrap(),
        )));
        let helper = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("release/hazlomexa-tools.exe");
        let bridge = ToolBridge::start(host, &helper).unwrap();
        let config = bridge.config().unwrap();
        let mut command = tokio::process::Command::new(&helper);
        command
            .envs(&config.env)
            .kill_on_drop(true)
            .creation_flags(0x08000000);
        let transport = TokioChildProcess::new(command).unwrap();
        let pid = transport.id().unwrap();
        let client = ().serve(transport).await.unwrap();
        assert_eq!(client.list_all_tools().await.unwrap().len(), 6);
        let result = client
            .call_tool(CallToolRequestParams::new("get_workflow_status"))
            .await
            .unwrap();
        assert_ne!(result.is_error, Some(true));
        drop(bridge);
        assert!(
            tokio::time::timeout(Duration::from_secs(5), client.waiting())
                .await
                .is_ok()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while crate::process::tests::is_alive(pid) && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            !crate::process::tests::is_alive(pid),
            "Owned helper survived revocation"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
