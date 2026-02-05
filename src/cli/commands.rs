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

    /// Toggle recording on/off
    Toggle,

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

    /// Transcribe a meeting recording
    Transcribe {
        /// Meeting ID to transcribe
        id: String,
        /// Use hosted API instead of local Whisper
        #[arg(long)]
        hosted: bool,
    },

    /// Run daemon mode (background meeting detection)
    Daemon,

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// Whisper model management
    Models {
        #[command(subcommand)]
        action: ModelCommands,
    },

    /// Parakeet model management (ONNX-based, faster)
    Parakeet {
        #[command(subcommand)]
        action: ParakeetCommands,
    },

    /// Audio device management
    Audio {
        #[command(subcommand)]
        action: AudioCommands,
    },

    /// Speaker diarization model management
    Diarization {
        #[command(subcommand)]
        action: DiarizationCommands,
    },

    /// Generate AI summary for a meeting
    Summarize {
        /// Meeting ID to summarize
        id: String,
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
    /// Print config file path
    Path,
    /// Initialize default configuration
    Init,
}

#[derive(Subcommand)]
pub enum ModelCommands {
    /// List available Whisper models
    List,
    /// Download a Whisper model
    Download {
        /// Model name: tiny, base, small, medium, large, large-v3-turbo, distil-large-v3
        model: String,
    },
    /// Delete a downloaded model
    Delete { model: String },
}

#[derive(Subcommand)]
pub enum ParakeetCommands {
    /// List available Parakeet models
    List,
    /// Download a Parakeet model
    Download {
        /// Model name: parakeet-v3, parakeet-v3-int8, nemotron-streaming
        model: String,
    },
    /// Delete a downloaded model
    Delete { model: String },
}

#[derive(Subcommand)]
pub enum AudioCommands {
    /// List available audio devices
    #[command(name = "list-devices")]
    ListDevices,
    /// Test microphone capture
    #[command(name = "test-mic")]
    TestMic {
        #[arg(short, long, default_value = "3")]
        duration: u64,
    },
    /// Test loopback capture
    #[command(name = "test-loopback")]
    TestLoopback {
        #[arg(short, long, default_value = "3")]
        duration: u64,
    },
}

#[derive(Subcommand)]
pub enum DiarizationCommands {
    /// List available diarization models
    List,
    /// Download a diarization model
    Download {
        /// Model name: sortformer-v2
        model: String,
    },
    /// Delete a downloaded model
    Delete { model: String },
}
