use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration struct
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

    #[serde(default)]
    pub qmd: QmdConfig,
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
    /// Transcription engine (currently: "whisper")
    #[serde(default = "default_engine")]
    pub engine: String,
    /// Model name (whisper: tiny/base/small/medium/large)
    #[serde(default = "default_model")]
    pub model: String,
    /// Legacy: whisper model (for backwards compatibility)
    #[serde(default)]
    pub whisper_model: Option<String>,
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
            whisper_model_path: None,
            use_gpu: false,
            deepgram_api_key: None,
            openai_api_key: None,
            fallback_to_local: true,
        }
    }
}

impl TranscriptionConfig {
    #[allow(dead_code)]
    pub fn effective_model(&self) -> &str {
        if !self.model.is_empty() && self.model != "base" {
            return &self.model;
        }

        self.whisper_model.as_deref().unwrap_or(&self.model)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider: "none", "local", "anthropic", "openai", "moonshot", "openrouter"
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    /// Model name for the selected provider
    #[serde(default)]
    pub model: String,
    /// API key for the selected provider
    pub api_key: Option<String>,
    /// Path to LM Studio CLI binary (auto-detect if empty, only used for "local" provider)
    #[serde(default)]
    pub local_lms_path: String,
    /// Override context window size in tokens (0 = auto-detect from models.dev)
    #[serde(default)]
    pub context_limit: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "none".to_string(),
            model: String::new(),
            api_key: None,
            local_lms_path: String::new(),
            context_limit: 0,
        }
    }
}

impl LlmConfig {
    /// Returns the default model for the configured provider
    pub fn effective_model(&self) -> &str {
        if !self.model.is_empty() {
            return &self.model;
        }
        match self.provider.as_str() {
            "anthropic" => "claude-sonnet-4-20250514",
            "openai" => "gpt-4o",
            "moonshot" => "kimi-k2.5",
            "openrouter" => "anthropic/claude-sonnet-4",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for meeting notes
    pub notes_dir: Option<PathBuf>,
    /// Path to SQLite database
    pub database_path: Option<PathBuf>,
    /// Directory for audio recordings
    pub recordings_dir: Option<PathBuf>,
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

fn default_llm_provider() -> String {
    "none".to_string()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QmdConfig {
    /// Enable qmd search integration
    #[serde(default)]
    pub enabled: bool,
    /// Automatically index notes after meeting completion
    #[serde(default = "default_true")]
    pub auto_index: bool,
    /// qmd collection name for meeting notes
    #[serde(default = "default_qmd_collection")]
    pub collection_name: String,
}

impl Default for QmdConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_index: true,
            collection_name: "muesli-meetings".to_string(),
        }
    }
}

fn default_qmd_collection() -> String {
    "muesli-meetings".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creates() {
        let config = MuesliConfig::default();
        assert_eq!(config.audio.sample_rate, 16000);
        assert_eq!(config.transcription.engine, "whisper");
        assert_eq!(config.llm.provider, "none");
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
        assert_eq!(llm.provider, "none");
        assert!(llm.model.is_empty());
        assert!(llm.api_key.is_none());
        assert!(llm.local_lms_path.is_empty());
    }

    #[test]
    fn test_llm_effective_model() {
        let mut llm = LlmConfig::default();
        llm.provider = "anthropic".to_string();
        assert_eq!(llm.effective_model(), "claude-sonnet-4-20250514");

        llm.model = "claude-opus-4-20250514".to_string();
        assert_eq!(llm.effective_model(), "claude-opus-4-20250514");
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
