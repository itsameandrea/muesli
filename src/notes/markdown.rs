use crate::error::Result;
use crate::llm::SummaryResult;
use crate::storage::Meeting;
use crate::transcription::Transcript;
use std::fs;
use std::path::PathBuf;

pub struct NoteGenerator {
    notes_dir: PathBuf,
}

impl NoteGenerator {
    pub fn new(notes_dir: PathBuf) -> Self {
        Self { notes_dir }
    }

    pub fn generate(
        &self,
        meeting: &Meeting,
        transcript: &Transcript,
        summary: &SummaryResult,
    ) -> Result<PathBuf> {
        fs::create_dir_all(&self.notes_dir)?;

        let mut content = String::new();

        content.push_str("---\n");
        content.push_str(&format!("title: \"{}\"\n", meeting.title));
        content.push_str(&format!(
            "date: {}\n",
            meeting.started_at.format("%Y-%m-%d %H:%M")
        ));
        if let Some(duration) = meeting.duration_seconds {
            content.push_str(&format!(
                "duration: {}m {}s\n",
                duration / 60,
                duration % 60
            ));
        }
        if let Some(app) = &meeting.detected_app {
            content.push_str(&format!("app: \"{}\"\n", app));
        }
        content.push_str(&format!("id: \"{}\"\n", meeting.id));
        content.push_str("---\n\n");

        content.push_str(&format!("# {}\n\n", meeting.title));

        content.push_str(&summary.markdown);
        content.push_str("\n\n---\n\n");

        content.push_str("## Full Transcript\n\n");
        content.push_str("<details>\n<summary>Click to expand transcript</summary>\n\n");
        for segment in &transcript.segments {
            let timestamp = format_timestamp(segment.start_ms);
            match &segment.speaker {
                Some(speaker) => {
                    content.push_str(&format!(
                        "**[{}] {}:** {}\n\n",
                        timestamp, speaker, segment.text
                    ));
                }
                None => {
                    content.push_str(&format!("**[{}]** {}\n\n", timestamp, segment.text));
                }
            }
        }
        content.push_str("</details>\n");

        let notes_path = self.notes_dir.join(format!("{}.md", meeting.id));
        fs::write(&notes_path, content)?;

        Ok(notes_path)
    }
}

fn format_timestamp(ms: u64) -> String {
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
    use crate::transcription::TranscriptSegment;

    #[test]
    fn test_format_timestamp() {
        assert_eq!(format_timestamp(0), "00:00");
        assert_eq!(format_timestamp(1000), "00:01");
        assert_eq!(format_timestamp(60000), "01:00");
        assert_eq!(format_timestamp(3600000), "01:00:00");
        assert_eq!(format_timestamp(3661000), "01:01:01");
    }

    #[test]
    fn test_note_generation() {
        let temp_dir = std::env::temp_dir().join("muesli_test_notes");
        let generator = NoteGenerator::new(temp_dir.clone());

        let mut meeting = Meeting::new("Test Meeting".to_string());
        meeting.duration_seconds = Some(3661);
        meeting.detected_app = Some("Zoom".to_string());

        let segments = vec![TranscriptSegment {
            start_ms: 0,
            end_ms: 5000,
            text: "Hello everyone".to_string(),
            speaker: Some("Alice".to_string()),
            confidence: Some(0.95),
        }];

        let transcript = Transcript::new(segments);
        let summary = SummaryResult {
            markdown: "## TL;DR\nTest meeting summary.".to_string(),
        };

        let result = generator.generate(&meeting, &transcript, &summary);
        assert!(result.is_ok());

        let notes_path = result.unwrap();
        assert!(notes_path.exists());

        let content = fs::read_to_string(&notes_path).unwrap();
        assert!(content.contains("title: \"Test Meeting\""));
        assert!(content.contains("## TL;DR"));
        assert!(content.contains("Hello everyone"));

        let _ = fs::remove_dir_all(temp_dir);
    }
}
