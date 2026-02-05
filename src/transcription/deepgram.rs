use crate::error::{MuesliError, Result};
use crate::transcription::{Transcript, TranscriptSegment};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use std::path::Path;

const DEEPGRAM_API_URL: &str = "https://api.deepgram.com/v1/listen";

#[derive(Debug, Deserialize)]
struct DeepgramResponse {
    results: DeepgramResults,
}

#[derive(Debug, Deserialize)]
struct DeepgramResults {
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Deserialize)]
struct DeepgramChannel {
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Deserialize)]
struct DeepgramAlternative {
    transcript: String,
    words: Option<Vec<DeepgramWord>>,
}

#[derive(Debug, Deserialize)]
struct DeepgramWord {
    word: String,
    start: f64,
    end: f64,
    confidence: f64,
}

/// Transcribe audio file via Deepgram API
pub async fn transcribe_file<P: AsRef<Path>>(api_key: &str, audio_path: P) -> Result<Transcript> {
    let audio_data = std::fs::read(audio_path.as_ref())?;
    transcribe_bytes(api_key, &audio_data).await
}

/// Transcribe audio bytes via Deepgram API
pub async fn transcribe_bytes(api_key: &str, audio_data: &[u8]) -> Result<Transcript> {
    let client = reqwest::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Token {}", api_key))
            .map_err(|e| MuesliError::Api(format!("Invalid API key format: {}", e)))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("audio/wav"));

    let response = client
        .post(format!(
            "{}?punctuate=true&utterances=true",
            DEEPGRAM_API_URL
        ))
        .headers(headers)
        .body(audio_data.to_vec())
        .send()
        .await
        .map_err(|e| MuesliError::Api(format!("Deepgram request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(MuesliError::Api(format!(
            "Deepgram error {}: {}",
            status, body
        )));
    }

    let result: DeepgramResponse = response
        .json()
        .await
        .map_err(|e| MuesliError::Api(format!("Failed to parse Deepgram response: {}", e)))?;

    let mut segments = Vec::new();

    if let Some(channel) = result.results.channels.first() {
        if let Some(alt) = channel.alternatives.first() {
            if let Some(words) = &alt.words {
                // Group words into segments (by pauses or sentence boundaries)
                let mut current_segment = Vec::new();
                let mut segment_start: Option<f64> = None;
                let mut last_end: f64 = 0.0;

                for word in words {
                    if segment_start.is_none() {
                        segment_start = Some(word.start);
                    }

                    // Start new segment on long pause (>0.5s)
                    if word.start - last_end > 0.5 && !current_segment.is_empty() {
                        let text = current_segment.join(" ");
                        segments.push(TranscriptSegment {
                            start_ms: (segment_start.unwrap() * 1000.0) as u64,
                            end_ms: (last_end * 1000.0) as u64,
                            text,
                            speaker: None,
                            confidence: None,
                        });
                        current_segment.clear();
                        segment_start = Some(word.start);
                    }

                    current_segment.push(word.word.clone());
                    last_end = word.end;
                }

                // Add final segment
                if !current_segment.is_empty() {
                    segments.push(TranscriptSegment {
                        start_ms: (segment_start.unwrap() * 1000.0) as u64,
                        end_ms: (last_end * 1000.0) as u64,
                        text: current_segment.join(" "),
                        speaker: None,
                        confidence: None,
                    });
                }
            }
        }
    }

    Ok(Transcript::new(segments))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_deepgram_url() {
        assert!(super::DEEPGRAM_API_URL.starts_with("https://"));
    }
}
