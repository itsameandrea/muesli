use anyhow::{Context, Result};
use std::io::Read;
use std::process::{Command, Stdio};
use tokio::task;

pub async fn summarize_with_local(
    lms_path: &str,
    model: &str,
    prompt: &str,
) -> Result<String> {
    let lms_binary = if lms_path.is_empty() {
        find_lms_binary().context("Could not find lms CLI. Install LM Studio or set local_lms_path in config.")?
    } else {
        lms_path.to_string()
    };

    tracing::info!("Running LM Studio CLI: {} chat {} (streaming)", lms_binary, model);

    let lms_binary_clone = lms_binary.clone();
    let model_clone = model.to_string();
    let prompt_clone = prompt.to_string();

    let result = task::spawn_blocking(move || {
        run_lms_streaming(&lms_binary_clone, &model_clone, &prompt_clone)
    })
    .await
    .context("Task join error")??;

    Ok(result)
}

fn run_lms_streaming(lms_binary: &str, model: &str, prompt: &str) -> Result<String> {
    let mut child = Command::new(lms_binary)
        .args(["chat", model, "-p", prompt, "-y"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn lms process. Is LM Studio installed?")?;

    let mut stdout = child.stdout.take().context("Failed to capture stdout")?;
    let mut output = String::new();
    let mut buffer = [0u8; 4096];
    let mut total_chars = 0;

    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = String::from_utf8_lossy(&buffer[..n]);
                output.push_str(&chunk);
                total_chars += chunk.len();
                
                if total_chars % 1000 < chunk.len() {
                    tracing::debug!("LLM streaming: {} chars", total_chars);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::warn!("Error reading stdout: {}", e);
                break;
            }
        }
    }

    let status = child.wait().context("Failed to wait for lms process")?;

    if !status.success() {
        let stderr = child.stderr.take()
            .map(|mut s| {
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                buf
            })
            .unwrap_or_default();
        anyhow::bail!("lms exited with status {}: {}", status, stderr);
    }

    let cleaned = clean_terminal_escapes(&output);
    Ok(cleaned.trim().to_string())
}

fn clean_terminal_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&'[') = chars.peek() {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c == '\r' || c == '\x00' {
            continue;
        } else {
            result.push(c);
        }
    }
    
    result
}

fn find_lms_binary() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let lmstudio_path = format!("{}/.lmstudio/bin/lms", home);
    
    if std::path::Path::new(&lmstudio_path).exists() {
        return Some(lmstudio_path);
    }

    if let Ok(output) = Command::new("which").arg("lms").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_lms_binary() {
        let result = find_lms_binary();
        println!("LMS binary: {:?}", result);
    }

    #[test]
    fn test_clean_terminal_escapes() {
        let input = "\x1b[KHello World\r\n";
        let cleaned = clean_terminal_escapes(input);
        assert_eq!(cleaned, "Hello World\n");
        
        let input2 = "\x1b[?25hTest";
        let cleaned2 = clean_terminal_escapes(input2);
        assert_eq!(cleaned2, "Test");
    }
}
