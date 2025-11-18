#!/bin/bash

# Script to manage Nexus server
# Stops all nexus-server processes and starts only one

echo "🛑 Stopping all nexus-server processes..."

# Kill all nexus-server processes
pkill -9 -f nexus-server 2>/dev/null || true

# Wait a bit to ensure all processes are closed
sleep 2

# Check if there are still processes running
if pgrep -f nexus-server > /dev/null; then
    echo "❌ There are still processes running, trying to force stop..."
    pkill -9 -f nexus-server 2>/dev/null || true
    sleep 1
fi

# Check again
if pgrep -f nexus-server > /dev/null; then
    echo "❌ Failed to stop all processes. Aborting."
    exit 1
fi

echo "✅ All nexus-server processes have been stopped."

# Go to project directory
cd /mnt/f/Node/hivellm/nexus

echo "🚀 Starting new server..."

# Start server in background
./target/release/nexus-server &
SERVER_PID=$!

echo "📝 Server PID: $SERVER_PID"

# Wait for server to start
sleep 5

# Check if server is responding
if curl -s http://localhost:15474/health | grep -q "Healthy"; then
    echo "✅ Server started successfully!"
    echo "🌐 Server running at: http://localhost:15474"
    echo "📊 PID: $SERVER_PID"
    echo ""
    echo "💡 To stop the server, run: kill $SERVER_PID"
    echo "💡 Or run this script again to restart"
else
    echo "❌ Server did not respond to health check"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi

# Keep script running to avoid killing the server
echo "🔄 Server running in background. Press Ctrl+C to stop."
trap "echo '🛑 Stopping server...'; kill $SERVER_PID 2>/dev/null || true; exit 0" INT
while true; do
    sleep 1
done
