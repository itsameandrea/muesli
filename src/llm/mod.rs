pub mod catalog;
pub mod chunking;
pub mod claude;
pub mod local;
pub mod openai_compat;
pub mod prompts;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::settings::LlmConfig;
use crate::transcription::Transcript;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    Anthropic,
    OpenAI,
    Moonshot,
    OpenRouter,
    Local,
}

impl LlmProvider {
    pub fn from_provider(provider: &str) -> Option<Self> {
        match provider.to_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAI),
            "moonshot" => Some(Self::Moonshot),
            "openrouter" => Some(Self::OpenRouter),
            "local" => Some(Self::Local),
            _ => None,
        }
    }

    pub fn base_url(&self) -> &'static str {
        match self {
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Moonshot => "https://api.moonshot.ai/v1",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Anthropic | Self::Local => "",
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
    let provider =
        LlmProvider::from_provider(&config.provider).context("Invalid LLM provider specified")?;

    let model = config.effective_model();
    let context_limit =
        chunking::resolve_context_limit(&config.provider, model, config.context_limit);
    tracing::info!(
        "Context limit for {}/{}: {} tokens",
        config.provider,
        model,
        context_limit
    );

    if chunking::needs_chunking(&transcript.segments, context_limit) {
        tracing::info!("Transcript is large, using chunked summarization");
        return summarize_chunked(config, provider, &transcript.segments, context_limit).await;
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

pub async fn generate_title(config: &LlmConfig, meeting_notes: &str) -> Result<String> {
    let provider =
        LlmProvider::from_provider(&config.provider).context("Invalid LLM provider specified")?;

    let prompt = prompts::title_generation_prompt(meeting_notes);
    let title = call_llm(config, provider, &prompt).await?;

    let cleaned = title
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .lines()
        .next()
        .unwrap_or("Untitled Meeting")
        .to_string();

    Ok(cleaned)
}

async fn summarize_chunked(
    config: &LlmConfig,
    provider: LlmProvider,
    segments: &[crate::transcription::TranscriptSegment],
    context_limit: usize,
) -> Result<SummaryResult> {
    let chunks = chunking::chunk_transcript(segments, context_limit);
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
        let prompt =
            prompts::chunk_summary_prompt(&chunk_text, chunk.chunk_index, chunk.total_chunks);

        let summary = call_llm(config, provider, &prompt).await?;
        chunk_summaries.push(summary);
    }

    tracing::info!(
        "Synthesizing {} chunk summaries into final notes",
        chunk_summaries.len()
    );
    let synthesis_prompt = prompts::synthesis_prompt(&chunk_summaries);
    let final_summary = call_llm(config, provider, &synthesis_prompt).await?;

    Ok(SummaryResult {
        markdown: final_summary.trim().to_string(),
    })
}

pub async fn ask(config: &LlmConfig, prompt: &str) -> Result<String> {
    if config.provider == "none" {
        anyhow::bail!("LLM is not configured. Run 'muesli setup' to set up an LLM provider.");
    }

    let provider =
        LlmProvider::from_provider(&config.provider).context("Invalid LLM provider specified")?;

    call_llm(config, provider, prompt).await
}

async fn call_llm(config: &LlmConfig, provider: LlmProvider, prompt: &str) -> Result<String> {
    let model = config.effective_model();

    match provider {
        LlmProvider::Anthropic => {
            let api_key = config
                .api_key
                .as_ref()
                .context("Anthropic API key not configured")?;
            claude::summarize_with_claude(api_key, model, prompt).await
        }
        LlmProvider::OpenAI | LlmProvider::Moonshot | LlmProvider::OpenRouter => {
            let api_key = config.api_key.as_ref().context("API key not configured")?;
            openai_compat::summarize(provider.base_url(), api_key, model, prompt).await
        }
        LlmProvider::Local => {
            local::summarize_with_local(&config.local_lms_path, model, prompt).await
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
    fn test_provider_from_provider() {
        assert_eq!(
            LlmProvider::from_provider("anthropic"),
            Some(LlmProvider::Anthropic)
        );
        assert_eq!(
            LlmProvider::from_provider("openai"),
            Some(LlmProvider::OpenAI)
        );
        assert_eq!(
            LlmProvider::from_provider("moonshot"),
            Some(LlmProvider::Moonshot)
        );
        assert_eq!(
            LlmProvider::from_provider("openrouter"),
            Some(LlmProvider::OpenRouter)
        );
        assert_eq!(
            LlmProvider::from_provider("local"),
            Some(LlmProvider::Local)
        );
        assert_eq!(LlmProvider::from_provider("invalid"), None);
    }

    #[test]
    fn test_base_urls() {
        assert_eq!(LlmProvider::OpenAI.base_url(), "https://api.openai.com/v1");
        assert_eq!(
            LlmProvider::Moonshot.base_url(),
            "https://api.moonshot.ai/v1"
        );
        assert_eq!(
            LlmProvider::OpenRouter.base_url(),
            "https://openrouter.ai/api/v1"
        );
    }

    #[test]
    fn test_summary_result_structure() {
        let result = SummaryResult {
            markdown: "## Summary\nTest notes".to_string(),
        };
        assert!(result.markdown.contains("Summary"));
    }
}
