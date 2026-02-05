use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "muesli")]
#[command(
    author,
    version,
    about = "AI-powered meeting note-taker for Linux/Hyprland"
)]
#[command(
    long_about = "Automatically record, transcribe, and summarize meetings with AI-powered note generation"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start recording a meeting
    Start {
        /// Meeting title (auto-detected if not provided)
        #[arg(short, long)]
        title: Option<String>,
    },

    /// Stop recording and process notes
    Stop,

    /// Show current recording status
    Status,

    /// List recorded meetings
    List {
        /// Maximum number of meetings to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// View meeting notes and summary
    Notes {
        /// Meeting ID (interactive selection if omitted)
        id: Option<String>,
    },

    /// View meeting transcript
    Transcript {
        /// Meeting ID (interactive selection if omitted)
        id: Option<String>,
    },

    /// Run daemon mode (background meeting detection)
    Daemon,

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// Model management
    Models {
        #[command(subcommand)]
        engine: ModelEngine,
    },

    /// Audio device management
    Audio {
        #[command(subcommand)]
        action: AudioCommands,
    },

    /// Interactive setup wizard for first-time configuration
    Setup,

    /// Uninstall muesli completely
    Uninstall,

    /// Output status in Waybar JSON format (for custom module integration)
    Waybar,
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Open config file in editor
    Edit,
}

#[derive(Subcommand)]
pub enum ModelEngine {
    /// Whisper models (whisper.cpp)
    Whisper {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Parakeet models (ONNX, 20-30x faster)
    Parakeet {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Speaker diarization models
    Diarization {
        #[command(subcommand)]
        action: ModelAction,
    },
}

#[derive(Subcommand)]
pub enum ModelAction {
    /// List available models
    List,
    /// Download a model
    Download { model: String },
    /// Delete a downloaded model
    Delete { model: String },
}

#[derive(Subcommand)]
pub enum AudioCommands {
    /// List available audio devices
    #[command(name = "list-devices")]
    ListDevices,
}
