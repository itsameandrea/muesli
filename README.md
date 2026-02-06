# muesli

AI-powered meeting note-taker for Linux/Hyprland that automatically detects, records, transcribes, and summarizes your meetings.

## Installation

Copy and run:

```bash
curl -fsSL https://raw.githubusercontent.com/itsameandrea/muesli/master/install.sh | bash
```

Then finish setup:

```bash
muesli setup
```

## Overview

muesli is a background daemon that monitors your Hyprland window manager for meeting applications (primarily Google Meet, with experimental support for Zoom, Teams, and others), automatically records audio from both your microphone and system audio, transcribes the conversation using local AI models (Whisper or Parakeet), and generates structured meeting notes with AI summarization.

## Features

- **Automatic Meeting Detection**: Monitors Hyprland windows to detect when you join a meeting
- **Dual Audio Capture**: Records both microphone input and system audio (loopback) via PipeWire
- **Local Transcription**: Uses Whisper.cpp for privacy-focused, offline speech-to-text
- **Hosted API Support** *(Experimental)*: Optional integration with Deepgram or OpenAI Whisper API (untested)
- **Multi-Provider LLM Summarization**: Generate notes with local LM Studio, Anthropic, OpenAI, Moonshot, or OpenRouter
- **Semantic Notes Search**: Search and ask questions across past meetings with qmd integration
- **SQLite Storage**: Persistent storage of meetings, transcripts, and metadata
- **Desktop Notifications**: Real-time status updates via Mako/notify-rust
- **Audio Cues**: Optional sound notifications when recording starts/stops
- **Waybar Integration**: Status bar module showing recording state
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

## Build and Manual Installation

### Quick Install

```bash
# Build and install
cargo build --release
cp target/release/muesli ~/.local/bin/  # or ~/.cargo/bin/

# Run the interactive setup wizard
muesli setup
```

### Setup Wizard

The `muesli setup` command provides an interactive wizard that configures everything:

```
==========================================
  muesli Setup Wizard
==========================================

[1/11] Creating directories...
[2/11] Initializing configuration...
[3/11] GPU Acceleration
[4/11] Transcription Model Selection
[5/11] Speaker Diarization Model
[6/11] Streaming Transcription (Optional)
[7/11] LLM for Meeting Notes
[8/11] Meeting Detection
[9/11] Audio Cues
[10/11] Systemd Service
[11/11] qmd Search Integration
```

**What the wizard configures:**

| Step | Description |
|------|-------------|
| **Directories** | Creates `~/.config/muesli/` and `~/.local/share/muesli/` |
| **GPU Acceleration** | Enables Vulkan/CUDA/Metal for faster transcription |
| **Transcription Model** | Choose Whisper (tiny→large) or Parakeet (20-30x faster ONNX) |
| **Diarization** | Speaker identification model (sortformer-v2) |
| **Streaming** | Optional Nemotron model for real-time transcription |
| **LLM** | Auto-detects LM Studio and lists available models for meeting summaries |
| **Meeting Detection** | Auto-detect meeting windows (Google Meet tested, others experimental) |
| **Audio Cues** | Play sounds when recording starts/stops |
| **Systemd Service** | Install user service for auto-start on login |
| **qmd Search** | Semantic search and Q&A over your meeting notes |

### Manual Installation

If you prefer to configure manually:

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

# Run setup wizard (creates config and downloads models)
muesli setup

# Or manually download models:
muesli models whisper download base              # Whisper base model
muesli models parakeet download parakeet-v3-int8 # Parakeet (faster)
muesli models diarization download sortformer-v2 # Speaker identification

# Install systemd service
mkdir -p ~/.config/systemd/user
cp assets/muesli.service ~/.config/systemd/user/
systemctl --user daemon-reload
```

## Configuration

Configuration file location: `~/.config/muesli/config.toml`

### Basic Configuration

```toml
[audio]
# Specific device names (omit for auto-detect)
# device_mic = "alsa_input.usb-Blue_Microphones_Yeti"
# device_loopback = "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
capture_system_audio = true
sample_rate = 16000

