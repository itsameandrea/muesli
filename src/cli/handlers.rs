use crate::cli::commands::*;
use crate::config;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::error::Result;
use crate::storage::database::Database;
use crate::storage::MeetingId;
use crate::transcription::diarization_models::{DiarizationModel, DiarizationModelManager};
use crate::transcription::models::{ModelManager, WhisperModel};
use crate::transcription::parakeet_models::{ParakeetModel, ParakeetModelManager};
use crate::transcription::Transcript;
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
                .ok_or_else(|| {
                    crate::error::MuesliError::MeetingNotFound(meeting_id.clone())
                })?;

            let segments = db.get_transcript_segments(&meeting.id)?;
            
            if !segments.is_empty() {
                println!("\nTranscript ({} segments, processing speakers in background):\n", segments.len());
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
        println!("**Date:** {} | **Duration:** {}\n", 
            meeting.started_at.format("%Y-%m-%d %H:%M"),
            meeting.duration_seconds.map(|d| format!("{}m {}s", d / 60, d % 60)).unwrap_or("?".to_string())
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
            let duration = m.duration_seconds
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

    let mut transcript = match engine.as_str() {
        "whisper" => {
            let models_dir = config::loader::models_dir()?;
            let manager = ModelManager::new(models_dir);

            let model = WhisperModel::from_str(&cfg.transcription.whisper_model)
                .unwrap_or(WhisperModel::Base);
            
            if !manager.model_exists(model) {
                return Err(crate::error::MuesliError::Config(format!(
                    "Whisper model '{}' not downloaded. Run: muesli models download {}",
                    cfg.transcription.whisper_model, cfg.transcription.whisper_model
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

            let model = ParakeetModel::from_str(&cfg.transcription.parakeet_model)
                .unwrap_or(ParakeetModel::TdtV3);
            
            if !manager.model_exists(model) {
                return Err(crate::error::MuesliError::Config(format!(
                    "Parakeet model '{}' not downloaded. Run: muesli parakeet download {}",
                    cfg.transcription.parakeet_model, cfg.transcription.parakeet_model
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
        _ => return Err(crate::error::MuesliError::Config(format!(
            "Unknown transcription engine '{}'. Use: whisper, parakeet, deepgram, or openai",
            engine
        ))),
    };

    let models_dir = config::loader::models_dir()?;
    let diar_manager = DiarizationModelManager::new(models_dir);
    
    let diar_model = DiarizationModel::SortformerV2;
    if !diar_manager.model_exists(diar_model) {
        return Err(crate::error::MuesliError::Config(
            "Diarization model not found. Run: muesli diarization download sortformer-v2".into()
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
                    "No API key configured. Set deepgram_api_key or openai_api_key in config".into(),
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
            .map_err(|e| crate::error::MuesliError::Config(format!("Download task failed: {}", e)))??;

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
    let meeting = db
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

            db.insert_summary(&meeting_id, &summary)?;
            println!("\n---\nSaved to database.");

            let notes_dir = config::loader::notes_dir()?;
            let generator = crate::notes::markdown::NoteGenerator::new(notes_dir);
            match generator.generate(&meeting, &transcript, &summary) {
                Ok(path) => {
                    println!("Notes file: {}", path.display());
                    let mut updated = meeting.clone();
                    updated.notes_path = Some(path);
                    let _ = db.update_meeting(&updated);
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn print_segment(segment: &crate::transcription::TranscriptSegment) {
    match &segment.speaker {
        Some(speaker) => println!("[{}] [{}] {}", segment.format_timestamp(), speaker, segment.text),
        None => println!("[{}] {}", segment.format_timestamp(), segment.text),
    }
}
