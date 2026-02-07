use crate::error::{MuesliError, Result};
use crate::transcription::Transcript;
use sortformer_rs::sortformer::Sortformer;
use std::path::Path;

const DIAR_CHUNK_SECS: usize = 600; // 10 minutes per chunk
const DIAR_OVERLAP_SECS: usize = 30; // 30 second overlap

pub struct SpeakerSegment {
    pub speaker_id: usize,
    pub start_ms: u64,
    pub end_ms: u64,
}

pub struct Diarizer {
    sortformer: Sortformer,
}

impl Diarizer {
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let sortformer = Sortformer::new(model_path.as_ref()).map_err(|e| {
            MuesliError::Transcription(format!("Failed to load Sortformer model: {}", e))
        })?;

        Ok(Self { sortformer })
    }

    pub fn diarize(&mut self, samples: Vec<f32>, sample_rate: u32) -> Result<Vec<SpeakerSegment>> {
        let chunk_samples = DIAR_CHUNK_SECS * sample_rate as usize;
        let overlap_samples = DIAR_OVERLAP_SECS * sample_rate as usize;

        if samples.len() <= chunk_samples {
            return self.diarize_single_chunk(&samples, sample_rate, 0);
        }

        let mut all_segments = Vec::new();
        let mut chunk_start = 0usize;
        let total_samples = samples.len();
        let mut chunk_num = 0;
        let total_chunks = (total_samples + chunk_samples - overlap_samples - 1)
            / (chunk_samples - overlap_samples);

        while chunk_start < total_samples {
            chunk_num += 1;
            let chunk_end = (chunk_start + chunk_samples).min(total_samples);
            let chunk = &samples[chunk_start..chunk_end];

            let time_offset_ms = (chunk_start as f64 / sample_rate as f64 * 1000.0) as u64;

            eprintln!(
                "  Diarizing chunk {}/{} ({:.1}m-{:.1}m)...",
                chunk_num,
                total_chunks,
                chunk_start as f64 / sample_rate as f64 / 60.0,
                chunk_end as f64 / sample_rate as f64 / 60.0
            );

            let chunk_segments = self.diarize_single_chunk(chunk, sample_rate, time_offset_ms)?;

            for seg in chunk_segments {
                if chunk_start > 0 {
                    let overlap_boundary_ms = time_offset_ms + (DIAR_OVERLAP_SECS as u64 * 1000);
                    if seg.end_ms <= overlap_boundary_ms {
                        continue;
                    }
                }
                all_segments.push(seg);
            }

            chunk_start = chunk_end - overlap_samples;
            if chunk_start >= total_samples - overlap_samples {
                break;
            }
        }

        merge_adjacent_segments(&mut all_segments);

        Ok(all_segments)
    }

    fn diarize_single_chunk(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        time_offset_ms: u64,
    ) -> Result<Vec<SpeakerSegment>> {
        let segments = self
            .sortformer
            .diarize(samples.to_vec(), sample_rate, 1)
            .map_err(|e| MuesliError::Transcription(format!("Diarization failed: {}", e)))?;

        Ok(segments
            .iter()
            .map(|seg| SpeakerSegment {
                speaker_id: seg.speaker_id,
                start_ms: (seg.start * 1000.0) as u64 + time_offset_ms,
                end_ms: (seg.end * 1000.0) as u64 + time_offset_ms,
            })
            .collect())
    }
}

fn merge_adjacent_segments(segments: &mut Vec<SpeakerSegment>) {
    if segments.len() < 2 {
        return;
    }

    segments.sort_by_key(|s| s.start_ms);

    let mut merged = Vec::new();
    let mut current = segments.remove(0);

    for seg in segments.drain(..) {
        if seg.speaker_id == current.speaker_id && seg.start_ms <= current.end_ms + 500 {
            current.end_ms = current.end_ms.max(seg.end_ms);
        } else {
            merged.push(current);
            current = seg;
        }
    }
    merged.push(current);

    *segments = merged;
}

pub fn assign_speakers(transcript: &mut Transcript, speaker_segments: &[SpeakerSegment]) {
    for segment in &mut transcript.segments {
        let mid_point = (segment.start_ms + segment.end_ms) / 2;

        let speaker = speaker_segments
            .iter()
            .find(|s| mid_point >= s.start_ms && mid_point <= s.end_ms)
            .map(|s| format!("SPEAKER_{}", s.speaker_id + 1));

        segment.speaker = speaker;
    }
}

pub fn diarize_transcript<P: AsRef<Path>>(
    model_path: P,
    samples: &[f32],
    sample_rate: u32,
    transcript: &mut Transcript,
) -> Result<()> {
    let mut diarizer = Diarizer::new(model_path)?;
    let speaker_segments = diarizer.diarize(samples.to_vec(), sample_rate)?;
    assign_speakers(transcript, &speaker_segments);
    Ok(())
}
