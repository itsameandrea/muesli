use crate::cli::commands::*;
use crate::config;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::error::Result;
use crate::llm::local::find_lms_binary;
use crate::storage::database::Database;
use crate::storage::MeetingId;
use crate::transcription::diarization_models::{DiarizationModel, DiarizationModelManager};
use crate::transcription::models::{ModelManager, WhisperModel};
use crate::waybar::WaybarStatus;
use cpal::traits::{DeviceTrait, HostTrait};
use std::io::Write;

pub async fn handle_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Start { title } => handle_start(title).await,
        Commands::Stop => handle_stop().await,
        Commands::Status => handle_status().await,
        Commands::List { limit } => handle_list(limit).await,
        Commands::Notes { id } => handle_notes(id).await,
        Commands::Transcript { id } => handle_transcript(id).await,
        Commands::Daemon => handle_daemon().await,
        Commands::Config { action } => handle_config(action).await,
        Commands::Models { engine } => handle_models(engine).await,
        Commands::Audio { action } => handle_audio(action).await,
        Commands::Setup => handle_setup().await,
        Commands::Uninstall => handle_uninstall().await,
        Commands::Update => handle_update().await,
        Commands::Waybar => handle_waybar().await,
        Commands::Redo { id, clean } => handle_redo(id, clean).await,
        Commands::Search {
            query,
            limit,
            keyword,
            action,
        } => handle_search(query, limit, keyword, action).await,
        Commands::Ask { question } => handle_ask(question).await,
    }
}

async fn handle_start(title: Option<String>) -> Result<()> {
    let mut client = match DaemonClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Error: Daemon is not running. Start it with: muesli daemon");
            return Ok(());
        }
    };

    let request = DaemonRequest::StartRecording { title };
    match client.send(request).await? {
        DaemonResponse::RecordingStarted { meeting_id } => {
            println!("Recording started (ID: {})", meeting_id);
        }
        DaemonResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}

async fn handle_stop() -> Result<()> {
    let mut client = match DaemonClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Error: Daemon is not running.");
            return Ok(());
        }
    };

    match client.send(DaemonRequest::StopRecording).await? {
        DaemonResponse::RecordingStopped { meeting_id } => {
            println!("Recording stopped (ID: {})", meeting_id);

            let db_path = config::loader::database_path()?;
            let db = Database::open(&db_path)?;

            let meeting = db
                .get_meeting(&MeetingId::from_string(meeting_id.clone()))?
                .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(meeting_id.clone()))?;

            let segments = db.get_transcript_segments(&meeting.id)?;

            if !segments.is_empty() {
                println!(
                    "\nTranscript ({} segments, processing speakers in background):\n",
                    segments.len()
                );
                for segment in &segments {
                    print_segment(segment);
                }
                println!("\nView final transcript with: muesli view {}", meeting_id);
            } else {
                println!("Transcription processing in background...");
                println!("View transcript with: muesli view {}", meeting_id);
            }
        }
        DaemonResponse::Error { message } => {
            eprintln!("Error: {}", message);
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}

async fn handle_status() -> Result<()> {
    let mut client = match DaemonClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            println!("Daemon: not running");
            println!("Status: offline");
            return Ok(());
        }
    };

    match client.send(DaemonRequest::GetStatus).await? {
        DaemonResponse::Status(status) => {
            println!("Daemon: running (uptime: {}s)", status.uptime_seconds);
            if status.recording {
                println!("Status: recording");
                if let Some(meeting) = status.current_meeting {
                    println!("Meeting: {}", meeting);
                }
            } else {
                println!("Status: idle");
            }
            if let Some(app) = status.meeting_detected {
                println!("Detected: {} meeting window", app);
            }
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}

async fn handle_list(limit: usize) -> Result<()> {
    let db_path = config::loader::database_path()?;

    if !db_path.exists() {
        println!("No meetings recorded yet.");
        return Ok(());
    }

    let db = Database::open(&db_path)?;
    let meetings = db.list_meetings(limit)?;

    if meetings.is_empty() {
        println!("No meetings recorded yet.");
        return Ok(());
    }

    println!(
        "{:<36} {:<30} {:<10} {:<10}",
        "ID", "Title", "Status", "Duration"
    );
    println!("{}", "-".repeat(90));

    for meeting in meetings {
        let duration = meeting
            .duration_seconds
            .map(|d| format!("{}m", d / 60))
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<36} {:<30} {:<10} {:<10}",
            meeting.id,
            truncate(&meeting.title, 28),
            meeting.status,
            duration
        );
    }

    Ok(())
}

async fn handle_notes(id: Option<String>) -> Result<()> {
    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting_id = match id {
        Some(id) => id,
        None => select_meeting_interactive(&db)?,
    };

    let meeting = db
        .get_meeting(&MeetingId::from_string(meeting_id.clone()))?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(meeting_id))?;

    if let Ok(Some(summary)) = db.get_summary(&meeting.id) {
        println!("\n# {}\n", meeting.title);
        println!(
            "**Date:** {} | **Duration:** {}\n",
            meeting.started_at.format("%Y-%m-%d %H:%M"),
            meeting
                .duration_seconds
                .map(|d| format!("{}m {}s", d / 60, d % 60))
                .unwrap_or("?".to_string())
        );
        println!("{}", summary.markdown);
    } else {
        println!("\nNo notes available for: {}", meeting.title);
        println!("Run: muesli summarize {}\n", meeting.id);
    }

    Ok(())
}

async fn handle_transcript(id: Option<String>) -> Result<()> {
    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting_id = match id {
        Some(id) => id,
        None => select_meeting_interactive(&db)?,
    };

    let meeting = db
        .get_meeting(&MeetingId::from_string(meeting_id.clone()))?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(meeting_id))?;

    println!("\n{}", "=".repeat(60));
    println!("  {} - Transcript", meeting.title);
    println!("{}", "=".repeat(60));
    println!();

    let segments = db.get_transcript_segments(&meeting.id)?;
    if segments.is_empty() {
        println!("No transcript available.");
        return Ok(());
    }

    println!("{} segments\n", segments.len());

    for segment in segments {
        print_segment(&segment);
    }

    Ok(())
}

