use crate::audio::capture::MicCapture;
use crate::audio::loopback::LoopbackCapture;
use cpal::Stream;
use crate::audio::mixer::mix_streams;
use crate::audio::recorder::WavRecorder;
use crate::audio::AudioChunk;
use crate::config::loader::{database_path, models_dir, recordings_dir, socket_path};
use crate::daemon::protocol::{DaemonRequest, DaemonResponse, DaemonStatus};
use crate::detection::hyprland::{is_hyprland_running, HyprlandMonitor};
use crate::detection::{DetectionEvent, MeetingApp};
use crate::error::{MuesliError, Result};
use crate::notification;
use crate::storage::database::Database;
use crate::storage::Meeting;
use crate::transcription::parakeet_models::{ParakeetModel, ParakeetModelManager};
use crate::transcription::streaming::StreamingTranscriber;
use crate::transcription::TranscriptSegment;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, broadcast, Mutex};

#[allow(dead_code)]
pub struct DaemonState {
    pub recording: bool,
    pub current_meeting: Option<Meeting>,
    pub meeting_detected: Option<MeetingApp>,
    pub prompt_active: bool,
    pub start_time: Instant,
    pub audio_running: Option<Arc<AtomicBool>>,
    pub audio_path: Option<PathBuf>,
    pub transcript_segments: Vec<TranscriptSegment>,
    pub streaming_enabled: bool,
    pub segment_rx: Option<std::sync::mpsc::Receiver<Vec<TranscriptSegment>>>,
    pub meeting_monitor_running: Option<Arc<AtomicBool>>,
    pub detection_tx: Option<mpsc::Sender<DetectionEvent>>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            recording: false,
            current_meeting: None,
            meeting_detected: None,
            prompt_active: false,
            start_time: Instant::now(),
            audio_running: None,
            audio_path: None,
            transcript_segments: Vec::new(),
            streaming_enabled: false,
            segment_rx: None,
            meeting_monitor_running: None,
            detection_tx: None,
        }
    }
}

