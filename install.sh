#!/usr/bin/env bash
set -euo pipefail

REPO="itsameandrea/muesli"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

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
                    echo "Unsupported architecture: $arch" >&2
                    exit 1
                    ;;
            esac
            ;;
        darwin)
            case "$arch" in
                x86_64) echo "macos-x86_64" ;;
                arm64) echo "macos-arm64" ;;
                *)
                    echo "Unsupported architecture: $arch" >&2
                    exit 1
                    ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $os" >&2
            exit 1
            ;;
    esac
}

get_latest_release() {
    curl -sL "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
}

main() {
    local variant=$(detect_variant)
    local version=${1:-$(get_latest_release)}
    
    if [ -z "$version" ]; then
        echo "No releases found. Build from source: cargo install --git https://github.com/$REPO"
        exit 1
    fi
    
    echo "Installing muesli $version ($variant)..."
    
    local url="https://github.com/$REPO/releases/download/$version/muesli-$variant"
    
    mkdir -p "$INSTALL_DIR"
    
    echo "Downloading from $url..."
    curl -fsSL "$url" -o "$INSTALL_DIR/muesli"
    chmod +x "$INSTALL_DIR/muesli"
    
    echo "Verifying checksum..."
    curl -fsSL "$url.sha256" -o /tmp/muesli.sha256
    (cd "$INSTALL_DIR" && sha256sum -c /tmp/muesli.sha256)
    rm /tmp/muesli.sha256
    
    echo ""
    echo "Installed muesli to $INSTALL_DIR/muesli"
    
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo ""
        echo "Add to PATH: export PATH=\"\$PATH:$INSTALL_DIR\""
    fi
    
    echo ""
    echo "Run 'muesli --help' to get started"
}

main "$@"
