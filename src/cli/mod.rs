pub mod commands;
pub mod handlers;

pub use commands::{Cli, Commands, ConfigCommands, ModelCommands, AudioCommands};
pub use handlers::handle_command;
