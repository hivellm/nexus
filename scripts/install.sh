#!/bin/bash
# Nexus Installation Script for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/hivellm/nexus/main/scripts/install.sh | bash

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="hivellm/nexus"
INSTALL_DIR="${NEXUS_INSTALL_DIR:-/usr/local/bin}"
SERVICE_DIR="/etc/systemd/system"
DATA_DIR="${NEXUS_DATA_DIR:-/var/lib/nexus}"
LOG_DIR="/var/log/nexus"
NEXUS_USER="nexus"
NEXUS_GROUP="nexus"

# Detect OS and architecture
detect_os() {
    local os=""
    local arch=""
    
    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        *)       echo -e "${RED}Unsupported OS: $(uname -s)${NC}" >&2; exit 1 ;;
    esac
    
    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *) echo -e "${RED}Unsupported architecture: $(uname -m)${NC}" >&2; exit 1 ;;
    esac
    
    echo "${os}-${arch}"
}

# Get latest release version from GitHub
get_latest_version() {
    curl -s "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/' | \
        sed 's/^v//'
}

# Download binary from GitHub releases
download_binary() {
    local version="$1"
    local platform="$2"
    local download_url="https://github.com/${REPO}/releases/download/v${version}/nexus-server-${platform}"
    
    echo -e "${YELLOW}Downloading Nexus v${version} for ${platform}...${NC}"
    
    local temp_file=$(mktemp)
    if ! curl -fsSL -o "$temp_file" "$download_url"; then
        echo -e "${RED}Failed to download binary from ${download_url}${NC}" >&2
        rm -f "$temp_file"
        exit 1
    fi
    
    echo "$temp_file"
}

# Install binary
install_binary() {
    local binary_path="$1"
    local target_path="${INSTALL_DIR}/nexus-server"
    
    echo -e "${YELLOW}Installing binary to ${target_path}...${NC}"
    
    # Create install directory if it doesn't exist
    sudo mkdir -p "$(dirname "$target_path")"
    
    # Copy binary
    sudo cp "$binary_path" "$target_path"
    sudo chmod +x "$target_path"
    
    # Cleanup temp file
    rm -f "$binary_path"
    
    echo -e "${GREEN}Binary installed successfully${NC}"
}

# Create systemd service (Linux only)
create_systemd_service() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        echo -e "${YELLOW}Skipping systemd service (not Linux)${NC}"
        return
    fi
    
    echo -e "${YELLOW}Creating systemd service...${NC}"
    
    # Create user and group if they don't exist
    if ! id "$NEXUS_USER" &>/dev/null; then
        sudo useradd -r -s /bin/false -d "$DATA_DIR" "$NEXUS_USER" || true
    fi
    
    # Create directories
    sudo mkdir -p "$DATA_DIR" "$LOG_DIR"
    sudo chown -R "${NEXUS_USER}:${NEXUS_GROUP}" "$DATA_DIR" "$LOG_DIR"
    
    # Create systemd service file
    sudo tee "${SERVICE_DIR}/nexus.service" > /dev/null <<EOF
[Unit]
Description=Nexus Graph Database Server
After=network.target

[Service]
Type=simple
User=${NEXUS_USER}
Group=${NEXUS_GROUP}
Environment="NEXUS_DATA_DIR=${DATA_DIR}"
ExecStart=${INSTALL_DIR}/nexus-server
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=nexus

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${DATA_DIR} ${LOG_DIR}

# Resource limits
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF
    
    echo -e "${GREEN}Systemd service created${NC}"
}

# Enable and start service (Linux only)
enable_service() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        echo -e "${YELLOW}Skipping service enable (not Linux)${NC}"
        return
    fi
    
    echo -e "${YELLOW}Enabling and starting Nexus service...${NC}"
    
    sudo systemctl daemon-reload
    sudo systemctl enable nexus.service
    sudo systemctl start nexus.service
    
    echo -e "${GREEN}Service enabled and started${NC}"
}

# Create launchd plist (macOS only)
create_launchd_service() {
    if [[ "$(uname -s)" != "Darwin" ]]; then
        return
    fi
    
    echo -e "${YELLOW}Creating launchd service...${NC}"
    
    local plist_path="${HOME}/Library/LaunchAgents/com.hivellm.nexus.plist"
    
    cat > "$plist_path" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.hivellm.nexus</string>
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/nexus-server</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>NEXUS_DATA_DIR</key>
        <string>${DATA_DIR}</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${LOG_DIR}/nexus.log</string>
    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/nexus.error.log</string>
</dict>
</plist>
EOF
    
    # Create data and log directories
    mkdir -p "$DATA_DIR" "$LOG_DIR"
    
    # Load service
    launchctl load "$plist_path" 2>/dev/null || true
    launchctl start com.hivellm.nexus 2>/dev/null || true
    
    echo -e "${GREEN}Launchd service created and started${NC}"
}

# Verify installation
verify_installation() {
    echo -e "${YELLOW}Verifying installation...${NC}"
    
    if ! command -v nexus-server &> /dev/null; then
        echo -e "${RED}Nexus server binary not found in PATH${NC}" >&2
        echo -e "${YELLOW}Make sure ${INSTALL_DIR} is in your PATH${NC}"
        return 1
    fi
    
    local version=$(nexus-server --version 2>/dev/null || echo "unknown")
    echo -e "${GREEN}Nexus installed successfully!${NC}"
    echo -e "${GREEN}Version: ${version}${NC}"
    echo -e "${GREEN}Binary location: $(which nexus-server)${NC}"
    
    if [[ "$(uname -s)" == "Linux" ]]; then
        if systemctl is-active --quiet nexus; then
            echo -e "${GREEN}Service status: Running${NC}"
        else
            echo -e "${YELLOW}Service status: Not running (check with: sudo systemctl status nexus)${NC}"
        fi
    fi
    
    return 0
}

# Main installation function
main() {
    echo -e "${GREEN}=== Nexus Installation ===${NC}"
    echo ""
    
    # Detect platform
    local platform=$(detect_os)
    echo -e "${YELLOW}Detected platform: ${platform}${NC}"
    
    # Get latest version
    echo -e "${YELLOW}Fetching latest version...${NC}"
    local version=$(get_latest_version)
    echo -e "${GREEN}Latest version: v${version}${NC}"
    
    # Download binary
    local temp_binary=$(download_binary "$version" "$platform")
    
    # Install binary
    install_binary "$temp_binary"
    
    # Create service
    if [[ "$(uname -s)" == "Linux" ]]; then
        create_systemd_service
        enable_service
    elif [[ "$(uname -s)" == "Darwin" ]]; then
        create_launchd_service
    fi
    
    # Verify
    verify_installation
    
    echo ""
    echo -e "${GREEN}Installation complete!${NC}"
    echo ""
    echo "Usage:"
    echo "  nexus-server --help"
    echo ""
    echo "Server will be available at: http://localhost:15474"
    echo ""
    
    if [[ "$(uname -s)" == "Linux" ]]; then
        echo "Service management:"
        echo "  sudo systemctl status nexus"
        echo "  sudo systemctl restart nexus"
        echo "  sudo systemctl stop nexus"
    fi
}

# Run main function
main "$@"