fn select_meeting_interactive(db: &Database) -> Result<String> {
    use dialoguer::{theme::ColorfulTheme, Select};

    let meetings = db.list_meetings(20)?;

    if meetings.is_empty() {
        return Err(crate::error::MuesliError::Config(
            "No meetings found".to_string(),
        ));
    }

    let items: Vec<String> = meetings
        .iter()
        .map(|m| {
            let date = m.started_at.format("%Y-%m-%d %H:%M");
            let duration = m
                .duration_seconds
                .map(|d| format!("{}m", d / 60))
                .unwrap_or_else(|| "?".to_string());
            format!("{} | {} | {}", date, duration, truncate(&m.title, 40))
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a meeting")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| crate::error::MuesliError::Config(format!("Selection cancelled: {}", e)))?;

    Ok(meetings[selection].id.0.clone())
}

async fn handle_daemon() -> Result<()> {
    if DaemonClient::ping().await? {
        eprintln!("Error: Daemon is already running.");
        return Ok(());
    }

    println!("Starting muesli daemon...");
    crate::daemon::run_daemon().await
}

async fn handle_config(action: ConfigCommands) -> Result<()> {
    match action {
        ConfigCommands::Show => {
            let cfg = config::loader::load_config()?;
            println!("{}", toml::to_string_pretty(&cfg)?);
        }
        ConfigCommands::Edit => {
            let path = config::loader::config_path()?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
            std::process::Command::new(&editor).arg(&path).status()?;
        }
    }
    Ok(())
}

async fn handle_models(engine: ModelEngine) -> Result<()> {
    match engine {
        ModelEngine::Whisper { action } => handle_whisper_models(action).await,
        ModelEngine::Diarization { action } => handle_diarization_models(action).await,
    }
}

async fn handle_whisper_models(action: ModelAction) -> Result<()> {
    let models_dir = config::loader::models_dir()?;
    let manager = ModelManager::new(models_dir);

    match action {
        ModelAction::List => {
            println!("{:<10} {:<12} {:<10}", "Model", "Size (MB)", "Downloaded");
            println!("{}", "-".repeat(35));

            for (model, exists, size) in manager.list_all() {
                let status = if exists { "✓" } else { "-" };
                println!("{:<10} {:<12} {:<10}", model, size, status);
            }
        }
        ModelAction::Download { model } => {
            let whisper_model = WhisperModel::parse(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!(
                    "Unknown model: {}. Use: tiny, base, small, medium, large, large-v3-turbo, distil-large-v3",
                    model
                ))
            })?;

            println!(
                "Downloading {} model (~{} MB)...",
                whisper_model,
                whisper_model.size_mb()
            );

            let path = manager.download_model(whisper_model, |downloaded, total| {
                let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                print!(
                    "\rProgress: {}% ({}/{} MB)",
                    percent,
                    downloaded / 1024 / 1024,
                    total / 1024 / 1024
                );
                std::io::stdout().flush().ok();
            })?;

            println!("\nDownloaded to: {}", path.display());
        }
        ModelAction::Delete { model } => {
            let whisper_model = WhisperModel::parse(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!("Unknown model: {}", model))
            })?;

            manager.delete_model(whisper_model)?;
            println!("Deleted {} model", model);
        }
    }
    Ok(())
}

async fn handle_diarization_models(action: ModelAction) -> Result<()> {
    let models_dir = config::loader::models_dir()?;
    let manager = DiarizationModelManager::new(models_dir);

    match action {
        ModelAction::List => {
            println!("{:<20} {:<12} {:<10}", "Model", "Size (MB)", "Downloaded");
            println!("{}", "-".repeat(45));

            for (model, exists, size) in manager.list_all() {
                let status = if exists { "✓" } else { "-" };
                println!("{:<20} {:<12} {:<10}", model, size, status);
            }
        }
        ModelAction::Download { model } => {
            let diar_model = DiarizationModel::parse(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!(
                    "Unknown model: {}. Use: sortformer-v2",
                    model
                ))
            })?;

            println!(
                "Downloading {} (~{} MB)...",
                diar_model,
                diar_model.size_mb()
            );

            let path = tokio::task::spawn_blocking(move || {
                manager.download_model(diar_model, |downloaded, total| {
                    let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                    print!(
                        "\rProgress: {}% ({}/{} MB)",
                        percent,
                        downloaded / 1024 / 1024,
                        total / 1024 / 1024
                    );
                    std::io::stdout().flush().ok();
                })
            })
            .await
            .map_err(|e| {
                crate::error::MuesliError::Config(format!("Download task failed: {}", e))
            })??;

            println!("\nDownloaded to: {}", path.display());
        }
        ModelAction::Delete { model } => {
            let diar_model = DiarizationModel::parse(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!("Unknown model: {}", model))
            })?;

            manager.delete_model(diar_model)?;
            println!("Deleted {} model", model);
        }
    }
    Ok(())
}

