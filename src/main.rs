use clap::Parser;
use tracing_subscriber::EnvFilter;

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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .init();

    let cli = cli::Cli::parse();

    if let Err(e) = cli::handle_command(cli).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
