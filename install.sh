#!/usr/bin/env bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print functions
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_header() {
    echo ""
    echo "=========================================="
    echo "  muesli Installation Script"
    echo "=========================================="
    echo ""
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Detect package manager
detect_package_manager() {
    if command_exists pacman; then
        echo "pacman"
    elif command_exists apt-get; then
        echo "apt"
    elif command_exists dnf; then
        echo "dnf"
    elif command_exists brew; then
        echo "brew"
    else
        echo "unknown"
    fi
}

# Check and install Vulkan development headers
ensure_vulkan_dev() {
    local pkg_manager
    pkg_manager=$(detect_package_manager)
    
    case "$pkg_manager" in
        pacman)
            if ! pacman -Qi vulkan-headers &>/dev/null; then
                print_warning "Vulkan development headers not found"
                read -p "Install vulkan-headers? (requires sudo) [Y/n]: " install_vulkan
                if [[ ! "$install_vulkan" =~ ^[Nn]$ ]]; then
                    sudo pacman -S --noconfirm vulkan-headers
                    print_success "Vulkan headers installed"
                else
                    print_error "Cannot build with GPU support without vulkan-headers"
                    return 1
                fi
            else
                print_success "Vulkan headers already installed"
            fi
            ;;
        apt)
            if ! dpkg -l libvulkan-dev &>/dev/null; then
                print_warning "Vulkan development headers not found"
                read -p "Install libvulkan-dev? (requires sudo) [Y/n]: " install_vulkan
                if [[ ! "$install_vulkan" =~ ^[Nn]$ ]]; then
                    sudo apt-get install -y libvulkan-dev
                    print_success "Vulkan headers installed"
                else
                    print_error "Cannot build with GPU support without libvulkan-dev"
                    return 1
                fi
            else
                print_success "Vulkan headers already installed"
            fi
            ;;
        dnf)
            if ! rpm -q vulkan-headers &>/dev/null; then
                print_warning "Vulkan development headers not found"
                read -p "Install vulkan-headers? (requires sudo) [Y/n]: " install_vulkan
                if [[ ! "$install_vulkan" =~ ^[Nn]$ ]]; then
                    sudo dnf install -y vulkan-headers vulkan-loader-devel
                    print_success "Vulkan headers installed"
                else
                    print_error "Cannot build with GPU support without vulkan-headers"
                    return 1
                fi
            else
                print_success "Vulkan headers already installed"
            fi
            ;;
        brew)
            print_info "macOS detected - Metal backend will be used (no extra deps needed)"
            ;;
        *)
            print_warning "Unknown package manager - please install Vulkan development headers manually"
            print_warning "  Arch: pacman -S vulkan-headers"
            print_warning "  Ubuntu/Debian: apt install libvulkan-dev"
            print_warning "  Fedora: dnf install vulkan-headers vulkan-loader-devel"
            read -p "Continue anyway? [y/N]: " continue_anyway
            if [[ ! "$continue_anyway" =~ ^[Yy]$ ]]; then
                return 1
            fi
            ;;
    esac
    return 0
}

# Get current default model from config
get_current_model() {
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        grep "^whisper_model" "$CONFIG_DIR/config.toml" 2>/dev/null | cut -d'"' -f2
    else
        echo "base"
    fi
}

# Set the default Whisper model in config
set_default_model() {
    local model="$1"
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        sed -i "s/^whisper_model = .*/whisper_model = \"$model\"/" "$CONFIG_DIR/config.toml"
        print_success "Default Whisper model set to '$model'"
    fi
}

# Set the transcription engine in config
set_engine() {
    local engine="$1"
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        sed -i "s/^engine = .*/engine = \"$engine\"/" "$CONFIG_DIR/config.toml"
        print_success "Transcription engine set to '$engine'"
    fi
}

# Set the default Parakeet model in config
set_parakeet_model() {
    local model="$1"
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        sed -i "s/^parakeet_model = .*/parakeet_model = \"$model\"/" "$CONFIG_DIR/config.toml"
        print_success "Default Parakeet model set to '$model'"
    fi
}

# Check if model is installed using muesli CLI
is_model_installed() {
    local model="$1"
    "$INSTALL_DIR/muesli" models list 2>/dev/null | grep "^$model" | grep -q "✓"
}

# Prompt to set Whisper model as default and use Whisper engine
maybe_set_default_model() {
    local model="$1"
    local current_model
    current_model=$(get_current_model)
    
    if [ "$model" != "$current_model" ]; then
        echo ""
        read -p "Set '$model' as your default Whisper model? [y/N]: " set_default
        if [[ "$set_default" =~ ^[Yy]$ ]]; then
            set_default_model "$model"
            set_engine "whisper"
        fi
    fi
}

