use crate::error::{MuesliError, Result};
use crate::transcription::whisper::WhisperEngine;
use crate::transcription::TranscriptSegment;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

const SAMPLE_RATE: usize = 16000;
const WHISPER_CHUNK_SAMPLES: usize = 15 * SAMPLE_RATE;

#[derive(Debug, Clone)]
pub struct WhisperStreamingConfig {
    pub model_path: std::path::PathBuf,
    pub use_gpu: bool,
}

pub struct StreamingTranscriber {
    audio_tx: mpsc::Sender<AudioCommand>,
    segment_rx: mpsc::Receiver<TranscriptSegment>,
    handle: Option<thread::JoinHandle<()>>,
}

enum AudioCommand {
    Samples(Vec<f32>),
    Flush,
    Stop,
}

impl StreamingTranscriber {
    pub fn new(config: WhisperStreamingConfig) -> Result<Self> {
        let (audio_tx, audio_rx) = mpsc::channel::<AudioCommand>();
        let (segment_tx, segment_rx) = mpsc::channel::<TranscriptSegment>();

        let handle = thread::spawn(move || {
            if let Err(e) = run_transcription_worker(config, audio_rx, segment_tx) {
                tracing::error!("Streaming transcription worker error: {}", e);
            }
        });

        Ok(Self {
            audio_tx,
            segment_rx,
            handle: Some(handle),
        })
    }

    pub fn feed_samples(&self, samples: &[f32]) -> Result<()> {
        self.audio_tx
            .send(AudioCommand::Samples(samples.to_vec()))
            .map_err(|_| MuesliError::Transcription("Worker channel closed".to_string()))?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        self.audio_tx
            .send(AudioCommand::Flush)
            .map_err(|_| MuesliError::Transcription("Worker channel closed".to_string()))?;
        Ok(())
    }

    pub fn stop(mut self) -> Result<Vec<TranscriptSegment>> {
        let _ = self.audio_tx.send(AudioCommand::Stop);

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        let mut segments = Vec::new();
        while let Ok(segment) = self.segment_rx.try_recv() {
            segments.push(segment);
        }
        Ok(segments)
    }

    pub fn try_recv_segment(&self) -> Option<TranscriptSegment> {
        self.segment_rx.try_recv().ok()
    }

    pub fn drain_segments(&self) -> Vec<TranscriptSegment> {
        let mut segments = Vec::new();
        while let Ok(segment) = self.segment_rx.try_recv() {
            segments.push(segment);
        }
        segments
    }
}

fn run_transcription_worker(
    config: WhisperStreamingConfig,
    audio_rx: mpsc::Receiver<AudioCommand>,
    segment_tx: mpsc::Sender<TranscriptSegment>,
) -> Result<()> {
    run_whisper_worker(
        config.model_path.as_ref(),
        config.use_gpu,
        audio_rx,
        segment_tx,
    )
}

fn run_whisper_worker(
    model_path: &Path,
    use_gpu: bool,
    audio_rx: mpsc::Receiver<AudioCommand>,
    segment_tx: mpsc::Sender<TranscriptSegment>,
) -> Result<()> {
    tracing::info!("Loading Whisper streaming model from {:?}", model_path);
    let engine = WhisperEngine::new(model_path, use_gpu)?;
    tracing::info!("Whisper model loaded, starting incremental transcription worker");

    let mut audio_buffer: Vec<f32> = Vec::new();
    let mut processed_ms: u64 = 0;

    loop {
        match audio_rx.recv() {
            Ok(AudioCommand::Samples(samples)) => {
                audio_buffer.extend_from_slice(&samples);

                while audio_buffer.len() >= WHISPER_CHUNK_SAMPLES {
                    let chunk: Vec<f32> = audio_buffer.drain(..WHISPER_CHUNK_SAMPLES).collect();
                    if let Err(e) =
                        transcribe_whisper_chunk(&engine, &chunk, processed_ms, &segment_tx)
                    {
                        tracing::warn!("Whisper chunk transcription error: {}", e);
                    }

                    processed_ms += (WHISPER_CHUNK_SAMPLES as u64 * 1000) / SAMPLE_RATE as u64;
                }
            }
            Ok(AudioCommand::Flush) => {
                if !audio_buffer.is_empty() {
                    let chunk = std::mem::take(&mut audio_buffer);
                    if let Err(e) =
                        transcribe_whisper_chunk(&engine, &chunk, processed_ms, &segment_tx)
                    {
                        tracing::warn!("Whisper final chunk transcription error: {}", e);
                    }
                    processed_ms += (chunk.len() as u64 * 1000) / SAMPLE_RATE as u64;
                }
            }
            Ok(AudioCommand::Stop) => {
                tracing::info!("Whisper incremental transcription worker stopping");
                break;
            }
            Err(_) => {
                tracing::info!("Audio channel closed, stopping worker");
                break;
            }
        }
    }

    Ok(())
}

fn transcribe_whisper_chunk(
    engine: &WhisperEngine,
    samples: &[f32],
    offset_ms: u64,
    segment_tx: &mpsc::Sender<TranscriptSegment>,
) -> Result<()> {
    let transcript = engine.transcribe(samples)?;

    for segment in transcript.segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }

        let adjusted = TranscriptSegment::new(
            segment.start_ms + offset_ms,
            segment.end_ms + offset_ms,
            text.to_string(),
        );
        let _ = segment_tx.send(adjusted);
    }

    Ok(())
}
