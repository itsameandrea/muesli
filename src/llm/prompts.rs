/// Generate a prompt for meeting summarization
pub fn meeting_summary_prompt(transcript: &str) -> String {
    format!(
        r#"You are an AI assistant that summarizes meeting transcripts. 
Analyze the following meeting transcript and provide:

1. A concise summary of the key discussion points (2-4 sentences)
2. A list of action items mentioned in the meeting

Format your response as JSON with this structure:
{{
  "summary": "Your summary here",
  "action_items": ["Action item 1", "Action item 2", ...]
}}

If no action items are found, return an empty array.

Transcript:
{}

Respond ONLY with valid JSON, no additional text."#,
        transcript
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_generation() {
        let transcript = "We discussed the project timeline and agreed to finish by Friday.";
        let prompt = meeting_summary_prompt(transcript);
        assert!(prompt.contains(transcript));
        assert!(prompt.contains("summary"));
        assert!(prompt.contains("action_items"));
    }
}