async fn handle_audio(action: AudioCommands) -> Result<()> {
    match action {
        AudioCommands::ListDevices => {
            println!("Input Devices (Microphones):");
            println!("{}", "-".repeat(50));

            let host = cpal::default_host();
            if let Ok(devices) = host.input_devices() {
                for device in devices {
                    if let Ok(name) = device.name() {
                        if let Ok(config) = device.default_input_config() {
                            let sample_rate = config.sample_rate().0;
                            let channels = config.channels();
                            println!("  {} ({}Hz, {} ch)", name, sample_rate, channels);
                        }
                    }
                }
            }

            println!("\nLoopback Devices (System Audio):");
            println!("{}", "-".repeat(50));

            if let Ok(devices) = host.input_devices() {
                for device in devices {
                    if let Ok(name) = device.name() {
                        let name_lower = name.to_lowercase();
                        if name_lower.contains("monitor") || name_lower.contains("loopback") {
                            if let Ok(config) = device.default_input_config() {
                                let sample_rate = config.sample_rate().0;
                                let channels = config.channels();
                                println!("  {} ({}Hz, {} ch)", name, sample_rate, channels);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_setup() -> Result<()> {
    use dialoguer::{theme::ColorfulTheme, Confirm, Select};

    println!();
    println!("==========================================");
    println!("  muesli Setup Wizard");
    println!("==========================================");
    println!();

    println!("[1/11] Creating directories...");
    config::loader::ensure_directories()?;
    let config_dir = config::loader::config_dir()?;
    let data_dir = config::loader::data_dir()?;
    let models_dir = config::loader::models_dir()?;
    println!("  Config: {}", config_dir.display());
    println!("  Data:   {}", data_dir.display());
    println!("  Models: {}", models_dir.display());
    println!();

    println!("[2/11] Initializing configuration...");
    let config_path = config::loader::config_path()?;
    if config_path.exists() {
        println!(
            "  Configuration already exists at {}",
            config_path.display()
        );
    } else {
        let _ = config::loader::load_config()?;
        println!(
            "  Created default configuration at {}",
            config_path.display()
        );
    }
    println!();

    println!("[3/11] GPU Acceleration");
    println!("  GPU acceleration provides faster transcription.");
    println!();

    let use_gpu = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Enable GPU acceleration? (requires Vulkan/CUDA/Metal)")
        .default(false)
        .interact()
        .unwrap_or(false);

    update_config_value("use_gpu", if use_gpu { "true" } else { "false" })?;
    println!(
        "  GPU acceleration: {}",
        if use_gpu { "enabled" } else { "disabled" }
    );
    println!();

    println!("[4/11] Transcription Model Selection");
    println!();

    let whisper_models = vec![
        ("tiny", 75, "Fastest, lowest accuracy"),
        ("base", 142, "Good balance (recommended)"),
        ("small", 466, "Better accuracy"),
        ("medium", 1500, "High accuracy"),
        ("large", 2900, "Best accuracy"),
        ("large-v3-turbo", 1620, "Fast + high quality"),
    ];

    let whisper_manager = ModelManager::new(models_dir.clone());

    let mut model_options: Vec<String> = vec![];

    model_options.push("--- Whisper Models (whisper.cpp) ---".to_string());
    for (name, size, desc) in &whisper_models {
        let model = WhisperModel::parse(name).unwrap();
        let installed = if whisper_manager.model_exists(model) {
            " [installed]"
        } else {
            ""
        };
        model_options.push(format!(
            "{:<18} ({:>4} MB) - {}{}",
            name, size, desc, installed
        ));
    }

    model_options.push("Skip model download".to_string());

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a transcription model")
        .items(&model_options)
        .default(2)
        .interact()
        .unwrap_or(model_options.len() - 1);

    if selection == 0 || selection == 7 {
        println!("  Skipping model download");
    } else if (1..=6).contains(&selection) {
        let model_name = whisper_models[selection - 1].0;
        let model = WhisperModel::parse(model_name).unwrap();

        if whisper_manager.model_exists(model) {
            println!("  Model '{}' is already installed", model_name);
        } else {
            println!("  Downloading {} model...", model_name);
            let path = whisper_manager.download_model(model, |downloaded, total| {
                let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                print!(
                    "\r  Progress: {}% ({}/{} MB)    ",
                    percent,
                    downloaded / 1024 / 1024,
                    total / 1024 / 1024
                );
                std::io::stdout().flush().ok();
            })?;
            println!("\n  Downloaded to: {}", path.display());
        }

        update_transcription_config("whisper", model_name)?;
    }
    println!();

    println!("[5/11] Speaker Diarization Model");
    let diar_manager = DiarizationModelManager::new(models_dir.clone());
    let diar_model = DiarizationModel::SortformerV2;

    if diar_manager.model_exists(diar_model) {
        println!("  Diarization model already installed");
    } else {
        let download_diar = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Download speaker diarization model (sortformer-v2, ~127 MB)?")
            .default(true)
            .interact()
            .unwrap_or(false);

        if download_diar {
            println!("  Downloading sortformer-v2...");
            let path = tokio::task::spawn_blocking(move || {
                diar_manager.download_model(diar_model, |downloaded, total| {
                    let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                    print!(
                        "\r  Progress: {}% ({}/{} MB)    ",
                        percent,
                        downloaded / 1024 / 1024,
                        total / 1024 / 1024
                    );
                    std::io::stdout().flush().ok();
                })
            })
            .await
            .map_err(|e| crate::error::MuesliError::Config(format!("Download failed: {}", e)))??;
            println!("\n  Downloaded to: {}", path.display());
        } else {
            println!("  Skipping diarization model");
            println!("  (You can download later with: muesli diarization download sortformer-v2)");
        }
    }
    println!();

    println!("[6/11] Faster Processing on Stop");
    println!("  muesli transcribes incrementally with Whisper while recording.");
    println!("  This reduces wait time after stopping for long meetings.");
    println!();

    println!("[7/11] LLM for Meeting Notes");
    println!("  An LLM generates meeting summaries and notes from transcripts.");
    println!();

    let provider_options = vec![
        "LM Studio (local, free)",
        "Anthropic (Claude)",
        "OpenAI (GPT)",
        "Moonshot (Kimi)",
        "OpenRouter (multi-provider)",
        "Skip LLM setup",
    ];

    let provider_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select an LLM provider")
        .items(&provider_options)
        .default(0)
        .interact()
        .unwrap_or(provider_options.len() - 1);

    match provider_selection {
        0 => {
            let lms_path = find_lms_binary();
            if let Some(ref lms) = lms_path {
                println!("  Found LM Studio CLI at: {}", lms);

                let models = discover_lms_models(lms);

                if !models.is_empty() {
                    println!("  Found {} LLM model(s)", models.len());
                    println!();

                    let mut options: Vec<String> = models.clone();
                    options.push("Skip LLM setup".to_string());

                    let selection = Select::with_theme(&ColorfulTheme::default())
                        .with_prompt("Select a model")
                        .items(&options)
                        .default(0)
                        .interact()
                        .unwrap_or(options.len() - 1);

                    if selection < models.len() {
                        update_llm_config("local", &models[selection], None)?;
                        println!("  LLM configured: {} (via LM Studio)", models[selection]);
                    } else {
                        update_llm_config("none", "", None)?;
                        println!("  LLM disabled");
                    }
                } else {
                    println!("  No LLM models found in LM Studio.");
                    println!("  Download a model in LM Studio first, then run setup again.");
                    update_llm_config("none", "", None)?;
                }
            } else {
                println!("  LM Studio not found.");
                println!("  Install from https://lmstudio.ai for local LLM support.");
                update_llm_config("none", "", None)?;
            }
        }
        1 => setup_cloud_provider("anthropic", "Anthropic", "claude-sonnet-4-20250514")?,
        2 => setup_cloud_provider("openai", "OpenAI", "gpt-4o")?,
        3 => setup_cloud_provider("moonshot", "Moonshot (Kimi)", "kimi-k2.5")?,
        4 => setup_cloud_provider("openrouter", "OpenRouter", "anthropic/claude-sonnet-4")?,
        _ => {
            update_llm_config("none", "", None)?;
            println!("  LLM disabled");
        }
    }
    println!();

    println!("[8/11] Meeting Detection");
    println!("  Auto-detection monitors your windows for meeting apps (Zoom, Meet, Teams, etc.)");
    println!("  When detected, a notification prompt asks if you want to record.");
    println!();

    let auto_prompt = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Enable automatic meeting detection and recording prompts?")
        .default(true)
        .interact()
        .unwrap_or(true);

    update_config_value("auto_prompt", if auto_prompt { "true" } else { "false" })?;
    update_config_value("prompt_timeout_secs", "30")?;
    println!(
        "  Meeting auto-detection: {}",
        if auto_prompt { "enabled" } else { "disabled" }
    );
    println!();

    println!("[9/11] Audio Cues");
    println!("  Play a sound when recording starts and stops.");
    println!();

    let enable_audio_cues = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Enable audio cues for recording start/stop?")
        .default(false)
        .interact()
        .unwrap_or(false);

    update_audio_cues_config(enable_audio_cues)?;
    println!(
        "  Audio cues: {}",
        if enable_audio_cues {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!();

    println!("[10/11] Systemd Service");
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let systemd_dir = std::path::PathBuf::from(&home).join(".config/systemd/user");

    let service_path = systemd_dir.join("muesli.service");

    if service_path.exists() {
        println!("  Service already installed at {}", service_path.display());
    } else {
        let install_service = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Install systemd user service for auto-start?")
            .default(true)
            .interact()
            .unwrap_or(false);

        if install_service {
            std::fs::create_dir_all(&systemd_dir)?;

            let binary_path =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("muesli"));

            let service_content = format!(
                r#"[Unit]
Description=muesli - AI-powered meeting note-taker
Documentation=https://github.com/itsameandrea/muesli
After=graphical-session.target

[Service]
Type=simple
ExecStart={} daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
                binary_path.display()
            );

            std::fs::write(&service_path, service_content)?;
            println!("  Service installed at {}", service_path.display());

            let _ = std::process::Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .status();

            println!("  To enable auto-start: systemctl --user enable muesli.service");
            println!("  To start now:         systemctl --user start muesli.service");
        } else {
            println!("  Skipping systemd service installation");
        }
    }
    println!();

    println!("[11/11] qmd Search Integration");
    println!("  qmd enables AI-powered semantic search across all your meeting notes.");
    println!();

    let enable_qmd = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Enable qmd search for meeting notes?")
        .default(true)
        .interact()
        .unwrap_or(false);

    if enable_qmd {
        if !crate::qmd::indexer::is_qmd_installed() {
            println!();
            println!("  qmd not found. Installing...");
            if install_qmd() {
                println!("  qmd installed!");
            } else {
                println!("  Failed to install qmd automatically.");
                println!("  Install manually: bun install -g github:tobi/qmd");
                println!("  Then re-run: muesli setup");
                update_qmd_config(false, false, "muesli-meetings")?;
            }
        } else {
            println!("  qmd detected!");
        }

        if crate::qmd::indexer::is_qmd_installed() {
            println!();
            update_qmd_config(true, true, "muesli-meetings")?;

            let notes_dir = config::loader::notes_dir()?;
            println!("  Setting up qmd collection...");

            if let Err(e) = crate::qmd::indexer::setup_collection(&notes_dir, "muesli-meetings") {
                println!("  Warning: Collection setup failed: {}", e);
                println!("  You can retry later with: muesli search reindex");
            } else {
                println!("  Collection created: muesli-meetings");

                println!("  Running initial index (this may take a moment)...");
                if let Err(e) = crate::qmd::indexer::update_index("muesli-meetings") {
                    println!("  Warning: Initial indexing failed: {}", e);
                    println!("  You can retry later with: muesli search reindex");
                } else {
                    println!("  Initial indexing complete!");
                }
            }
        }
    } else {
        update_qmd_config(false, false, "muesli-meetings")?;
        println!("  qmd search disabled.");
    }
    println!();

    println!("==========================================");
    println!("  Setup Complete!");
    println!("==========================================");
    println!();

    println!("Starting daemon...");
    let daemon_bin = std::env::current_exe().unwrap_or_else(|_| "muesli".into());
    match std::process::Command::new(&daemon_bin)
        .arg("daemon")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            std::thread::sleep(std::time::Duration::from_millis(500));
            println!("  Daemon started! muesli is now monitoring for meetings.");
        }
        Err(e) => {
            println!("  Failed to start daemon: {}", e);
            println!("  Start manually with: muesli daemon");
        }
    }

    println!();
    println!("Tips:");
    println!();
    println!("  - Auto-start on login:");
    println!("    systemctl --user enable --now muesli.service");
    println!();
    println!("  - Test audio devices:");
    println!("    muesli audio list-devices");
    println!();
    println!("  - Edit configuration:");
    println!("    muesli config edit");
    println!();

    Ok(())
}

async fn handle_update() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let version_info = env!("MUESLI_VERSION_INFO");
    let source_dir = env!("MUESLI_SOURCE_DIR");

    println!("Current: muesli {} ({})", current_version, version_info);
    println!("Source:  {}", source_dir);
    println!();

    let repo_dir = std::path::Path::new(source_dir);
    if !repo_dir.join("Cargo.toml").exists() {
        eprintln!("Source directory no longer exists: {}", source_dir);
        eprintln!("Falling back to latest GitHub release binary...");
        println!();

        let install_path = std::env::current_exe().ok();
        if let Some(path) = install_path {
            if let Err(e) = update_from_latest_release(current_version, &path) {
                eprintln!("Release update failed: {}", e);
            }
        } else {
            eprintln!("Could not determine installed binary path for update");
        }

        return Ok(());
    }

    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    println!("Building...");
    println!();

    let status = std::process::Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(repo_dir)
        .status()
        .map_err(|e| crate::error::MuesliError::Config(format!("Failed to run cargo: {}", e)))?;

    if !status.success() {
        eprintln!("Build failed.");
        return Ok(());
    }

    let built_binary = repo_dir.join("target/release/muesli");

    if let Some(dir) = &install_dir {
        let dest = dir.join("muesli");
        let tmp = dir.join(".muesli.update.tmp");
        if let Err(e) = std::fs::copy(&built_binary, &tmp) {
            eprintln!("Failed to copy to {}: {}", tmp.display(), e);
            return Ok(());
        }
        let _ = std::fs::remove_file(&dest);
        if let Err(e) = std::fs::rename(&tmp, &dest) {
            eprintln!("Failed to install to {}: {}", dest.display(), e);
            let _ = std::fs::remove_file(&tmp);
            return Ok(());
        }
    }

    println!();

    let check_binary = install_dir
        .map(|d| d.join("muesli"))
        .unwrap_or(built_binary);

    let new_version = std::process::Command::new(&check_binary)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("Updated: {}", new_version);

    Ok(())
}

fn update_from_latest_release(current_version: &str, install_path: &std::path::Path) -> Result<()> {
    let release = fetch_latest_release()?;
    let tag = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::error::MuesliError::Config("Latest release tag missing".to_string())
        })?;

    if tag.trim_start_matches('v') == current_version {
        println!("Already on latest release: {}", tag);
        return Ok(());
    }

    let assets = release
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            crate::error::MuesliError::Config("Latest release assets missing".to_string())
        })?;

    let candidates = detect_release_asset_candidates();
    let selected = candidates.iter().find_map(|suffix| {
        let asset_name = format!("muesli-{}", suffix);
        assets.iter().find_map(|asset| {
            let name = asset.get("name").and_then(|v| v.as_str())?;
            if name != asset_name {
                return None;
            }

            let url = asset.get("browser_download_url").and_then(|v| v.as_str())?;
            Some((name.to_string(), url.to_string()))
        })
    });

    let (asset_name, asset_url) = match selected {
        Some(v) => v,
        None => {
            let available: Vec<String> = assets
                .iter()
                .filter_map(|asset| asset.get("name").and_then(|v| v.as_str()))
                .filter(|name| name.starts_with("muesli-"))
                .map(|s| s.to_string())
                .collect();

            return Err(crate::error::MuesliError::Config(format!(
                "No matching release asset for this platform. Available: {}",
                available.join(", ")
            )));
        }
    };

    println!("Updating to {} using {}...", tag, asset_name);

    let client = reqwest::blocking::Client::new();
    let bytes = client
        .get(&asset_url)
        .header(reqwest::header::USER_AGENT, "muesli-updater")
        .send()?
        .error_for_status()?
        .bytes()?;

    let tmp_path = install_path.with_extension("update.tmp");
    std::fs::write(&tmp_path, &bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp_path, perms)?;
    }

    let _ = std::fs::remove_file(install_path);
    std::fs::rename(&tmp_path, install_path)?;

    let new_version = std::process::Command::new(install_path)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("Updated: {}", new_version);
    Ok(())
}

fn fetch_latest_release() -> Result<serde_json::Value> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://api.github.com/repos/itsameandrea/muesli/releases/latest")
        .header(reqwest::header::USER_AGENT, "muesli-updater")
        .send()?
        .error_for_status()?;

    Ok(response.json()?)
}

