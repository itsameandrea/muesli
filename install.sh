#!/usr/bin/env bash
set -euo pipefail

REPO="itsameandrea/muesli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
print_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
print_error() { echo -e "${RED}[ERROR]${NC} $1"; }

detect_variant() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)
    
    case "$os" in
        linux)
            case "$arch" in
                x86_64)
                    if command -v vulkaninfo &>/dev/null; then
                        echo "linux-x86_64-vulkan"
                    else
                        echo "linux-x86_64-cpu"
                    fi
                    ;;
                *)
                    print_error "Unsupported architecture: $arch"
                    exit 1
                    ;;
            esac
            ;;
        darwin)
            case "$arch" in
                x86_64) echo "macos-x86_64" ;;
                arm64) echo "macos-arm64" ;;
                *)
                    print_error "Unsupported architecture: $arch"
                    exit 1
                    ;;
            esac
            ;;
        *)
            print_error "Unsupported OS: $os"
            exit 1
            ;;
    esac
}

get_latest_release() {
    curl -sL "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
}

stop_daemon() {
    if pgrep -f "muesli daemon" >/dev/null 2>&1; then
        print_info "Stopping running muesli daemon..."
        pkill -f "muesli daemon" 2>/dev/null || true
        sleep 1
    fi
    rm -f "$INSTALL_DIR/muesli" 2>/dev/null || true
}

build_from_source() {
    local ref="${1:-}"

    if ! command -v cargo &>/dev/null; then
        print_error "Rust/cargo not found. Install from https://rustup.rs/"
        exit 1
    fi

    if ! command -v git &>/dev/null; then
        print_error "git not found. Install git to build from source."
        exit 1
    fi

    local tmp_dir
    tmp_dir=$(mktemp -d)

    print_info "Cloning muesli source..."
    if ! git clone --depth 1 "https://github.com/$REPO.git" "$tmp_dir/muesli"; then
        rm -rf "$tmp_dir"
        print_error "Failed to clone source repository"
        exit 1
    fi

    if [ -n "$ref" ]; then
        if git -C "$tmp_dir/muesli" fetch --depth 1 origin "refs/tags/$ref" && git -C "$tmp_dir/muesli" checkout -q FETCH_HEAD; then
            print_info "Building tag $ref from source..."
        else
            print_warn "Could not checkout tag $ref, building default branch instead"
        fi
    fi

    print_info "Building muesli from source..."
    (
        cd "$tmp_dir/muesli"
        cargo build --release
    )

    mkdir -p "$INSTALL_DIR"
    cp "$tmp_dir/muesli/target/release/muesli" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/muesli"
    rm -rf "$tmp_dir"
}

main() {
    echo ""
    echo "=========================================="
    echo "  muesli Installer"
    echo "=========================================="
    echo ""

    local variant=$(detect_variant)
    local version=${1:-$(get_latest_release)}
    
    stop_daemon
    
    if [ -z "$version" ]; then
        print_warn "No releases found. Building from source..."

        build_from_source
        
        local built_version=$("$INSTALL_DIR/muesli" --version 2>/dev/null || echo "unknown")
        print_info "Built: $built_version"
    else
        print_info "Installing muesli $version ($variant)..."
        
        local url="https://github.com/$REPO/releases/download/$version/muesli-$variant"
        
        mkdir -p "$INSTALL_DIR"
        
        print_info "Downloading from $url..."
        if ! curl -fsSL "$url" -o "$INSTALL_DIR/muesli"; then
            print_error "Download failed. Building from source instead..."

            build_from_source "$version"
        fi
        
        chmod +x "$INSTALL_DIR/muesli"
        
        print_info "Verifying checksum..."
        if curl -fsSL "$url.sha256" -o /tmp/muesli.sha256 2>/dev/null; then
            (cd "$INSTALL_DIR" && sha256sum -c /tmp/muesli.sha256) || print_warn "Checksum verification failed"
            rm -f /tmp/muesli.sha256
        fi
    fi
    
    local installed_version=$("$INSTALL_DIR/muesli" --version 2>/dev/null || echo "unknown")
    print_info "Installed: $installed_version"
    print_info "Location:  $INSTALL_DIR/muesli"
    
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo ""
        print_warn "$INSTALL_DIR is not in your PATH"
        echo "  Add this to your shell config:"
        echo "    export PATH=\"\$PATH:$INSTALL_DIR\""
        echo ""
    fi
    
    echo ""
    print_info "Running setup wizard..."
    echo ""
    
    "$INSTALL_DIR/muesli" setup
}

main "$@"
