use crate::error::{MuesliError, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
    LargeV3Turbo,
    DistilLargeV3,
}

impl WhisperModel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace("-", "").replace("_", "").as_str() {
            "tiny" => Some(Self::Tiny),
            "base" => Some(Self::Base),
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            "largev3turbo" => Some(Self::LargeV3Turbo),
            "distillargev3" => Some(Self::DistilLargeV3),
            _ => None,
        }
    }

    pub fn filename(&self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::Large => "ggml-large.bin",
            Self::LargeV3Turbo => "ggml-large-v3-turbo.bin",
            Self::DistilLargeV3 => "ggml-distil-large-v3.bin",
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            Self::Tiny => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
            Self::Base => "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            Self::Small => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin"
            }
            Self::Medium => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin"
            }
            Self::Large => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large.bin"
            }
            Self::LargeV3Turbo => {
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
            }
            Self::DistilLargeV3 => {
                "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/ggml-distil-large-v3.bin"
            }
        }
    }

    pub fn size_mb(&self) -> u64 {
        match self {
            Self::Tiny => 75,
            Self::Base => 142,
            Self::Small => 466,
            Self::Medium => 1500,
            Self::Large => 2900,
            Self::LargeV3Turbo => 1620,
            Self::DistilLargeV3 => 1520,
        }
    }

    pub fn all() -> &'static [WhisperModel] {
        &[
            Self::Tiny,
            Self::Base,
            Self::Small,
            Self::Medium,
            Self::Large,
            Self::LargeV3Turbo,
            Self::DistilLargeV3,
        ]
    }
}

impl std::fmt::Display for WhisperModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tiny => write!(f, "tiny"),
            Self::Base => write!(f, "base"),
            Self::Small => write!(f, "small"),
            Self::Medium => write!(f, "medium"),
            Self::Large => write!(f, "large"),
            Self::LargeV3Turbo => write!(f, "large-v3-turbo"),
            Self::DistilLargeV3 => write!(f, "distil-large-v3"),
        }
    }
}

pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    pub fn model_path(&self, model: WhisperModel) -> PathBuf {
        self.models_dir.join(model.filename())
    }

    pub fn model_exists(&self, model: WhisperModel) -> bool {
        self.model_path(model).exists()
    }

    pub fn list_available(&self) -> Vec<WhisperModel> {
        WhisperModel::all()
            .iter()
            .filter(|m| self.model_exists(**m))
            .copied()
            .collect()
    }

    pub fn list_all(&self) -> Vec<(WhisperModel, bool, u64)> {
        WhisperModel::all()
            .iter()
            .map(|m| (*m, self.model_exists(*m), m.size_mb()))
            .collect()
    }

    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }

    pub fn download_model<F>(&self, model: WhisperModel, progress: F) -> Result<PathBuf>
    where
        F: Fn(u64, u64),
    {
        self.ensure_dir()?;

        let path = self.model_path(model);

        if path.exists() {
            let size = fs::metadata(&path)?.len();
            progress(size, size);
            return Ok(path);
        }

        let url = model.download_url();
        let temp_path = path.with_extension("bin.tmp");

        let response = reqwest::blocking::Client::new()
            .get(url)
            .send()
            .map_err(|e| MuesliError::Api(format!("Download failed: {}", e)))?;

        let total_size = response
            .content_length()
            .unwrap_or(model.size_mb() * 1024 * 1024);

        let mut file = fs::File::create(&temp_path)?;
        let mut downloaded: u64 = 0;

        let mut reader = response;
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = reader.read(&mut buffer).map_err(|e| MuesliError::Io(e))?;

            if bytes_read == 0 {
                break;
            }

            file.write_all(&buffer[..bytes_read])?;
            downloaded += bytes_read as u64;
            progress(downloaded, total_size);
        }

        fs::rename(&temp_path, &path)?;

        Ok(path)
    }

    pub fn delete_model(&self, model: WhisperModel) -> Result<()> {
        let path = self.model_path(model);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

pub fn default_models_dir() -> Result<PathBuf> {
    use directories::ProjectDirs;
    ProjectDirs::from("", "", "muesli")
        .map(|dirs| dirs.data_dir().join("models"))
        .ok_or_else(|| MuesliError::Config("Could not determine models directory".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_model_from_str() {
        assert_eq!(WhisperModel::from_str("base"), Some(WhisperModel::Base));
        assert_eq!(WhisperModel::from_str("BASE"), Some(WhisperModel::Base));
        assert_eq!(WhisperModel::from_str("invalid"), None);
    }

    #[test]
    fn test_model_path() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        let path = manager.model_path(WhisperModel::Base);
        assert!(path.ends_with("ggml-base.bin"));
    }

    #[test]
    fn test_list_all() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        let models = manager.list_all();
        assert_eq!(models.len(), 7);
    }

    #[test]
    fn test_model_display() {
        assert_eq!(WhisperModel::Tiny.to_string(), "tiny");
        assert_eq!(WhisperModel::Base.to_string(), "base");
        assert_eq!(WhisperModel::Large.to_string(), "large");
    }

    #[test]
    fn test_model_filenames() {
        assert_eq!(WhisperModel::Tiny.filename(), "ggml-tiny.bin");
        assert_eq!(WhisperModel::Base.filename(), "ggml-base.bin");
        assert_eq!(WhisperModel::Large.filename(), "ggml-large.bin");
    }

    #[test]
    fn test_model_sizes() {
        assert_eq!(WhisperModel::Tiny.size_mb(), 75);
        assert_eq!(WhisperModel::Base.size_mb(), 142);
        assert_eq!(WhisperModel::Large.size_mb(), 2900);
    }

    #[test]
    fn test_ensure_dir() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        assert!(manager.ensure_dir().is_ok());
        assert!(manager.models_dir.exists());
    }

    #[test]
    fn test_model_exists() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        manager.ensure_dir().unwrap();

        let path = manager.model_path(WhisperModel::Base);
        fs::write(&path, b"dummy").unwrap();

        assert!(manager.model_exists(WhisperModel::Base));
        assert!(!manager.model_exists(WhisperModel::Tiny));
    }

    #[test]
    fn test_list_available() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        manager.ensure_dir().unwrap();

        fs::write(manager.model_path(WhisperModel::Base), b"dummy").unwrap();
        fs::write(manager.model_path(WhisperModel::Tiny), b"dummy").unwrap();

        let available = manager.list_available();
        assert_eq!(available.len(), 2);
        assert!(available.contains(&WhisperModel::Base));
        assert!(available.contains(&WhisperModel::Tiny));
    }

    #[test]
    fn test_delete_model() {
        let dir = tempdir().unwrap();
        let manager = ModelManager::new(dir.path().to_path_buf());
        manager.ensure_dir().unwrap();

        let path = manager.model_path(WhisperModel::Base);
        fs::write(&path, b"dummy").unwrap();
        assert!(path.exists());

        assert!(manager.delete_model(WhisperModel::Base).is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn test_all_models() {
        let all = WhisperModel::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&WhisperModel::Tiny));
        assert!(all.contains(&WhisperModel::Base));
        assert!(all.contains(&WhisperModel::Small));
        assert!(all.contains(&WhisperModel::Medium));
        assert!(all.contains(&WhisperModel::Large));
        assert!(all.contains(&WhisperModel::LargeV3Turbo));
        assert!(all.contains(&WhisperModel::DistilLargeV3));
    }
}
