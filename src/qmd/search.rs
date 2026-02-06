use std::process::Command;
use std::time::Duration;

use crate::error::{MuesliError, Result};
use crate::qmd::indexer::ensure_qmd;

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

pub fn search(
    query: &str,
    collection_name: &str,
    limit: usize,
    keyword_only: bool,
) -> Result<String> {
    ensure_qmd()?;

    let subcommand = if keyword_only { "search" } else { "query" };
    let limit_str = limit.to_string();
    let args = vec![subcommand, query, "-c", collection_name, "-n", &limit_str];

    let output = run_qmd(args, QUERY_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd {} failed: {}",
            subcommand,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

pub fn status() -> Result<String> {
    ensure_qmd()?;

    let output = run_qmd(vec!["collection", "list"], QUERY_TIMEOUT)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MuesliError::Qmd(format!(
            "qmd status failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

fn run_qmd(args: Vec<&str>, _timeout: Duration) -> Result<std::process::Output> {
    use std::process::Stdio;

    let child = Command::new("qmd")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| MuesliError::Qmd(format!("Failed to run qmd: {}", e)))?;

    child
        .wait_with_output()
        .map_err(|e| MuesliError::Qmd(format!("qmd process failed: {}", e)))
}