is_parakeet_model_installed() {
    local model="$1"
    "$INSTALL_DIR/muesli" parakeet list 2>/dev/null | grep "^$model" | grep -q "✓"
}

select_transcription_model() {
    echo ""
    echo "=========================================="
    echo "  Transcription Model Selection"
    echo "=========================================="
    echo ""
    echo "Models directory: $DATA_DIR/models"
    echo ""
    
    echo "--- Whisper Models (whisper.cpp) ---"
    echo ""
    
    local whisper_models=("tiny" "base" "small" "medium" "large" "large-v3-turbo" "distil-large-v3")
    local whisper_sizes=("75" "142" "466" "1500" "2900" "1620" "1520")
    local whisper_descs=(
        "Fastest, lowest accuracy"
        "Good balance (recommended)"
        "Better accuracy"
        "High accuracy"
        "Best accuracy"
        "Fast + high quality (GPU)"
        "Distilled, fast + good quality"
    )
    
    for i in "${!whisper_models[@]}"; do
        local num=$((i + 1))
        local model="${whisper_models[$i]}"
        local size="${whisper_sizes[$i]}"
        local desc="${whisper_descs[$i]}"
        
        local installed=""
        if is_model_installed "$model"; then
            installed="${GREEN}[installed]${NC}"
        fi
        
        local star=" "
        if [ "$model" = "base" ]; then
            star="*"
        fi
        
        printf " %s[%d] %-16s (%5s MB) - %-30s %b\n" "$star" "$num" "$model" "$size" "$desc" "$installed"
    done
    
    echo ""
    echo "--- Parakeet Models (ONNX, 20-30x faster) ---"
    echo ""
    
    local parakeet_models=("parakeet-v3" "parakeet-v3-int8")
    local parakeet_sizes=("632" "217")
    local parakeet_descs=(
        "Full precision, best quality"
        "INT8 quantized, fastest"
    )
    
    for i in "${!parakeet_models[@]}"; do
        local num=$((i + 8))
        local model="${parakeet_models[$i]}"
        local size="${parakeet_sizes[$i]}"
        local desc="${parakeet_descs[$i]}"
        
        local installed=""
        if is_parakeet_model_installed "$model"; then
            installed="${GREEN}[installed]${NC}"
        fi
        
        local star=" "
        if [ "$model" = "parakeet-v3-int8" ]; then
            star="*"
        fi
        
        printf " %s[%d] %-16s (%5s MB) - %-30s %b\n" "$star" "$num" "$model" "$size" "$desc" "$installed"
    done
    
    echo ""
    echo "  [0] Skip model download"
    echo ""
    
    while true; do
        read -p "Select model [0-9]: " selection
        
        if [ -z "$selection" ]; then
            print_warning "Please enter a number (0-9)"
            continue
        fi
        
        if ! [[ "$selection" =~ ^[0-9]$ ]]; then
            print_warning "Invalid selection. Please enter 0-9"
            continue
        fi
        
        break
    done
    
    if [ "$selection" = "0" ]; then
        print_info "Skipping model download"
        echo "You can download models later with:"
        echo "  muesli models download <model>     (Whisper)"
        echo "  muesli parakeet download <model>   (Parakeet)"
        return
    fi
    
    if [ "$selection" -le 7 ]; then
        local selected_model="${whisper_models[$((selection - 1))]}"
        
        if is_model_installed "$selected_model"; then
            print_info "Model '$selected_model' is already installed"
            maybe_set_default_model "$selected_model"
        else
            print_info "Downloading $selected_model model..."
            print_warning "This may take a few minutes depending on your connection..."
            
            if "$INSTALL_DIR/muesli" models download "$selected_model"; then
                print_success "Model '$selected_model' downloaded successfully"
                maybe_set_default_model "$selected_model"
            else
                print_error "Model download failed"
                echo "You can try again later with: muesli models download $selected_model"
            fi
        fi
    else
        local selected_model="${parakeet_models[$((selection - 8))]}"
        
        if is_parakeet_model_installed "$selected_model"; then
            print_info "Model '$selected_model' is already installed"
        else
            print_info "Downloading $selected_model model..."
            print_warning "This may take a few minutes depending on your connection..."
            
            if "$INSTALL_DIR/muesli" parakeet download "$selected_model"; then
                print_success "Model '$selected_model' downloaded successfully"
            else
                print_error "Model download failed"
                echo "You can try again later with: muesli parakeet download $selected_model"
            fi
        fi
        
        echo ""
        read -p "Use Parakeet as your default transcription engine? [Y/n]: " use_parakeet
        if [[ ! "$use_parakeet" =~ ^[Nn]$ ]]; then
            set_engine "parakeet"
            set_parakeet_model "$selected_model"
        fi
    fi
}

