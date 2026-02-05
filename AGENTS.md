# AGENTS.md - muesli

AI-powered meeting note-taker for Linux/Hyprland. Rust codebase using Cargo.

## Build, Lint, Test Commands

```bash
# Build
cargo build              # Debug build
cargo build --release    # Release build (optimized)

# Feature flags for GPU acceleration
cargo build --release --features vulkan  # Vulkan GPU
cargo build --release --features cuda    # NVIDIA CUDA
cargo build --release --features metal   # Apple Metal

# Lint
cargo fmt --check        # Check formatting (CI enforced)
cargo fmt                # Auto-fix formatting
cargo clippy -- -D warnings  # Lint with warnings as errors (CI enforced)

# Test
cargo test               # Run all tests
cargo test <test_name>   # Run single test by name
cargo test <module>::    # Run tests in module (e.g., cargo test config::)
cargo test -- --nocapture  # Show println! output

# Run single test example:
cargo test test_default_config_creates
cargo test config::settings::tests::test_audio_config_defaults

# Check (fast compile check without codegen)
cargo check
```

## Project Structure

```
src/
├── main.rs           # Entry point, tokio async main
├── lib.rs            # Library root, re-exports all modules
├── error.rs          # Centralized error types (thiserror)
├── cli/              # CLI with clap derive
│   ├── mod.rs        # Re-exports
│   ├── commands.rs   # Clap command definitions
│   └── handlers.rs   # Command implementations
├── config/           # Configuration (TOML, serde)
├── daemon/           # Background service, Unix socket IPC
├── audio/            # Audio capture (cpal, PipeWire loopback)
├── transcription/    # Whisper, Parakeet (ONNX), Deepgram, OpenAI, Diarization
├── detection/        # Hyprland window detection
├── notes/            # Markdown note generation
├── llm/              # AI summarization (Claude, OpenAI, local)
├── storage/          # SQLite database
├── notification/     # Desktop notifications (Mako), audio cues (rodio)
└── waybar/           # Waybar status bar integration
```

## Code Style Guidelines

### Imports

Order imports by specificity, separated by blank lines:
```rust
use std::path::PathBuf;           // 1. Standard library
use std::sync::Arc;

use serde::{Deserialize, Serialize};  // 2. External crates
use tokio::sync::broadcast;

use crate::error::{MuesliError, Result};  // 3. Crate modules
use crate::audio::AudioChunk;
```

### Error Handling

- Use `thiserror` for defining error types in `src/error.rs`
- Custom `Result<T>` type alias: `pub type Result<T> = std::result::Result<T, MuesliError>`
- Use `#[from]` attribute for automatic conversions
- Mark unused variants with `#[allow(dead_code)]`

```rust
#[derive(Error, Debug)]
pub enum MuesliError {
    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[allow(dead_code)]
    #[error("Not recording")]
    NotRecording,
}
```

### Structs and Enums

- Derive common traits: `#[derive(Debug, Clone)]`
- For serializable types: `#[derive(Debug, Clone, Serialize, Deserialize)]`
- Use `#[serde(default)]` and `#[serde(default = "fn_name")]` for config defaults
- Implement `Default` explicitly for complex defaults

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
}

fn default_sample_rate() -> u32 {
    16000
}
```

### Naming Conventions

- Types: `PascalCase` (e.g., `MuesliConfig`, `AudioChunk`)
- Functions/methods: `snake_case` (e.g., `from_default`, `device_info`)
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case` (e.g., `audio_capture`)
- Prefix error variants with domain: `AudioDeviceNotFound`, `WhisperModelNotFound`

### Documentation

- Use `///` for public item docs
- Use `//!` for module-level docs at file top
- Document public APIs, skip internal implementation details

```rust
//! Microphone capture using cpal
//!
//! Provides device enumeration and audio streaming.

/// Audio capture from microphone input
pub struct MicCapture { ... }

/// Create capture from default input device
pub fn from_default() -> Result<Self> { ... }
```

### Module Organization

- Each module has `mod.rs` with re-exports
- Use `pub use` to expose public interface
- Keep implementation in separate files

```rust
// src/cli/mod.rs
pub mod commands;
pub mod handlers;

pub use commands::Cli;
pub use handlers::handle_command;
```

### Async Patterns

- Use `tokio` as async runtime with `#[tokio::main]`
- Use `tokio::sync::broadcast` for multi-consumer channels
- Use `Arc<AtomicBool>` for cross-thread signaling
- Error handling: return `Result`, propagate with `?`

### Testing

