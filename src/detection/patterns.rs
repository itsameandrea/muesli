use crate::detection::MeetingApp;

/// Check if window class/title matches a meeting app
pub fn detect_meeting_app(class: &str, title: &str) -> Option<MeetingApp> {
    let class_lower = class.to_lowercase();
    let title_lower = title.to_lowercase();

    // Zoom
    if class_lower.contains("zoom") {
        return Some(MeetingApp::Zoom);
    }

    // Google Meet (browser-based)
    // Matches: "meet.google.com", "Google Meet", or active call "Meet – abc-xyz" in browser
    if title_lower.contains("meet.google.com")
        || (title_lower.contains("google meet") && !title_lower.contains("calendar"))
        || (is_browser(class)
            && (title_lower.starts_with("meet –") || title_lower.starts_with("meet -")))
    {
        return Some(MeetingApp::GoogleMeet);
    }

    // Microsoft Teams
    if class_lower.contains("teams")
        || class_lower.contains("microsoft teams")
        || title_lower.contains("microsoft teams")
    {
        return Some(MeetingApp::MicrosoftTeams);
    }

    // Slack (huddles/calls)
    if class_lower.contains("slack")
        && (title_lower.contains("huddle") || title_lower.contains("call"))
    {
        return Some(MeetingApp::Slack);
    }

    // Discord (voice channels)
    if class_lower.contains("discord")
        && (title_lower.contains("voice") || title_lower.contains("stage"))
    {
        return Some(MeetingApp::Discord);
    }

    // WebEx
    if class_lower.contains("webex") || title_lower.contains("webex") {
        return Some(MeetingApp::WebEx);
    }

    None
}

/// Common browser classes to check for web-based meetings
pub fn is_browser(class: &str) -> bool {
    let class_lower = class.to_lowercase();
    class_lower.contains("firefox")
        || class_lower.contains("chrome")
        || class_lower.contains("chromium")
        || class_lower.contains("brave")
        || class_lower.contains("edge")
        || class_lower.contains("safari")
        || class_lower.contains("zen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_zoom() {
        assert_eq!(
            detect_meeting_app("zoom", "Zoom Meeting"),
            Some(MeetingApp::Zoom)
        );
        assert_eq!(detect_meeting_app("Zoom", ""), Some(MeetingApp::Zoom));
    }

    #[test]
    fn test_detect_google_meet() {
        assert_eq!(
            detect_meeting_app("firefox", "meet.google.com - Firefox"),
            Some(MeetingApp::GoogleMeet)
        );
        assert_eq!(
            detect_meeting_app("chromium", "Meet – nga-fhgo-jph - Chromium"),
            Some(MeetingApp::GoogleMeet)
        );
        assert_eq!(
            detect_meeting_app("chrome", "Meet - abc-defg-hij - Google Chrome"),
            Some(MeetingApp::GoogleMeet)
        );
    }

    #[test]
    fn test_detect_teams() {
        assert_eq!(
            detect_meeting_app("teams", ""),
            Some(MeetingApp::MicrosoftTeams)
        );
        assert_eq!(
            detect_meeting_app("Microsoft Teams", "Chat"),
            Some(MeetingApp::MicrosoftTeams)
        );
    }

    #[test]
    fn test_detect_slack_huddle() {
        assert_eq!(
            detect_meeting_app("slack", "Huddle with Team"),
            Some(MeetingApp::Slack)
        );
        assert_eq!(detect_meeting_app("slack", "General Channel"), None);
    }

    #[test]
    fn test_detect_discord() {
        assert_eq!(
            detect_meeting_app("discord", "Voice Channel"),
            Some(MeetingApp::Discord)
        );
        assert_eq!(detect_meeting_app("discord", "Text Channel"), None);
    }

    #[test]
    fn test_no_meeting() {
        assert_eq!(detect_meeting_app("alacritty", "Terminal"), None);
        assert_eq!(detect_meeting_app("code", "VS Code"), None);
    }
}
