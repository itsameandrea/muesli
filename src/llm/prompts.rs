use crate::transcription::TranscriptSegment;

pub fn meeting_summary_prompt(transcript: &str) -> String {
    format!(
        r#"You are an expert meeting analyst. Create comprehensive meeting notes from this transcript.

TRANSCRIPT:
{}

Output well-structured MARKDOWN meeting notes with these sections:

## TL;DR
One sentence summary.

## Summary  
3-5 sentence executive summary of the meeting's purpose, discussions, and outcomes.

## Key Topics Discussed
For each major topic (aim for 3-7):
### [Topic Name]
2-3 sentences explaining what was discussed.

## Decisions Made
- Each explicit decision or agreement made (if any)

## Action Items
- [ ] Task description — Owner (if mentioned) — Due date (if mentioned)

## Open Questions
- Unresolved items or topics needing follow-up (if any)

---

RULES:
- Be specific and detailed, not generic
- Use actual content from the transcript, not placeholders
- Skip sections that have no content (except TL;DR, Summary, Key Topics which are required)
- Format action items as checklist items with owner/due when available

Output ONLY the markdown, no preamble or explanation."#,
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
        r#"You are an expert meeting analyst. Create comprehensive meeting notes from this transcript.

Speakers are labeled SPEAKER_1, SPEAKER_2, etc. Identify their roles from context (host, guest, interviewer, etc.) and use those roles in your notes.

TRANSCRIPT:
{}

Output well-structured MARKDOWN meeting notes with these sections:

## TL;DR
One sentence summary.

## Participants
Brief description of who was in the meeting and their roles (based on what you can infer).

## Summary  
3-5 sentence executive summary covering who participated, what was discussed, and key outcomes.

## Key Topics Discussed
For each major topic (aim for 3-7):
### [Topic Name]
2-3 sentences explaining what was discussed, referencing who said key points.

## Decisions Made
- Each explicit decision or agreement (if any)

## Action Items
- [ ] Task description — Owner — Due date (if mentioned)

## Open Questions  
- Unresolved items needing follow-up (if any)

## Notable Quotes
> "Memorable or important quote" — Speaker

---

RULES:
- Be specific and detailed, use actual content from the transcript
- Reference speakers by their inferred role when possible (e.g., "The host asked about...")
- Skip sections that have no content (except TL;DR, Summary, Key Topics which are required)
- Include 2-3 notable quotes that capture key insights

Output ONLY the markdown, no preamble or explanation."#,
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
        r#"You are an expert meeting analyst. Summarize this portion of a meeting transcript.

This is CHUNK {current} of {total} from a longer meeting.

TRANSCRIPT CHUNK:
{transcript}

Create a DETAILED summary of this portion including:

## Chunk {current} Summary
3-5 sentences covering what was discussed in this section.

## Topics in This Section
- List each distinct topic discussed
- Include key points and who said what (if speakers are identified)

## Decisions & Action Items
- Any decisions made in this section
- Any action items assigned

## Notable Points
- Important quotes or insights from this section

Be thorough - this will be merged with other chunk summaries to create final meeting notes.
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
        r#"You are an expert meeting analyst. Synthesize these chunk summaries into cohesive final meeting notes.

{combined}

Create UNIFIED meeting notes by:
1. Merging related topics across chunks
2. Consolidating all action items into one list
3. Combining decisions made throughout
4. Removing redundancy while preserving all important details

Output well-structured MARKDOWN meeting notes with these sections:

## TL;DR
One sentence summary of the entire meeting.

## Participants
Who was in the meeting and their roles (if identifiable from the summaries).

## Summary  
3-5 sentence executive summary of the full meeting.

## Key Topics Discussed
For each major topic (aim for 5-10 for long meetings):
### [Topic Name]
2-3 sentences explaining what was discussed.

## Decisions Made
- All decisions from the entire meeting

## Action Items
- [ ] Task description — Owner — Due date (if mentioned)

## Open Questions  
- Unresolved items needing follow-up

## Notable Quotes
> "Memorable or important quote" — Speaker

---

RULES:
- Synthesize, don't just concatenate
- Preserve chronological flow where it matters
- Group related topics even if discussed across multiple chunks
- Include all action items and decisions (don't lose any)

Output ONLY the markdown, no preamble."#,
        combined = combined
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
        assert!(prompt.contains("Summary"));
        assert!(prompt.contains("Action Items"));
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
        assert!(prompt.contains("Decisions Made"));
    }
}