fn detect_release_asset_candidates() -> Vec<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => {
            if std::process::Command::new("vulkaninfo")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                vec!["linux-x86_64-vulkan", "linux-x86_64-cpu"]
            } else {
                vec!["linux-x86_64-cpu", "linux-x86_64-vulkan"]
            }
        }
        ("macos", "x86_64") => vec!["macos-x86_64"],
        ("macos", "aarch64") => vec!["macos-arm64"],
        _ => vec!["linux-x86_64-cpu"],
    }
}

async fn handle_uninstall() -> Result<()> {
    use dialoguer::{theme::ColorfulTheme, Confirm};

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let install_dir =
        std::env::var("INSTALL_DIR").unwrap_or_else(|_| format!("{}/.local/bin", home));
    let config_dir = config::loader::config_dir()?;
    let data_dir = config::loader::data_dir()?;
    let systemd_service =
        std::path::PathBuf::from(&home).join(".config/systemd/user/muesli.service");
    let binary_path = std::path::PathBuf::from(&install_dir).join("muesli");

    println!();
    println!("==========================================");
    println!("  muesli Uninstaller");
    println!("==========================================");
    println!();

    let has_binary = binary_path.exists();
    let has_service = systemd_service.exists();
    let has_config = config_dir.exists();
    let has_data = data_dir.exists();

    if !has_binary && !has_service && !has_config && !has_data {
        println!("muesli does not appear to be installed.");
        return Ok(());
    }

    println!("This will remove:");
    if has_binary {
        println!("  - Binary: {}", binary_path.display());
    }
    if has_service {
        println!("  - Systemd service: {}", systemd_service.display());
    }
    if has_config {
        println!("  - Config directory: {}", config_dir.display());
    }
    if has_data {
        println!(
            "  - Data directory: {} (recordings, models, database)",
            data_dir.display()
        );
    }
    println!();

    let proceed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Proceed with uninstallation?")
        .default(false)
        .interact()
        .unwrap_or(false);

    if !proceed {
        println!("Uninstallation cancelled.");
        return Ok(());
    }
    println!();

    println!("Stopping muesli daemon...");
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "stop", "muesli.service"])
        .status();
    let _ = std::process::Command::new("pkill")
        .args(["-f", "muesli daemon"])
        .status();
    std::thread::sleep(std::time::Duration::from_secs(1));

    if has_service {
        println!("Removing systemd service...");
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "muesli.service"])
            .status();
        let _ = std::fs::remove_file(&systemd_service);
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .status();
    }

    if has_config {
        let remove_config = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Remove configuration directory ({})?",
                config_dir.display()
            ))
            .default(false)
            .interact()
            .unwrap_or(false);

        if remove_config {
            println!("Removing configuration...");
            let _ = std::fs::remove_dir_all(&config_dir);
        } else {
            println!("Keeping configuration directory.");
        }
    }

    if has_data {
        println!();
        println!("Data directory contains recordings, models, and meeting database.");
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            let size: u64 = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            println!("  Approximate size: {} MB", size / 1024 / 1024);
        }
        println!();

        let remove_data = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Remove data directory ({})?", data_dir.display()))
            .default(false)
            .interact()
            .unwrap_or(false);

        if remove_data {
            println!("Removing data directory...");
            let _ = std::fs::remove_dir_all(&data_dir);
        } else {
            println!("Keeping data directory.");
        }
    }

    if has_binary {
        println!();
        println!("To complete uninstallation, remove the binary:");
        println!("  rm {}", binary_path.display());
        println!();
        println!("(Cannot self-delete while running)");
    }

    println!();
    println!("==========================================");
    println!("  Uninstallation Complete");
    println!("==========================================");
    println!();

    let config_kept = config_dir.exists();
    let data_kept = data_dir.exists();
    if config_kept || data_kept {
        println!("Some directories were kept. Remove manually if needed:");
        if config_kept {
            println!("  rm -rf {}", config_dir.display());
        }
        if data_kept {
            println!("  rm -rf {}", data_dir.display());
        }
        println!();
    }

    Ok(())
}

