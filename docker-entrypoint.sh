#!/bin/bash
# KI-Browser Docker Entrypoint
# Starts ki-browser with Chromiumoxide (CDP-based browser automation)
#
# Environment variables:
#   CHROME_PATH   - Path to Chrome/Chromium executable
#   KI_BROWSER_*  - Browser configuration options

set -e

echo "=============================================="
echo "KI-Browser Standalone - Chromiumoxide Edition"
echo "=============================================="
echo "Chrome: ${CHROME_PATH:-/usr/bin/chromium}"
echo "=============================================="

# Function to cleanup on exit
cleanup() {
    echo "Shutting down..."
    if [ -n "$BROWSER_PID" ]; then
        echo "Stopping KI-Browser (PID: $BROWSER_PID)"
        kill $BROWSER_PID 2>/dev/null || true
        wait $BROWSER_PID 2>/dev/null || true
    fi
    exit 0
}

# Set up signal handlers
trap cleanup SIGTERM SIGINT SIGQUIT

# Set up shared memory for Chrome
# This is critical - Chrome needs at least 1GB of shared memory
if [ ! -d /dev/shm ]; then
    echo "WARNING: /dev/shm not available. Run with --shm-size=2gb"
fi

# Start KI-Browser
echo "Starting KI-Browser..."
echo "Arguments: $@"

# Run the browser
/app/ki-browser "$@" &
BROWSER_PID=$!

echo "KI-Browser started (PID: $BROWSER_PID)"

# Wait for the browser process
wait $BROWSER_PID
EXIT_CODE=$?

echo "KI-Browser exited with code: $EXIT_CODE"

# Cleanup
cleanup

exit $EXIT_CODE
