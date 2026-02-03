use crate::detection::{MeetingApp, WindowInfo};
use crate::detection::patterns::detect_meeting_app;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Meeting detection state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionState {
    Idle,
    Detected { app: MeetingApp, since: Instant },
    Recording { app: MeetingApp, since: Instant },
}

/// Events emitted by the detector
#[derive(Debug, Clone)]
pub enum DetectorEvent {
    MeetingDetected { app: MeetingApp, window: WindowInfo },
    MeetingEnded { app: MeetingApp },
    RecordingStarted { app: MeetingApp },
    RecordingStopped { app: MeetingApp },
}

/// Meeting detector with debouncing
pub struct MeetingDetector {
    state: DetectionState,
    last_window: Option<WindowInfo>,
    last_change: Instant,
    debounce_ms: u64,
    event_tx: mpsc::Sender<DetectorEvent>,
}

impl MeetingDetector {
    pub fn new(event_tx: mpsc::Sender<DetectorEvent>, debounce_ms: u64) -> Self {
        Self {
            state: DetectionState::Idle,
            last_window: None,
            last_change: Instant::now(),
            debounce_ms,
            event_tx,
        }
    }
    
    /// Process a window change event
    pub async fn on_window_change(&mut self, window: WindowInfo) {
        let now = Instant::now();
        
        // Debounce rapid switches
        if now.duration_since(self.last_change) < Duration::from_millis(self.debounce_ms) {
            if let Some(ref last) = self.last_window {
                if last.class == window.class && last.title == window.title {
                    return;
                }
            }
        }
        
        self.last_change = now;
        self.last_window = Some(window.clone());
        
        // Check if this is a meeting app
        let meeting_app = detect_meeting_app(&window.class, &window.title);
        
        match (&self.state, meeting_app) {
            // Idle → Detected
            (DetectionState::Idle, Some(app)) => {
                self.state = DetectionState::Detected { app, since: now };
                let _ = self.event_tx.send(DetectorEvent::MeetingDetected { 
                    app, 
                    window 
                }).await;
            }
            
            // Detected → Idle (switched away)
            (DetectionState::Detected { app, .. }, None) => {
                let app = *app;
                self.state = DetectionState::Idle;
                let _ = self.event_tx.send(DetectorEvent::MeetingEnded { app }).await;
            }
            
            // Recording → Recording (still in meeting, possibly different window)
            (DetectionState::Recording { app, since }, Some(new_app)) if *app == new_app => {
                // Same meeting, no change
                self.state = DetectionState::Recording { app: *app, since: *since };
            }
            
            // Recording → Idle (meeting ended)
            (DetectionState::Recording { app, .. }, None) => {
                let app = *app;
                self.state = DetectionState::Idle;
                let _ = self.event_tx.send(DetectorEvent::RecordingStopped { app }).await;
                let _ = self.event_tx.send(DetectorEvent::MeetingEnded { app }).await;
            }
            
            _ => {}
        }
    }
    
    /// Start recording (user confirmed)
    pub async fn start_recording(&mut self) {
        if let DetectionState::Detected { app, .. } = self.state {
            self.state = DetectionState::Recording { app, since: Instant::now() };
            let _ = self.event_tx.send(DetectorEvent::RecordingStarted { app }).await;
        }
    }
    
    /// Stop recording (user requested or meeting ended)
    pub async fn stop_recording(&mut self) {
        if let DetectionState::Recording { app, .. } = self.state {
            self.state = DetectionState::Detected { app, since: Instant::now() };
            let _ = self.event_tx.send(DetectorEvent::RecordingStopped { app }).await;
        }
    }
    
    /// Get current state
    pub fn state(&self) -> &DetectionState {
        &self.state
    }
    
    /// Check if currently in a meeting
    pub fn is_in_meeting(&self) -> bool {
        !matches!(self.state, DetectionState::Idle)
    }
    
    /// Check if currently recording
    pub fn is_recording(&self) -> bool {
        matches!(self.state, DetectionState::Recording { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_detect_meeting() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut detector = MeetingDetector::new(tx, 0); // No debounce for test
        
        let window = WindowInfo {
            class: "zoom".to_string(),
            title: "Zoom Meeting".to_string(),
            pid: None,
        };
        
        detector.on_window_change(window).await;
        
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DetectorEvent::MeetingDetected { app: MeetingApp::Zoom, .. }));
    }
    
    #[tokio::test]
    async fn test_meeting_ended() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut detector = MeetingDetector::new(tx, 0);
        
        // Enter meeting
        detector.on_window_change(WindowInfo {
            class: "zoom".to_string(),
            title: "Meeting".to_string(),
            pid: None,
        }).await;
        let _ = rx.recv().await; // Consume detected event
        
        // Leave meeting
        detector.on_window_change(WindowInfo {
            class: "alacritty".to_string(),
            title: "Terminal".to_string(),
            pid: None,
        }).await;
        
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DetectorEvent::MeetingEnded { app: MeetingApp::Zoom }));
    }
    
    #[tokio::test]
    async fn test_start_recording() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut detector = MeetingDetector::new(tx, 0);
        
        // Enter meeting
        detector.on_window_change(WindowInfo {
            class: "zoom".to_string(),
            title: "Meeting".to_string(),
            pid: None,
        }).await;
        let _ = rx.recv().await; // Consume detected event
        
        // Start recording
        detector.start_recording().await;
        
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DetectorEvent::RecordingStarted { app: MeetingApp::Zoom }));
        assert!(detector.is_recording());
    }
    
    #[tokio::test]
    async fn test_stop_recording() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut detector = MeetingDetector::new(tx, 0);
        
        // Enter meeting
        detector.on_window_change(WindowInfo {
            class: "zoom".to_string(),
            title: "Meeting".to_string(),
            pid: None,
        }).await;
        let _ = rx.recv().await; // Consume detected event
        
        // Start recording
        detector.start_recording().await;
        let _ = rx.recv().await; // Consume recording started event
        
        // Stop recording
        detector.stop_recording().await;
        
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, DetectorEvent::RecordingStopped { app: MeetingApp::Zoom }));
        assert!(!detector.is_recording());
    }
    
    #[tokio::test]
    async fn test_debounce() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut detector = MeetingDetector::new(tx, 500); // 500ms debounce
        
        let window = WindowInfo {
            class: "zoom".to_string(),
            title: "Meeting".to_string(),
            pid: None,
        };
        
        // First change
        detector.on_window_change(window.clone()).await;
        let _ = rx.recv().await; // Consume detected event
        
        // Rapid second change (same window) - should be debounced
        detector.on_window_change(window).await;
        
        // Should not receive another event
        assert!(rx.try_recv().is_err());
    }
}
