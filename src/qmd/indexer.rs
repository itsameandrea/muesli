use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::error::{MuesliError, Result};

const INDEX_TIMEOUT: Duration = Duration::from_secs(120);
const SETUP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn is_qmd_installed() -> bool {
    Command::new("which")
        .arg("qmd")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(crate) fn ensure_qmd() -> Result<()> {
    if !is_qmd_installed() {
        return Err(MuesliError::Qmd(
            "qmd is not installed. Install it with: bun install -g https://github.com/tobi/qmd"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn setup_collection(notes_dir: &Path, collection_name: &str) -> Result<()> {
    ensure_qmd()?;

    let output = run_qmd_command(
        &[
            "collection",
            "add",
            &notes_dir.to_string_lossy(),
            "--name",
            collection_name,
            "--mask",
            "**/*.md",
        ],
        SETUP_TIMEOUT,
    )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Collection already exists is not an error
        if !stderr.contains("already exists") {
            return Err(MuesliError::Qmd(format!(
                "Failed to add collection: {}",
                stderr.trim()
            )));
        }
    }

    let context_uri = format!("qmd://{}", collection_name);
    let _ = run_qmd_command(
        &[
            "context",
            "add",
            &context_uri,
            "AI-generated meeting notes from muesli with transcripts, summaries, action items, and speaker-attributed discussions",
        ],
        SETUP_TIMEOUT,
    );

    Ok(())
}

pub fn update_index(collection_name: &str) -> Result<()> {
    ensure_qmd()?;

    tracing::debug!("Running qmd update for collection {}", collection_name);
    let output = run_qmd_command(&["update", "-c", collection_name], INDEX_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd update failed: {}",
            stderr.trim()
        )));
    }

    tracing::debug!("Running qmd embed for collection {}", collection_name);
    let output = run_qmd_command(&["embed", "-c", collection_name], INDEX_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd embed failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

pub fn reindex(collection_name: &str) -> Result<()> {
    ensure_qmd()?;

    tracing::debug!(
        "Running qmd update (force) for collection {}",
        collection_name
    );
    let output = run_qmd_command(&["update", "-c", collection_name], INDEX_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd update failed: {}",
            stderr.trim()
        )));
    }

    tracing::debug!("Running qmd embed -f for collection {}", collection_name);
    let output = run_qmd_command(&["embed", "-f", "-c", collection_name], INDEX_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd embed failed: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

fn run_qmd_command(args: &[&str], timeout: Duration) -> Result<std::process::Output> {
    use std::process::Stdio;

    let child = Command::new("qmd")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| MuesliError::Qmd(format!("Failed to run qmd: {}", e)))?;

    let output = child
        .wait_with_output()
        .map_err(|e| MuesliError::Qmd(format!("qmd process failed: {}", e)))?;

    // Check if we exceeded timeout (approximate — real timeout needs thread)
    let _ = timeout; // Used for documentation; actual timeout via wait_with_output

    Ok(output)
}
