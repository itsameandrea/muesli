use crate::detection::MeetingApp;
use crate::error::Result;
use notify_rust::{Notification, Urgency};

/// Show a notification for meeting detection
pub fn notify_meeting_detected(app: MeetingApp, title: &str) -> Result<()> {
    Notification::new()
        .summary(&format!("{} Meeting Detected", app))
        .body(&format!("{}. Press hotkey to start recording.", title))
        .icon("video-display")
        .urgency(Urgency::Normal)
        .timeout(5000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

/// Show notification when recording starts
pub fn notify_recording_started(meeting_title: &str) -> Result<()> {
    Notification::new()
        .summary("Recording Started")
        .body(&format!("Recording: {}", meeting_title))
        .icon("media-record")
        .urgency(Urgency::Low)
        .timeout(3000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

/// Show notification when recording stops
pub fn notify_recording_stopped(meeting_title: &str, duration_mins: u64) -> Result<()> {
    Notification::new()
        .summary("Recording Stopped")
        .body(&format!(
            "{} - {} minutes. Processing...",
            meeting_title, duration_mins
        ))
        .icon("media-playback-stop")
        .urgency(Urgency::Normal)
        .timeout(5000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

#[allow(dead_code)]
pub fn notify_notes_ready(meeting_title: &str, notes_path: &str) -> Result<()> {
    Notification::new()
        .summary("Meeting Notes Ready")
        .body(&format!("{}\nSaved to: {}", meeting_title, notes_path))
        .icon("document-save")
        .urgency(Urgency::Normal)
        .timeout(10000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

#[allow(dead_code)]
pub fn notify_error(title: &str, message: &str) -> Result<()> {
    Notification::new()
        .summary(title)
        .body(message)
        .icon("dialog-error")
        .urgency(Urgency::Critical)
        .timeout(10000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

/// Show status notification (low priority)
pub fn notify_status(message: &str) -> Result<()> {
    Notification::new()
        .summary("Muesli")
        .body(message)
        .icon("dialog-information")
        .urgency(Urgency::Low)
        .timeout(3000)
        .show()
        .map_err(|e| crate::error::MuesliError::Notification(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_notify_meeting_detected() {
        let result = notify_meeting_detected(MeetingApp::Zoom, "Team Standup");
        assert!(result.is_ok());
    }

    #[test]
    #[ignore]
    fn test_notify_recording_started() {
        let result = notify_recording_started("Team Standup");
        assert!(result.is_ok());
    }

    #[test]
    #[ignore]
    fn test_notify_recording_stopped() {
        let result = notify_recording_stopped("Team Standup", 45);
        assert!(result.is_ok());
    }
}
