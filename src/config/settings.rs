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
    /// Transcription engine: "whisper", "parakeet", "deepgram", "openai"
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Whisper model: tiny, base, small, medium, large, large-v3-turbo, distil-large-v3
    #[serde(default = "default_model")]
    pub whisper_model: String,
    pub whisper_model_path: Option<PathBuf>,
    /// Parakeet model: parakeet-v3, parakeet-v3-int8, nemotron-streaming
    #[serde(default = "default_parakeet_model")]
    pub parakeet_model: String,
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
            whisper_model: "base".to_string(),
            whisper_model_path: None,
            parakeet_model: "parakeet-v3".to_string(),
            use_gpu: false,
            deepgram_api_key: None,
            openai_api_key: None,
            fallback_to_local: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Summarization engine: "none", "local", "claude", "openai"
    #[serde(default = "default_llm_engine")]
    pub engine: String,
    /// Claude API key
    pub claude_api_key: Option<String>,
    /// OpenAI API key (for GPT)
    pub openai_api_key: Option<String>,
    /// Local LLM model path
    pub local_model_path: Option<PathBuf>,
    /// Claude model to use
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    /// OpenAI model to use
    #[serde(default = "default_openai_model")]
    pub openai_model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            engine: "none".to_string(),
            claude_api_key: None,
            openai_api_key: None,
            local_model_path: None,
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

fn default_parakeet_model() -> String {
    "parakeet-v3".to_string()
}

fn default_llm_engine() -> String {
    "none".to_string()
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
        assert_eq!(trans.whisper_model, "base");
        assert!(trans.fallback_to_local);
    }

    #[test]
    fn test_llm_config_defaults() {
        let llm = LlmConfig::default();
        assert_eq!(llm.engine, "none");
        assert_eq!(llm.claude_model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_detection_config_defaults() {
        let detection = DetectionConfig::default();
        assert!(detection.auto_detect);
        assert_eq!(detection.debounce_ms, 500);
        assert_eq!(detection.poll_interval_secs, 30);
    }
}
