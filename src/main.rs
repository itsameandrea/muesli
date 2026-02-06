use clap::Parser;
use tracing_subscriber::EnvFilter;

mod audio;
mod cli;
mod config;
mod daemon;
mod detection;
mod error;
mod llm;
mod notes;
mod notification;
mod qmd;
mod storage;
mod transcription;
mod waybar;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = cli::Cli::parse();

    if let Err(e) = cli::handle_command(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
