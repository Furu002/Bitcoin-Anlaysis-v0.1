use signal_core::runtime::{Handle, Request};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("SIGNAL_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".signal/preview.sqlite3"));
    let handle = Handle::start(path).await?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.len() > 8192 {
            return Err("Oversized local request".into());
        }
        let result = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle.call(req).await,
            Err(e) => Err(format!("Invalid request: {e}")),
        };
        let response = match result {
            Ok(data) => serde_json::json!({"ok":true,"data":data}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        };
        stdout.write_all(format!("{response}\n").as_bytes()).await?;
        stdout.flush().await?;
        if response["data"]["shutdown"] == true {
            return Ok(());
        }
    }
    let _ = handle.call(Request::Shutdown).await;
    Ok(())
}
