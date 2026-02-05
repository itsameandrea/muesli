#![allow(dead_code)]

use crate::detection::{DetectionEvent, WindowInfo};
use crate::error::{MuesliError, Result};
use hyprland::data::{Client, Clients};
use hyprland::event_listener::AsyncEventListener;
use hyprland::prelude::*;
use tokio::sync::mpsc;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 30;

pub struct HyprlandMonitor {
    event_tx: mpsc::Sender<DetectionEvent>,
    poll_interval_secs: u64,
}

impl HyprlandMonitor {
    pub fn new(event_tx: mpsc::Sender<DetectionEvent>) -> Self {
        Self {
            event_tx,
            poll_interval_secs: DEFAULT_POLL_INTERVAL_SECS,
        }
    }

    pub fn with_poll_interval(
        event_tx: mpsc::Sender<DetectionEvent>,
        poll_interval_secs: u64,
    ) -> Self {
        Self {
            event_tx,
            poll_interval_secs,
        }
    }

    pub fn get_active_window() -> Result<Option<WindowInfo>> {
        let client = Client::get_active()
            .map_err(|e| MuesliError::HyprlandIpc(format!("Failed to get active window: {}", e)))?;

        Ok(client.map(|c| WindowInfo {
            class: c.class,
            title: c.title,
            pid: Some(c.pid),
        }))
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        let tx_events = self.event_tx.clone();
        let tx_poll = self.event_tx.clone();
        let poll_interval = self.poll_interval_secs;

        tokio::select! {
            result = Self::run_event_listener(tx_events) => {
                if let Err(e) = result {
                    tracing::error!("Event listener error: {}", e);
                }
            }
            _ = Self::poll_active_window(tx_poll, poll_interval) => {
                tracing::warn!("Polling loop ended unexpectedly");
            }
        }

        Ok(())
    }

    async fn run_event_listener(event_tx: mpsc::Sender<DetectionEvent>) -> Result<()> {
        tracing::info!("Creating Hyprland async event listener");
        let mut listener = AsyncEventListener::new();

        let tx = event_tx.clone();
        listener.add_active_window_changed_handler(move |data| {
            let tx = tx.clone();
            Box::pin(async move {
                tracing::debug!("Hyprland event received: {:?}", data);
                if let Some(window_data) = data {
                    let window_info = WindowInfo {
                        class: window_data.class.clone(),
                        title: window_data.title.clone(),
                        pid: None,
                    };

                    tracing::info!(
                        "Window change event: {} - {}",
                        window_data.class,
                        window_data.title
                    );

                    let event = DetectionEvent::WindowChanged {
                        window: window_info,
                    };

                    if let Err(e) = tx.send(event).await {
                        tracing::warn!("Failed to send window event: {}", e);
                    }
                }
            })
        });

        tracing::info!("Starting Hyprland event listener");
        listener
            .start_listener_async()
            .await
            .map_err(|e| MuesliError::HyprlandIpc(format!("Event listener failed: {}", e)))?;

        tracing::warn!("Hyprland event listener stopped");
        Ok(())
    }

    async fn poll_active_window(event_tx: mpsc::Sender<DetectionEvent>, interval_secs: u64) {
        let mut last_meeting_key: Option<String> = None;
        tracing::info!("Starting window polling with {}s interval", interval_secs);

        if let Err(e) = Self::scan_all_windows(&event_tx, &mut last_meeting_key).await {
            tracing::error!("Initial window scan failed: {}", e);
        }

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
            tracing::debug!("Polling all windows for meetings...");

            if let Err(e) = Self::scan_all_windows(&event_tx, &mut last_meeting_key).await {
                tracing::warn!("Window scan failed: {}", e);
            }
        }
    }

    async fn scan_all_windows(
        event_tx: &mpsc::Sender<DetectionEvent>,
        last_meeting_key: &mut Option<String>,
    ) -> Result<bool> {
        let windows = list_all_windows()?;
        tracing::debug!("Scanning {} windows for meeting apps", windows.len());

        for window in &windows {
            if let Some(app) =
                crate::detection::patterns::detect_meeting_app(&window.class, &window.title)
            {
                let window_key = format!("{}:{}", window.class, window.title);

                if last_meeting_key.as_ref() != Some(&window_key) {
                    tracing::info!(
                        "Poll detected meeting window ({}): {} - {}",
                        app,
                        window.class,
                        window.title
                    );
                    *last_meeting_key = Some(window_key);

                    let event = DetectionEvent::WindowChanged {
                        window: window.clone(),
                    };
                    if let Err(e) = event_tx.send(event).await {
                        tracing::warn!("Failed to send polled window event: {}", e);
                    }
                }
                return Ok(true);
            }
        }

        if last_meeting_key.is_some() {
            tracing::debug!("No meeting windows found, clearing last meeting");
            *last_meeting_key = None;
        }

        Ok(false)
    }
}

