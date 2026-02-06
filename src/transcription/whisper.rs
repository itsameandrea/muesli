use crate::error::{MuesliError, Result};
use crate::transcription::models::{ModelManager, WhisperModel};
use crate::transcription::{Transcript, TranscriptSegment};
use std::path::Path;
use std::sync::Arc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper transcription engine
pub struct WhisperEngine {
    ctx: Arc<WhisperContext>,
}

impl WhisperEngine {
    pub fn new<P: AsRef<Path>>(model_path: P, use_gpu: bool) -> Result<Self> {
        let mut params = WhisperContextParameters::default();
        params.use_gpu = use_gpu;

        let ctx = WhisperContext::new_with_params(
            model_path.as_ref().to_str().unwrap_or_default(),
            params,
        )
        .map_err(|e| MuesliError::Transcription(format!("Failed to load model: {}", e)))?;

        Ok(Self { ctx: Arc::new(ctx) })
    }

    pub fn from_model(manager: &ModelManager, model: WhisperModel, use_gpu: bool) -> Result<Self> {
        let path = manager.model_path(model);
        if !path.exists() {
            return Err(MuesliError::WhisperModelNotFound(path));
        }
        Self::new(path, use_gpu)
    }

    /// Transcribe audio samples (must be 16kHz mono f32)
    pub fn transcribe(&self, samples: &[f32]) -> Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| MuesliError::Transcription(format!("Failed to create state: {}", e)))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, samples)
            .map_err(|e| MuesliError::Transcription(format!("Transcription failed: {}", e)))?;

        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                let text = segment.to_str_lossy().map_err(|e| {
                    MuesliError::Transcription(format!("Failed to get text: {}", e))
                })?;
                let start = segment.start_timestamp();
                let end = segment.end_timestamp();

                // Convert centiseconds to milliseconds
                segments.push(TranscriptSegment::new(
                    (start * 10) as u64,
                    (end * 10) as u64,
                    text.trim().to_string(),
                ));
            }
        }

        Ok(Transcript::new(segments))
    }

    /// Transcribe with explicit language
    pub fn transcribe_with_language(&self, samples: &[f32], language: &str) -> Result<Transcript> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| MuesliError::Transcription(format!("Failed to create state: {}", e)))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(language));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, samples)
            .map_err(|e| MuesliError::Transcription(format!("Transcription failed: {}", e)))?;

        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                let text = segment.to_str_lossy().map_err(|e| {
                    MuesliError::Transcription(format!("Failed to get text: {}", e))
                })?;
                let start = segment.start_timestamp();
                let end = segment.end_timestamp();

                segments.push(TranscriptSegment::new(
                    (start * 10) as u64,
                    (end * 10) as u64,
                    text.trim().to_string(),
                ));
            }
        }

        let mut transcript = Transcript::new(segments);
        transcript.language = Some(language.to_string());
        Ok(transcript)
    }
}

/// Transcribe a WAV file
pub fn transcribe_wav_file<P: AsRef<Path>>(
    engine: &WhisperEngine,
    wav_path: P,
) -> Result<Transcript> {
    let reader = hound::WavReader::open(wav_path.as_ref())
        .map_err(|e| MuesliError::Audio(format!("Failed to open WAV: {}", e)))?;

    let spec = reader.spec();
    if spec.sample_rate != 16000 || spec.channels != 1 {
        return Err(MuesliError::Audio(format!(
            "WAV must be 16kHz mono, got {}Hz {} channels",
            spec.sample_rate, spec.channels
        )));
    }

    let samples: Vec<f32> = reader
        .into_samples::<f32>()
        .map(|s| s.unwrap_or(0.0))
        .collect();

    engine.transcribe(&samples)
}

/// Chunk size for batch processing (30 seconds at 16kHz)
const CHUNK_SAMPLES: usize = 30 * 16000;
/// Overlap between chunks (2 seconds)
const OVERLAP_SAMPLES: usize = 2 * 16000;

/// Transcribe long audio in chunks
pub fn transcribe_chunked(engine: &WhisperEngine, samples: &[f32]) -> Result<Transcript> {
    if samples.len() <= CHUNK_SAMPLES {
        return engine.transcribe(samples);
    }

    let mut all_segments = Vec::new();
    let mut offset_ms: u64 = 0;
    let mut pos = 0;

    while pos < samples.len() {
        let end = (pos + CHUNK_SAMPLES).min(samples.len());
        let chunk = &samples[pos..end];

        let transcript = engine.transcribe(chunk)?;

        for mut segment in transcript.segments {
            segment.start_ms += offset_ms;
            segment.end_ms += offset_ms;
            all_segments.push(segment);
        }

        // Move position, accounting for overlap
        if end >= samples.len() {
            break;
        }
        pos = end - OVERLAP_SAMPLES;
        offset_ms = (pos as u64 * 1000) / 16000;
    }

    Ok(Transcript::new(all_segments))
}