async fn handle_waybar() -> Result<()> {
    let mut client = match DaemonClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            println!("{}", WaybarStatus::idle().to_json());
            return Ok(());
        }
    };

    match client.send(DaemonRequest::GetStatus).await? {
        DaemonResponse::Status(status) => {
            let waybar_status = if status.recording {
                let title = status.current_meeting.as_deref().unwrap_or("Recording");
                WaybarStatus::recording(title, status.uptime_seconds)
            } else {
                WaybarStatus::idle()
            };
            println!("{}", waybar_status.to_json());
        }
        _ => {
            println!("{}", WaybarStatus::idle().to_json());
        }
    }
    Ok(())
}

async fn handle_redo(id: Option<String>, clean: bool) -> Result<()> {
    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting_id = match id {
        Some(id) => id,
        None => select_meeting_with_audio(&db)?,
    };

    let meeting = db
        .get_meeting(&MeetingId::from_string(meeting_id.clone()))?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(meeting_id.clone()))?;

    let audio_path = meeting
        .audio_path
        .as_ref()
        .ok_or_else(|| crate::error::MuesliError::Audio("No audio file for this meeting".into()))?;

    if !audio_path.exists() {
        eprintln!("Error: Audio file not found: {:?}", audio_path);
        return Ok(());
    }

    println!("Re-processing: {}", meeting.title);
    println!("Audio file: {:?}", audio_path);

    let config = config::loader::load_config()?;
    let models_dir = config::loader::models_dir()?;

    let existing_segments = db.get_transcript_segments(&meeting.id)?;
    let needs_transcription = clean || existing_segments.is_empty();

    if needs_transcription {
        let step_count = if config.llm.provider != "none" { 3 } else { 2 };

        println!("\n[1/{}] Transcribing...", step_count);
        let transcript = run_transcription(&config, &models_dir, audio_path)?;
        println!("  {} segments transcribed", transcript.segments.len());

        db.delete_transcript_segments(&meeting.id)?;
        db.insert_transcript_segments(&meeting.id, &transcript.segments)?;

        println!(
            "\n[2/{}] Diarization (speaker identification)...",
            step_count
        );
        let diarization_manager = DiarizationModelManager::new(models_dir.clone());
        if diarization_manager.model_exists(DiarizationModel::SortformerV2) {
            let model_path = diarization_manager.model_path(DiarizationModel::SortformerV2);
            match run_diarization(audio_path, &model_path) {
                Ok(speaker_segments) => {
                    let mut segments = db.get_transcript_segments(&meeting.id)?;
                    for seg in segments.iter_mut() {
                        if let Some(speaker) = speaker_segments
                            .iter()
                            .find(|s| {
                                let mid = (seg.start_ms + seg.end_ms) / 2;
                                mid >= s.start_ms && mid <= s.end_ms
                            })
                            .map(|s| format!("SPEAKER_{}", s.speaker_id + 1))
                        {
                            seg.speaker = Some(speaker);
                        }
                    }
                    db.delete_transcript_segments(&meeting.id)?;
                    db.insert_transcript_segments(&meeting.id, &segments)?;
                    println!("  Speakers identified");
                }
                Err(e) => println!("  Skipped: {}", e),
            }
        } else {
            println!("  Skipped (model not installed)");
        }

        if config.llm.provider != "none" {
            println!("\n[3/{}] Summarizing...", step_count);
        }
    } else {
        println!(
            "\n  Using existing transcript ({} segments)",
            existing_segments.len()
        );
        if clean {
            println!("  (use --clean to re-transcribe from scratch)");
        }
        println!();
        println!("Summarizing...");
    }

    if config.llm.provider != "none" {
        let segments = db.get_transcript_segments(&meeting.id)?;
        let transcript = crate::transcription::Transcript::new(segments);
        match crate::llm::summarize_transcript(&config.llm, &transcript).await {
            Ok(summary) => {
                db.insert_summary(&meeting.id, &summary)?;
                println!("  Summary generated");

                let mut updated_meeting = meeting.clone();
                match crate::llm::generate_title(&config.llm, &summary.markdown).await {
                    Ok(title) => {
                        println!("  Title: {}", title);
                        updated_meeting.title = title;
                        let _ = db.update_meeting(&updated_meeting);
                    }
                    Err(e) => println!("  Title generation failed: {}", e),
                }

                let notes_dir = config::loader::notes_dir()?;
                let generator = crate::notes::markdown::NoteGenerator::new(notes_dir);
                if let Ok(path) = generator.generate(&updated_meeting, &transcript, &summary) {
                    println!("  Notes saved: {:?}", path);
                }
            }
            Err(e) => println!("  Failed: {}", e),
        }
    } else {
        println!("  Skipped (LLM not configured)");
    }

    println!("\nDone! View with: muesli notes {}", meeting_id);
    Ok(())
}