pub async fn run_daemon() -> Result<()> {
    let socket = socket_path()?;

    if socket.exists() {
        std::fs::remove_file(&socket)?;
    }

    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket).map_err(MuesliError::Io)?;

    tracing::info!("Daemon listening on {:?}", socket);

    let state = Arc::new(Mutex::new(DaemonState::default()));
    let shutdown = Arc::new(AtomicBool::new(false));

    let (detection_tx, mut detection_rx) = mpsc::channel::<DetectionEvent>(100);
    
    {
        let mut s = state.lock().await;
        s.detection_tx = Some(detection_tx.clone());
    }
    
    let state_for_detection = state.clone();
    let detection_tx_for_handler = detection_tx.clone();

    tokio::spawn(async move {
        while let Some(event) = detection_rx.recv().await {
            match event {
                DetectionEvent::MeetingDetected { app, window } => {
                    {
                        let mut state = state_for_detection.lock().await;
                        state.meeting_detected = Some(app);
                    }

                    let config = crate::config::loader::load_config();
                    let auto_prompt = config
                        .as_ref()
                        .map(|c| c.detection.auto_prompt)
                        .unwrap_or(false);

                    if auto_prompt {
                        let state_clone = state_for_detection.clone();
                        let window_title = window.title.clone();
                        let timeout = config
                            .map(|c| c.detection.prompt_timeout_secs)
                            .unwrap_or(30);
                        let tx_for_monitor = detection_tx_for_handler.clone();

                        tokio::spawn(async move {
                            let title_for_prompt = window_title.clone();
                            let response = tokio::task::spawn_blocking(move || {
                                notification::prompt_meeting_detected(app, &title_for_prompt, timeout)
                            })
                            .await;

                            match response {
                                Ok(notification::PromptResponse::Record) => {
                                    tracing::info!("User clicked Record, starting recording...");
                                    let mut state = state_clone.lock().await;
                                    if !state.recording {
                                        let title = "Untitled Meeting".to_string();
                                        match start_recording_internal(&mut state, title).await {
                                            Ok(id) => {
                                                tracing::info!("Recording started: {}", id);
                                                state.meeting_monitor_running = Some(
                                                    start_meeting_window_monitor(app, tx_for_monitor)
                                                );
                                            }
                                            Err(e) => tracing::error!("Failed to auto-start recording: {}", e),
                                        }
                                    } else {
                                        tracing::info!("Already recording, skipping");
                                    }
                                }
                                Ok(notification::PromptResponse::Skip) => {
                                    tracing::info!("User clicked Skip");
                                }
                                Ok(notification::PromptResponse::Closed) => {
                                    tracing::debug!("Notification was closed/timed out");
                                }
                                Err(e) => {
                                    tracing::error!("Prompt task failed: {:?}", e);
                                }
                            }
                        });
                    } else {
                        let _ = notification::notify_meeting_detected(app, &window.title);
                    }
                }
                DetectionEvent::MeetingEnded { app } => {
                    let mut state = state_for_detection.lock().await;
                    if state.meeting_detected == Some(app) {
                        state.meeting_detected = None;
                    }
                }
                DetectionEvent::WindowChanged { window } => {
                    tracing::debug!("Window changed: class={}, title={}", window.class, window.title);
                    let detected_app = crate::detection::patterns::detect_meeting_app(&window.class, &window.title);
                    
                    let (current_detected, is_recording, prompt_active) = {
                        let state = state_for_detection.lock().await;
                        (state.meeting_detected, state.recording, state.prompt_active)
                    };

                    if let Some(app) = detected_app {
                        tracing::debug!("Meeting app detected: {}", app);
                        
                        if is_recording || prompt_active {
                            continue;
                        }
                        
                        if current_detected != Some(app) {
                            tracing::info!("New meeting detected ({}), showing prompt", app);
                            {
                                let mut state = state_for_detection.lock().await;
                                state.meeting_detected = Some(app);
                                state.prompt_active = true;
                            }

                            let config = crate::config::loader::load_config();
                            let auto_prompt = config
                                .as_ref()
                                .map(|c| c.detection.auto_prompt)
                                .unwrap_or(false);

                            if auto_prompt {
                                let state_clone = state_for_detection.clone();
                                let window_title = window.title.clone();
                                let timeout = config
                                    .map(|c| c.detection.prompt_timeout_secs)
                                    .unwrap_or(30);
                                let tx_for_monitor = detection_tx_for_handler.clone();

                                tokio::spawn(async move {
                                    let title_for_prompt = window_title.clone();
                                    let response = tokio::task::spawn_blocking(move || {
                                        notification::prompt_meeting_detected(app, &title_for_prompt, timeout)
                                    })
                                    .await;

                                    let mut state = state_clone.lock().await;
                                    state.prompt_active = false;
                                    
                                    if let Ok(notification::PromptResponse::Record) = response {
                                        if !state.recording {
                                            let title = "Untitled Meeting".to_string();
                                            match start_recording_internal(&mut state, title).await {
                                                Ok(_) => {
                                                    state.meeting_monitor_running = Some(
                                                        start_meeting_window_monitor(app, tx_for_monitor)
                                                    );
                                                }
                                                Err(e) => tracing::error!("Failed to auto-start recording: {}", e),
                                            }
                                        }
                                    }
                                });
                            } else {
                                let _ = notification::notify_meeting_detected(app, &window.title);
                            }
                        }
                    }
                }
                DetectionEvent::MeetingWindowClosed { app } => {
                    tracing::info!("Meeting window closed ({}), checking if should stop recording", app);
                    let mut state = state_for_detection.lock().await;
                    if state.recording && state.meeting_detected == Some(app) {
                        tracing::info!("Auto-stopping recording due to meeting window closure");
                        if let Some(running) = state.meeting_monitor_running.take() {
                            running.store(false, Ordering::Relaxed);
                        }
                        let meeting_id = state.current_meeting.as_ref()
                            .map(|m| m.id.to_string())
                            .unwrap_or_default();
                        
                        let audio_path = state.audio_path.clone();
                        let audio_running = state.audio_running.take();
                        let segment_rx = state.segment_rx.take();
                        let streaming_enabled = state.streaming_enabled;

                        if let Some(running) = audio_running {
                            running.store(false, Ordering::Relaxed);
                        }

                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                        let segments = if let Some(rx) = segment_rx {
                            rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or_default()
                        } else {
                            Vec::new()
                        };

                        if let Some(meeting) = &mut state.current_meeting {
                            let ended = chrono::Utc::now();
                            meeting.ended_at = Some(ended);
                            let duration_secs = (ended.timestamp() - meeting.started_at.timestamp()) as u64;
                            meeting.duration_seconds = Some(duration_secs);
                            meeting.status = crate::storage::MeetingStatus::Processing;

                            if let Some(ref path) = audio_path {
                                meeting.audio_path = Some(path.clone());
                            }

                            if let Ok(db_path) = database_path() {
                                if let Ok(db) = Database::open(&db_path) {
                                    let _ = db.update_meeting(meeting);
                                    if !segments.is_empty() {
                                        let _ = db.insert_transcript_segments(&meeting.id, &segments);
                                    }
                                }
                            }

                            let duration_mins = meeting.duration_seconds.unwrap_or(0) / 60;
                            let meeting_title = meeting.title.clone();
                            let meeting_id_clone = meeting_id.clone();
                            
                            let _ = notification::notify_recording_stopped(&meeting_title, duration_mins);
                            
                            if let Some(path) = audio_path {
                                if streaming_enabled && !segments.is_empty() {
                                    std::thread::spawn(move || {
                                        run_background_diarization(meeting_id_clone, path);
                                    });
                                } else {
                                    std::thread::spawn(move || {
                                        run_background_transcription_and_diarization(meeting_id_clone, path);
                                    });
                                }
                            }
                        }

                        state.recording = false;
                        state.current_meeting = None;
                        state.audio_path = None;
                        state.streaming_enabled = false;
                        state.meeting_detected = None;
                    }
                }
            }
        }
    });

    if is_hyprland_running() {
        tracing::info!("Hyprland detected, starting window monitor");
        let poll_interval = crate::config::loader::load_config()
            .map(|c| c.detection.poll_interval_secs)
            .unwrap_or(5);
        let monitor = HyprlandMonitor::with_poll_interval(detection_tx, poll_interval);
        tokio::spawn(async move {
            tracing::info!("Hyprland monitor task started");
            if let Err(e) = monitor.start_monitoring().await {
                tracing::error!("Hyprland monitoring error: {}", e);
            }
            tracing::warn!("Hyprland monitor task ended");
        });
    } else {
        tracing::warn!("Hyprland not detected, window monitoring disabled");
    }

    while !shutdown.load(Ordering::Relaxed) {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let state = state.clone();
                        let shutdown = shutdown.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, state, shutdown).await {
                                tracing::error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {}", e);
                    }
                }
            }
        }
    }

    let _ = std::fs::remove_file(&socket);
    tracing::info!("Daemon shutdown complete");

    Ok(())
}

