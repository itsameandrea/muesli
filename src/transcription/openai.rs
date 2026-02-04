use crate::error::{MuesliError, Result};
use crate::transcription::{TranscriptSegment, Transcript};
use reqwest::multipart;
use serde::Deserialize;
use std::path::Path;

const OPENAI_API_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    text: String,
    #[serde(default)]
    segments: Option<Vec<OpenAISegment>>,
}

#[derive(Debug, Deserialize)]
struct OpenAISegment {
    start: f64,
    end: f64,
    text: String,
}

/// Transcribe audio file via OpenAI Whisper API
pub async fn transcribe_file<P: AsRef<Path>>(api_key: &str, audio_path: P) -> Result<Transcript> {
    let audio_data = std::fs::read(audio_path.as_ref())?;
    let filename = audio_path.as_ref()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    
    transcribe_bytes(api_key, &audio_data, &filename).await
}

/// Transcribe audio bytes via OpenAI Whisper API
pub async fn transcribe_bytes(api_key: &str, audio_data: &[u8], filename: &str) -> Result<Transcript> {
    let client = reqwest::Client::new();
    
    let file_part = multipart::Part::bytes(audio_data.to_vec())
        .file_name(filename.to_string())
        .mime_str("audio/wav")
        .map_err(|e| MuesliError::Api(format!("Failed to create multipart: {}", e)))?;
    
    let form = multipart::Form::new()
        .text("model", "whisper-1")
        .text("response_format", "verbose_json")
        .text("timestamp_granularities[]", "segment")
        .part("file", file_part);
    
    let response = client
        .post(OPENAI_API_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| MuesliError::Api(format!("OpenAI request failed: {}", e)))?;
    
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(MuesliError::Api(format!("OpenAI error {}: {}", status, body)));
    }
    
    let result: OpenAIResponse = response
        .json()
        .await
        .map_err(|e| MuesliError::Api(format!("Failed to parse OpenAI response: {}", e)))?;
    
    let segments = if let Some(api_segments) = result.segments {
        api_segments
            .into_iter()
            .map(|s| TranscriptSegment {
                start_ms: (s.start * 1000.0) as u64,
                end_ms: (s.end * 1000.0) as u64,
                text: s.text.trim().to_string(),
                speaker: None,
                confidence: None,
            })
            .collect()
    } else {
        // Fallback: single segment with full text
        vec![TranscriptSegment {
            start_ms: 0,
            end_ms: 0,
            text: result.text,
            speaker: None,
            confidence: None,
        }]
    };
    
    Ok(Transcript::new(segments))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_openai_url() {
        assert!(super::OPENAI_API_URL.starts_with("https://"));
    }
}
