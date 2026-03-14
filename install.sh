#!/bin/bash
set -euo pipefail

# Velocity installer
# Usage: curl -fsSL https://raw.githubusercontent.com/omkar-dev/velocity/main/install.sh | bash

VERSION="${VELOCITY_VERSION:-latest}"
INSTALL_DIR="${VELOCITY_INSTALL_DIR:-$HOME/.velocity/bin}"
REPO="omkar-dev/velocity"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo -e "${BLUE}${BOLD}info:${NC} $1"; }
success() { echo -e "${GREEN}${BOLD}success:${NC} $1"; }
error() { echo -e "${RED}${BOLD}error:${NC} $1" >&2; exit 1; }

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        *)       error "Unsupported OS: $(uname -s). Velocity supports macOS and Linux." ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${os}-${arch}"
}

# Get the download URL for the latest release
get_download_url() {
    local platform="$1"

    if [ "$VERSION" = "latest" ]; then
        local url="https://api.github.com/repos/${REPO}/releases/latest"
        local release_info
        release_info=$(curl -fsSL "$url" 2>/dev/null) || error "Failed to fetch latest release info"
        VERSION=$(echo "$release_info" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')
    fi

    echo "https://github.com/${REPO}/releases/download/${VERSION}/velocity-${platform}.tar.gz"
}

main() {
    echo ""
    echo -e "${BOLD}  ⚡ Velocity Installer${NC}"
    echo ""

    local platform
    platform=$(detect_platform)
    info "Detected platform: ${platform}"

    local download_url
    download_url=$(get_download_url "$platform")
    info "Downloading velocity ${VERSION}..."

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    if curl -fsSL "$download_url" -o "${tmp_dir}/velocity.tar.gz" 2>/dev/null; then
        tar -xzf "${tmp_dir}/velocity.tar.gz" -C "$INSTALL_DIR"
        chmod +x "${INSTALL_DIR}/velocity"
    else
        # If no release binary, fall back to cargo install
        info "No pre-built binary found. Building from source via cargo..."
        if ! command -v cargo &>/dev/null; then
            error "cargo not found. Install Rust first: https://rustup.rs"
        fi
        cargo install --git "https://github.com/${REPO}.git" velocity-cli --root "${INSTALL_DIR%/bin}"
    fi

    # Add to PATH
    local shell_config=""
    case "${SHELL:-}" in
        */zsh)  shell_config="$HOME/.zshrc" ;;
        */bash) shell_config="$HOME/.bashrc" ;;
        */fish) shell_config="$HOME/.config/fish/config.fish" ;;
    esac

    local path_entry="export PATH=\"${INSTALL_DIR}:\$PATH\""
    if [ -n "$shell_config" ] && ! grep -q ".velocity/bin" "$shell_config" 2>/dev/null; then
        echo "" >> "$shell_config"
        echo "# Velocity" >> "$shell_config"
        if [[ "$shell_config" == *"fish"* ]]; then
            echo "set -gx PATH ${INSTALL_DIR} \$PATH" >> "$shell_config"
        else
            echo "$path_entry" >> "$shell_config"
        fi
        info "Added ${INSTALL_DIR} to PATH in ${shell_config}"
    fi

    export PATH="${INSTALL_DIR}:$PATH"

    echo ""
    success "Velocity installed successfully!"
    echo ""
    echo -e "  Run ${BOLD}velocity version${NC} to verify."
    echo -e "  Run ${BOLD}velocity run tests/${NC} to start testing."
    echo ""

    if [ -n "$shell_config" ]; then
        echo -e "  ${BLUE}Restart your terminal or run:${NC} source ${shell_config}"
        echo ""
    fi
}

main
