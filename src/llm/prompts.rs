use crate::transcription::TranscriptSegment;

pub fn meeting_summary_prompt(transcript: &str) -> String {
    format!(
        r#"Create meeting minutes from this transcript.

TRANSCRIPT:
{}

Output MARKDOWN with these sections:

## Topics Covered
- Bullet list of main topics discussed (3-7 items)

## Discussion
For each topic above, create a subsection:
### [Topic Name]
- Bullet points of what was discussed
- Include specific details, names, numbers mentioned
- Note any disagreements or alternative viewpoints

## Decisions Made
- List each decision (ONLY if explicit decisions were made, otherwise omit this section entirely)

## Action Items
- [ ] Task — Owner — Due date (ONLY if action items exist, otherwise omit this section entirely)

RULES:
- Be specific and detailed, use actual content from the transcript
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI")
- Omit Decisions/Action Items sections if none exist (don't write "None")
- No fluff, no filler, no corporate speak

Output ONLY the markdown."#,
        transcript
    )
}

pub fn meeting_summary_prompt_with_speakers(segments: &[TranscriptSegment]) -> String {
    let mut transcript = String::new();

    for segment in segments {
        let timestamp = format_timestamp(segment.start_ms);
        match &segment.speaker {
            Some(speaker) => {
                transcript.push_str(&format!("[{}] {}: {}\n", timestamp, speaker, segment.text));
            }
            None => {
                transcript.push_str(&format!("[{}] {}\n", timestamp, segment.text));
            }
        }
    }

    format!(
        r#"Create meeting minutes from this transcript.

Speakers are labeled SPEAKER_1, SPEAKER_2, etc. Try to identify them by name if mentioned, otherwise use Speaker 1, Speaker 2.

TRANSCRIPT:
{}

Output MARKDOWN with these sections:

## Attendees
- Speaker 1 (or name if identified): brief role if inferable
- Speaker 2: ...

## Topics Covered
- Bullet list of main topics discussed (3-7 items)

## Discussion
For each topic above, create a subsection:
### [Topic Name]
- Bullet points of what was discussed
- Attribute key points to speakers (e.g., "Speaker 1 explained...", "John suggested...")
- Include specific details, names, numbers, tools mentioned
- Note any disagreements or alternative viewpoints

## Decisions Made
- List each decision with who made/agreed to it (ONLY if explicit decisions were made, otherwise omit this section entirely)

## Action Items
- [ ] Task — Owner — Due date (ONLY if action items exist, otherwise omit this section entirely)

RULES:
- Be specific and detailed, use actual content from the transcript
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI", "Paracate" → "Parakeet")
- Omit Decisions/Action Items sections if none exist (don't write "None")
- No fluff, no filler, no corporate speak
- Attribute statements to speakers throughout

Output ONLY the markdown."#,
        transcript
    )
}

fn format_timestamp(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{:02}:{:02}", minutes, seconds)
}

pub fn chunk_summary_prompt(
    chunk_transcript: &str,
    chunk_index: usize,
    total_chunks: usize,
) -> String {
    format!(
        r#"Summarize this portion of a meeting transcript (chunk {current} of {total}).

TRANSCRIPT CHUNK:
{transcript}

Output:

## Topics in This Section
- List each distinct topic discussed

## Discussion Details
For each topic:
### [Topic Name]
- Key points discussed (attribute to speakers if identified)
- Specific details, names, numbers mentioned

## Decisions (if any)
- Any explicit decisions made

## Action Items (if any)
- Any tasks assigned

RULES:
- Be thorough - this will be merged with other chunks
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI")
- Only include Decisions/Action Items if they actually exist

Output ONLY the markdown."#,
        current = chunk_index + 1,
        total = total_chunks,
        transcript = chunk_transcript
    )
}

pub fn synthesis_prompt(chunk_summaries: &[String]) -> String {
    let combined = chunk_summaries
        .iter()
        .enumerate()
        .map(|(i, s)| format!("--- CHUNK {} SUMMARY ---\n{}\n", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"Synthesize these chunk summaries into unified meeting minutes.

{combined}

Merge related topics, consolidate decisions/actions, remove redundancy.

Output MARKDOWN:

## Attendees
- List participants and roles (if identifiable)

## Topics Covered
- Bullet list of all main topics (5-10 for long meetings)

## Discussion
For each topic:
### [Topic Name]
- Key points discussed (attribute to speakers)
- Specific details mentioned

## Decisions Made
- All decisions from the meeting (OMIT section if none)

## Action Items
- [ ] Task — Owner — Due date (OMIT section if none)

RULES:
- Synthesize, don't concatenate
- Group related topics even if discussed across chunks
- Keep all decisions and action items (don't lose any)
- No fluff

Output ONLY the markdown."#,
        combined = combined
    )
}

pub fn title_generation_prompt(meeting_notes: &str) -> String {
    format!(
        r#"Generate a concise, descriptive title for this meeting based on the notes below.

MEETING NOTES:
{notes}

RULES:
- 3-8 words maximum
- Capture the main topic or purpose
- Use title case
- No quotes, no punctuation at the end
- Be specific, not generic (avoid "Team Meeting", "Weekly Sync")

Examples of good titles:
- Product Roadmap Q2 Planning
- Customer Onboarding Flow Review
- Engineering Hiring Strategy
- Bug Triage and Sprint Planning

Output ONLY the title, nothing else."#,
        notes = meeting_notes
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
        assert!(prompt.contains("Topics Covered"));
        assert!(prompt.contains("Discussion"));
    }

    #[test]
    fn test_prompt_with_speakers() {
        let segments = vec![
            TranscriptSegment {
                start_ms: 0,
                end_ms: 5000,
                text: "Hello everyone".to_string(),
                speaker: Some("SPEAKER_0".to_string()),
                confidence: None,
            },
            TranscriptSegment {
                start_ms: 5000,
                end_ms: 10000,
                text: "Hi there".to_string(),
                speaker: Some("SPEAKER_1".to_string()),
                confidence: None,
            },
        ];
        let prompt = meeting_summary_prompt_with_speakers(&segments);
        assert!(prompt.contains("[00:00] SPEAKER_0: Hello everyone"));
        assert!(prompt.contains("[00:05] SPEAKER_1: Hi there"));
        assert!(prompt.contains("Attendees"));
    }
}