async fn handle_search(
    query: Option<String>,
    limit: usize,
    keyword: bool,
    action: Option<SearchCommands>,
) -> Result<()> {
    match action {
        Some(SearchCommands::Reindex) => {
            let config = config::loader::load_config()?;
            if !config.qmd.enabled {
                eprintln!("qmd search is not enabled. Run 'muesli setup' to configure.");
                return Ok(());
            }
            println!("Re-indexing meeting notes...");
            match crate::qmd::reindex(&config.qmd.collection_name) {
                Ok(()) => println!("Re-indexing complete."),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        Some(SearchCommands::Status) => match crate::qmd::status() {
            Ok(output) => print!("{}", output),
            Err(e) => eprintln!("Error: {}", e),
        },
        None => {
            if let Some(q) = query {
                let config = config::loader::load_config()?;
                if !config.qmd.enabled {
                    eprintln!("qmd search is not enabled. Run 'muesli setup' to configure.");
                    return Ok(());
                }
                match crate::qmd::search(&q, &config.qmd.collection_name, limit, keyword) {
                    Ok(output) => {
                        if output.trim().is_empty() {
                            println!("No results found for: {}", q);
                        } else {
                            print!("{}", output);
                        }
                    }
                    Err(e) => eprintln!("Error: {}", e),
                }
            } else {
                eprintln!("Usage: muesli search <query>");
                eprintln!("       muesli search reindex");
                eprintln!("       muesli search status");
            }
        }
    }
    Ok(())
}

async fn handle_ask(question: Vec<String>) -> Result<()> {
    if question.is_empty() {
        eprintln!("Usage: muesli ask <your question>");
        return Ok(());
    }

    let question_str = question.join(" ");
    crate::qmd::ask(&question_str).await?;
    Ok(())
}

fn select_meeting_with_audio(db: &Database) -> Result<String> {
    use dialoguer::{theme::ColorfulTheme, Select};

    let meetings: Vec<_> = db
        .list_meetings(20)?
        .into_iter()
        .filter(|m| m.audio_path.as_ref().map(|p| p.exists()).unwrap_or(false))
        .collect();

    if meetings.is_empty() {
        return Err(crate::error::MuesliError::Config(
            "No meetings with audio files found".to_string(),
        ));
    }

    let items: Vec<String> = meetings
        .iter()
        .map(|m| {
            let date = m.started_at.format("%Y-%m-%d %H:%M");
            let duration = m
                .duration_seconds
                .map(|d| format!("{}m", d / 60))
                .unwrap_or_else(|| "?".to_string());
            format!("{} | {} | {}", date, duration, truncate(&m.title, 40))
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a meeting to re-process")
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| crate::error::MuesliError::Config(format!("Selection cancelled: {}", e)))?;

    Ok(meetings[selection].id.0.clone())
}

fn run_transcription(
    config: &crate::config::settings::MuesliConfig,
    models_dir: &std::path::Path,
    audio_path: &std::path::Path,
) -> Result<crate::transcription::Transcript> {
    let manager = ModelManager::new(models_dir.to_path_buf());
    let model =
        WhisperModel::parse(config.transcription.effective_model()).unwrap_or(WhisperModel::Base);

    if !manager.model_exists(model) {
        return Err(crate::error::MuesliError::Config(format!(
            "Whisper model {:?} not found. Run: muesli models whisper download {}",
            model,
            config.transcription.effective_model()
        )));
    }

    let model_path = manager.model_path(model);
    let engine = crate::transcription::whisper::WhisperEngine::new(
        &model_path,
        config.transcription.use_gpu,
    )?;
    crate::transcription::whisper::transcribe_wav_file(&engine, audio_path)
}

fn run_diarization(
    audio_path: &std::path::Path,
    model_path: &std::path::Path,
) -> Result<Vec<crate::transcription::diarization::SpeakerSegment>> {
    let samples = load_wav_samples_for_diarization(audio_path)?;
    let mut diarizer = crate::transcription::diarization::Diarizer::new(model_path)?;
    diarizer.diarize(samples, 16000)
}

fn load_wav_samples_for_diarization(path: &std::path::Path) -> Result<Vec<f32>> {
    use hound::WavReader;

    let mut reader = WavReader::open(path)
        .map_err(|e| crate::error::MuesliError::Audio(format!("Failed to open WAV: {}", e)))?;

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
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
    };

    if spec.channels > 1 {
        Ok(samples
            .chunks(spec.channels as usize)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect())
    } else {
        Ok(samples)
    }
}

fn update_llm_config(provider: &str, model: &str, api_key: Option<&str>) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    let has_llm_section = content.contains("[llm]");
    let mut in_llm = false;
    let mut set_provider = false;
    let mut set_model = false;
    let mut set_api_key = false;
    const REMOVE_LINE: &str = "\x00__REMOVE__\x00";

    let mut updated: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[llm]" {
                in_llm = true;
            } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_llm = false;
            }

            if in_llm {
                if trimmed.starts_with("provider =") || trimmed.starts_with("engine =") {
                    set_provider = true;
                    return format!("provider = \"{}\"", provider);
                }
                if trimmed.starts_with("model =") || trimmed.starts_with("local_model =") {
                    set_model = true;
                    return format!("model = \"{}\"", model);
                }
                if trimmed.starts_with("api_key =")
                    || trimmed.starts_with("claude_api_key =")
                    || trimmed.starts_with("openai_api_key =")
                {
                    set_api_key = true;
                    if let Some(key) = api_key {
                        return format!("api_key = \"{}\"", key);
                    } else {
                        return REMOVE_LINE.to_string();
                    }
                }
                if trimmed.starts_with("claude_model =") || trimmed.starts_with("openai_model =") {
                    return REMOVE_LINE.to_string();
                }
            }
            line.to_string()
        })
        .filter(|line| line != REMOVE_LINE)
        .collect();

    if !has_llm_section {
        updated.push(String::new());
        updated.push("[llm]".to_string());
        updated.push(format!("provider = \"{}\"", provider));
        updated.push(format!("model = \"{}\"", model));
        if let Some(key) = api_key {
            updated.push(format!("api_key = \"{}\"", key));
        }
    } else {
        let llm_idx = updated.iter().position(|l| l.trim() == "[llm]");
        if let Some(idx) = llm_idx {
            let mut insert_at = idx + 1;
            while insert_at < updated.len() {
                let trimmed = updated[insert_at].trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    break;
                }
                insert_at += 1;
            }
            if !set_api_key {
                if let Some(key) = api_key {
                    updated.insert(insert_at, format!("api_key = \"{}\"", key));
                }
            }
            if !set_model {
                updated.insert(insert_at, format!("model = \"{}\"", model));
            }
            if !set_provider {
                updated.insert(insert_at, format!("provider = \"{}\"", provider));
            }
        }
    }

    std::fs::write(&config_path, updated.join("\n"))?;
    Ok(())
}

