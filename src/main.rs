use clap::Parser;

mod cli;
mod config;
mod daemon;
mod audio;
mod transcription;
mod detection;
mod notes;
mod llm;
mod storage;
mod notification;
mod error;

#[derive(Parser)]
#[command(name = "muesli")]
#[command(about = "AI-powered meeting note-taker for Linux/Hyprland")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Start recording a meeting
    Start {
        #[arg(short, long)]
        title: Option<String>,
    },
    /// Stop recording
    Stop,
    /// Show current status
    Status,
    /// List recorded meetings
    List {
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// View a meeting's notes
    View {
        id: String,
    },
    /// Run in daemon mode
    Daemon,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage Whisper models
    Models {
        #[command(subcommand)]
        action: ModelAction,
    },
}

#[derive(clap::Subcommand)]
enum ConfigAction {
    Show,
    Edit,
    Path,
}

#[derive(clap::Subcommand)]
enum ModelAction {
    List,
    Download { model: String },
}

fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        Some(Commands::Start { title }) => {
            println!("Starting recording: {:?}", title);
        }
        Some(Commands::Stop) => {
            println!("Stopping recording...");
        }
        Some(Commands::Status) => {
            println!("Status: idle");
        }
        Some(Commands::List { limit }) => {
            println!("Listing last {} meetings...", limit);
        }
        Some(Commands::View { id }) => {
            println!("Viewing meeting: {}", id);
        }
        Some(Commands::Daemon) => {
            println!("Starting daemon...");
        }
        Some(Commands::Config { action }) => {
            match action {
                ConfigAction::Show => println!("Showing config..."),
                ConfigAction::Edit => println!("Opening config editor..."),
                ConfigAction::Path => println!("~/.config/muesli/config.toml"),
            }
        }
        Some(Commands::Models { action }) => {
            match action {
                ModelAction::List => println!("Available models: tiny, base, small, medium, large"),
                ModelAction::Download { model } => println!("Downloading model: {}", model),
            }
        }
        None => {
            println!("Use --help for usage");
        }
    }
}