async fn handle_connection(
    stream: UnixStream,
    state: Arc<Mutex<DaemonState>>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: DaemonRequest = serde_json::from_str(&line)
            .map_err(|e| MuesliError::Config(format!("Invalid request: {}", e)))?;

        let response = handle_request(request, &state, &shutdown).await;

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

async fn handle_request(
    request: DaemonRequest,
    state: &Arc<Mutex<DaemonState>>,
    shutdown: &Arc<AtomicBool>,
) -> DaemonResponse {
    match request {
        DaemonRequest::Ping => DaemonResponse::Pong,

        DaemonRequest::GetStatus => {
            let state = state.lock().await;
            DaemonResponse::Status(DaemonStatus {
                running: true,
                recording: state.recording,
                current_meeting: state.current_meeting.as_ref().map(|m| m.title.clone()),
                current_meeting_id: state.current_meeting.as_ref().map(|m| m.id.to_string()),
                meeting_detected: state.meeting_detected.map(|app| app.to_string()),
                uptime_seconds: state.start_time.elapsed().as_secs(),
            })
        }

        DaemonRequest::StartRecording { title } => {
            let mut state = state.lock().await;
            let title = title.unwrap_or_else(|| "Untitled Meeting".to_string());

            match start_recording_internal(&mut state, title).await {
                Ok(meeting_id) => {
                    tracing::info!("Recording started, checking for meeting windows. meeting_detected={:?}", state.meeting_detected);
                    let detected_app = state.meeting_detected.or_else(|| {
                        find_any_meeting_window()
                    });
                    
                    if let Some(app) = detected_app {
                        state.meeting_detected = Some(app);
                        if let Some(tx) = state.detection_tx.clone() {
                            tracing::info!("Starting meeting window monitor for manual recording (detected: {})", app);
                            state.meeting_monitor_running = Some(start_meeting_window_monitor(app, tx));
                        } else {
                            tracing::warn!("No detection_tx available, cannot start monitor");
                        }
                    } else {
                        tracing::info!("No meeting window detected, monitor not started");
                    }
                    DaemonResponse::RecordingStarted { meeting_id }
                }
                Err(MuesliError::AlreadyRecording) => DaemonResponse::Error {
                    message: "Already recording".to_string(),
                },
                Err(e) => DaemonResponse::Error {
                    message: format!("Failed to start recording: {}", e),
                },
            }
        }

        DaemonRequest::StopRecording => {
            let mut state = state.lock().await;
            if !state.recording {
                return DaemonResponse::Error {
                    message: "Not recording".to_string(),
                };
            }

            let meeting_id = state
                .current_meeting
                .as_ref()
                .map(|m| m.id.to_string())
                .unwrap_or_default();
            
            let meeting_id_clone = meeting_id.clone();

            let audio_path = state.audio_path.clone();
            let audio_running = state.audio_running.take();
            let segment_rx = state.segment_rx.take();
            let streaming_enabled = state.streaming_enabled;

            if let Some(running) = state.meeting_monitor_running.take() {
                running.store(false, Ordering::Relaxed);
            }

            if let Some(running) = audio_running {
                running.store(false, Ordering::Relaxed);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let segments = if let Some(rx) = segment_rx {
                match rx.recv_timeout(std::time::Duration::from_secs(5)) {
                    Ok(segs) => {
                        tracing::info!("Received {} segments from streaming transcriber", segs.len());
                        segs
                    }
                    Err(_) => {
                        tracing::warn!("Timeout waiting for streaming segments");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

            if let Some(meeting) = &mut state.current_meeting {
                let ended = chrono::Utc::now();
                meeting.ended_at = Some(ended);
                let duration_secs = (ended.timestamp() - meeting.started_at.timestamp()) as u64;
                meeting.duration_seconds = Some(duration_secs);
                meeting.status = crate::storage::MeetingStatus::Processing;

                if let Some(ref path) = audio_path {
                    meeting.audio_path = Some(path.clone());
                }

                if let Ok(db_path) = database_path() {
                    if let Ok(db) = Database::open(&db_path) {
                        if let Err(e) = db.update_meeting(meeting) {
                            tracing::error!("Failed to update meeting in database: {}", e);
                        }
                        
                        if !segments.is_empty() {
                            if let Err(e) = db.insert_transcript_segments(&meeting.id, &segments) {
                                tracing::error!("Failed to save transcript segments: {}", e);
                            } else {
                                tracing::info!("Saved {} transcript segments", segments.len());
                            }
                        }
                    }
                }

                let duration_mins = meeting.duration_seconds.unwrap_or(0) / 60;
                let meeting_title = meeting.title.clone();
                
                if streaming_enabled && !segments.is_empty() {
                    let _ = notification::notify_recording_stopped(&meeting_title, duration_mins);
                    
                    if let Some(path) = audio_path {
                        std::thread::spawn(move || {
                            run_background_diarization(meeting_id_clone, path);
                        });
                    }
                } else {
                    let _ = notification::notify_recording_stopped(&meeting_title, duration_mins);
                    
                    if let Some(path) = audio_path {
                        std::thread::spawn(move || {
                            run_background_transcription_and_diarization(meeting_id_clone, path);
                        });
                    }
                }
            }

            state.recording = false;
            state.current_meeting = None;
            state.audio_path = None;
            state.streaming_enabled = false;

            DaemonResponse::RecordingStopped { meeting_id }
        }

        DaemonRequest::Shutdown => {
            shutdown.store(true, Ordering::Relaxed);
            DaemonResponse::Ok
        }
    }
}

async fn start_recording_internal(state: &mut DaemonState, title: String) -> Result<String> {
    if state.recording {
        return Err(MuesliError::AlreadyRecording);
    }

    let mut meeting = Meeting::new(title.clone());
    let meeting_id = meeting.id.to_string();

    let audio_path = setup_recording_path(&meeting_id).await?;
    meeting.audio_path = Some(audio_path.clone());

    start_audio_recording(state, audio_path.clone()).await?;
    tracing::info!("Audio recording started for meeting {}", meeting_id);

    if let Ok(db_path) = database_path() {
        if let Ok(db) = Database::open(&db_path) {
            if let Err(e) = db.insert_meeting(&meeting) {
                tracing::error!("Failed to save meeting to database: {}", e);
            }
        }
    }

    state.recording = true;
    state.current_meeting = Some(meeting);

    let _ = notification::notify_recording_started(&title);

    Ok(meeting_id)
}

async fn setup_recording_path(meeting_id: &str) -> Result<PathBuf> {
    let recordings_dir = recordings_dir()?;
    tokio::fs::create_dir_all(&recordings_dir).await?;
    Ok(recordings_dir.join(format!("{}.wav", meeting_id)))
}

async fn start_audio_recording(
    state: &mut DaemonState,
    audio_path: PathBuf,
) -> Result<()> {
    let audio_running = Arc::new(AtomicBool::new(true));
    let audio_running_task = audio_running.clone();
    let audio_path_task = audio_path.clone();
    
    let nemotron_model_dir = check_nemotron_model();
    let streaming_enabled = nemotron_model_dir.is_some();
    
    if streaming_enabled {
        tracing::info!("Streaming transcription enabled (Nemotron model found)");
    } else {
        tracing::info!("Streaming transcription disabled (Nemotron model not found, will transcribe at end)");
    }

    let (segment_tx, segment_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(async move {
            let segments = run_recording_task(audio_path_task, audio_running_task, nemotron_model_dir).await;
            let _ = segment_tx.send(segments);
        });
    });

    state.audio_running = Some(audio_running);
    state.audio_path = Some(audio_path);
    state.streaming_enabled = streaming_enabled;
    state.segment_rx = Some(segment_rx);

    Ok(())
}

fn check_nemotron_model() -> Option<PathBuf> {
    let models_dir = models_dir().ok()?;
    let manager = ParakeetModelManager::new(models_dir);
    
    if manager.model_exists(ParakeetModel::NemotronStreaming) {
        Some(manager.model_dir(ParakeetModel::NemotronStreaming))
    } else {
        None
    }
}

async fn run_recording_task(
    audio_path: PathBuf, 
    is_running: Arc<AtomicBool>,
    nemotron_model_dir: Option<PathBuf>,
) -> Vec<TranscriptSegment> {
    let mut recorder = match WavRecorder::new(&audio_path) {
        Ok(rec) => rec,
        Err(e) => {
            tracing::error!("Failed to create WAV recorder: {}", e);
            return Vec::new();
        }
    };

    let transcriber = nemotron_model_dir.and_then(|model_dir| {
        match StreamingTranscriber::new(&model_dir) {
            Ok(t) => {
                tracing::info!("Streaming transcriber initialized");
                Some(t)
            }
            Err(e) => {
                tracing::warn!("Failed to create streaming transcriber: {}. Will transcribe at end.", e);
                None
            }
        }
    });

    let mic_capture = match MicCapture::from_default() {
        Ok(capture) => capture,
        Err(e) => {
            tracing::error!("Failed to initialize microphone: {}", e);
            return Vec::new();
        }
    };

    let (mic_stream, mic_rx) = match mic_capture.start(is_running.clone()) {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to start microphone: {}", e);
            return Vec::new();
        }
    };

    let loopback_capture_result = LoopbackCapture::find_monitor();

    let (loopback_stream_opt, loopback_rx_opt): (
        Option<Stream>,
        Option<broadcast::Receiver<AudioChunk>>,
    ) = if let Ok(loopback_capture) = loopback_capture_result {
        match loopback_capture.start(is_running.clone()) {
            Ok((stream, rx)) => {
                tracing::info!("Loopback capture started successfully");
                (Some(stream), Some(rx))
            }
            Err(e) => {
                tracing::warn!("Failed to start loopback capture: {}. Recording microphone only.", e);
                (None, None)
            }
        }
    } else {
        tracing::info!("No loopback device available. Recording microphone only.");
        (None, None)
    };

    let (mixed_tx, mut mixed_rx) = broadcast::channel::<AudioChunk>(100);

    if let Some(loopback_rx) = loopback_rx_opt {
        let _mixer_handle = tokio::spawn(async move {
            mix_streams(mic_rx, loopback_rx, mixed_tx, 16000, 1).await;
        });

        loop {
            if !is_running.load(Ordering::Relaxed) {
                break;
            }

            match tokio::time::timeout(tokio::time::Duration::from_millis(100), mixed_rx.recv()).await {
                Ok(Ok(chunk)) => {
                    if let Err(e) = recorder.write_chunk(&chunk) {
                        tracing::error!("Failed to write audio chunk: {}", e);
                    }
                    if let Some(ref t) = transcriber {
                        let _ = t.feed_samples(&chunk.samples);
                    }
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Err(_) => continue,
            }
        }
    } else {
        let _mic_handle = tokio::spawn(async move {
            mic_only_task(mic_rx, mixed_tx).await;
        });

        loop {
            if !is_running.load(Ordering::Relaxed) {
                break;
            }

            match tokio::time::timeout(tokio::time::Duration::from_millis(100), mixed_rx.recv()).await {
                Ok(Ok(chunk)) => {
                    if let Err(e) = recorder.write_chunk(&chunk) {
                        tracing::error!("Failed to write audio chunk: {}", e);
                    }
                    if let Some(ref t) = transcriber {
                        let _ = t.feed_samples(&chunk.samples);
                    }
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => break,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Err(_) => continue,
            }
        }
    }

    drop(mic_stream);
    drop(loopback_stream_opt);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    while let Ok(chunk) = mixed_rx.recv().await {
        if let Err(e) = recorder.write_chunk(&chunk) {
            tracing::error!("Failed to write audio chunk: {}", e);
        }
        if let Some(ref t) = transcriber {
            let _ = t.feed_samples(&chunk.samples);
        }
    }

    match recorder.finalize() {
        Ok(path) => {
            tracing::info!("Recording finalized: {:?}", path);
        }
        Err(e) => {
            tracing::error!("Failed to finalize recording: {}", e);
        }
    }

    if let Some(t) = transcriber {
        let _ = t.flush();
        match t.stop() {
            Ok(segments) => {
                tracing::info!("Streaming transcription complete: {} segments", segments.len());
                return segments;
            }
            Err(e) => {
                tracing::error!("Failed to stop transcriber: {}", e);
            }
        }
    }

    Vec::new()
}

async fn mic_only_task(
    mut mic_rx: broadcast::Receiver<AudioChunk>,
    output_tx: broadcast::Sender<AudioChunk>,
) {
    loop {
        match mic_rx.recv().await {
            Ok(chunk) => {
                let _ = output_tx.send(chunk);
            }
            Err(broadcast::error::RecvError::Closed) => break,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
}

fn run_background_diarization(meeting_id: String, audio_path: PathBuf) {
    tracing::info!("Starting background diarization for meeting {}", meeting_id);
    
    let models_dir = match models_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("Failed to get models dir: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    let diar_manager = crate::transcription::diarization_models::DiarizationModelManager::new(models_dir);
    let diar_model = crate::transcription::diarization_models::DiarizationModel::SortformerV2;
    
    if !diar_manager.model_exists(diar_model) {
        tracing::warn!("Diarization model not found, skipping diarization");
        mark_meeting_complete(&meeting_id);
        return;
    }
    
    let samples = match load_wav_samples(&audio_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to load audio for diarization: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    let model_path = diar_manager.model_path(diar_model);
    let mut diarizer = match crate::transcription::diarization::Diarizer::new(&model_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to create diarizer: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    let speaker_segments = match diarizer.diarize(samples, 16000) {
        Ok(segs) => segs,
        Err(e) => {
            tracing::error!("Diarization failed: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    tracing::info!("Diarization complete: {} speaker segments", speaker_segments.len());
    
    if let Ok(db_path) = database_path() {
        if let Ok(db) = Database::open(&db_path) {
            let meeting_id_obj = crate::storage::MeetingId::from_string(meeting_id.clone());
            if let Ok(mut segments) = db.get_transcript_segments(&meeting_id_obj) {
                for seg in segments.iter_mut() {
                    if let Some(speaker) = speaker_segments.iter()
                        .find(|s| {
                            let mid = (seg.start_ms + seg.end_ms) / 2;
                            mid >= s.start_ms && mid <= s.end_ms
                        })
                        .map(|s| format!("SPEAKER_{}", s.speaker_id + 1))
                    {
                        seg.speaker = Some(speaker);
                    }
                }
                
                let _ = db.delete_transcript_segments(&meeting_id_obj);
                let _ = db.insert_transcript_segments(&meeting_id_obj, &segments);
                tracing::info!("Updated {} segments with speaker labels", segments.len());
            }
        }
    }
    
    run_background_summarization(meeting_id.clone());
    
    mark_meeting_complete(&meeting_id);
    let _ = notification::notify_status(&format!("Processing complete for meeting"));
}

fn run_background_summarization(meeting_id: String) {
    let cfg = match crate::config::loader::load_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to load config for summarization: {}", e);
            return;
        }
    };

    if cfg.llm.engine == "none" {
        tracing::debug!("LLM engine is 'none', skipping summarization");
        return;
    }

    tracing::info!("Starting background summarization for meeting {}", meeting_id);

    let db_path = match database_path() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to get database path: {}", e);
            return;
        }
    };

    let db = match Database::open(&db_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to open database: {}", e);
            return;
        }
    };

    let meeting_id_obj = crate::storage::MeetingId::from_string(meeting_id.clone());
    let segments = match db.get_transcript_segments(&meeting_id_obj) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get transcript segments: {}", e);
            return;
        }
    };

    if segments.is_empty() {
        tracing::warn!("No transcript segments for summarization");
        return;
    }

    let transcript = crate::transcription::Transcript::new(segments);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();

    let rt = match rt {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create tokio runtime: {}", e);
            return;
        }
    };

    let result = rt.block_on(crate::llm::summarize_transcript(&cfg.llm, &transcript));

    match result {
        Ok(summary) => {
            tracing::info!("Summarization complete: {} chars", summary.markdown.len());

            if let Err(e) = db.insert_summary(&meeting_id_obj, &summary) {
                tracing::error!("Failed to save summary: {}", e);
            } else {
                tracing::info!("Summary saved for meeting {}", meeting_id);
            }

            if let Ok(Some(meeting)) = db.get_meeting(&meeting_id_obj) {
                if meeting.title == "Untitled Meeting" {
                    tracing::info!("Generating title for untitled meeting");
                    let title_result = rt.block_on(crate::llm::generate_title(&cfg.llm, &summary.markdown));
                    if let Ok(title) = title_result {
                        tracing::info!("Generated title: {}", title);
                        let mut updated = meeting;
                        updated.title = title;
                        let _ = db.update_meeting(&updated);
                    }
                }
            }

            generate_meeting_notes(&db, &meeting_id_obj, &transcript, &summary);
        }
        Err(e) => {
            tracing::error!("Summarization failed: {}", e);
        }
    }
}

fn generate_meeting_notes(
    db: &Database,
    meeting_id: &crate::storage::MeetingId,
    transcript: &crate::transcription::Transcript,
    summary: &crate::llm::SummaryResult,
) {
    let meeting = match db.get_meeting(meeting_id) {
        Ok(Some(m)) => m,
        _ => {
            tracing::error!("Failed to get meeting for notes generation");
            return;
        }
    };

    let notes_dir = match crate::config::loader::notes_dir() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to get notes dir: {}", e);
            return;
        }
    };

    let generator = crate::notes::markdown::NoteGenerator::new(notes_dir);
    match generator.generate(&meeting, transcript, summary) {
        Ok(path) => {
            tracing::info!("Generated notes: {}", path.display());
            let mut updated_meeting = meeting;
            updated_meeting.notes_path = Some(path);
            if let Err(e) = db.update_meeting(&updated_meeting) {
                tracing::error!("Failed to update meeting with notes path: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("Failed to generate notes: {}", e);
        }
    }
}

fn run_background_transcription_and_diarization(meeting_id: String, audio_path: PathBuf) {
    tracing::info!("Starting background transcription for meeting {}", meeting_id);
    
    let models_dir = match models_dir() {
        Ok(dir) => dir,
        Err(e) => {
            tracing::error!("Failed to get models dir: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    let parakeet_manager = ParakeetModelManager::new(models_dir.clone());
    let parakeet_model = ParakeetModel::TdtV3;
    
    if !parakeet_manager.model_exists(parakeet_model) {
        tracing::error!("Parakeet model not found for batch transcription");
        mark_meeting_complete(&meeting_id);
        return;
    }
    
    let model_dir = parakeet_manager.model_dir(parakeet_model);
    let mut engine = crate::transcription::parakeet::ParakeetEngine::new();
    if let Err(e) = engine.load_model(&model_dir, false) {
        tracing::error!("Failed to load Parakeet model: {}", e);
        mark_meeting_complete(&meeting_id);
        return;
    }
    
    let transcript = match crate::transcription::parakeet::transcribe_wav_file(&mut engine, &audio_path) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Transcription failed: {}", e);
            mark_meeting_complete(&meeting_id);
            return;
        }
    };
    
    tracing::info!("Transcription complete: {} segments", transcript.segments.len());
    
    if let Ok(db_path) = database_path() {
        if let Ok(db) = Database::open(&db_path) {
            let meeting_id_obj = crate::storage::MeetingId::from_string(meeting_id.clone());
            if let Err(e) = db.insert_transcript_segments(&meeting_id_obj, &transcript.segments) {
                tracing::error!("Failed to save transcript: {}", e);
            }
        }
    }
    
    run_background_diarization(meeting_id, audio_path);
}

fn mark_meeting_complete(meeting_id: &str) {
    if let Ok(db_path) = database_path() {
        if let Ok(db) = Database::open(&db_path) {
            let meeting_id_obj = crate::storage::MeetingId::from_string(meeting_id.to_string());
            if let Ok(Some(mut meeting)) = db.get_meeting(&meeting_id_obj) {
                meeting.status = crate::storage::MeetingStatus::Complete;
                let _ = db.update_meeting(&meeting);
            }
        }
    }
}

fn load_wav_samples(path: &PathBuf) -> Result<Vec<f32>> {
    use hound::WavReader;
    
    let mut reader = WavReader::open(path)
        .map_err(|e| MuesliError::Audio(format!("Failed to open WAV: {}", e)))?;
    
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => {
            reader.samples::<f32>().filter_map(|s| s.ok()).collect()
        }
    };
    
    if spec.channels > 1 {
        Ok(samples.chunks(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect())
    } else {
        Ok(samples)
    }
}

fn find_any_meeting_window() -> Option<MeetingApp> {
    tracing::info!("Searching for meeting windows...");
    let windows = match crate::detection::hyprland::list_all_windows() {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to list windows: {}", e);
            return None;
        }
    };
    
    tracing::info!("Found {} windows, checking for meeting apps", windows.len());
    for window in &windows {
        tracing::debug!("Checking window: class='{}' title='{}'", window.class, window.title);
        if let Some(app) = crate::detection::patterns::detect_meeting_app(&window.class, &window.title) {
            tracing::info!("Found meeting window: {} ({} - {})", app, window.class, window.title);
            return Some(app);
        }
    }
    
    tracing::info!("No meeting windows found");
    None
}

fn start_meeting_window_monitor(
    app: MeetingApp,
    detection_tx: mpsc::Sender<DetectionEvent>,
) -> Arc<AtomicBool> {
    let monitor_running = Arc::new(AtomicBool::new(true));
    let monitor_running_clone = monitor_running.clone();
    
    tracing::info!("Starting meeting window monitor for {}", app);
    
    tokio::spawn(async move {
        let check_interval = std::time::Duration::from_secs(3);
        
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        tracing::debug!("Meeting window monitor active for {}", app);
        
        while monitor_running_clone.load(Ordering::Relaxed) {
            tokio::time::sleep(check_interval).await;
            
            if !monitor_running_clone.load(Ordering::Relaxed) {
                tracing::debug!("Monitor flag set to false, stopping");
                break;
            }
            
            let window_exists = crate::detection::hyprland::meeting_window_exists(app);
            tracing::trace!("Meeting window check for {}: exists={}", app, window_exists);
            
            if !window_exists {
                tracing::info!("Meeting window for {} no longer exists, triggering auto-stop", app);
                let _ = detection_tx.send(DetectionEvent::MeetingWindowClosed { app }).await;
                break;
            }
        }
        
        tracing::debug!("Meeting window monitor stopped for {}", app);
    });
    
    monitor_running
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_daemon_state_default() {
        let state = DaemonState::default();
        assert!(!state.recording);
        assert!(state.current_meeting.is_none());
        assert!(state.meeting_detected.is_none());
    }

    #[tokio::test]
    async fn test_handle_ping() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let response = handle_request(DaemonRequest::Ping, &state, &shutdown).await;
        assert!(matches!(response, DaemonResponse::Pong));
    }

    #[tokio::test]
    async fn test_handle_get_status() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let response = handle_request(DaemonRequest::GetStatus, &state, &shutdown).await;
        match response {
            DaemonResponse::Status(status) => {
                assert!(status.running);
                assert!(!status.recording);
                assert!(status.current_meeting.is_none());
            }
            _ => panic!("Expected Status response"),
        }
    }

    #[tokio::test]
    async fn test_handle_start_recording() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let response = handle_request(
            DaemonRequest::StartRecording {
                title: Some("Test Meeting".to_string()),
            },
            &state,
            &shutdown,
        )
        .await;

        match response {
            DaemonResponse::RecordingStarted { meeting_id } => {
                assert!(!meeting_id.is_empty());
            }
            _ => panic!("Expected RecordingStarted response"),
        }

        let state = state.lock().await;
        assert!(state.recording);
        assert_eq!(
            state.current_meeting.as_ref().unwrap().title,
            "Test Meeting"
        );
    }

    #[tokio::test]
    async fn test_handle_start_recording_already_recording() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let _ = handle_request(
            DaemonRequest::StartRecording { title: None },
            &state,
            &shutdown,
        )
        .await;

        let response = handle_request(
            DaemonRequest::StartRecording { title: None },
            &state,
            &shutdown,
        )
        .await;

        match response {
            DaemonResponse::Error { message } => {
                assert_eq!(message, "Already recording");
            }
            _ => panic!("Expected Error response"),
        }
    }

    #[tokio::test]
    async fn test_handle_stop_recording() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let _ = handle_request(
            DaemonRequest::StartRecording { title: None },
            &state,
            &shutdown,
        )
        .await;

        let response = handle_request(DaemonRequest::StopRecording, &state, &shutdown).await;

        match response {
            DaemonResponse::RecordingStopped { meeting_id } => {
                assert!(!meeting_id.is_empty());
            }
            _ => panic!("Expected RecordingStopped response"),
        }

        let state = state.lock().await;
        assert!(!state.recording);
        assert!(state.current_meeting.is_none());
    }

    #[tokio::test]
    async fn test_handle_stop_recording_not_recording() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let response = handle_request(DaemonRequest::StopRecording, &state, &shutdown).await;

        match response {
            DaemonResponse::Error { message } => {
                assert_eq!(message, "Not recording");
            }
            _ => panic!("Expected Error response"),
        }
    }

    #[tokio::test]
    async fn test_handle_shutdown() {
        let state = Arc::new(Mutex::new(DaemonState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let response = handle_request(DaemonRequest::Shutdown, &state, &shutdown).await;

        assert!(matches!(response, DaemonResponse::Ok));
        assert!(shutdown.load(Ordering::Relaxed));
    }
}
