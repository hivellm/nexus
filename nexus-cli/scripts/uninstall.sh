#!/bin/bash
# Nexus CLI Uninstall Script for Linux/macOS

set -e

BINARY_NAME="nexus"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/nexus"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warning() { echo -e "${YELLOW}[WARNING]${NC} $1"; }

echo ""
echo "================================"
echo "  Nexus CLI Uninstaller"
echo "================================"
echo ""

# Remove binary
if [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
    info "Removing binary from $INSTALL_DIR..."
    if [ -w "$INSTALL_DIR" ]; then
        rm -f "$INSTALL_DIR/$BINARY_NAME"
    else
        sudo rm -f "$INSTALL_DIR/$BINARY_NAME"
    fi
    info "Binary removed."
else
    warning "Binary not found at $INSTALL_DIR/$BINARY_NAME"
fi

# Ask about config
if [ -d "$CONFIG_DIR" ]; then
    echo ""
    read -p "Remove configuration directory ($CONFIG_DIR)? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$CONFIG_DIR"
        info "Configuration removed."
    else
        info "Configuration preserved at $CONFIG_DIR"
    fi
fi

# Remove man pages
MAN_PAGES=(
    "/usr/local/share/man/man1/nexus.1"
    "/usr/local/share/man/man1/nexus-query.1"
    "/usr/local/share/man/man1/nexus-db.1"
    "/usr/local/share/man/man1/nexus-user.1"
    "/usr/local/share/man/man1/nexus-key.1"
)

for man_page in "${MAN_PAGES[@]}"; do
    if [ -f "$man_page" ]; then
        if [ -w "$(dirname "$man_page")" ]; then
            rm -f "$man_page"
        else
            sudo rm -f "$man_page" 2>/dev/null || true
        fi
    fi
done

echo ""
info "Nexus CLI has been uninstalled."
echo ""
