use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MuesliError {
    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Audio device not found: {0}")]
    AudioDeviceNotFound(String),

    #[error("Audio stream error: {0}")]
    AudioStream(String),

    #[error("Transcription error: {0}")]
    Transcription(String),

    #[error("Whisper model not found: {0}")]
    WhisperModelNotFound(PathBuf),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Config file not found: {0}")]
    ConfigNotFound(PathBuf),

    #[error("Invalid config: {0}")]
    InvalidConfig(String),

    #[error("Hyprland IPC error: {0}")]
    HyprlandIpc(String),

    #[error("Notification error: {0}")]
    Notification(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Daemon not running")]
    DaemonNotRunning,

    #[error("Meeting not found: {0}")]
    MeetingNotFound(String),

    #[error("Already recording")]
    AlreadyRecording,

    #[error("Not recording")]
    NotRecording,
}

pub type Result<T> = std::result::Result<T, MuesliError>;