- Place tests in `#[cfg(test)] mod tests` at file bottom
- Use `super::*` to import module items
- Test function names: `test_<what_is_tested>`
- Use `assert!`, `assert_eq!`, `assert!(result.is_ok())`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creates() {
        let config = MuesliConfig::default();
        assert_eq!(config.audio.sample_rate, 16000);
    }
}
```

### CLI with Clap

- Use derive macros: `#[derive(Parser)]`, `#[derive(Subcommand)]`
- Add doc comments for help text
- Use `#[arg(...)]` for argument configuration

```rust
#[derive(Parser)]
#[command(name = "muesli", about = "AI-powered meeting note-taker")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing (derive) |
| `tokio` | Async runtime |
| `serde` / `serde_json` / `toml` | Serialization |
| `thiserror` / `anyhow` | Error handling |
| `tracing` | Logging |
| `cpal` | Audio capture |
| `rodio` | Audio playback (notification sounds) |
| `whisper-rs` | Local speech-to-text |
| `rusqlite` | SQLite database |
| `reqwest` | HTTP client for APIs |
| `hyprland` | Hyprland IPC |

## Logging

Use `tracing` macros. Set level via `RUST_LOG` environment variable:
```bash
RUST_LOG=debug cargo run -- daemon
RUST_LOG=muesli=trace cargo run -- daemon  # Only this crate
```

## System Dependencies (for building)

```bash
# Arch Linux
sudo pacman -S clang alsa-lib dbus

# Ubuntu/Debian
sudo apt-get install libclang-dev libasound2-dev libdbus-1-dev pkg-config
```

## Common Patterns

### Result Propagation
```rust
pub fn do_thing() -> Result<Value> {
    let config = load_config()?;  // Propagate errors with ?
    Ok(value)
}
```

### Optional Configuration
```rust
pub device_mic: Option<String>,  // None = auto-detect
```

### Graceful Shutdown
```rust
let is_running = Arc::new(AtomicBool::new(true));
// In handler: is_running.store(false, Ordering::Relaxed);
// In loop: if !is_running.load(Ordering::Relaxed) { break; }
```

## CLI Commands

```bash
# Setup wizard (interactive, recommended for first-time)
muesli setup

# Uninstall
muesli uninstall

# Recording
muesli start [--title "Title"]
muesli stop
muesli status

# Meeting management
muesli list [--limit N]
muesli notes <id>
muesli transcript <id>

# Daemon
muesli daemon

# Configuration
muesli config show|edit

# Models - Whisper (whisper.cpp)
muesli models whisper list|download|delete <model>

# Models - Parakeet (ONNX, 20-30x faster)
muesli models parakeet list|download|delete <model>
# Models: parakeet-v3, parakeet-v3-int8, nemotron-streaming

# Models - Diarization (speaker identification)
muesli models diarization list|download|delete <model>
# Models: sortformer-v2

# Audio devices
muesli audio list-devices

# Waybar integration
muesli waybar              # Output JSON status for Waybar custom module
```

## Configuration Structure

Config location: `~/.config/muesli/config.toml`

```toml
[audio]
device_mic = "..."           # Optional, auto-detect if omitted
device_loopback = "..."      # Optional, auto-detect if omitted
capture_system_audio = true
sample_rate = 16000

[transcription]
engine = "parakeet"          # "whisper" or "parakeet"
model = "parakeet-v3-int8"   # Model name for selected engine
use_gpu = false
fallback_to_local = true

[llm]
engine = "local"             # "none", "local", "claude", "openai"
local_model = "qwen2.5-7b"   # For LM Studio
claude_model = "claude-sonnet-4-20250514"
openai_model = "gpt-4o"

[detection]
auto_detect = true
auto_prompt = true           # Show notification prompt for meetings
prompt_timeout_secs = 30

[storage]
notes_dir = "..."            # Optional, defaults to ~/.local/share/muesli/notes
database_path = "..."        # Optional, defaults to ~/.local/share/muesli/muesli.db
recordings_dir = "..."       # Optional, defaults to ~/.local/share/muesli/recordings

[daemon]
socket_path = "..."          # Optional
log_level = "info"

[audio_cues]
enabled = false              # Play sounds on recording start/stop
volume = 0.5                 # Volume level (0.0 - 1.0)
start_sound = "..."          # Optional: custom WAV/OGG/MP3 for start
stop_sound = "..."           # Optional: custom WAV/OGG/MP3 for stop

[waybar]
enabled = false              # Write status to file for Waybar integration
status_file = "..."          # Optional, defaults to $XDG_RUNTIME_DIR/muesli/waybar.json
```

### TranscriptionConfig.effective_model()

The `effective_model()` method returns the correct model based on engine:
- If `model` is set and non-default, use it
- Otherwise fall back to legacy `whisper_model`/`parakeet_model` fields (for backwards compatibility)
