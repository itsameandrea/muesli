use serde::{Deserialize, Serialize};

pub mod hyprland;
pub mod patterns;
pub mod detector;

/// Detected meeting application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MeetingApp {
    Zoom,
    GoogleMeet,
    MicrosoftTeams,
    Slack,
    Discord,
    WebEx,
    Unknown,
}

impl std::fmt::Display for MeetingApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetingApp::Zoom => write!(f, "Zoom"),
            MeetingApp::GoogleMeet => write!(f, "Google Meet"),
            MeetingApp::MicrosoftTeams => write!(f, "Microsoft Teams"),
            MeetingApp::Slack => write!(f, "Slack"),
            MeetingApp::Discord => write!(f, "Discord"),
            MeetingApp::WebEx => write!(f, "WebEx"),
            MeetingApp::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Window information from Hyprland
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub class: String,
    pub title: String,
    pub pid: Option<i32>,
}

/// Meeting detection event
#[derive(Debug, Clone)]
pub enum DetectionEvent {
    MeetingDetected {
        app: MeetingApp,
        window: WindowInfo,
    },
    MeetingEnded {
        app: MeetingApp,
    },
    WindowChanged {
        window: WindowInfo,
    },
}
