pub mod local;
pub mod claude;
pub mod openai;
pub mod prompts;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::settings::LlmConfig;
use crate::transcription::Transcript;

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
    pub summary: String,
    pub action_items: Vec<String>,
}

pub async fn summarize_transcript(
    config: &LlmConfig,
    transcript: &Transcript,
) -> Result<SummaryResult> {
    let provider = LlmProvider::from_engine(&config.engine)
        .context("Invalid LLM engine specified")?;

    let transcript_text = transcript.full_text();
    let prompt = prompts::meeting_summary_prompt(&transcript_text);

    let response_text = match provider {
        LlmProvider::Claude => {
            let api_key = config
                .claude_api_key
                .as_ref()
                .context("Claude API key not configured")?;
            claude::summarize_with_claude(api_key, &config.claude_model, &prompt).await?
        }
        LlmProvider::OpenAI => {
            let api_key = config
                .openai_api_key
                .as_ref()
                .context("OpenAI API key not configured")?;
            openai::summarize_with_openai(api_key, &config.openai_model, &prompt).await?
        }
        LlmProvider::Local => {
            anyhow::bail!("Local LLM not yet implemented");
        }
    };

    parse_summary_response(&response_text)
}

fn parse_summary_response(response: &str) -> Result<SummaryResult> {
    let trimmed = response.trim();
    
    let json_start = trimmed.find('{').unwrap_or(0);
    let json_end = trimmed.rfind('}').map(|i| i + 1).unwrap_or(trimmed.len());
    let json_str = &trimmed[json_start..json_end];

    serde_json::from_str(json_str)
        .context("Failed to parse LLM response as JSON")
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
    fn test_parse_summary_response() {
        let response = r#"{"summary": "Test summary", "action_items": ["Item 1", "Item 2"]}"#;
        let result = parse_summary_response(response).unwrap();
        assert_eq!(result.summary, "Test summary");
        assert_eq!(result.action_items.len(), 2);
    }

    #[test]
    fn test_parse_summary_with_extra_text() {
        let response = r#"Here is the JSON:
        {"summary": "Test", "action_items": []}
        That's all."#;
        let result = parse_summary_response(response).unwrap();
        assert_eq!(result.summary, "Test");
        assert_eq!(result.action_items.len(), 0);
    }
}
