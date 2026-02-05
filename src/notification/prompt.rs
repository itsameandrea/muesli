use crate::detection::MeetingApp;
use notify_rust::{Hint, Notification, Timeout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptResponse {
    Record,
    Skip,
    Closed,
}

pub fn prompt_meeting_detected(
    app: MeetingApp,
    window_title: &str,
    timeout_secs: u64,
) -> PromptResponse {
    let timeout = if timeout_secs == 0 {
        Timeout::Never
    } else {
        Timeout::Milliseconds((timeout_secs * 1000) as u32)
    };

    let notification = Notification::new()
        .summary(&format!("{} Detected - Click to Record", app))
        .body(&format!(
            "{}\n\nClick this notification to start recording, or dismiss to skip.",
            window_title
        ))
        .icon("video-display")
        .action("default", "Start Recording")
        .hint(Hint::Transient(true))
        .timeout(timeout)
        .show();

    match notification {
        Ok(handle) => {
            let mut response = PromptResponse::Closed;
            handle.wait_for_action(|action| {
                tracing::debug!("Notification action received: {}", action);
                response = match action {
                    "default" | "record" => PromptResponse::Record,
                    "skip" => PromptResponse::Skip,
                    _ => PromptResponse::Closed,
                };
            });
            response
        }
        Err(e) => {
            tracing::error!("Failed to show notification prompt: {}", e);
            PromptResponse::Closed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_response_variants() {
        assert_ne!(PromptResponse::Record, PromptResponse::Skip);
        assert_ne!(PromptResponse::Skip, PromptResponse::Closed);
    }
}
