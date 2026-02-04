# muesli

AI-powered meeting note-taker for Linux/Hyprland that automatically detects, records, transcribes, and summarizes your meetings.

## Overview

muesli is a background daemon that monitors your Hyprland window manager for meeting applications (Zoom, Google Meet, Microsoft Teams, etc.), automatically records audio from both your microphone and system audio, transcribes the conversation using Whisper, and generates structured meeting notes with AI summarization.

## Features

- **Automatic Meeting Detection**: Monitors Hyprland windows to detect when you join a meeting
- **Dual Audio Capture**: Records both microphone input and system audio (loopback) via PipeWire
- **Local Transcription**: Uses Whisper.cpp for privacy-focused, offline speech-to-text
- **Hosted API Support**: Optional integration with Deepgram or OpenAI Whisper API
- **AI Summarization**: Generate structured notes with Claude or GPT (optional)
- **SQLite Storage**: Persistent storage of meetings, transcripts, and metadata
- **Desktop Notifications**: Real-time status updates via Mako/notify-rust
- **CLI Interface**: Full command-line control for manual recording and management
- **Systemd Integration**: Run as a user service for automatic startup

## Requirements

### System Dependencies

- **Operating System**: Linux (tested on Arch Linux)
- **Window Manager**: Hyprland (for automatic meeting detection)
- **Audio Server**: PipeWire (for audio capture)
- **Rust**: 1.70 or later (for building from source)

### Optional Dependencies

- **Mako**: For desktop notifications (or any notification daemon)
- **systemd**: For daemon service management

### Runtime Requirements

- Whisper model files (automatically downloaded during installation)
- Sufficient disk space for audio recordings and models (~500MB for base model)

## Installation

### Quick Install

Run the installation script to build and install muesli:

```bash
cd /home/itsameandrea/Code/itsameandrea/muesli
./install.sh
```

The script will:
1. Check for Rust/cargo installation
2. Build the release binary
3. Install to `~/.cargo/bin/` or `~/.local/bin/`
4. Create configuration directory at `~/.config/muesli/`
5. Set up data directory at `~/.local/share/muesli/`
6. Install systemd user service
7. Download the default Whisper model (base)

### Manual Installation

If you prefer to install manually:

```bash
# Build release binary
cargo build --release

# Copy binary to PATH
cp target/release/muesli ~/.cargo/bin/
# or
cp target/release/muesli ~/.local/bin/

# Create directories
mkdir -p ~/.config/muesli
mkdir -p ~/.local/share/muesli/{recordings,notes,models}

# Initialize configuration
muesli config init

# Install systemd service
mkdir -p ~/.config/systemd/user
cp assets/muesli.service ~/.config/systemd/user/
systemctl --user daemon-reload

# Download Whisper model
muesli models download base
```

## Configuration

Configuration file location: `~/.config/muesli/config.toml`

### Basic Configuration

```toml
[audio]
# Specific device names (None = auto-detect)
device_mic = "alsa_input.usb-Blue_Microphones_Yeti_Stereo_Microphone"
device_loopback = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
capture_system_audio = true
sample_rate = 16000

[transcription]
# Engine: "local", "deepgram", "openai"
engine = "local"
# Model: "tiny", "base", "small", "medium", "large"
whisper_model = "base"
fallback_to_local = true

[llm]
# Engine: "none", "local", "claude", "openai"
engine = "none"
claude_model = "claude-sonnet-4-20250514"
openai_model = "gpt-4o"

[detection]
auto_detect = true
debounce_ms = 500
poll_interval_secs = 30
```

### API Keys (Optional)

For hosted transcription or AI summarization, add API keys:

```toml
[transcription]
deepgram_api_key = "your-deepgram-key"
openai_api_key = "your-openai-key"

[llm]
claude_api_key = "your-claude-key"
openai_api_key = "your-openai-key"
```

### Configuration Commands

```bash
# Show current configuration
muesli config show

# Edit configuration file
muesli config edit

# Print config file path
muesli config path

# Reinitialize default configuration
muesli config init
```

## Quick Start

### 1. Start the Daemon

Enable and start the systemd service:

```bash
systemctl --user enable muesli.service
systemctl --user start muesli.service
```

Check status:

```bash
systemctl --user status muesli.service
```

View logs:

```bash
journalctl --user -u muesli.service -f
```

### 2. Manual Recording

Start a recording manually:

```bash
muesli start --title "Team Standup"
```

Stop recording:

```bash
muesli stop
```

Check status:

```bash
muesli status
```

### 3. View Meetings

List recorded meetings:

```bash
muesli list --limit 10
```

View meeting notes:

```bash
muesli view <meeting-id>
```

Transcribe a recording:

```bash
muesli transcribe <meeting-id>
```

## CLI Commands Reference

### Recording Commands

```bash
# Start recording with optional title
muesli start [--title "Meeting Title"]

# Stop current recording
muesli stop

# Show recording status
muesli status
```

### Meeting Management

```bash
# List recorded meetings
muesli list [--limit 10]

# View meeting notes
muesli view <meeting-id>

# Transcribe a meeting
muesli transcribe <meeting-id> [--hosted]
```