fn setup_cloud_provider(provider: &str, display_name: &str, default_model: &str) -> Result<()> {
    use dialoguer::{theme::ColorfulTheme, Input, Select};

    println!("  {} selected. An API key is required.", display_name);
    println!();

    let api_key: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Enter your {} API key", display_name))
        .interact_text()
        .unwrap_or_default();

    if api_key.trim().is_empty() {
        println!("  No API key provided. LLM disabled.");
        update_llm_config("none", "", None)?;
        return Ok(());
    }

    let catalog_models = crate::llm::catalog::models_for_provider(provider);
    let model = if catalog_models.is_empty() {
        default_model.to_string()
    } else {
        let mut options: Vec<String> = catalog_models
            .iter()
            .map(|m| format!("{} ({})", m.name, m.id))
            .collect();
        options.push("Enter custom model ID".to_string());

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a model")
            .items(&options)
            .default(0)
            .interact()
            .unwrap_or(0);

        if selection < catalog_models.len() {
            catalog_models[selection].id.clone()
        } else {
            let custom: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt("Enter model ID")
                .default(default_model.to_string())
                .interact_text()
                .unwrap_or_else(|_| default_model.to_string());
            custom
        }
    };

    update_llm_config(provider, &model, Some(api_key.trim()))?;
    println!("  LLM configured: {} (model: {})", display_name, model);
    Ok(())
}

