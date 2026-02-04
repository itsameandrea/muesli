use crate::config::loader::socket_path;
use crate::daemon::protocol::{DaemonRequest, DaemonResponse};
use crate::error::{MuesliError, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    pub async fn connect() -> Result<Self> {
        let socket = socket_path()?;

        let stream = UnixStream::connect(&socket)
            .await
            .map_err(|_| MuesliError::DaemonNotRunning)?;

        Ok(Self { stream })
    }

    pub async fn send(&mut self, request: DaemonRequest) -> Result<DaemonResponse> {
        let request_json = serde_json::to_string(&request)?;

        self.stream.write_all(request_json.as_bytes()).await?;
        self.stream.write_all(b"\n").await?;
        self.stream.flush().await?;

        let mut reader = BufReader::new(&mut self.stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: DaemonResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    pub async fn ping() -> Result<bool> {
        match Self::connect().await {
            Ok(mut client) => match client.send(DaemonRequest::Ping).await {
                Ok(DaemonResponse::Pong) => Ok(true),
                _ => Ok(false),
            },
            Err(_) => Ok(false),
        }
    }
}

pub async fn is_daemon_running() -> bool {
    DaemonClient::ping().await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::protocol::DaemonStatus;
    use crate::daemon::server::DaemonState;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::net::UnixListener;
    use tokio::sync::Mutex;

    async fn setup_test_server(socket_path: &std::path::Path) -> UnixListener {
        if socket_path.exists() {
            std::fs::remove_file(socket_path).unwrap();
        }
        UnixListener::bind(socket_path).unwrap()
    }

    #[tokio::test]
    async fn test_is_daemon_running_returns_false_when_not_running() {
        let result = is_daemon_running().await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_client_connect_fails_when_daemon_not_running() {
        let result = DaemonClient::connect().await;
        assert!(result.is_err());
        match result {
            Err(MuesliError::DaemonNotRunning) => {}
            _ => panic!("Expected DaemonNotRunning error"),
        }
    }

    #[tokio::test]
    async fn test_ping_returns_false_when_daemon_not_running() {
        let result = DaemonClient::ping().await.unwrap();
        assert!(!result);
    }
}