### Daemon Control

```bash
# Run daemon in foreground (for debugging)
muesli daemon

# Use systemd for background operation
systemctl --user start muesli.service
systemctl --user stop muesli.service
systemctl --user restart muesli.service
```

### Configuration

```bash
# Show current configuration
muesli config show

# Edit configuration file
muesli config edit

# Print config file path
muesli config path

# Initialize default configuration
muesli config init
```

### Model Management

```bash
# List available Whisper models
muesli models list

# Download a Whisper model
muesli models download <tiny|base|small|medium|large>

# Delete a downloaded model
muesli models delete <model-name>
```

### Audio Testing

```bash
# List available audio devices
muesli audio list-devices

# Test microphone capture (3 seconds)
muesli audio test-mic [--duration 3]

# Test loopback capture (3 seconds)
muesli audio test-loopback [--duration 3]
```

## Hyprland Keybindings

Add these keybindings to your Hyprland configuration for quick access:

### Option 1: Source the Provided Config

Add to `~/.config/hypr/hyprland.conf`:

```conf
source = ~/.config/hypr/muesli-keybindings.conf
```

Then copy the keybindings file:

```bash
cp assets/hyprland-keybindings.conf ~/.config/hypr/muesli-keybindings.conf
```

### Option 2: Manual Keybindings

Add directly to `~/.config/hypr/hyprland.conf`:

```conf
# Toggle muesli recording
bind = SUPER SHIFT, M, exec, muesli start || muesli stop

# Check recording status
bind = SUPER SHIFT, S, exec, muesli status
```

After adding keybindings, reload Hyprland:

```bash
hyprctl reload
```

## File Locations

- **Binary**: `~/.cargo/bin/muesli` or `~/.local/bin/muesli`
- **Configuration**: `~/.config/muesli/config.toml`
- **Data Directory**: `~/.local/share/muesli/`
- **Recordings**: `~/.local/share/muesli/recordings/`
- **Notes**: `~/.local/share/muesli/notes/`
- **Models**: `~/.local/share/muesli/models/`
- **Database**: `~/.local/share/muesli/muesli.db`
- **Systemd Service**: `~/.config/systemd/user/muesli.service`
- **Socket**: `$XDG_RUNTIME_DIR/muesli/muesli.sock`

## Supported Meeting Applications

muesli automatically detects the following meeting applications:

- Zoom
- Google Meet (Chrome/Chromium/Firefox)
- Microsoft Teams
- Slack Huddles
- Discord
- Jitsi Meet
- WebEx
- GoToMeeting

Detection is based on window title patterns in Hyprland. You can customize detection patterns in the source code at `src/detection/patterns.rs`.

## Troubleshooting

### Audio Capture Issues

If audio capture fails:

1. List available devices:
   ```bash
   muesli audio list-devices
   ```

2. Test microphone:
   ```bash
   muesli audio test-mic --duration 5
   ```

3. Test system audio:
   ```bash
   muesli audio test-loopback --duration 5
   ```

4. Update config with specific device names:
   ```bash
   muesli config edit
   ```

### Whisper Model Issues

If transcription fails:

1. Check available models:
   ```bash
   muesli models list
   ```

2. Download missing model:
   ```bash
   muesli models download base
   ```

3. Verify model path in config:
   ```bash
   muesli config show
   ```

### Daemon Not Starting

Check systemd logs:

```bash
journalctl --user -u muesli.service -n 50
```

Verify Hyprland is running:

```bash
echo $HYPRLAND_INSTANCE_SIGNATURE
```

Test daemon manually:

```bash
muesli daemon
```

### Meeting Not Detected

1. Check detection is enabled:
   ```bash
   muesli config show | grep auto_detect
   ```

2. Verify window title matches patterns:
   ```bash
   hyprctl activewindow
   ```

3. Start recording manually:
   ```bash
   muesli start --title "Manual Meeting"
   ```

## Privacy & Security

- **Local-First**: All transcription can run locally with Whisper
- **No Cloud Required**: Works completely offline (except for optional hosted APIs)
- **User-Level Storage**: All data stored in your home directory
- **No Telemetry**: No data collection or external reporting
- **API Keys**: Stored in plain text config file (use appropriate file permissions)

Recommended config file permissions:

```bash
chmod 600 ~/.config/muesli/config.toml
```

## Development

### Building from Source

```bash
git clone https://github.com/itsameandrea/muesli.git
cd muesli
cargo build --release
```

### Running Tests

```bash
cargo test
```

### Debug Mode

Run with verbose logging:

```bash
muesli -vvv daemon
```

Or set environment variable:

```bash
RUST_LOG=debug muesli daemon
```

## License

MIT License - see LICENSE file for details

## Contributing

Contributions welcome! Please open an issue or pull request on GitHub.

## Acknowledgments

- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) - Fast Whisper inference
- [whisper-rs](https://codeberg.org/tazz4843/whisper-rs) - Rust bindings for Whisper
- [Hyprland](https://hyprland.org/) - Dynamic tiling Wayland compositor
- [PipeWire](https://pipewire.org/) - Modern audio server for Linux
