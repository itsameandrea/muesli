use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Debug, Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIResponseMessage {
    content: String,
}

pub async fn summarize_with_openai(api_key: &str, model: &str, prompt: &str) -> Result<String> {
    let client = reqwest::Client::new();

    let request = OpenAIRequest {
        model: model.to_string(),
        messages: vec![OpenAIMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        max_tokens: 4096,
        temperature: 0.3,
    };

    let response = client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to send request to OpenAI API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API error {}: {}", status, error_text);
    }

    let openai_response: OpenAIResponse = response
        .json()
        .await
        .context("Failed to parse OpenAI API response")?;

    openai_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .context("No choices in OpenAI response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_structure() {
        let request = OpenAIRequest {
            model: "gpt-4o".to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            max_tokens: 4096,
            temperature: 0.3,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4o"));
        assert!(json.contains("test"));
    }
}
