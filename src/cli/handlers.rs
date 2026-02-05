use crate::cli::commands::*;
use crate::config;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::error::Result;
use crate::llm::local::find_lms_binary;
use crate::storage::database::Database;
use crate::storage::MeetingId;
use crate::transcription::diarization_models::{DiarizationModel, DiarizationModelManager};
use crate::transcription::models::{ModelManager, WhisperModel};
use crate::transcription::parakeet_models::{ParakeetModel, ParakeetModelManager};
use crate::transcription::Transcript;
use crate::waybar::WaybarStatus;
use cpal::traits::{DeviceTrait, HostTrait};
use std::io::Write;
use std::path::Path;

pub async fn handle_command(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Start { title } => handle_start(title).await,
        Commands::Stop => handle_stop().await,
        Commands::Toggle => handle_toggle().await,
        Commands::Status => handle_status().await,
        Commands::List { limit } => handle_list(limit).await,
        Commands::Notes { id } => handle_notes(id).await,
        Commands::Transcript { id } => handle_transcript(id).await,
        Commands::Transcribe { id, hosted } => handle_transcribe(&id, hosted).await,
        Commands::Daemon => handle_daemon().await,
        Commands::Config { action } => handle_config(action).await,
        Commands::Models { action } => handle_models(action).await,
        Commands::Parakeet { action } => handle_parakeet(action).await,
        Commands::Audio { action } => handle_audio(action).await,
        Commands::Diarization { action } => handle_diarization(action).await,
        Commands::Summarize { id } => handle_summarize(&id).await,
        Commands::Setup => handle_setup().await,
        Commands::Uninstall => handle_uninstall().await,
        Commands::Waybar => handle_waybar().await,
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

async fn handle_toggle() -> Result<()> {
    let mut client = match DaemonClient::connect().await {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Error: Daemon is not running. Start it with: muesli daemon");
            return Ok(());
        }
    };

    match client.send(DaemonRequest::GetStatus).await? {
        DaemonResponse::Status(status) => {
            if status.recording {
                drop(client);
                return handle_stop().await;
            } else {
                drop(client);
                return handle_start(None).await;
            }
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
        return Err(crate::error::MuesliError::Config("No meetings found".to_string()).into());
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

async fn do_transcribe<P: AsRef<Path>>(audio_path: P, verbose: bool) -> Result<Transcript> {
    let cfg = config::loader::load_config()?;
    let engine = cfg.transcription.engine.to_lowercase();
    let audio_path = audio_path.as_ref();

    let model_name = cfg.transcription.effective_model();

    let mut transcript = match engine.as_str() {
        "whisper" => {
            let models_dir = config::loader::models_dir()?;
            let manager = ModelManager::new(models_dir);

            let model = WhisperModel::from_str(model_name).unwrap_or(WhisperModel::Base);

            if !manager.model_exists(model) {
                return Err(crate::error::MuesliError::Config(format!(
                    "Whisper model '{}' not downloaded. Run: muesli models download {}",
                    model_name, model_name
                )));
            }

            if verbose {
                println!("Using local Whisper ({})...", model);
            }
            let whisper = crate::transcription::whisper::WhisperEngine::from_model(
                &manager,
                model,
                cfg.transcription.use_gpu,
            )?;
            crate::transcription::whisper::transcribe_wav_file(&whisper, audio_path)?
        }
        "parakeet" => {
            let models_dir = config::loader::models_dir()?;
            let manager = ParakeetModelManager::new(models_dir);

            let model = ParakeetModel::from_str(model_name).unwrap_or(ParakeetModel::TdtV3);

            if !manager.model_exists(model) {
                return Err(crate::error::MuesliError::Config(format!(
                    "Parakeet model '{}' not downloaded. Run: muesli parakeet download {}",
                    model_name, model_name
                )));
            }

            if verbose {
                println!("Using local Parakeet ({})...", model);
            }
            let model_dir = manager.model_dir(model);
            let mut parakeet = crate::transcription::parakeet::ParakeetEngine::new();
            parakeet.load_model(&model_dir, model.uses_int8())?;
            crate::transcription::parakeet::transcribe_wav_file(&mut parakeet, audio_path)?
        }
        "deepgram" => {
            let api_key = cfg.transcription.deepgram_api_key.as_ref().ok_or_else(|| {
                crate::error::MuesliError::Config(
                    "Deepgram API key not configured. Set deepgram_api_key in config".into(),
                )
            })?;

            if verbose {
                println!("Using Deepgram API...");
            }
            crate::transcription::deepgram::transcribe_file(api_key, audio_path).await?
        }
        "openai" => {
            let api_key = cfg.transcription.openai_api_key.as_ref().ok_or_else(|| {
                crate::error::MuesliError::Config(
                    "OpenAI API key not configured. Set openai_api_key in config".into(),
                )
            })?;

            if verbose {
                println!("Using OpenAI Whisper API...");
            }
            crate::transcription::openai::transcribe_file(api_key, audio_path).await?
        }
        _ => {
            return Err(crate::error::MuesliError::Config(format!(
                "Unknown transcription engine '{}'. Use: whisper, parakeet, deepgram, or openai",
                engine
            )))
        }
    };

    let models_dir = config::loader::models_dir()?;
    let diar_manager = DiarizationModelManager::new(models_dir);

    let diar_model = DiarizationModel::SortformerV2;
    if !diar_manager.model_exists(diar_model) {
        return Err(crate::error::MuesliError::Config(
            "Diarization model not found. Run: muesli diarization download sortformer-v2".into(),
        ));
    }

    if verbose {
        println!("Running speaker diarization...");
    }

    let model_path = diar_manager.model_path(diar_model);
    let samples = load_wav_samples(audio_path)?;

    crate::transcription::diarization::diarize_transcript(
        &model_path,
        &samples,
        16000,
        &mut transcript,
    )?;

    Ok(transcript)
}

fn load_wav_samples<P: AsRef<Path>>(path: P) -> Result<Vec<f32>> {
    use hound::WavReader;

    let mut reader = WavReader::open(path.as_ref())
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

async fn handle_transcribe(id: &str, hosted: bool) -> Result<()> {
    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting = db
        .get_meeting(&MeetingId::from_string(id.to_string()))?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(id.to_string()))?;

    let audio_path = meeting
        .audio_path
        .as_ref()
        .ok_or_else(|| crate::error::MuesliError::Audio("No audio file for this meeting".into()))?;

    if !audio_path.exists() {
        return Err(crate::error::MuesliError::Audio(format!(
            "Audio file not found: {}",
            audio_path.display()
        )));
    }

    println!("Transcribing: {}", meeting.title);
    println!("Audio: {}", audio_path.display());

    let transcript = if hosted {
        let cfg = config::loader::load_config()?;
        let api_key = cfg
            .transcription
            .deepgram_api_key
            .as_ref()
            .or(cfg.transcription.openai_api_key.as_ref())
            .ok_or_else(|| {
                crate::error::MuesliError::Config(
                    "No API key configured. Set deepgram_api_key or openai_api_key in config"
                        .into(),
                )
            })?;

        println!("Using hosted API...");
        if cfg.transcription.deepgram_api_key.is_some() {
            crate::transcription::deepgram::transcribe_file(api_key, audio_path).await?
        } else {
            crate::transcription::openai::transcribe_file(api_key, audio_path).await?
        }
    } else {
        do_transcribe(audio_path, true).await?
    };

    println!("\nTranscript ({} segments):\n", transcript.segments.len());
    for segment in &transcript.segments {
        print_segment(segment);
    }

    db.insert_transcript_segments(&meeting.id, &transcript.segments)?;
    println!("\nTranscript saved to database.");

    Ok(())
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

async fn handle_parakeet(action: ParakeetCommands) -> Result<()> {
    let models_dir = config::loader::models_dir()?;
    let manager = ParakeetModelManager::new(models_dir);

    match action {
        ParakeetCommands::List => {
            println!("{:<20} {:<12} {:<10}", "Model", "Size (MB)", "Downloaded");
            println!("{}", "-".repeat(45));

            for (model, exists, size) in manager.list_all() {
                let status = if exists { "✓" } else { "-" };
                println!("{:<20} {:<12} {:<10}", model, size, status);
            }
        }
        ParakeetCommands::Download { model } => {
            let parakeet_model = ParakeetModel::from_str(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!(
                    "Unknown model: {}. Use: parakeet-v3, parakeet-v3-int8, nemotron-streaming",
                    model
                ))
            })?;

            println!(
                "Downloading {} (~{} MB total)...",
                parakeet_model,
                parakeet_model.size_mb()
            );

            let path = manager.download_model(parakeet_model, |filename, downloaded, total| {
                let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                print!(
                    "\r{}: {}% ({}/{} MB)    ",
                    filename,
                    percent,
                    downloaded / 1024 / 1024,
                    total / 1024 / 1024
                );
                std::io::stdout().flush().ok();
            })?;

            println!("\nDownloaded to: {}", path.display());
        }
        ParakeetCommands::Delete { model } => {
            let parakeet_model = ParakeetModel::from_str(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!("Unknown model: {}", model))
            })?;

            manager.delete_model(parakeet_model)?;
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

async fn handle_diarization(action: DiarizationCommands) -> Result<()> {
    let models_dir = config::loader::models_dir()?;
    let manager = DiarizationModelManager::new(models_dir);

    match action {
        DiarizationCommands::List => {
            println!("{:<20} {:<12} {:<10}", "Model", "Size (MB)", "Downloaded");
            println!("{}", "-".repeat(45));

            for (model, exists, size) in manager.list_all() {
                let status = if exists { "✓" } else { "-" };
                println!("{:<20} {:<12} {:<10}", model, size, status);
            }
        }
        DiarizationCommands::Download { model } => {
            let diar_model = DiarizationModel::from_str(&model).ok_or_else(|| {
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
        DiarizationCommands::Delete { model } => {
            let diar_model = DiarizationModel::from_str(&model).ok_or_else(|| {
                crate::error::MuesliError::Config(format!("Unknown model: {}", model))
            })?;

            manager.delete_model(diar_model)?;
            println!("Deleted {} model", model);
        }
    }
    Ok(())
}

async fn handle_summarize(id: &str) -> Result<()> {
    let cfg = config::loader::load_config()?;

    if cfg.llm.engine == "none" {
        eprintln!("Error: LLM engine not configured. Set [llm] engine = \"local\" in config.");
        return Ok(());
    }

    let db_path = config::loader::database_path()?;
    let db = Database::open(&db_path)?;

    let meeting_id = MeetingId::from_string(id.to_string());
    let mut meeting = db
        .get_meeting(&meeting_id)?
        .ok_or_else(|| crate::error::MuesliError::MeetingNotFound(id.to_string()))?;

    println!("Summarizing meeting: {}", meeting.title);

    let segments = db.get_transcript_segments(&meeting_id)?;
    if segments.is_empty() {
        eprintln!("Error: No transcript found for this meeting.");
        return Ok(());
    }

    println!("Found {} transcript segments", segments.len());
    println!("Calling LLM (this may take a while)...\n");

    let transcript = crate::transcription::Transcript::new(segments);
    let result = crate::llm::summarize_transcript(&cfg.llm, &transcript).await;

    match result {
        Ok(summary) => {
            println!("{}\n", summary.markdown);

            if meeting.title == "Untitled Meeting" {
                println!("Generating title...");
                match crate::llm::generate_title(&cfg.llm, &summary.markdown).await {
                    Ok(title) => {
                        println!("Generated title: {}", title);
                        meeting.title = title;
                        let _ = db.update_meeting(&meeting);
                    }
                    Err(e) => {
                        eprintln!("Failed to generate title: {}", e);
                    }
                }
            }

            db.insert_summary(&meeting_id, &summary)?;
            println!("\n---\nSaved to database.");

            let notes_dir = config::loader::notes_dir()?;
            let generator = crate::notes::markdown::NoteGenerator::new(notes_dir);
            match generator.generate(&meeting, &transcript, &summary) {
                Ok(path) => {
                    println!("Notes file: {}", path.display());
                    meeting.notes_path = Some(path);
                    let _ = db.update_meeting(&meeting);
                }
                Err(e) => {
                    eprintln!("Failed to save notes file: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Error generating summary: {}", e);
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

    println!("[1/10] Creating directories...");
    config::loader::ensure_directories()?;
    let config_dir = config::loader::config_dir()?;
    let data_dir = config::loader::data_dir()?;
    let models_dir = config::loader::models_dir()?;
    println!("  Config: {}", config_dir.display());
    println!("  Data:   {}", data_dir.display());
    println!("  Models: {}", models_dir.display());
    println!();

    println!("[2/10] Initializing configuration...");
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

    println!("[3/10] GPU Acceleration");
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

    println!("[4/10] Transcription Model Selection");
    println!();

    let whisper_models = vec![
        ("tiny", 75, "Fastest, lowest accuracy"),
        ("base", 142, "Good balance (recommended)"),
        ("small", 466, "Better accuracy"),
        ("medium", 1500, "High accuracy"),
        ("large", 2900, "Best accuracy"),
        ("large-v3-turbo", 1620, "Fast + high quality"),
    ];

    let parakeet_models = vec![
        ("parakeet-v3", 632, "Full precision, best quality"),
        (
            "parakeet-v3-int8",
            217,
            "INT8 quantized, fastest (recommended)",
        ),
    ];

    let whisper_manager = ModelManager::new(models_dir.clone());
    let parakeet_manager = ParakeetModelManager::new(models_dir.clone());

    let mut model_options: Vec<String> = vec![];

    model_options.push("--- Whisper Models (whisper.cpp) ---".to_string());
    for (name, size, desc) in &whisper_models {
        let model = WhisperModel::from_str(name).unwrap();
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

    model_options.push("--- Parakeet Models (ONNX, 20-30x faster) ---".to_string());
    for (name, size, desc) in &parakeet_models {
        let model = ParakeetModel::from_str(name).unwrap();
        let installed = if parakeet_manager.model_exists(model) {
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
    } else if selection >= 1 && selection <= 6 {
        let model_name = whisper_models[selection - 1].0;
        let model = WhisperModel::from_str(model_name).unwrap();

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
    } else if selection >= 8 && selection <= 9 {
        let model_name = parakeet_models[selection - 8].0;
        let model = ParakeetModel::from_str(model_name).unwrap();

        if parakeet_manager.model_exists(model) {
            println!("  Model '{}' is already installed", model_name);
        } else {
            println!("  Downloading {} model...", model_name);
            let path = parakeet_manager.download_model(model, |filename, downloaded, total| {
                let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                print!(
                    "\r  {}: {}% ({}/{} MB)    ",
                    filename,
                    percent,
                    downloaded / 1024 / 1024,
                    total / 1024 / 1024
                );
                std::io::stdout().flush().ok();
            })?;
            println!("\n  Downloaded to: {}", path.display());
        }

        update_transcription_config("parakeet", model_name)?;
    }
    println!();

    println!("[5/10] Speaker Diarization Model");
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

    println!("[6/10] Streaming Transcription (Optional)");
    println!("  Nemotron streaming enables real-time transcription during recording.");
    println!("  No wait time after stopping - transcription is already done!");
    println!();

    let nemotron_model = ParakeetModel::NemotronStreaming;
    let parakeet_manager = ParakeetModelManager::new(models_dir.clone());

    if parakeet_manager.model_exists(nemotron_model) {
        println!("  Nemotron streaming model already installed");
    } else {
        let download_nemotron = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Download Nemotron streaming model (~2.5 GB)?")
            .default(false)
            .interact()
            .unwrap_or(false);

        if download_nemotron {
            println!("  Downloading nemotron-streaming (this may take a while)...");
            let path = parakeet_manager.download_model(
                nemotron_model,
                |filename, downloaded, total| {
                    let percent = (downloaded as f64 / total as f64 * 100.0) as u32;
                    print!(
                        "\r  {}: {}% ({}/{} MB)    ",
                        filename,
                        percent,
                        downloaded / 1024 / 1024,
                        total / 1024 / 1024
                    );
                    std::io::stdout().flush().ok();
                },
            )?;
            println!("\n  Downloaded to: {}", path.display());
        } else {
            println!("  Skipping streaming model");
            println!(
                "  (You can download later with: muesli parakeet download nemotron-streaming)"
            );
        }
    }
    println!();

    println!("[7/10] LLM for Meeting Notes");
    println!("  An LLM generates meeting summaries and notes from transcripts.");
    println!("  LM Studio provides free local LLM support.");
    println!();

    let lms_path = find_lms_binary();
    if let Some(ref lms) = lms_path {
        println!("  Found LM Studio CLI at: {}", lms);

        let output = std::process::Command::new(lms)
            .args(["ls", "--json"])
            .output();

        let mut models: Vec<String> = vec![];
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

        if !models.is_empty() {
            println!("  Found {} LLM model(s) in LM Studio", models.len());
            println!();

            let mut options: Vec<String> = models.iter().map(|m| m.clone()).collect();
            options.push("Skip LLM setup".to_string());

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select an LLM model for meeting notes")
                .items(&options)
                .default(0)
                .interact()
                .unwrap_or(options.len() - 1);

            if selection < models.len() {
                let model = &models[selection];
                update_llm_config("local", model)?;
                println!("  LLM configured: {} (via LM Studio)", model);
            } else {
                update_llm_config("none", "")?;
                println!("  LLM disabled");
            }
        } else {
            println!("  No LLM models found in LM Studio.");
            println!("  Download a model in LM Studio first, then run setup again.");
            update_llm_config("none", "")?;
        }
    } else {
        println!("  LM Studio not found.");
        println!("  Install from https://lmstudio.ai for local LLM support.");
        println!("  Or configure Claude/OpenAI API keys in config.toml");
        update_llm_config("none", "")?;
    }
    println!();

    println!("[8/10] Meeting Detection");
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

    println!("[9/10] Audio Cues");
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

    println!("[10/10] Systemd Service");
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

    println!("==========================================");
    println!("  Setup Complete!");
    println!("==========================================");
    println!();
    println!("Next steps:");
    println!();
    println!("  1. Start the daemon:");
    println!("     muesli daemon");
    println!();
    println!("  2. Or enable auto-start:");
    println!("     systemctl --user enable --now muesli.service");
    println!();
    println!("  3. Test audio devices:");
    println!("     muesli audio list-devices");
    println!();
    println!("  4. Edit configuration if needed:");
    println!("     muesli config edit");
    println!();

    Ok(())
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

fn update_llm_config(engine: &str, model: &str) -> Result<()> {
    let config_path = config::loader::config_path()?;
    let content = std::fs::read_to_string(&config_path)?;

    let mut in_llm = false;
    let updated = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == "[llm]" {
                in_llm = true;
            } else if trimmed.starts_with("[") && trimmed.ends_with("]") {
                in_llm = false;
            }

            if in_llm && trimmed.starts_with("engine =") {
                format!("engine = \"{}\"", engine)
            } else if in_llm && trimmed.starts_with("local_model =") {
                format!("local_model = \"{}\"", model)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(&config_path, updated)?;
    Ok(())
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
            } else if engine == "parakeet" && trimmed.starts_with("parakeet_model =") {
                format!("parakeet_model = \"{}\"", model)
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