pub fn is_hyprland_running() -> bool {
    ensure_hyprland_env();
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
}

pub fn get_socket_path() -> Option<String> {
    ensure_hyprland_env();
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    Some(format!("{}/hypr/{}/.socket.sock", runtime_dir, sig))
}

fn ensure_hyprland_env() {
    if let Ok(current_sig) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        let runtime_dir =
            std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
        let socket_path = format!("{}/hypr/{}/.socket.sock", runtime_dir, current_sig);
        if std::path::Path::new(&socket_path).exists() {
            return;
        }
        tracing::warn!("Current HYPRLAND_INSTANCE_SIGNATURE points to non-existent socket, scanning for active session...");
    }

    if let Some(sig) = find_active_hyprland_session() {
        tracing::info!("Found active Hyprland session: {}", sig);
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", sig);
    }
}

fn find_active_hyprland_session() -> Option<String> {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string());
    let hypr_dir = format!("{}/hypr", runtime_dir);

    let entries = match std::fs::read_dir(&hypr_dir) {
        Ok(e) => e,
        Err(_) => return None,
    };

    let mut best_session: Option<(String, std::time::SystemTime)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let socket_path = path.join(".socket.sock");
        if !socket_path.exists() {
            continue;
        }

        let log_path = path.join("hyprland.log");
        let modified = log_path
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        let dir_name = entry.file_name().to_string_lossy().to_string();
        tracing::debug!(
            "Found Hyprland session: {} (log modified: {:?})",
            dir_name,
            modified
        );

        match &best_session {
            None => best_session = Some((dir_name, modified)),
            Some((_, best_time)) if modified > *best_time => {
                best_session = Some((dir_name, modified));
            }
            _ => {}
        }
    }

    best_session.map(|(sig, _)| sig)
}

pub fn list_all_windows() -> Result<Vec<WindowInfo>> {
    let clients = Clients::get()
        .map_err(|e| MuesliError::HyprlandIpc(format!("Failed to get clients: {}", e)))?;

    Ok(clients
        .iter()
        .map(|c| WindowInfo {
            class: c.class.clone(),
            title: c.title.clone(),
            pid: Some(c.pid),
        })
        .collect())
}

pub fn meeting_window_exists(app: crate::detection::MeetingApp) -> bool {
    let windows = match list_all_windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Failed to list windows for meeting check: {}", e);
            return true;
        }
    };

    let exists = windows
        .iter()
        .any(|w| crate::detection::patterns::detect_meeting_app(&w.class, &w.title) == Some(app));

    tracing::trace!(
        "Checking if {} window exists among {} windows: {}",
        app,
        windows.len(),
        exists
    );
    exists
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_hyprland_running() {
        let _ = is_hyprland_running();
    }

    #[test]
    fn test_get_socket_path() {
        let _ = get_socket_path();
    }

    #[tokio::test]
    async fn test_get_active_window() {
        if !is_hyprland_running() {
            return;
        }

        let result = HyprlandMonitor::get_active_window();
        assert!(
            result.is_ok(),
            "Failed to get active window: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_list_all_windows() {
        if !is_hyprland_running() {
            return;
        }

        let result = list_all_windows();
        assert!(result.is_ok(), "Failed to list windows: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_monitor_creation() {
        let (tx, _rx) = mpsc::channel(10);
        let monitor = HyprlandMonitor::new(tx);
        assert_eq!(monitor.poll_interval_secs, DEFAULT_POLL_INTERVAL_SECS);

        let (tx2, _rx2) = mpsc::channel(10);
        let monitor2 = HyprlandMonitor::with_poll_interval(tx2, 60);
        assert_eq!(monitor2.poll_interval_secs, 60);
    }
}