# Get current GPU setting from config
get_current_gpu_setting() {
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        grep "^use_gpu" "$CONFIG_DIR/config.toml" 2>/dev/null | cut -d'=' -f2 | tr -d ' '
    else
        echo "false"
    fi
}

# Set the GPU setting in config
set_gpu_setting() {
    local use_gpu="$1"
    if [ -f "$CONFIG_DIR/config.toml" ]; then
        if grep -q "^use_gpu" "$CONFIG_DIR/config.toml"; then
            sed -i "s/^use_gpu = .*/use_gpu = $use_gpu/" "$CONFIG_DIR/config.toml"
        else
            # Add under [transcription] section if not present
            sed -i "/^\[transcription\]/a use_gpu = $use_gpu" "$CONFIG_DIR/config.toml"
        fi
        print_success "GPU acceleration set to $use_gpu"
    fi
}

# Interactive compute backend selection
select_compute_backend() {
    echo ""
    echo "=========================================="
    echo "  Compute Backend Selection"
    echo "=========================================="
    echo ""
    
    local current_setting
    current_setting=$(get_current_gpu_setting)
    
    local current_label="CPU"
    if [ "$current_setting" = "true" ]; then
        current_label="GPU"
    fi
    
    echo "Current setting: $current_label"
    echo ""
    echo "--- Available Backends ---"
    echo ""
    echo " *[1] CPU  - Works everywhere, no special drivers needed (default)"
    echo "  [2] GPU  - Faster transcription (requires CUDA, Metal, or Vulkan)"
    echo ""
    echo "  [0] Skip (keep current: $current_label)"
    echo ""
    
    while true; do
        read -p "Select backend [0-2]: " selection
        
        if [ -z "$selection" ]; then
            print_warning "Please enter a number (0-2)"
            continue
        fi
        
        if ! [[ "$selection" =~ ^[0-2]$ ]]; then
            print_warning "Invalid selection. Please enter 0-2"
            continue
        fi
        
        break
    done
    
    case "$selection" in
        0)
            print_info "Keeping current setting: $current_label"
            ;;
        1)
            set_gpu_setting "false"
            ;;
        2)
            print_warning "GPU acceleration requires compatible hardware and drivers"
            echo "  - NVIDIA: CUDA toolkit"
            echo "  - AMD: ROCm or Vulkan"
            echo "  - Apple: Metal (automatic on macOS)"
            echo ""
            read -p "Continue with GPU? [y/N]: " confirm
            if [[ "$confirm" =~ ^[Yy]$ ]]; then
                set_gpu_setting "true"
            else
                print_info "Keeping CPU backend"
            fi
            ;;
    esac
}

