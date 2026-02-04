use crate::config::settings::MuesliConfig;
use crate::error::{MuesliError, Result};
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

/// Get XDG-compliant config directory
pub fn config_dir() -> Result<PathBuf> {
    ProjectDirs::from("", "", "muesli")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or_else(|| MuesliError::Config("Could not determine config directory".to_string()))
}

/// Get XDG-compliant data directory
pub fn data_dir() -> Result<PathBuf> {
    ProjectDirs::from("", "", "muesli")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .ok_or_else(|| MuesliError::Config("Could not determine data directory".to_string()))
}

/// Get config file path
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Get database path
pub fn database_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("muesli.db"))
}

/// Get notes directory
pub fn notes_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("notes"))
}

/// Get recordings directory
pub fn recordings_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("recordings"))
}

/// Get models directory
pub fn models_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("models"))
}

/// Get socket path
pub fn socket_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("muesli.sock"))
}

/// Load config from file, creating default if not exists
pub fn load_config() -> Result<MuesliConfig> {
    let path = config_path()?;

    if !path.exists() {
        let config = MuesliConfig::default();
        save_config(&config)?;
        return Ok(config);
    }

    let content = fs::read_to_string(&path)?;
    let config: MuesliConfig = toml::from_str(&content)?;
    Ok(config)
}

/// Save config to file
pub fn save_config(config: &MuesliConfig) -> Result<()> {
    let path = config_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Ensure all data directories exist
pub fn ensure_directories() -> Result<()> {
    fs::create_dir_all(config_dir()?)?;
    fs::create_dir_all(data_dir()?)?;
    fs::create_dir_all(notes_dir()?)?;
    fs::create_dir_all(recordings_dir()?)?;
    fs::create_dir_all(models_dir()?)?;
    Ok(())
}

#[allow(dead_code)]
pub fn load_config_with_env() -> Result<MuesliConfig> {
    let mut config = load_config()?;

    if let Ok(key) = std::env::var("MUESLI_DEEPGRAM_API_KEY") {
        config.transcription.deepgram_api_key = Some(key);
    }
    if let Ok(key) = std::env::var("MUESLI_OPENAI_API_KEY") {
        config.transcription.openai_api_key = Some(key.clone());
        config.llm.openai_api_key = Some(key);
    }
    if let Ok(key) = std::env::var("MUESLI_CLAUDE_API_KEY") {
        config.llm.claude_api_key = Some(key);
    }
    if let Ok(engine) = std::env::var("MUESLI_TRANSCRIPTION_ENGINE") {
        config.transcription.engine = engine;
    }
    if let Ok(model) = std::env::var("MUESLI_WHISPER_MODEL") {
        config.transcription.whisper_model = model;
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializes() {
        let config = MuesliConfig::default();
        let toml = toml::to_string_pretty(&config).unwrap();
        assert!(toml.contains("[audio]"));
        assert!(toml.contains("[transcription]"));
        assert!(toml.contains("[llm]"));
    }

    #[test]
    fn test_config_roundtrip() {
        let config = MuesliConfig::default();
        let toml = toml::to_string_pretty(&config).unwrap();
        let parsed: MuesliConfig = toml::from_str(&toml).unwrap();
        assert_eq!(
            config.audio.capture_system_audio,
            parsed.audio.capture_system_audio
        );
    }

    #[test]
    fn test_config_paths_are_valid() {
        let _ = config_dir();
        let _ = data_dir();
        let _ = config_path();
        let _ = database_path();
    }
}
