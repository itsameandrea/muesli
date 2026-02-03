use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod database;
pub mod models;
pub mod migrations;

/// Unique meeting identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeetingId(pub String);

impl MeetingId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(s: String) -> Self {
        Self(s)
    }
}

impl Default for MeetingId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MeetingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Meeting status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeetingStatus {
    Recording,
    Processing,
    Complete,
    Failed,
}

impl std::fmt::Display for MeetingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetingStatus::Recording => write!(f, "recording"),
            MeetingStatus::Processing => write!(f, "processing"),
            MeetingStatus::Complete => write!(f, "complete"),
            MeetingStatus::Failed => write!(f, "failed"),
        }
    }
}

/// A recorded meeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: MeetingId,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<u64>,
    pub audio_path: Option<std::path::PathBuf>,
    pub transcript_path: Option<std::path::PathBuf>,
    pub notes_path: Option<std::path::PathBuf>,
    pub status: MeetingStatus,
    pub detected_app: Option<String>,
}

impl Meeting {
    pub fn new(title: String) -> Self {
        Self {
            id: MeetingId::new(),
            title,
            started_at: Utc::now(),
            ended_at: None,
            duration_seconds: None,
            audio_path: None,
            transcript_path: None,
            notes_path: None,
            status: MeetingStatus::Recording,
            detected_app: None,
        }
    }
}
