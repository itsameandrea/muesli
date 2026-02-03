use crate::cli::commands::*;
use crate::config;
use crate::storage::database::Database;
use crate::storage::MeetingId;
use crate::transcription::models::{ModelManager, WhisperModel};
use crate::error::Result;
use std::io::Write;
use cpal::traits::{HostTrait, DeviceTrait};

pub async fn handle_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Start { title } => handle_start(title).await,
        Commands::Stop => handle_stop().await,
        Commands::Status => handle_status().await,
        Commands::List { limit } => handle_list(limit).await,
        Commands::View { id } => handle_view(&id).await,
        Commands::Daemon => handle_daemon().await,
        Commands::Config { action } => handle_config(action).await,
        Commands::Models { action } => handle_models(action).await,
        Commands::Audio { action } => handle_audio(action).await,
    }
}

async fn handle_start(title: Option<String>) -> Result<()> {
    let title = title.unwrap_or_else(|| "Untitled Meeting".to_string());
    println!("Starting recording: {}", title);
    println!("(Daemon mode required for full functionality)");
    Ok(())
}

async fn handle_stop() -> Result<()> {
    println!("Stopping recording...");
    println!("(Daemon mode required for full functionality)");
    Ok(())
}

async fn handle_status() -> Result<()> {
    println!("Status: idle");
    println!("Daemon: not running");
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

    println!("{:<36} {:<30} {:<10} {:<10}", "ID", "Title", "Status", "Duration");
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

async fn handle_view(id: &str) -> Result<()> {
    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting = db
        .get_meeting(&MeetingId::from_string(id.to_string()))?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(id.to_string()))?;

    println!("Meeting: {}", meeting.title);
    println!("ID: {}", meeting.id);
    println!("Started: {}", meeting.started_at);
    if let Some(ended) = meeting.ended_at {
        println!("Ended: {}", ended);
    }
    if let Some(duration) = meeting.duration_seconds {
        println!("Duration: {} minutes", duration / 60);
    }
    println!("Status: {}", meeting.status);

    if let Some(notes_path) = &meeting.notes_path {
        if notes_path.exists() {
            println!("\n--- Notes ---\n");
            let content = std::fs::read_to_string(notes_path)?;
            println!("{}", content);
        }
    }

    Ok(())
}

async fn handle_daemon() -> Result<()> {
    println!("Starting daemon...");
    println!("(Full daemon implementation in Task 17)");
    Ok(())
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
        ConfigCommands::Path => {
            println!("{}", config::loader::config_path()?.display());
        }
        ConfigCommands::Init => {
            config::loader::ensure_directories()?;
            let cfg = config::loader::load_config()?;
            println!(
                "Configuration initialized at: {}",
                config::loader::config_path()?.display()
            );
            println!("\nDefault settings:");
            println!("  Transcription: local (Whisper)");
            println!("  Sample rate: {} Hz", cfg.audio.sample_rate);
            println!("  Capture system audio: {}", cfg.audio.capture_system_audio);
        }
    }
    Ok(())
}

async fn handle_models(action: ModelCommands) -> Result<()> {
    let models_dir = config::loader::models_dir()?;
    let manager = ModelManager::new(models_dir);

    match action {
        ModelCommands::List => {
            println!("{:<10} {:<12} {:<10}", "Model", "Size (MB)", "Downloaded");
            println!("{}", "-".repeat(35));

            for (model, exists, size) in manager.list_all() {
                let status = if exists { "✓" } else { "-" };
                println!("{:<10} {:<12} {:<10}", model, size, status);
            }
        }
        ModelCommands::Download { model } => {
            let whisper_model = WhisperModel::from_str(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!(
                    "Unknown model: {}. Use: tiny, base, small, medium, large",
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
        ModelCommands::Delete { model } => {
            let whisper_model = WhisperModel::from_str(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!("Unknown model: {}", model))
            })?;

            manager.delete_model(whisper_model)?;
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
        AudioCommands::TestMic { duration } => {
            println!("Testing microphone for {} seconds...", duration);
            println!("(Full implementation requires recorder module)");
        }
        AudioCommands::TestLoopback { duration } => {
            println!("Testing loopback for {} seconds...", duration);
            println!("(Full implementation requires recorder module)");
        }
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
