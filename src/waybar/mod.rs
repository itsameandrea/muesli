use crate::config::settings::WaybarConfig;
use crate::error::{MuesliError, Result};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct WaybarStatus {
    pub text: String,
    pub tooltip: String,
    pub class: String,
    pub alt: String,
    pub percentage: u8,
}

impl WaybarStatus {
    pub fn recording(title: &str, duration_secs: u64) -> Self {
        let mins = duration_secs / 60;
        let secs = duration_secs % 60;
        let duration_text = format!("{:02}:{:02}", mins, secs);

        Self {
            text: String::new(),
            tooltip: format!("Recording: {} ({})", title, duration_text),
            class: "recording".to_string(),
            alt: "recording".to_string(),
            percentage: 100,
        }
    }

    pub fn idle() -> Self {
        Self {
            text: String::new(),
            tooltip: "Click to start recording".to_string(),
            class: "idle".to_string(),
            alt: "idle".to_string(),
            percentage: 0,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"text":"","class":"error"}"#.to_string())
    }
}

pub fn waybar_status_path(config: &WaybarConfig) -> Result<PathBuf> {
    if let Some(ref custom_path) = config.status_file {
        return Ok(custom_path.clone());
    }

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));

    let muesli_dir = runtime_dir.join("muesli");
    Ok(muesli_dir.join("waybar.json"))
}

pub fn update_waybar_status(config: &WaybarConfig, status: &WaybarStatus) {
    if !config.enabled {
        return;
    }

    if let Err(e) = write_status_file(config, status) {
        tracing::warn!("Failed to update waybar status file: {}", e);
    }

    signal_waybar();
}

fn write_status_file(config: &WaybarConfig, status: &WaybarStatus) -> Result<()> {
    let path = waybar_status_path(config)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = status.to_json();
    fs::write(&path, &json).map_err(|e| {
        MuesliError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to write waybar status: {}", e),
        ))
    })?;

    tracing::debug!("Wrote waybar status to {:?}: {}", path, json);
    Ok(())
}

fn signal_waybar() {
    match Command::new("pkill")
        .args(["-SIGRTMIN+8", "waybar"])
        .spawn()
    {
        Ok(_) => tracing::debug!("Sent SIGRTMIN+8 to waybar"),
        Err(e) => tracing::debug!("Could not signal waybar (may not be running): {}", e),
    }
}

#[allow(dead_code)]
pub fn clear_waybar_status(config: &WaybarConfig) {
    update_waybar_status(config, &WaybarStatus::idle());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waybar_status_recording() {
        let status = WaybarStatus::recording("Test Meeting", 125);
        assert_eq!(status.text, "");
        assert_eq!(status.alt, "recording");
        assert!(status.tooltip.contains("02:05"));
    }

    #[test]
    fn test_waybar_status_idle() {
        let status = WaybarStatus::idle();
        assert_eq!(status.text, "");
        assert_eq!(status.alt, "idle");
    }

    #[test]
    fn test_waybar_status_json() {
        let status = WaybarStatus::idle();
        let json = status.to_json();
        assert!(json.contains("\"alt\":\"idle\""));
    }
}
