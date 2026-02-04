use crate::error::{MuesliError, Result};
use crate::transcription::{Transcript, TranscriptSegment};
use parakeet_rs::{ParakeetTDT, TimestampMode, Transcriber};
use std::path::Path;

const CHUNK_DURATION_SECS: usize = 300;
const OVERLAP_SECS: usize = 2;

pub struct ParakeetEngine {
    inner: Option<ParakeetTDT>,
}

impl ParakeetEngine {
    pub fn new() -> Self {
        Self { inner: None }
    }

    pub fn load_model<P: AsRef<Path>>(&mut self, model_dir: P, _use_int8: bool) -> Result<()> {
        let parakeet = ParakeetTDT::from_pretrained(model_dir.as_ref(), None).map_err(|e| {
            MuesliError::Transcription(format!("Failed to load Parakeet model: {}", e))
        })?;

        self.inner = Some(parakeet);
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.inner.is_some()
    }

    pub fn transcribe(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<Transcript> {
        let chunk_samples = CHUNK_DURATION_SECS * sample_rate as usize;
        let overlap_samples = OVERLAP_SECS * sample_rate as usize;

        if samples.len() <= chunk_samples {
            return self.transcribe_chunk(&samples, sample_rate, 0.0);
        }

        let mut all_segments = Vec::new();
        let mut chunk_start = 0usize;
        let total_samples = samples.len();
        let total_chunks = (total_samples + chunk_samples - overlap_samples - 1)
            / (chunk_samples - overlap_samples);
        let mut chunk_num = 0;

        while chunk_start < total_samples {
            chunk_num += 1;
            let chunk_end = (chunk_start + chunk_samples).min(total_samples);
            let chunk = &samples[chunk_start..chunk_end];

            let time_offset_ms = (chunk_start as f64 / sample_rate as f64 * 1000.0) as u64;

            eprintln!(
                "  Transcribing chunk {}/{} ({:.1}m-{:.1}m)...",
                chunk_num,
                total_chunks,
                chunk_start as f64 / sample_rate as f64 / 60.0,
                chunk_end as f64 / sample_rate as f64 / 60.0
            );

            let chunk_transcript =
                self.transcribe_chunk(chunk, sample_rate, time_offset_ms as f64)?;

            for mut segment in chunk_transcript.segments {
                segment.start_ms += time_offset_ms;
                segment.end_ms += time_offset_ms;

                if chunk_start > 0 {
                    let overlap_boundary_ms = time_offset_ms + (OVERLAP_SECS as u64 * 1000);
                    if segment.end_ms <= overlap_boundary_ms {
                        continue;
                    }
                }

                all_segments.push(segment);
            }

            chunk_start = chunk_end - overlap_samples;
            if chunk_start >= total_samples - overlap_samples {
                break;
            }
        }

        Ok(Transcript::new(all_segments))
    }

    fn transcribe_chunk(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        _time_offset_ms: f64,
    ) -> Result<Transcript> {
        let parakeet = self
            .inner
            .as_mut()
            .ok_or_else(|| MuesliError::Transcription("Parakeet model not loaded".to_string()))?;

        let result = parakeet
            .transcribe_samples(
                samples.to_vec(),
                sample_rate,
                1,
                Some(TimestampMode::Sentences),
            )
            .map_err(|e| {
                MuesliError::Transcription(format!("Parakeet transcription failed: {}", e))
            })?;

        let segments = result
            .tokens
            .into_iter()
            .map(|token| {
                TranscriptSegment::new(
                    (token.start * 1000.0) as u64,
                    (token.end * 1000.0) as u64,
                    token.text,
                )
            })
            .collect();

        Ok(Transcript::new(segments))
    }

    pub fn unload(&mut self) {
        self.inner = None;
    }
}

impl Default for ParakeetEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub fn transcribe_wav_file<P: AsRef<Path>>(
    engine: &mut ParakeetEngine,
    wav_path: P,
) -> Result<Transcript> {
    let reader = hound::WavReader::open(wav_path.as_ref())
        .map_err(|e| MuesliError::Audio(format!("Failed to open WAV: {}", e)))?;

    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
    };

    let mono_samples = if spec.channels > 1 {
        samples
            .chunks(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect()
    } else {
        samples
    };

    engine.transcribe(mono_samples, spec.sample_rate)
}
