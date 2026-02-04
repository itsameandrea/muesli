use crate::error::{MuesliError, Result};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiarizationModel {
    SortformerV2,
}

impl DiarizationModel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace("-", "").replace("_", "").as_str() {
            "sortformer" | "sortformerv2" | "sortformer2" => Some(Self::SortformerV2),
            _ => None,
        }
    }

    pub fn filename(&self) -> &'static str {
        match self {
            Self::SortformerV2 => "diar_streaming_sortformer_4spk-v2.onnx",
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            Self::SortformerV2 => {
                "https://huggingface.co/altunenes/parakeet-rs/resolve/main/diar_streaming_sortformer_4spk-v2.onnx"
            }
        }
    }

    pub fn size_mb(&self) -> u64 {
        match self {
            Self::SortformerV2 => 50,
        }
    }

    pub fn all() -> &'static [DiarizationModel] {
        &[Self::SortformerV2]
    }
}

impl std::fmt::Display for DiarizationModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SortformerV2 => write!(f, "sortformer-v2"),
        }
    }
}

pub struct DiarizationModelManager {
    models_dir: PathBuf,
}

impl DiarizationModelManager {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }

    pub fn model_path(&self, model: DiarizationModel) -> PathBuf {
        self.models_dir.join(model.filename())
    }

    pub fn model_exists(&self, model: DiarizationModel) -> bool {
        self.model_path(model).exists()
    }

    pub fn list_all(&self) -> Vec<(DiarizationModel, bool, u64)> {
        DiarizationModel::all()
            .iter()
            .map(|m| (*m, self.model_exists(*m), m.size_mb()))
            .collect()
    }

    pub fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }

    pub fn download_model<F>(&self, model: DiarizationModel, progress: F) -> Result<PathBuf>
    where
        F: Fn(u64, u64),
    {
        self.ensure_dir()?;
        let file_path = self.model_path(model);

        if file_path.exists() {
            let size = model.size_mb() * 1024 * 1024;
            progress(size, size);
            return Ok(file_path);
        }

        let url = model.download_url();
        let temp_path = file_path.with_extension("tmp");

        let response = reqwest::blocking::Client::new()
            .get(url)
            .send()
            .map_err(|e| MuesliError::Api(format!("Download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(MuesliError::Api(format!(
                "Failed to download: HTTP {}",
                response.status()
            )));
        }

        let total_size = response
            .content_length()
            .unwrap_or(model.size_mb() * 1024 * 1024);

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
            progress(downloaded, total_size);
        }

        fs::rename(&temp_path, &file_path)?;
        Ok(file_path)
    }

    pub fn delete_model(&self, model: DiarizationModel) -> Result<()> {
        let path = self.model_path(model);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
