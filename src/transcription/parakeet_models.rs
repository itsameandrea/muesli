use crate::error::{MuesliError, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParakeetModel {
    TdtV3,
    TdtV3Int8,
    NemotronStreaming,
}

impl ParakeetModel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace("-", "").replace("_", "").as_str() {
            "parakeetv3" | "tdtv3" | "parakeet" => Some(Self::TdtV3),
            "parakeetv3int8" | "tdtv3int8" | "parakeetint8" => Some(Self::TdtV3Int8),
            "nemotron" | "nemotronstreaming" | "streaming" => Some(Self::NemotronStreaming),
            _ => None,
        }
    }

    pub fn dir_name(&self) -> &'static str {
        match self {
            Self::TdtV3 => "parakeet-tdt-0.6b-v3",
            Self::TdtV3Int8 => "parakeet-tdt-0.6b-v3-int8",
            Self::NemotronStreaming => "nemotron-speech-streaming-en-0.6b",
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            Self::TdtV3 => {
                "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main"
            }
            Self::TdtV3Int8 => {
                "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main"
            }
            Self::NemotronStreaming => {
                "https://huggingface.co/altunenes/parakeet-rs/resolve/main/nemotron-speech-streaming-en-0.6b"
            }
        }
    }

    pub fn required_files(&self) -> Vec<(&'static str, u64)> {
        match self {
            Self::TdtV3 => vec![
                ("encoder-model.onnx", 40),
                ("encoder-model.onnx.data", 560),
                ("decoder_joint-model.onnx", 70),
                ("nemo128.onnx", 1),
                ("vocab.txt", 1),
            ],
            Self::TdtV3Int8 => vec![
                ("encoder-model.int8.onnx", 155),
                ("decoder_joint-model.int8.onnx", 60),
                ("nemo128.onnx", 1),
                ("vocab.txt", 1),
            ],
            Self::NemotronStreaming => vec![
                ("encoder.onnx", 42),
                ("encoder.onnx.data", 2436),
                ("decoder_joint.onnx", 36),
                ("tokenizer.model", 1),
            ],
        }
    }

    pub fn size_mb(&self) -> u64 {
        self.required_files().iter().map(|(_, s)| s).sum()
    }

    pub fn uses_int8(&self) -> bool {
        matches!(self, Self::TdtV3Int8)
    }

    /// Whether this model supports streaming transcription
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::NemotronStreaming)
    }

    pub fn all() -> &'static [ParakeetModel] {
        &[Self::TdtV3, Self::TdtV3Int8, Self::NemotronStreaming]
    }
}

impl std::fmt::Display for ParakeetModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TdtV3 => write!(f, "parakeet-v3"),
            Self::TdtV3Int8 => write!(f, "parakeet-v3-int8"),
            Self::NemotronStreaming => write!(f, "nemotron-streaming"),
        }
    }
}

pub struct ParakeetModelManager {
    models_dir: PathBuf,
}

impl ParakeetModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    pub fn model_dir(&self, model: ParakeetModel) -> PathBuf {
        self.models_dir.join(model.dir_name())
    }

    pub fn model_exists(&self, model: ParakeetModel) -> bool {
        let dir = self.model_dir(model);
        if !dir.exists() {
            return false;
        }

        model
            .required_files()
            .iter()
            .all(|(file, _)| dir.join(file).exists())
    }

    pub fn list_all(&self) -> Vec<(ParakeetModel, bool, u64)> {
        ParakeetModel::all()
            .iter()
            .map(|m| (*m, self.model_exists(*m), m.size_mb()))
            .collect()
    }

    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }

    pub fn download_model<F>(&self, model: ParakeetModel, progress: F) -> Result<PathBuf>
    where
        F: Fn(&str, u64, u64),
    {
        self.ensure_dir()?;
        let model_dir = self.model_dir(model);
        fs::create_dir_all(&model_dir)?;

        let base_url = model.download_url();
        let files = model.required_files();

        for (filename, expected_mb) in files {
            let file_path = model_dir.join(filename);

            if file_path.exists() {
                progress(
                    filename,
                    expected_mb * 1024 * 1024,
                    expected_mb * 1024 * 1024,
                );
                continue;
            }

            let url = format!("{}/{}", base_url, filename);
            let temp_path = file_path.with_extension("tmp");

            let response = reqwest::blocking::Client::new()
                .get(&url)
                .send()
                .map_err(|e| {
                    MuesliError::Api(format!("Download failed for {}: {}", filename, e))
                })?;

            if !response.status().is_success() {
                return Err(MuesliError::Api(format!(
                    "Failed to download {}: HTTP {}",
                    filename,
                    response.status()
                )));
            }

            let total_size = response
                .content_length()
                .unwrap_or(expected_mb * 1024 * 1024);

            let mut file = fs::File::create(&temp_path)?;
            let mut downloaded: u64 = 0;
            let mut reader = response;
            let mut buffer = [0u8; 8192];

            loop {
                let bytes_read = reader.read(&mut buffer).map_err(MuesliError::Io)?;
                if bytes_read == 0 {
                    break;
                }
                file.write_all(&buffer[..bytes_read])?;
                downloaded += bytes_read as u64;
                progress(filename, downloaded, total_size);
            }

            fs::rename(&temp_path, &file_path)?;
        }

        Ok(model_dir)
    }

    pub fn delete_model(&self, model: ParakeetModel) -> Result<()> {
        let dir = self.model_dir(model);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}
