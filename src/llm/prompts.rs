use crate::transcription::TranscriptSegment;

pub fn meeting_summary_prompt(transcript: &str) -> String {
    let char_count = transcript.len();
    let length_hint = length_guidance(char_count);

    format!(
        r#"Create comprehensive meeting notes from this transcript.

TRANSCRIPT:
{transcript}

Output MARKDOWN with these sections:

## TL;DR
2-3 sentence summary of the meeting's purpose and outcome.

## Topics Covered
- Bullet list of every distinct topic discussed ({topic_range})

## Discussion
For each topic above, create a subsection:
### [Topic Name]
- Write detailed paragraphs and bullet points covering what was discussed
- Include specific details: names, numbers, tools, dates, URLs, code references
- Capture the reasoning and context behind statements, not just conclusions
- Note disagreements, alternative viewpoints, or open questions
- Include relevant quotes when they capture important nuance
{length_instruction}

## Decisions Made
- List each decision with context for why it was made (ONLY if explicit decisions were made, otherwise omit this section entirely)

## Action Items
- [ ] Task — Owner — Due date (ONLY if action items exist, otherwise omit this section entirely)

## Open Questions
- Unresolved questions or topics that need follow-up (OMIT if none)

RULES:
- Be thorough — these notes replace attending the meeting
- Someone reading this should understand not just WHAT was discussed but WHY
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI")
- Omit Decisions/Action Items/Open Questions sections if none exist
- No fluff, no filler, no corporate speak — but DO include all substantive detail

Output ONLY the markdown."#,
        transcript = transcript,
        topic_range = length_hint.topic_range,
        length_instruction = length_hint.detail_instruction,
    )
}

struct LengthGuidance {
    topic_range: &'static str,
    detail_instruction: &'static str,
}

fn length_guidance(transcript_chars: usize) -> LengthGuidance {
    match transcript_chars {
        0..=10_000 => LengthGuidance {
            topic_range: "2-5 items",
            detail_instruction: "- Aim for 2-4 bullet points per topic",
        },
        10_001..=40_000 => LengthGuidance {
            topic_range: "5-10 items",
            detail_instruction: "- Aim for a thorough paragraph + bullet points per topic\n- This is a medium-length meeting — capture all important discussion threads",
        },
        _ => LengthGuidance {
            topic_range: "8-15+ items — do not compress, this was a long meeting",
            detail_instruction: "- Write extensively for each topic — multiple paragraphs if needed\n- This is a long meeting — the notes should be proportionally detailed\n- Capture the full arc of each discussion: context, debate, reasoning, conclusion",
        },
    }
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

    let char_count = transcript.len();
    let length_hint = length_guidance(char_count);

    format!(
        r#"Create comprehensive meeting notes from this transcript.

Speakers are labeled SPEAKER_1, SPEAKER_2, etc. Try to identify them by name if mentioned in conversation, otherwise use Speaker 1, Speaker 2.

TRANSCRIPT:
{transcript}

Output MARKDOWN with these sections:

## TL;DR
2-3 sentence summary of the meeting's purpose and outcome.

## Attendees
- Speaker 1 (or name if identified): brief role if inferable
- Speaker 2: ...

## Topics Covered
- Bullet list of every distinct topic discussed ({topic_range})

## Discussion
For each topic above, create a subsection:
### [Topic Name]
- Write detailed paragraphs and bullet points covering what was discussed
- Attribute key points to speakers (e.g., "Speaker 1 explained...", "John suggested...")
- Include specific details: names, numbers, tools, dates, URLs, code references
- Capture the reasoning and context behind statements, not just conclusions
- Note disagreements, alternative viewpoints, or open questions
- Include relevant quotes when they capture important nuance
{length_instruction}

## Decisions Made
- List each decision with context and who made/agreed to it (ONLY if explicit decisions were made, otherwise omit this section entirely)

## Action Items
- [ ] Task — Owner — Due date (ONLY if action items exist, otherwise omit this section entirely)

## Open Questions
- Unresolved questions or topics that need follow-up (OMIT if none)

RULES:
- Be thorough — these notes replace attending the meeting
- Someone reading this should understand not just WHAT was discussed but WHY
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI", "Paracate" → "Parakeet")
- Omit Decisions/Action Items/Open Questions sections if none exist
- No fluff, no filler, no corporate speak — but DO include all substantive detail
- Attribute statements to speakers throughout

Output ONLY the markdown."#,
        transcript = transcript,
        topic_range = length_hint.topic_range,
        length_instruction = length_hint.detail_instruction,
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
        r#"Create detailed notes for this portion of a meeting transcript (chunk {current} of {total}).

IMPORTANT: This is one section of a longer meeting. Write thorough notes — they will be merged with other chunks later. Do NOT compress or summarize aggressively. Preserve detail.

TRANSCRIPT CHUNK:
{transcript}

Output:

## Topics in This Section
- List every distinct topic discussed in this chunk

## Discussion Details
For each topic:
### [Topic Name]
- Write detailed paragraphs and bullet points covering what was discussed
- Attribute points to speakers if identified
- Include specific details: names, numbers, tools, dates, URLs, code references
- Capture reasoning and context, not just conclusions
- Note disagreements, open questions, or alternative viewpoints
- Include relevant quotes when they capture important nuance

## Decisions (if any)
- Decisions made with context for why

## Action Items (if any)
- Tasks assigned with owner and any mentioned timeline

RULES:
- Be thorough — detail lost here cannot be recovered during synthesis
- Fix transcription errors from context (e.g., "get" → "git", "hey I" → "AI")
- Only include Decisions/Action Items if they actually exist
- Write as much detail as the transcript warrants

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
        .map(|(i, s)| format!("--- CHUNK {} NOTES ---\n{}\n", i + 1, s))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"Merge these chunk notes into unified, comprehensive meeting notes.

This was a long meeting ({chunk_count} chunks). The final notes should be proportionally detailed.

{combined}

Output MARKDOWN:

## TL;DR
2-3 sentence summary of the meeting's purpose and outcome.

## Attendees
- List participants and roles (if identifiable)

## Topics Covered
- Bullet list of ALL main topics — do not drop topics to be brief

## Discussion
For each topic, create a subsection:
### [Topic Name]
- Merge related discussion from different chunks into coherent narratives
- Write detailed paragraphs and bullet points — preserve the depth from the chunk notes
- Attribute points to speakers
- Include specific details: names, numbers, tools, dates, code references
- Capture reasoning and context behind decisions
- Note disagreements and open questions

## Decisions Made
- Every decision from the meeting with context (OMIT section if none)

## Action Items
- [ ] Task — Owner — Due date (OMIT section if none)

## Open Questions
- Unresolved questions or topics needing follow-up (OMIT if none)

RULES:
- Merge related topics that span chunks, but do NOT compress detail
- The output should be comprehensive enough to replace attending the meeting
- Keep ALL decisions and action items — losing any is a failure
- No fluff, no filler — but DO preserve all substantive detail
- If chunks covered the same topic, merge the discussion — don't repeat

Output ONLY the markdown."#,
        chunk_count = chunk_summaries.len(),
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
