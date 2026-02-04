use crate::transcription::TranscriptSegment;

const CHARS_PER_TOKEN: usize = 4;
const MAX_CHUNK_TOKENS: usize = 12000;
const MAX_CHUNK_CHARS: usize = MAX_CHUNK_TOKENS * CHARS_PER_TOKEN;

pub struct TranscriptChunk {
    pub segments: Vec<TranscriptSegment>,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub start_time_ms: u64,
    pub end_time_ms: u64,
}

impl TranscriptChunk {
    pub fn format_for_prompt(&self) -> String {
        let mut output = String::new();

        for segment in &self.segments {
            let timestamp = format_timestamp(segment.start_ms);
            match &segment.speaker {
                Some(speaker) => {
                    output.push_str(&format!("[{}] {}: {}\n", timestamp, speaker, segment.text));
                }
                None => {
                    output.push_str(&format!("[{}] {}\n", timestamp, segment.text));
                }
            }
        }

        output
    }

    pub fn char_count(&self) -> usize {
        self.segments.iter().map(|s| s.text.len()).sum()
    }
}

pub fn chunk_transcript(segments: &[TranscriptSegment]) -> Vec<TranscriptChunk> {
    let total_chars: usize = segments.iter().map(|s| s.text.len()).sum();

    if total_chars <= MAX_CHUNK_CHARS {
        return vec![TranscriptChunk {
            segments: segments.to_vec(),
            chunk_index: 0,
            total_chunks: 1,
            start_time_ms: segments.first().map(|s| s.start_ms).unwrap_or(0),
            end_time_ms: segments.last().map(|s| s.end_ms).unwrap_or(0),
        }];
    }

    let mut chunks = Vec::new();
    let mut current_segments: Vec<TranscriptSegment> = Vec::new();
    let mut current_chars = 0;

    for segment in segments {
        let segment_chars = segment.text.len();

        if current_chars + segment_chars > MAX_CHUNK_CHARS && !current_segments.is_empty() {
            chunks.push(current_segments);
            current_segments = Vec::new();
            current_chars = 0;
        }

        current_segments.push(segment.clone());
        current_chars += segment_chars;
    }

    if !current_segments.is_empty() {
        chunks.push(current_segments);
    }

    let total_chunks = chunks.len();

    chunks
        .into_iter()
        .enumerate()
        .map(|(i, segs)| TranscriptChunk {
            start_time_ms: segs.first().map(|s| s.start_ms).unwrap_or(0),
            end_time_ms: segs.last().map(|s| s.end_ms).unwrap_or(0),
            segments: segs,
            chunk_index: i,
            total_chunks,
        })
        .collect()
}

pub fn needs_chunking(segments: &[TranscriptSegment]) -> bool {
    let total_chars: usize = segments.iter().map(|s| s.text.len()).sum();
    total_chars > MAX_CHUNK_CHARS
}

fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(start_ms: u64, text: &str, speaker: Option<&str>) -> TranscriptSegment {
        TranscriptSegment {
            start_ms,
            end_ms: start_ms + 5000,
            text: text.to_string(),
            speaker: speaker.map(|s| s.to_string()),
            confidence: None,
        }
    }

    #[test]
    fn test_small_transcript_no_chunking() {
        let segments = vec![
            make_segment(0, "Hello world", Some("SPEAKER_0")),
            make_segment(5000, "How are you", Some("SPEAKER_1")),
        ];

        let chunks = chunk_transcript(&segments);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chunk_index, 0);
        assert_eq!(chunks[0].total_chunks, 1);
    }

    #[test]
    fn test_needs_chunking_false_for_small() {
        let segments = vec![make_segment(0, "Short text", None)];
        assert!(!needs_chunking(&segments));
    }

    #[test]
    fn test_large_transcript_gets_chunked() {
        let long_text = "x".repeat(20000);
        let segments = vec![
            make_segment(0, &long_text, Some("SPEAKER_0")),
            make_segment(60000, &long_text, Some("SPEAKER_1")),
            make_segment(120000, &long_text, Some("SPEAKER_0")),
        ];

        let chunks = chunk_transcript(&segments);
        assert!(chunks.len() > 1);

        for (i, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk.chunk_index, i);
            assert_eq!(chunk.total_chunks, chunks.len());
        }
    }

    #[test]
    fn test_chunk_format_with_speakers() {
        let segments = vec![
            make_segment(0, "Hello", Some("SPEAKER_0")),
            make_segment(5000, "Hi there", Some("SPEAKER_1")),
        ];

        let chunks = chunk_transcript(&segments);
        let formatted = chunks[0].format_for_prompt();

        assert!(formatted.contains("[00:00] SPEAKER_0: Hello"));
        assert!(formatted.contains("[00:05] SPEAKER_1: Hi there"));
    }

    #[test]
    fn test_timestamp_format_hours() {
        assert_eq!(format_timestamp(0), "00:00");
        assert_eq!(format_timestamp(65000), "01:05");
        assert_eq!(format_timestamp(3665000), "01:01:05");
    }
}
