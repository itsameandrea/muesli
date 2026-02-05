use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuesliConfig {
    #[serde(default)]
    pub audio: AudioConfig,

    #[serde(default)]
    pub transcription: TranscriptionConfig,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub daemon: DaemonConfig,

    #[serde(default)]
    pub detection: DetectionConfig,

    #[serde(default)]
    pub audio_cues: AudioCuesConfig,

    #[serde(default)]
    pub waybar: WaybarConfig,
}

impl Default for MuesliConfig {
    fn default() -> Self {
        Self {
            audio: AudioConfig::default(),
            transcription: TranscriptionConfig::default(),
            llm: LlmConfig::default(),
            storage: StorageConfig::default(),
            daemon: DaemonConfig::default(),
            detection: DetectionConfig::default(),
            audio_cues: AudioCuesConfig::default(),
            waybar: WaybarConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Specific microphone device name (None = default)
    pub device_mic: Option<String>,
    /// Specific loopback device name (None = auto-detect)
    pub device_loopback: Option<String>,
    /// Enable system audio capture (loopback)
    #[serde(default = "default_true")]
    pub capture_system_audio: bool,
    /// Sample rate for recording (default: 16000 for Whisper)
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            device_mic: None,
            device_loopback: None,
            capture_system_audio: true,
            sample_rate: 16000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    /// Transcription engine: "whisper" or "parakeet"
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Model name (whisper: tiny/base/small/medium/large, parakeet: parakeet-v3/parakeet-v3-int8)
    #[serde(default = "default_model")]
    pub model: String,
    /// Legacy: whisper model (for backwards compatibility)
    #[serde(default)]
    pub whisper_model: Option<String>,
    /// Legacy: parakeet model (for backwards compatibility)
    #[serde(default)]
    pub parakeet_model: Option<String>,
    pub whisper_model_path: Option<PathBuf>,
    #[serde(default)]
    pub use_gpu: bool,
    pub deepgram_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    #[serde(default = "default_true")]
    pub fallback_to_local: bool,
}

impl Default for TranscriptionConfig {
    fn default() -> Self {
        Self {
            engine: "whisper".to_string(),
            model: "base".to_string(),
            whisper_model: None,
            parakeet_model: None,
            whisper_model_path: None,
            use_gpu: false,
            deepgram_api_key: None,
            openai_api_key: None,
            fallback_to_local: true,
        }
    }
}

impl TranscriptionConfig {
    pub fn effective_model(&self) -> &str {
        if !self.model.is_empty() && self.model != "base" {
            return &self.model;
        }

        match self.engine.as_str() {
            "whisper" => self.whisper_model.as_deref().unwrap_or(&self.model),
            "parakeet" => self.parakeet_model.as_deref().unwrap_or("parakeet-v3-int8"),
            _ => &self.model,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_llm_engine")]
    pub engine: String,
    pub claude_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    #[serde(default)]
    pub local_lms_path: String,
    #[serde(default = "default_local_model")]
    pub local_model: String,
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    #[serde(default = "default_openai_model")]
    pub openai_model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            engine: "none".to_string(),
            claude_api_key: None,
            openai_api_key: None,
            local_lms_path: String::new(),
            local_model: String::new(),
            claude_model: "claude-sonnet-4-20250514".to_string(),
            openai_model: "gpt-4o".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for meeting notes
    pub notes_dir: Option<PathBuf>,
    /// Path to SQLite database
    pub database_path: Option<PathBuf>,
    /// Directory for audio recordings
    pub recordings_dir: Option<PathBuf>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            notes_dir: None,
            database_path: None,
            recordings_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Unix socket path
    pub socket_path: Option<PathBuf>,
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: None,
            log_level: "info".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    /// Enable automatic meeting detection
    #[serde(default = "default_true")]
    pub auto_detect: bool,
    /// Show interactive prompt when meeting detected (with Record/Skip buttons)
    #[serde(default = "default_true")]
    pub auto_prompt: bool,
    /// Timeout for the recording prompt in seconds (0 = no timeout)
    #[serde(default = "default_prompt_timeout")]
    pub prompt_timeout_secs: u64,
    /// Debounce time for window switches (ms)
    #[serde(default = "default_debounce")]
    pub debounce_ms: u64,
    /// Fallback polling interval (seconds)
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            auto_prompt: true,
            prompt_timeout_secs: 30,
            debounce_ms: 500,
            poll_interval_secs: 30,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_engine() -> String {
    "whisper".to_string()
}

fn default_model() -> String {
    "base".to_string()
}

fn default_llm_engine() -> String {
    "none".to_string()
}

fn default_local_model() -> String {
    String::new()
}

fn default_claude_model() -> String {
    "claude-sonnet-4-20250514".to_string()
}

fn default_openai_model() -> String {
    "gpt-4o".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_debounce() -> u64 {
    500
}

fn default_poll_interval() -> u64 {
    30
}

fn default_prompt_timeout() -> u64 {
    30
}

fn default_volume() -> f32 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioCuesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_volume")]
    pub volume: f32,
    pub start_sound: Option<std::path::PathBuf>,
    pub stop_sound: Option<std::path::PathBuf>,
}

impl Default for AudioCuesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.5,
            start_sound: None,
            stop_sound: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WaybarConfig {
    #[serde(default)]
    pub enabled: bool,
    pub status_file: Option<std::path::PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creates() {
        let config = MuesliConfig::default();
        assert_eq!(config.audio.sample_rate, 16000);
        assert_eq!(config.transcription.engine, "whisper");
        assert_eq!(config.llm.engine, "none");
    }

    #[test]
    fn test_audio_config_defaults() {
        let audio = AudioConfig::default();
        assert!(audio.capture_system_audio);
        assert_eq!(audio.sample_rate, 16000);
        assert!(audio.device_mic.is_none());
    }

    #[test]
    fn test_transcription_config_defaults() {
        let trans = TranscriptionConfig::default();
        assert_eq!(trans.engine, "whisper");
        assert_eq!(trans.model, "base");
        assert!(trans.fallback_to_local);
    }

    #[test]
    fn test_llm_config_defaults() {
        let llm = LlmConfig::default();
        assert_eq!(llm.engine, "none");
        assert_eq!(llm.claude_model, "claude-sonnet-4-20250514");
        assert!(llm.local_lms_path.is_empty());
    }

    #[test]
    fn test_detection_config_defaults() {
        let detection = DetectionConfig::default();
        assert!(detection.auto_detect);
        assert!(detection.auto_prompt);
        assert_eq!(detection.prompt_timeout_secs, 30);
        assert_eq!(detection.debounce_ms, 500);
        assert_eq!(detection.poll_interval_secs, 30);
    }
}
