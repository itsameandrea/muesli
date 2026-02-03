use crate::error::Result;
use rusqlite::Connection;

/// Current schema version
pub const SCHEMA_VERSION: i32 = 1;

/// Run all migrations
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        migrate_v1(conn)?;
    }

    Ok(())
}

fn get_schema_version(conn: &Connection) -> Result<i32> {
    // Create version table if not exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
        [],
    )?;

    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(version)
}

fn set_schema_version(conn: &Connection, version: i32) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            duration_seconds INTEGER,
            audio_path TEXT,
            transcript_path TEXT,
            notes_path TEXT,
            status TEXT NOT NULL DEFAULT 'recording',
            detected_app TEXT
        );
        
        CREATE TABLE IF NOT EXISTS transcripts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            segment_index INTEGER NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            text TEXT NOT NULL,
            speaker TEXT,
            confidence REAL
        );
        
        CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id);
        ",
    )?;

    set_schema_version(conn, 1)?;
    Ok(())
}
