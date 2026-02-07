#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub mod deepgram;
pub mod diarization;
pub mod diarization_models;
pub mod models;
pub mod openai;
pub mod streaming;
pub mod whisper;

/// A segment of transcribed text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker: Option<String>,
    pub confidence: Option<f32>,
}

impl TranscriptSegment {
    pub fn new(start_ms: u64, end_ms: u64, text: String) -> Self {
        Self {
            start_ms,
            end_ms,
            text,
            speaker: None,
            confidence: None,
        }
    }

    pub fn format_timestamp(&self) -> String {
        let start_sec = self.start_ms / 1000;
        let start_min = start_sec / 60;
        let start_sec = start_sec % 60;
        format!("{:02}:{:02}", start_min, start_sec)
    }
}

/// Full transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub segments: Vec<TranscriptSegment>,
    pub language: Option<String>,
    pub duration_ms: u64,
}

impl Transcript {
    pub fn new(segments: Vec<TranscriptSegment>) -> Self {
        let duration_ms = segments.last().map(|s| s.end_ms).unwrap_or(0);
        Self {
            segments,
            language: None,
            duration_ms,
        }
    }

    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Transcription engine selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionEngine {
    /// Local Whisper (whisper.cpp)
    Local,
    /// Deepgram API
    Deepgram,
    /// OpenAI Whisper API
    OpenAI,
}
