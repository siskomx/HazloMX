#[cfg(windows)]
#[tokio::main]
async fn main() {
    if std::env::args().nth(1).as_deref()==Some("--native-console") {
        std::process::exit(hazlomexa::native_tools::console_child().unwrap_or(1));
    }
    use std::io::{Read, Write};
    use tokio::io::AsyncWriteExt;
    if let Some(code) = hazlomexa::konnect::movement_validator_cli() {
        std::process::exit(code);
    }
    async fn run() -> std::io::Result<()> {
        let pipe = std::env::var("HAZLOMEXA_PIPE")
            .map_err(|_| std::io::Error::other("Enable agent tools in Hazlo Mexa first"))?;
        let secret = std::env::var("HAZLOMEXA_TOKEN")
            .map_err(|_| std::io::Error::other("Missing Hazlo Mexa connection"))?;
        let connection = hazlomexa::tool_bridge::connect(&pipe, &secret).await?;
        let (mut read, mut write) = tokio::io::split(connection);
        let (input_tx, mut input_rx) = tokio::sync::mpsc::channel(8);
        // Native stdin can block forever; detached OS threads do not hold Tokio shutdown open.
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            let mut buffer = [0u8; 8192];
            loop {
                match input.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if input_tx
                            .blocking_send(Ok(buffer[..count].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = input_tx.blocking_send(Err(error));
                        break;
                    }
                }
            }
        });
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<(
            Vec<u8>,
            tokio::sync::oneshot::Sender<std::io::Result<()>>,
        )>(8);
        std::thread::spawn(move || {
            let mut output = std::io::stdout().lock();
            while let Some((bytes, ack)) = output_rx.blocking_recv() {
                let result = output.write_all(&bytes).and_then(|_| output.flush());
                let failed = result.is_err();
                let _ = ack.send(result);
                if failed {
                    break;
                }
            }
        });
        let input = async {
            while let Some(bytes) = input_rx.recv().await {
                write.write_all(&bytes?).await?;
            }
            write.shutdown().await
        };
        let output = async {
            use tokio::io::AsyncReadExt;
            let mut buffer = [0u8; 8192];
            loop {
                let count = read.read(&mut buffer).await?;
                if count == 0 {
                    return Ok(());
                }
                let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
                output_tx
                    .send((buffer[..count].to_vec(), ack_tx))
                    .await
                    .map_err(|_| std::io::Error::other("Output closed"))?;
                tokio::time::timeout(std::time::Duration::from_secs(5), ack_rx)
                    .await
                    .map_err(|_| std::io::Error::other("Output stalled"))?
                    .map_err(|_| std::io::Error::other("Output closed"))??;
            }
        };
        tokio::select! {result=input=>result,result=output=>result}
    }
    if run().await.is_err() {
        eprintln!(
            "Hazlo Mexa tools are unavailable. Open the app and enable a new agent connection."
        );
        std::process::exit(1);
    }
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Hazlo Mexa tools require Windows.");
    std::process::exit(1);
}