[transcription]
# Engine: "whisper" or "parakeet"
engine = "parakeet"
# Model name (depends on engine)
# Whisper: tiny, base, small, medium, large, large-v3-turbo
# Parakeet: parakeet-v3, parakeet-v3-int8
model = "parakeet-v3-int8"
use_gpu = false
fallback_to_local = true

[llm]
# Provider: "none", "local", "anthropic", "openai", "moonshot", "openrouter"
provider = "local"
# Model name for selected provider
model = "qwen2.5-7b-instruct-1m"
# API key for cloud providers (optional)
api_key = ""
# LM Studio binary path (auto-detect if empty)
local_lms_path = ""
# Context window override (0 = auto-detect)
context_limit = 0

[qmd]
enabled = false              # Enable semantic search over meeting notes
auto_index = true            # Re-index notes automatically after meetings
collection_name = "muesli-meetings"

[detection]
auto_detect = true
auto_prompt = true           # Show record/skip prompt when meeting detected
prompt_timeout_secs = 30     # Auto-dismiss prompt after 30s
debounce_ms = 500
poll_interval_secs = 30

[audio_cues]
enabled = false              # Play sounds on recording start/stop
volume = 0.5                 # Volume level (0.0 - 1.0)
start_sound = "..."          # Optional: custom WAV/OGG/MP3 for start
stop_sound = "..."           # Optional: custom WAV/OGG/MP3 for stop

[waybar]
enabled = false              # Write status to file for Waybar integration
status_file = "..."          # Optional, defaults to $XDG_RUNTIME_DIR/muesli/waybar.json
```

### Transcription Engines

| Engine | Speed | Quality | Offline | Status | Notes |
|--------|-------|---------|---------|--------|-------|
| **parakeet** | 20-30x faster | Excellent | Yes | ✅ Tested | ONNX-based, recommended |
| **whisper** | Baseline | Excellent | Yes | ✅ Tested | Original whisper.cpp |
| **deepgram** | Fast | Excellent | No | ⚠️ Experimental | Requires API key, untested |
| **openai** | Fast | Excellent | No | ⚠️ Experimental | Requires API key, untested |

### LLM Providers

| Provider | Cost | Setup |
|----------|------|-------|
| **local** | Free | Requires [LM Studio](https://lmstudio.ai) |
| **anthropic** | Paid | Set `llm.api_key` |
| **openai** | Paid | Set `llm.api_key` |
| **moonshot** | Paid | Set `llm.api_key` |
| **openrouter** | Paid | Set `llm.api_key` |
| **none** | - | Disable summarization |

### API Keys (Optional)

For hosted transcription or AI summarization, add API keys:

```toml
[transcription]
deepgram_api_key = "your-deepgram-key"
openai_api_key = "your-openai-key"

[llm]
provider = "anthropic" # or openai, moonshot, openrouter
api_key = "your-provider-key"
```

### Configuration Commands

```bash
# Show current configuration
muesli config show

# Edit configuration file
muesli config edit
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
muesli notes <meeting-id>
```

## CLI Commands Reference

### Setup

```bash
# Interactive setup wizard (recommended for first-time setup)
muesli setup
```

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

# View meeting notes and summary
muesli notes [meeting-id]

# View meeting transcript
muesli transcript [meeting-id]

# Re-process a meeting (summary only, or full re-transcribe with --clean)
muesli redo [meeting-id] [--clean]
```

Note: Transcription and summarization happen automatically when recording stops.

### Daemon Control

```bash
# Run daemon in foreground (for debugging)
muesli daemon

# Rebuild and reinstall from source used by current binary
muesli update

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
```

### Model Management