# Main installation function
main() {
    print_header

    # Step 1: Check for Rust/cargo
    print_info "Checking for Rust installation..."
    if ! command_exists cargo; then
        print_error "Rust/cargo not found!"
        echo ""
        echo "Please install Rust from https://rustup.rs/"
        echo "Run: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    print_success "Rust $(rustc --version) found"

    # Step 2: Ask about GPU acceleration BEFORE building
    echo ""
    echo "=========================================="
    echo "  GPU Acceleration"
    echo "=========================================="
    echo ""
    echo "GPU acceleration provides faster transcription using Vulkan."
    echo ""
    read -p "Enable GPU acceleration? [y/N]: " enable_gpu
    
    USE_GPU="false"
    BUILD_FEATURES=""
    
    if [[ "$enable_gpu" =~ ^[Yy]$ ]]; then
        if ensure_vulkan_dev; then
            USE_GPU="true"
            BUILD_FEATURES="--features vulkan"
            print_success "GPU acceleration enabled (Vulkan)"
        else
            print_warning "Falling back to CPU-only build"
        fi
    else
        print_info "Using CPU backend"
    fi

    # Step 3: Build release binary
    print_info "Building muesli in release mode..."
    if ! cargo build --release $BUILD_FEATURES; then
        print_error "Build failed!"
        exit 1
    fi
    print_success "Build completed successfully"

    # Step 3: Determine installation directory
    if [ -d "$HOME/.cargo/bin" ]; then
        INSTALL_DIR="$HOME/.cargo/bin"
    else
        INSTALL_DIR="$HOME/.local/bin"
        mkdir -p "$INSTALL_DIR"
    fi

    print_info "Installing binary to $INSTALL_DIR..."
    cp target/release/muesli "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/muesli"
    print_success "Binary installed to $INSTALL_DIR/muesli"

    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        print_warning "$INSTALL_DIR is not in your PATH"
        echo "Add this line to your ~/.bashrc or ~/.zshrc:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
    fi

    # Step 4: Create configuration directory
    CONFIG_DIR="$HOME/.config/muesli"
    print_info "Creating configuration directory..."
    mkdir -p "$CONFIG_DIR"
    print_success "Configuration directory created at $CONFIG_DIR"

    # Step 5: Create data directories
    DATA_DIR="$HOME/.local/share/muesli"
    print_info "Creating data directories..."
    mkdir -p "$DATA_DIR/recordings"
    mkdir -p "$DATA_DIR/notes"
    mkdir -p "$DATA_DIR/models"
    print_success "Data directories created at $DATA_DIR"

    # Step 6: Initialize default configuration
    print_info "Initializing default configuration..."
    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        "$INSTALL_DIR/muesli" config init
        print_success "Default configuration created at $CONFIG_DIR/config.toml"
    else
        print_warning "Configuration file already exists, skipping initialization"
    fi

    # Step 7: Install systemd service
    SYSTEMD_DIR="$HOME/.config/systemd/user"
    print_info "Installing systemd user service..."
    mkdir -p "$SYSTEMD_DIR"
    
    # Update service file with actual binary path
    sed "s|%h/.cargo/bin/muesli|$INSTALL_DIR/muesli|g" assets/muesli.service > "$SYSTEMD_DIR/muesli.service"
    
    systemctl --user daemon-reload
    print_success "Systemd service installed at $SYSTEMD_DIR/muesli.service"

    # Step 8: Select and download transcription model
    select_transcription_model

    # Step 9: Download speaker diarization model
    print_info "Downloading speaker diarization model (sortformer-v2)..."
    if "$INSTALL_DIR/muesli" diarization download sortformer-v2; then
        print_success "Diarization model downloaded successfully"
    else
        print_warning "Diarization model download failed"
        echo "You can try again later with: muesli diarization download sortformer-v2"
    fi

    # Step 10: Optionally download Nemotron streaming model
    echo ""
    echo "=========================================="
    echo "  Streaming Transcription (Optional)"
    echo "=========================================="
    echo ""
    echo "Nemotron streaming model enables real-time transcription during recording."
    echo "This eliminates wait time after stopping - transcription is already done!"
    echo ""
    echo "Size: ~2.5 GB download"
    echo ""
    read -p "Download Nemotron streaming model? [y/N]: " download_nemotron
    
    if [[ "$download_nemotron" =~ ^[Yy]$ ]]; then
        print_info "Downloading Nemotron streaming model..."
        print_warning "This may take a while (~2.5 GB)..."
        if "$INSTALL_DIR/muesli" parakeet download nemotron-streaming; then
            print_success "Nemotron streaming model downloaded successfully"
            print_info "Streaming transcription will be automatically enabled during recording"
        else
            print_warning "Nemotron download failed"
            echo "You can try again later with: muesli parakeet download nemotron-streaming"
            echo "Without it, muesli will use batch transcription after recording stops."
        fi
    else
        print_info "Skipping Nemotron streaming model"
        echo "You can download it later with: muesli parakeet download nemotron-streaming"
    fi

    # Step 12: Set GPU config based on earlier selection
    if [ "$USE_GPU" = "true" ]; then
        set_gpu_setting "true"
    else
        set_gpu_setting "false"
    fi

    # Step 13: Copy Hyprland keybindings (optional)
    if [ -d "$HOME/.config/hypr" ]; then
        print_info "Copying Hyprland keybindings..."
        cp assets/hyprland-keybindings.conf "$HOME/.config/hypr/muesli-keybindings.conf"
        print_success "Keybindings copied to ~/.config/hypr/muesli-keybindings.conf"
        echo ""
        print_info "To enable keybindings, add this line to ~/.config/hypr/hyprland.conf:"
        echo "  source = ~/.config/hypr/muesli-keybindings.conf"
        echo "Then reload Hyprland: hyprctl reload"
    fi

    # Final success message
    echo ""
    echo "=========================================="
    print_success "Installation completed successfully!"
    echo "=========================================="
    echo ""
    echo "Next steps:"
    echo ""
    echo "1. Review and edit configuration:"
    echo "   muesli config edit"
    echo ""
    echo "2. Test audio devices:"
    echo "   muesli audio list-devices"
    echo "   muesli audio test-mic"
    echo "   muesli audio test-loopback"
    echo ""
    echo "3. Enable and start the daemon:"
    echo "   systemctl --user enable muesli.service"
    echo "   systemctl --user start muesli.service"
    echo ""
    echo "4. Check daemon status:"
    echo "   systemctl --user status muesli.service"
    echo ""
    echo "5. View logs:"
    echo "   journalctl --user -u muesli.service -f"
    echo ""
    echo "6. Manual recording:"
    echo "   muesli start --title \"My Meeting\""
    echo "   muesli stop"
    echo ""
    echo "For more information, see README.md or run: muesli --help"
    echo ""
}

# Run main function
main "$@"
