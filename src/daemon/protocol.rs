use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonRequest {
    StartRecording { title: Option<String> },
    StopRecording,
    GetStatus,
    Shutdown,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonResponse {
    Ok,
    Error { message: String },
    Status(DaemonStatus),
    Pong,
    RecordingStarted { meeting_id: String },
    RecordingStopped { meeting_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub recording: bool,
    pub current_meeting: Option<String>,
    pub current_meeting_id: Option<String>,
    pub meeting_detected: Option<String>,
    pub uptime_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let request = DaemonRequest::StartRecording {
            title: Some("Test Meeting".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: DaemonRequest = serde_json::from_str(&json).unwrap();

        match parsed {
            DaemonRequest::StartRecording { title } => {
                assert_eq!(title, Some("Test Meeting".to_string()));
            }
            _ => panic!("Wrong request type"),
        }
    }

    #[test]
    fn test_response_serialization() {
        let response = DaemonResponse::RecordingStarted {
            meeting_id: "test-123".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: DaemonResponse = serde_json::from_str(&json).unwrap();

        match parsed {
            DaemonResponse::RecordingStarted { meeting_id } => {
                assert_eq!(meeting_id, "test-123");
            }
            _ => panic!("Wrong response type"),
        }
    }

    #[test]
    fn test_status_serialization() {
        let status = DaemonStatus {
            running: true,
            recording: false,
            current_meeting: None,
            current_meeting_id: None,
            meeting_detected: Some("Zoom".to_string()),
            uptime_seconds: 3600,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: DaemonStatus = serde_json::from_str(&json).unwrap();

        assert!(parsed.running);
        assert!(!parsed.recording);
        assert_eq!(parsed.meeting_detected, Some("Zoom".to_string()));
        assert_eq!(parsed.uptime_seconds, 3600);
    }

    #[test]
    fn test_all_request_variants() {
        let requests = vec![
            DaemonRequest::StartRecording { title: None },
            DaemonRequest::StopRecording,
            DaemonRequest::GetStatus,
            DaemonRequest::Shutdown,
            DaemonRequest::Ping,
        ];

        for request in requests {
            let json = serde_json::to_string(&request).unwrap();
            let _: DaemonRequest = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_all_response_variants() {
        let responses = vec![
            DaemonResponse::Ok,
            DaemonResponse::Error {
                message: "test".to_string(),
            },
            DaemonResponse::Status(DaemonStatus {
                running: true,
                recording: false,
                current_meeting: None,
                current_meeting_id: None,
                meeting_detected: None,
                uptime_seconds: 0,
            }),
            DaemonResponse::Pong,
            DaemonResponse::RecordingStarted {
                meeting_id: "123".to_string(),
            },
            DaemonResponse::RecordingStopped {
                meeting_id: "123".to_string(),
            },
        ];

        for response in responses {
            let json = serde_json::to_string(&response).unwrap();
            let _: DaemonResponse = serde_json::from_str(&json).unwrap();
        }
    }
}
