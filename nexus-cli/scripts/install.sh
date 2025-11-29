#!/bin/bash
# Nexus CLI Installation Script for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/nexus-cli/scripts/install.sh | bash

set -e

# Configuration
REPO="hivellm/nexus"
BINARY_NAME="nexus"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/nexus"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect OS and architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)
            case "$ARCH" in
                x86_64) PLATFORM="linux-x86_64" ;;
                aarch64|arm64) PLATFORM="linux-aarch64" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        darwin)
            case "$ARCH" in
                x86_64) PLATFORM="darwin-x86_64" ;;
                arm64) PLATFORM="darwin-aarch64" ;;
                *) error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        *)
            error "Unsupported operating system: $OS"
            ;;
    esac

    info "Detected platform: $PLATFORM"
}

# Get latest version from GitHub
get_latest_version() {
    if command -v curl &> /dev/null; then
        VERSION=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/')
    elif command -v wget &> /dev/null; then
        VERSION=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/')
    else
        error "Neither curl nor wget is available. Please install one."
    fi

    if [ -z "$VERSION" ]; then
        VERSION="0.11.0"  # Fallback version
        warning "Could not determine latest version, using $VERSION"
    fi

    info "Installing version: $VERSION"
}

# Download and install binary
install_binary() {
    local DOWNLOAD_URL="https://github.com/$REPO/releases/download/v$VERSION/nexus-$PLATFORM.tar.gz"
    local TMP_DIR=$(mktemp -d)

    info "Downloading from: $DOWNLOAD_URL"

    # Download
    if command -v curl &> /dev/null; then
        curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/nexus.tar.gz" || error "Download failed"
    else
        wget -q "$DOWNLOAD_URL" -O "$TMP_DIR/nexus.tar.gz" || error "Download failed"
    fi

    # Extract
    info "Extracting..."
    tar -xzf "$TMP_DIR/nexus.tar.gz" -C "$TMP_DIR"

    # Install
    info "Installing to $INSTALL_DIR..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMP_DIR/nexus" "$INSTALL_DIR/$BINARY_NAME"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        sudo mv "$TMP_DIR/nexus" "$INSTALL_DIR/$BINARY_NAME"
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    # Cleanup
    rm -rf "$TMP_DIR"
}

# Install man pages (optional)
install_man_pages() {
    local MAN_DIR="/usr/local/share/man/man1"

    if [ -d "$MAN_DIR" ] || [ -w "/usr/local/share/man" ]; then
        info "Installing man pages..."
        # Man pages would be included in the release archive
    fi
}

# Create default config
create_default_config() {
    if [ ! -d "$CONFIG_DIR" ]; then
        info "Creating config directory: $CONFIG_DIR"
        mkdir -p "$CONFIG_DIR"
    fi

    if [ ! -f "$CONFIG_DIR/config.toml" ]; then
        info "Creating default configuration..."
        cat > "$CONFIG_DIR/config.toml" << 'EOF'
# Nexus CLI Configuration
# See: nexus config --help

url = "http://localhost:3000"
# username = "root"
# password = ""
# api_key = ""

# Connection profiles
# [profiles.production]
# url = "https://production.example.com:3000"
# api_key = "your-api-key"

# [profiles.staging]
# url = "https://staging.example.com:3000"
# api_key = "your-api-key"
EOF
    fi
}

# Add to PATH if needed
update_path() {
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warning "$INSTALL_DIR is not in your PATH"

        # Detect shell
        if [ -n "$ZSH_VERSION" ]; then
            SHELL_RC="$HOME/.zshrc"
        elif [ -n "$BASH_VERSION" ]; then
            SHELL_RC="$HOME/.bashrc"
        else
            SHELL_RC="$HOME/.profile"
        fi

        echo ""
        info "Add the following to your $SHELL_RC:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        echo ""
    fi
}

# Verify installation
verify_installation() {
    if command -v "$BINARY_NAME" &> /dev/null; then
        local installed_version=$("$BINARY_NAME" --version 2>/dev/null | head -1)
        success "Nexus CLI installed successfully!"
        info "Version: $installed_version"
        info "Location: $(which $BINARY_NAME)"
    else
        warning "Binary installed but not in PATH"
        info "Binary location: $INSTALL_DIR/$BINARY_NAME"
    fi
}

# Main
main() {
    echo ""
    echo "================================"
    echo "  Nexus CLI Installer"
    echo "================================"
    echo ""

    detect_platform
    get_latest_version
    install_binary
    create_default_config
    update_path
    verify_installation

    echo ""
    echo "Quick start:"
    echo "  nexus --help"
    echo "  nexus config init"
    echo "  nexus db ping"
    echo ""
}

main "$@"