```bash
# Whisper models (whisper.cpp)
muesli models whisper list
muesli models whisper download <tiny|base|small|medium|large|large-v3-turbo>
muesli models whisper delete <model-name>

# Parakeet models (ONNX, 20-30x faster)
muesli models parakeet list
muesli models parakeet download <parakeet-v3|parakeet-v3-int8|nemotron-streaming>
muesli models parakeet delete <model-name>

# Diarization models (speaker identification)
muesli models diarization list
muesli models diarization download sortformer-v2
muesli models diarization delete sortformer-v2
```

### Meeting Search and Q&A

```bash
# Semantic search over indexed meeting notes
muesli search "roadmap decisions" [-n 5] [--keyword]

# Ask a natural-language question across your meetings
muesli ask what did we decide about pricing

# Rebuild qmd index
muesli search reindex

# Show qmd collection status
muesli search status
```

### Audio Devices

```bash
# List available audio devices
muesli audio list-devices
```

### Waybar Integration

```bash
# Output JSON status for Waybar custom module
muesli waybar
```

## Waybar Integration

muesli can display recording status in Waybar using a custom module.

### Setup

1. Enable waybar integration in `~/.config/muesli/config.toml`:

```toml
[waybar]
enabled = true
```

2. Add to your Waybar config (`~/.config/waybar/config`):

```json
{
  "custom/muesli": {
    "exec": "muesli waybar",
    "return-type": "json",
    "interval": 5,
    "signal": 8,
    "format": "{icon}",
    "format-icons": {
      "idle": "",
      "recording": "󰻂"
    },
    "tooltip": true,
    "on-click": "muesli start || muesli stop"
  }
}
```

3. Add to your modules (e.g., in `modules-right`):

```json
"modules-right": ["custom/muesli", "clock"]
```

4. Style in `~/.config/waybar/style.css`:

```css
#custom-muesli {
    font-size: 14px;
}

#custom-muesli.recording {
    color: #ff5555;
}

#custom-muesli.idle {
    color: #888888;
}
```

The module shows a microphone icon that turns red when recording. Hover for tooltip with details.

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

| Application | Status | Notes |
|-------------|--------|-------|
| **Google Meet** | ✅ Tested | Chrome, Chromium, Firefox, Brave, Edge, Zen browsers |
| Zoom | ⚠️ Untested | Detection implemented but not verified |
| Microsoft Teams | ⚠️ Untested | Detection implemented but not verified |
| Slack Huddles | ⚠️ Untested | Only detects huddles/calls, not text channels |
| Discord | ⚠️ Untested | Only detects voice/stage channels, not text |
| WebEx | ⚠️ Untested | Detection implemented but not verified |

Detection is based on window class and title patterns in Hyprland. You can customize detection patterns in the source code at `src/detection/patterns.rs`.

## Troubleshooting

### Audio Capture Issues

If audio capture fails:

1. List available devices:
   ```bash
   muesli audio list-devices
   ```

2. Update config with specific device names:
   ```bash
   muesli config edit
   ```

### Transcription Model Issues

If transcription fails:

1. Check available models:
   ```bash
   muesli models whisper list
   muesli models parakeet list
   ```

2. Download missing model:
   ```bash
   muesli models whisper download base
   # or
   muesli models parakeet download parakeet-v3-int8
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

## Uninstallation

To completely remove muesli:

```bash
muesli uninstall
```

The uninstaller will:
1. Stop the running daemon
2. Disable and remove the systemd service
3. Prompt to remove configuration (`~/.config/muesli/`)
4. Prompt to remove data directory (`~/.local/share/muesli/`) - recordings, models, database
5. Print instructions to remove the binary (cannot self-delete)

Manual removal:

```bash
systemctl --user stop muesli.service
systemctl --user disable muesli.service
rm ~/.config/systemd/user/muesli.service
rm ~/.local/bin/muesli
rm -rf ~/.config/muesli
rm -rf ~/.local/share/muesli
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
