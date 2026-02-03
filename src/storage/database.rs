use crate::error::Result;
use crate::storage::migrations;
use crate::storage::{Meeting, MeetingId, MeetingStatus};
use crate::transcription::TranscriptSegment;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        migrations::run_migrations(&conn)?;

        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        migrations::run_migrations(&conn)?;
        Ok(Self { conn })
    }

    pub fn insert_meeting(&self, meeting: &Meeting) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meetings (id, title, started_at, ended_at, duration_seconds, audio_path, transcript_path, notes_path, status, detected_app)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                meeting.id.0,
                meeting.title,
                meeting.started_at.to_rfc3339(),
                meeting.ended_at.map(|t| t.to_rfc3339()),
                meeting.duration_seconds,
                meeting.audio_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                meeting.transcript_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                meeting.notes_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                meeting.status.to_string(),
                meeting.detected_app,
            ],
        )?;
        Ok(())
    }

    pub fn get_meeting(&self, id: &MeetingId) -> Result<Option<Meeting>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, started_at, ended_at, duration_seconds, audio_path, transcript_path, notes_path, status, detected_app
             FROM meetings WHERE id = ?1"
        )?;

        let meeting = stmt
            .query_row([&id.0], |row| {
                Ok(Meeting {
                    id: MeetingId::from_string(row.get(0)?),
                    title: row.get(1)?,
                    started_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    ended_at: row
                        .get::<_, Option<String>>(3)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|t| t.with_timezone(&chrono::Utc)),
                    duration_seconds: row.get(4)?,
                    audio_path: row
                        .get::<_, Option<String>>(5)?
                        .map(std::path::PathBuf::from),
                    transcript_path: row
                        .get::<_, Option<String>>(6)?
                        .map(std::path::PathBuf::from),
                    notes_path: row
                        .get::<_, Option<String>>(7)?
                        .map(std::path::PathBuf::from),
                    status: parse_status(&row.get::<_, String>(8)?),
                    detected_app: row.get(9)?,
                })
            })
            .optional()?;

        Ok(meeting)
    }

    pub fn update_meeting(&self, meeting: &Meeting) -> Result<()> {
        self.conn.execute(
            "UPDATE meetings SET 
                title = ?2, ended_at = ?3, duration_seconds = ?4, audio_path = ?5,
                transcript_path = ?6, notes_path = ?7, status = ?8, detected_app = ?9
             WHERE id = ?1",
            params![
                meeting.id.0,
                meeting.title,
                meeting.ended_at.map(|t| t.to_rfc3339()),
                meeting.duration_seconds,
                meeting
                    .audio_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                meeting
                    .transcript_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                meeting
                    .notes_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                meeting.status.to_string(),
                meeting.detected_app,
            ],
        )?;
        Ok(())
    }

    pub fn delete_meeting(&self, id: &MeetingId) -> Result<()> {
        self.conn
            .execute("DELETE FROM meetings WHERE id = ?1", [&id.0])?;
        Ok(())
    }

    pub fn list_meetings(&self, limit: usize) -> Result<Vec<Meeting>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, started_at, ended_at, duration_seconds, audio_path, transcript_path, notes_path, status, detected_app
             FROM meetings ORDER BY started_at DESC LIMIT ?1"
        )?;

        let meetings = stmt
            .query_map([limit], |row| {
                Ok(Meeting {
                    id: MeetingId::from_string(row.get(0)?),
                    title: row.get(1)?,
                    started_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    ended_at: row
                        .get::<_, Option<String>>(3)?
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                        .map(|t| t.with_timezone(&chrono::Utc)),
                    duration_seconds: row.get(4)?,
                    audio_path: row
                        .get::<_, Option<String>>(5)?
                        .map(std::path::PathBuf::from),
                    transcript_path: row
                        .get::<_, Option<String>>(6)?
                        .map(std::path::PathBuf::from),
                    notes_path: row
                        .get::<_, Option<String>>(7)?
                        .map(std::path::PathBuf::from),
                    status: parse_status(&row.get::<_, String>(8)?),
                    detected_app: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(meetings)
    }

    pub fn insert_transcript_segments(
        &self,
        meeting_id: &MeetingId,
        segments: &[TranscriptSegment],
    ) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO transcripts (meeting_id, segment_index, start_ms, end_ms, text, speaker, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )?;

        for (i, segment) in segments.iter().enumerate() {
            stmt.execute(params![
                meeting_id.0,
                i as i32,
                segment.start_ms as i64,
                segment.end_ms as i64,
                segment.text,
                segment.speaker,
                segment.confidence,
            ])?;
        }

        Ok(())
    }

    pub fn get_transcript_segments(
        &self,
        meeting_id: &MeetingId,
    ) -> Result<Vec<TranscriptSegment>> {
        let mut stmt = self.conn.prepare(
            "SELECT start_ms, end_ms, text, speaker, confidence
             FROM transcripts WHERE meeting_id = ?1 ORDER BY segment_index",
        )?;

        let segments = stmt
            .query_map([&meeting_id.0], |row| {
                Ok(TranscriptSegment {
                    start_ms: row.get::<_, i64>(0)? as u64,
                    end_ms: row.get::<_, i64>(1)? as u64,
                    text: row.get(2)?,
                    speaker: row.get(3)?,
                    confidence: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(segments)
    }
}

fn parse_status(s: &str) -> MeetingStatus {
    match s {
        "recording" => MeetingStatus::Recording,
        "processing" => MeetingStatus::Processing,
        "complete" => MeetingStatus::Complete,
        "failed" => MeetingStatus::Failed,
        _ => MeetingStatus::Failed,
    }
}

trait OptionalExt<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_in_memory() {
        let db = Database::open_in_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn test_meeting_crud() {
        let db = Database::open_in_memory().unwrap();

        let meeting = Meeting::new("Test Meeting".to_string());
        let id = meeting.id.clone();
        db.insert_meeting(&meeting).unwrap();

        let loaded = db.get_meeting(&id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().title, "Test Meeting");

        let mut updated = db.get_meeting(&id).unwrap().unwrap();
        updated.title = "Updated Title".to_string();
        db.update_meeting(&updated).unwrap();

        let reloaded = db.get_meeting(&id).unwrap().unwrap();
        assert_eq!(reloaded.title, "Updated Title");

        let meetings = db.list_meetings(10).unwrap();
        assert_eq!(meetings.len(), 1);

        db.delete_meeting(&id).unwrap();
        let deleted = db.get_meeting(&id).unwrap();
        assert!(deleted.is_none());
    }

    #[test]
    fn test_transcript_segments() {
        let db = Database::open_in_memory().unwrap();

        let meeting = Meeting::new("Test".to_string());
        db.insert_meeting(&meeting).unwrap();

        let segments = vec![
            TranscriptSegment::new(0, 5000, "Hello".to_string()),
            TranscriptSegment::new(5000, 10000, "World".to_string()),
        ];

        db.insert_transcript_segments(&meeting.id, &segments)
            .unwrap();

        let loaded = db.get_transcript_segments(&meeting.id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].text, "Hello");
        assert_eq!(loaded[1].text, "World");
    }
}
