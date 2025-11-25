#!/bin/bash
# Start Nexus server in background with logging

cd /mnt/f/Node/hivellm/nexus || exit 1

# Kill any existing server
pkill -f nexus-server || true

# Start server in background and log to file
env NEXUS_DATA_DIR=/mnt/f/Node/hivellm/nexus/data RUST_LOG=debug \
    nohup ./target/release/nexus-server > /tmp/nexus-server.log 2>&1 &

# Get PID
SERVER_PID=$!
echo $SERVER_PID
