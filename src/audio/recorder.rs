use crate::audio::convert::{prepare_for_whisper, WHISPER_SAMPLE_RATE};
use crate::audio::AudioChunk;
use crate::error::{MuesliError, Result};
use hound::{WavSpec, WavWriter};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

/// WAV recorder that writes audio chunks to file
pub struct WavRecorder {
    writer: Option<WavWriter<BufWriter<File>>>,
    temp_path: PathBuf,
    final_path: PathBuf,
    samples_written: u64,
}

impl WavRecorder {
    /// Create a new recorder that will write to the given path
    pub fn new<P: AsRef<Path>>(output_path: P) -> Result<Self> {
        let final_path = output_path.as_ref().to_path_buf();
        let temp_path = final_path.with_extension("wav.tmp");

        // Ensure parent directory exists
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let spec = WavSpec {
            channels: 1,
            sample_rate: WHISPER_SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let file = File::create(&temp_path)?;
        let writer = WavWriter::new(BufWriter::new(file), spec)
            .map_err(|e| MuesliError::Audio(format!("Failed to create WAV writer: {}", e)))?;

        Ok(Self {
            writer: Some(writer),
            temp_path,
            final_path,
            samples_written: 0,
        })
    }

    /// Write an audio chunk (will be converted to Whisper format)
    pub fn write_chunk(&mut self, chunk: &AudioChunk) -> Result<()> {
        let samples = prepare_for_whisper(chunk)?;
        self.write_samples(&samples)
    }

    /// Write raw f32 samples (already in correct format)
    pub fn write_samples(&mut self, samples: &[f32]) -> Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| MuesliError::Audio("Recorder already finalized".to_string()))?;

        for &sample in samples {
            writer
                .write_sample(sample)
                .map_err(|e| MuesliError::Audio(format!("Failed to write sample: {}", e)))?;
        }

        self.samples_written += samples.len() as u64;
        Ok(())
    }

    /// Finalize recording and move to final path
    pub fn finalize(mut self) -> Result<PathBuf> {
        if let Some(writer) = self.writer.take() {
            writer
                .finalize()
                .map_err(|e| MuesliError::Audio(format!("Failed to finalize WAV: {}", e)))?;
        }

        // Atomic rename from temp to final
        fs::rename(&self.temp_path, &self.final_path)?;

        Ok(self.final_path.clone())
    }

    /// Get duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        self.samples_written as f64 / WHISPER_SAMPLE_RATE as f64
    }

    /// Get number of samples written
    pub fn samples_written(&self) -> u64 {
        self.samples_written
    }

    /// Cancel recording and clean up temp file
    pub fn cancel(mut self) -> Result<()> {
        self.writer.take(); // Drop writer
        if self.temp_path.exists() {
            fs::remove_file(&self.temp_path)?;
        }
        Ok(())
    }
}

impl Drop for WavRecorder {
    fn drop(&mut self) {
        // Clean up temp file if not finalized
        if self.writer.is_some() && self.temp_path.exists() {
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

/// Record from a broadcast receiver to a WAV file
pub async fn record_to_file<P: AsRef<Path>>(
    mut rx: broadcast::Receiver<AudioChunk>,
    output_path: P,
) -> Result<PathBuf> {
    let mut recorder = WavRecorder::new(output_path)?;

    while let Ok(chunk) = rx.recv().await {
        recorder.write_chunk(&chunk)?;
    }

    recorder.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_recorder_basic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.wav");

        let mut recorder = WavRecorder::new(&path).unwrap();

        // Write some samples
        let samples = vec![0.5f32; 16000]; // 1 second at 16kHz
        recorder.write_samples(&samples).unwrap();

        assert!((recorder.duration_seconds() - 1.0).abs() < 0.01);

        let final_path = recorder.finalize().unwrap();
        assert!(final_path.exists());
        assert!(!path.with_extension("wav.tmp").exists());
    }

    #[test]
    fn test_recorder_cancel() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.wav");

        let mut recorder = WavRecorder::new(&path).unwrap();
        recorder.write_samples(&[0.5f32; 1000]).unwrap();

        let temp_path = path.with_extension("wav.tmp");
        assert!(temp_path.exists());

        recorder.cancel().unwrap();
        assert!(!temp_path.exists());
        assert!(!path.exists());
    }

    #[test]
    fn test_write_chunk() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.wav");

        let mut recorder = WavRecorder::new(&path).unwrap();

        // Create a chunk at 48kHz stereo - should be converted
        let chunk = AudioChunk::new(vec![0.5f32; 48000 * 2], 48000, 2, 0);
        recorder.write_chunk(&chunk).unwrap();

        // Should have ~16000 samples (1 second converted from 48kHz stereo)
        assert!(recorder.samples_written() > 15000);
        assert!(recorder.samples_written() < 17000);

        recorder.finalize().unwrap();
    }

    #[test]
    fn test_wav_readable() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.wav");

        {
            let mut recorder = WavRecorder::new(&path).unwrap();
            recorder.write_samples(&[0.1, 0.2, 0.3, 0.4, 0.5]).unwrap();
            recorder.finalize().unwrap();
        }

        // Read it back with hound
        let reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();

        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.bits_per_sample, 32);

        let samples: Vec<f32> = reader.into_samples().map(|s| s.unwrap()).collect();
        assert_eq!(samples.len(), 5);
    }
}
