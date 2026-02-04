use crate::error::Result;
use crate::storage::Meeting;
use crate::transcription::Transcript;
use std::fs;
use std::path::PathBuf;

/// Generates markdown meeting notes
pub struct NoteGenerator {
    notes_dir: PathBuf,
}

impl NoteGenerator {
    /// Create a new note generator
    pub fn new(notes_dir: PathBuf) -> Self {
        Self { notes_dir }
    }

    /// Generate markdown notes for a meeting
    pub fn generate(
        &self,
        meeting: &Meeting,
        transcript: &Transcript,
        participants: Option<Vec<String>>,
    ) -> Result<PathBuf> {
        // Ensure notes directory exists
        fs::create_dir_all(&self.notes_dir)?;

        // Build the markdown content
        let mut content = String::new();

        // YAML frontmatter
        content.push_str("---\n");
        content.push_str(&format!("title: \"{}\"\n", meeting.title));
        content.push_str(&format!(
            "date: {}\n",
            meeting.started_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));

        if let Some(duration) = meeting.duration_seconds {
            let hours = duration / 3600;
            let minutes = (duration % 3600) / 60;
            let seconds = duration % 60;
            if hours > 0 {
                content.push_str(&format!("duration: {}h {}m {}s\n", hours, minutes, seconds));
            } else if minutes > 0 {
                content.push_str(&format!("duration: {}m {}s\n", minutes, seconds));
            } else {
                content.push_str(&format!("duration: {}s\n", seconds));
            }
        }

        if let Some(app) = &meeting.detected_app {
            content.push_str(&format!("app: \"{}\"\n", app));
        }

        if let Some(parts) = participants {
            if !parts.is_empty() {
                content.push_str("participants:\n");
                for participant in parts {
                    content.push_str(&format!("  - \"{}\"\n", participant));
                }
            }
        }

        content.push_str("---\n\n");

        // Title
        content.push_str(&format!("# {}\n\n", meeting.title));

        // Metadata section
        content.push_str("## Meeting Information\n\n");
        content.push_str(&format!(
            "- **Date**: {}\n",
            meeting.started_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        if let Some(app) = &meeting.detected_app {
            content.push_str(&format!("- **Application**: {}\n", app));
        }
        if let Some(duration) = meeting.duration_seconds {
            let hours = duration / 3600;
            let minutes = (duration % 3600) / 60;
            let seconds = duration % 60;
            if hours > 0 {
                content.push_str(&format!(
                    "- **Duration**: {}h {}m {}s\n",
                    hours, minutes, seconds
                ));
            } else if minutes > 0 {
                content.push_str(&format!("- **Duration**: {}m {}s\n", minutes, seconds));
            } else {
                content.push_str(&format!("- **Duration**: {}s\n", seconds));
            }
        }
        content.push_str("\n");

        // Summary section (placeholder)
        content.push_str("## Summary\n\n");
        content.push_str("*Summary will be generated here*\n\n");

        // Transcript section
        content.push_str("## Transcript\n\n");
        for segment in &transcript.segments {
            let timestamp = format_timestamp_hms(segment.start_ms);
            if let Some(speaker) = &segment.speaker {
                content.push_str(&format!(
                    "[{}] **{}**: {}\n\n",
                    timestamp, speaker, segment.text
                ));
            } else {
                content.push_str(&format!("[{}] {}\n\n", timestamp, segment.text));
            }
        }

        // Action items section (placeholder)
        content.push_str("## Action Items\n\n");
        content.push_str("*Action items will be extracted here*\n\n");

        // Write to file
        let notes_path = self.notes_dir.join(format!("{}.md", meeting.id));
        fs::write(&notes_path, content)?;

        Ok(notes_path)
    }
}

/// Format milliseconds as HH:MM:SS
fn format_timestamp_hms(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::TranscriptSegment;

    #[test]
    fn test_format_timestamp_hms() {
        assert_eq!(format_timestamp_hms(0), "00:00:00");
        assert_eq!(format_timestamp_hms(1000), "00:00:01");
        assert_eq!(format_timestamp_hms(60000), "00:01:00");
        assert_eq!(format_timestamp_hms(3600000), "01:00:00");
        assert_eq!(format_timestamp_hms(3661000), "01:01:01");
    }

    #[test]
    fn test_note_generation() {
        let temp_dir = std::env::temp_dir().join("muesli_test_notes");
        let generator = NoteGenerator::new(temp_dir.clone());

        let mut meeting = Meeting::new("Test Meeting".to_string());
        meeting.duration_seconds = Some(3661);
        meeting.detected_app = Some("Zoom".to_string());

        let segments = vec![
            TranscriptSegment {
                start_ms: 0,
                end_ms: 5000,
                text: "Hello everyone".to_string(),
                speaker: Some("Alice".to_string()),
                confidence: Some(0.95),
            },
            TranscriptSegment {
                start_ms: 5000,
                end_ms: 10000,
                text: "Hi Alice".to_string(),
                speaker: Some("Bob".to_string()),
                confidence: Some(0.92),
            },
        ];

        let transcript = Transcript::new(segments);
        let participants = Some(vec!["Alice".to_string(), "Bob".to_string()]);

        let result = generator.generate(&meeting, &transcript, participants);
        assert!(result.is_ok());

        let notes_path = result.unwrap();
        assert!(notes_path.exists());

        let content = fs::read_to_string(&notes_path).unwrap();
        assert!(content.contains("title: \"Test Meeting\""));
        assert!(content.contains("app: \"Zoom\""));
        assert!(content.contains("duration: 1h 1m 1s"));
        assert!(content.contains("[00:00:00] **Alice**: Hello everyone"));
        assert!(content.contains("[00:00:05] **Bob**: Hi Alice"));
        assert!(content.contains("## Summary"));
        assert!(content.contains("## Action Items"));

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir);
    }
}