fn discover_lms_models(lms: &str) -> Vec<String> {
    let mut models: Vec<String> = vec![];

    let output = std::process::Command::new(lms)
        .args(["ls", "--json"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            if let Ok(text) = String::from_utf8(out.stdout) {
                for line in text.lines() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(path) = json.get("path").and_then(|p| p.as_str()) {
                            let name = std::path::Path::new(path)
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or(path);
                            if !name.contains("embedding") && !name.contains("Embedding") {
                                models.push(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if models.is_empty() {
        let output = std::process::Command::new(lms).args(["ls"]).output();
        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty()
                        && !trimmed.starts_with("LLM")
                        && !trimmed.starts_with("EMBEDDING")
                        && !trimmed.starts_with("---")
                        && !trimmed.starts_with("You have")
                        && !trimmed.contains("embedding")
                        && !trimmed.contains("Embedding")
                    {
                        if let Some(name) = trimmed.split_whitespace().next() {
                            models.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    models
}

fn update_transcription_config(engine: &str, model: &str) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    let mut in_transcription = false;
    let updated = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[transcription]" {
                in_transcription = true;
            } else if trimmed.starts_with("[") && trimmed.ends_with("]") {
                in_transcription = false;
            }

            if in_transcription && trimmed.starts_with("engine =") {
                format!("engine = \"{}\"", engine)
            } else if in_transcription && trimmed.starts_with("model =") {
                format!("model = \"{}\"", model)
            } else if engine == "whisper" && trimmed.starts_with("whisper_model =") {
                format!("whisper_model = \"{}\"", model)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(&config_path, updated)?;
    Ok(())
}

fn update_config_value(key: &str, value: &str) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    let key_pattern = format!("{} =", key);
    let key_found = content
        .lines()
        .any(|line| line.trim().starts_with(&key_pattern));

    if key_found {
        let updated = content
            .lines()
            .map(|line| {
                if line.trim().starts_with(&key_pattern) {
                    if value == "true" || value == "false" || value.parse::<i64>().is_ok() {
                        format!("{} = {}", key, value)
                    } else {
                        format!("{} = \"{}\"", key, value)
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config_path, updated)?;
    } else {
        let mut content = content;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        if value == "true" || value == "false" || value.parse::<i64>().is_ok() {
            content.push_str(&format!("{} = {}\n", key, value));
        } else {
            content.push_str(&format!("{} = \"{}\"\n", key, value));
        }
        std::fs::write(&config_path, content)?;
    }

    Ok(())
}

fn update_audio_cues_config(enabled: bool) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    if content.contains("[audio_cues]") {
        let mut in_audio_cues = false;
        let updated = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed == "[audio_cues]" {
                    in_audio_cues = true;
                } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    in_audio_cues = false;
                }

                if in_audio_cues && trimmed.starts_with("enabled =") {
                    format!("enabled = {}", enabled)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config_path, updated)?;
    } else {
        let mut content = content;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!(
            "\n[audio_cues]\nenabled = {}\nvolume = 0.5\n",
            enabled
        ));
        std::fs::write(&config_path, content)?;
    }

    Ok(())
}

fn bun_global_bin_dir() -> Vec<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    vec![
        std::path::PathBuf::from(format!("{}/.bun/bin", home)),
        std::path::PathBuf::from(format!("{}/.cache/.bun/bin", home)),
    ]
}

fn add_bun_to_path() {
    if let Ok(current_path) = std::env::var("PATH") {
        let mut paths: Vec<String> = vec![];
        for dir in bun_global_bin_dir() {
            if dir.exists() {
                paths.push(dir.to_string_lossy().to_string());
            }
        }
        if !paths.is_empty() {
            paths.push(current_path);
            std::env::set_var("PATH", paths.join(":"));
        }
    }
}

fn find_bun() -> Option<String> {
    if std::process::Command::new("bun")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("bun".to_string());
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{}/.bun/bin/bun", home);
    if std::path::Path::new(&path).exists() {
        return Some(path);
    }

    None
}

fn install_bun() -> bool {
    println!("  bun not found. Installing bun...");
    let status = std::process::Command::new("sh")
        .args(["-c", "curl -fsSL https://bun.sh/install | bash"])
        .status();

    match status {
        Ok(s) if s.success() => {
            add_bun_to_path();
            println!("  bun installed!");
            true
        }
        _ => {
            println!("  Failed to install bun.");
            println!("  Install manually: https://bun.sh");
            false
        }
    }
}

fn install_qmd() -> bool {
    let bun = match find_bun() {
        Some(b) => b,
        None => {
            if !install_bun() {
                return false;
            }
            match find_bun() {
                Some(b) => b,
                None => {
                    println!("  bun installed but not found in PATH.");
                    return false;
                }
            }
        }
    };

    println!("  Installing qmd...");
    let status = std::process::Command::new(&bun)
        .args(["install", "-g", "github:tobi/qmd"])
        .status();

    match status {
        Ok(s) if s.success() => {
            add_bun_to_path();
            crate::qmd::indexer::is_qmd_installed()
        }
        _ => {
            println!("  qmd installation failed.");
            false
        }
    }
}

fn update_qmd_config(enabled: bool, auto_index: bool, collection_name: &str) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    if content.contains("[qmd]") {
        let mut in_qmd = false;
        let updated = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed == "[qmd]" {
                    in_qmd = true;
                } else if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    in_qmd = false;
                }

                if in_qmd {
                    if trimmed.starts_with("enabled =") {
                        return format!("enabled = {}", enabled);
                    }
                    if trimmed.starts_with("auto_index =") {
                        return format!("auto_index = {}", auto_index);
                    }
                    if trimmed.starts_with("collection_name =") {
                        return format!("collection_name = \"{}\"", collection_name);
                    }
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&config_path, updated)?;
    } else {
        let mut content = content;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!(
            "\n[qmd]\nenabled = {}\nauto_index = {}\ncollection_name = \"{}\"\n",
            enabled, auto_index, collection_name
        ));
        std::fs::write(&config_path, content)?;
    }
    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn print_segment(segment: &crate::transcription::TranscriptSegment) {
    match &segment.speaker {
        Some(speaker) => println!(
            "[{}] [{}] {}",
            segment.format_timestamp(),
            speaker,
            segment.text
        ),
        None => println!("[{}] {}", segment.format_timestamp(), segment.text),
    }
}
