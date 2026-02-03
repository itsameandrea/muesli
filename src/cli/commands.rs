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

    /// View meeting notes
    View {
        /// Meeting ID to view
        id: String,
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

    /// Audio device management
    Audio {
        #[command(subcommand)]
        action: AudioCommands,
    },
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
        /// Model name: tiny, base, small, medium, large
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
