use crate::error::{MuesliError, Result};
use crate::transcription::TranscriptSegment;
use parakeet_rs::Nemotron;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

const CHUNK_SIZE_SAMPLES: usize = 8960; // 560ms at 16kHz

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
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Result<Self> {
        let model_path = model_dir.as_ref().to_path_buf();

        let (audio_tx, audio_rx) = mpsc::channel::<AudioCommand>();
        let (segment_tx, segment_rx) = mpsc::channel::<TranscriptSegment>();

        let handle = thread::spawn(move || {
            if let Err(e) = run_transcription_worker(model_path, audio_rx, segment_tx) {
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
    model_path: std::path::PathBuf,
    audio_rx: mpsc::Receiver<AudioCommand>,
    segment_tx: mpsc::Sender<TranscriptSegment>,
) -> Result<()> {
    tracing::info!("Loading Nemotron streaming model from {:?}", model_path);

    let mut model = Nemotron::from_pretrained(&model_path, None)
        .map_err(|e| MuesliError::Transcription(format!("Failed to load Nemotron: {}", e)))?;

    tracing::info!("Nemotron model loaded, starting transcription worker");

    let mut audio_buffer: Vec<f32> = Vec::new();
    let mut current_time_ms: u64 = 0;
    let mut last_transcript = String::new();

    loop {
        match audio_rx.recv() {
            Ok(AudioCommand::Samples(samples)) => {
                audio_buffer.extend_from_slice(&samples);

                while audio_buffer.len() >= CHUNK_SIZE_SAMPLES {
                    let chunk: Vec<f32> = audio_buffer.drain(..CHUNK_SIZE_SAMPLES).collect();

                    match model.transcribe_chunk(&chunk) {
                        Ok(text) => {
                            if !text.is_empty() && text != last_transcript {
                                let new_text = if last_transcript.is_empty() {
                                    text.clone()
                                } else if text.starts_with(&last_transcript) {
                                    text[last_transcript.len()..].trim().to_string()
                                } else {
                                    text.clone()
                                };

                                if !new_text.is_empty() {
                                    let segment = TranscriptSegment::new(
                                        current_time_ms,
                                        current_time_ms + 560,
                                        new_text,
                                    );
                                    let _ = segment_tx.send(segment);
                                }
                                last_transcript = text;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Transcription chunk error: {}", e);
                        }
                    }

                    current_time_ms += 560;
                }
            }
            Ok(AudioCommand::Flush) => {
                for _ in 0..3 {
                    let silent_chunk = vec![0.0f32; CHUNK_SIZE_SAMPLES];
                    if let Ok(text) = model.transcribe_chunk(&silent_chunk) {
                        if !text.is_empty() && text != last_transcript {
                            let new_text = if text.starts_with(&last_transcript) {
                                text[last_transcript.len()..].trim().to_string()
                            } else {
                                text.clone()
                            };

                            if !new_text.is_empty() {
                                let segment = TranscriptSegment::new(
                                    current_time_ms,
                                    current_time_ms + 560,
                                    new_text,
                                );
                                let _ = segment_tx.send(segment);
                            }
                            last_transcript = text;
                        }
                    }
                    current_time_ms += 560;
                }
            }
            Ok(AudioCommand::Stop) => {
                tracing::info!("Streaming transcription worker stopping");
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
