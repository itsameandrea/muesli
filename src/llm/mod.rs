pub mod local;
pub mod claude;
pub mod openai;
pub mod prompts;
pub mod chunking;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::settings::LlmConfig;
use crate::transcription::Transcript;
use chunking::{chunk_transcript, needs_chunking};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Claude,
    OpenAI,
    Local,
}

impl LlmProvider {
    pub fn from_engine(engine: &str) -> Option<Self> {
        match engine.to_lowercase().as_str() {
            "claude" => Some(Self::Claude),
            "openai" => Some(Self::OpenAI),
            "local" => Some(Self::Local),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryResult {
    pub markdown: String,
}

pub async fn summarize_transcript(
    config: &LlmConfig,
    transcript: &Transcript,
) -> Result<SummaryResult> {
    let provider = LlmProvider::from_engine(&config.engine)
        .context("Invalid LLM engine specified")?;

    if needs_chunking(&transcript.segments) {
        tracing::info!("Transcript is large, using chunked summarization");
        return summarize_chunked(config, provider, &transcript.segments).await;
    }

    let has_speakers = transcript.segments.iter().any(|s| s.speaker.is_some());
    let prompt = if has_speakers {
        prompts::meeting_summary_prompt_with_speakers(&transcript.segments)
    } else {
        let transcript_text = transcript.full_text();
        prompts::meeting_summary_prompt(&transcript_text)
    };

    let response_text = call_llm(config, provider, &prompt).await?;

    Ok(SummaryResult {
        markdown: response_text.trim().to_string(),
    })
}

async fn summarize_chunked(
    config: &LlmConfig,
    provider: LlmProvider,
    segments: &[crate::transcription::TranscriptSegment],
) -> Result<SummaryResult> {
    let chunks = chunk_transcript(segments);
    tracing::info!("Split transcript into {} chunks", chunks.len());

    let mut chunk_summaries = Vec::new();

    for chunk in &chunks {
        tracing::info!(
            "Summarizing chunk {}/{} ({} chars, {}—{})",
            chunk.chunk_index + 1,
            chunk.total_chunks,
            chunk.char_count(),
            format_time(chunk.start_time_ms),
            format_time(chunk.end_time_ms)
        );

        let chunk_text = chunk.format_for_prompt();
        let prompt = prompts::chunk_summary_prompt(&chunk_text, chunk.chunk_index, chunk.total_chunks);
        
        let summary = call_llm(config, provider, &prompt).await?;
        chunk_summaries.push(summary);
    }

    tracing::info!("Synthesizing {} chunk summaries into final notes", chunk_summaries.len());
    let synthesis_prompt = prompts::synthesis_prompt(&chunk_summaries);
    let final_summary = call_llm(config, provider, &synthesis_prompt).await?;

    Ok(SummaryResult {
        markdown: final_summary.trim().to_string(),
    })
}

async fn call_llm(config: &LlmConfig, provider: LlmProvider, prompt: &str) -> Result<String> {
    match provider {
        LlmProvider::Claude => {
            let api_key = config
                .claude_api_key
                .as_ref()
                .context("Claude API key not configured")?;
            claude::summarize_with_claude(api_key, &config.claude_model, prompt).await
        }
        LlmProvider::OpenAI => {
            let api_key = config
                .openai_api_key
                .as_ref()
                .context("OpenAI API key not configured")?;
            openai::summarize_with_openai(api_key, &config.openai_model, prompt).await
        }
        LlmProvider::Local => {
            local::summarize_with_local(
                &config.local_lms_path,
                &config.local_model,
                prompt,
            )
            .await
        }
    }
}

fn format_time(ms: u64) -> String {
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

    #[test]
    fn test_provider_from_engine() {
        assert_eq!(LlmProvider::from_engine("claude"), Some(LlmProvider::Claude));
        assert_eq!(LlmProvider::from_engine("openai"), Some(LlmProvider::OpenAI));
        assert_eq!(LlmProvider::from_engine("local"), Some(LlmProvider::Local));
        assert_eq!(LlmProvider::from_engine("invalid"), None);
    }

    #[test]
    fn test_summary_result_structure() {
        let result = SummaryResult {
            markdown: "## Summary\nTest notes".to_string(),
        };
        assert!(result.markdown.contains("Summary"));
    }
}
