#!/bin/bash
# KI-Browser Docker Entrypoint
# Starts Xvfb virtual framebuffer for headless CEF operation
#
# Environment variables:
#   DISPLAY       - X display number (default: :99)
#   XVFB_RESOLUTION - Screen resolution (default: 1920x1080x24)
#   KI_BROWSER_*  - Browser configuration options

set -e

# Default values
DISPLAY="${DISPLAY:-:99}"
XVFB_RESOLUTION="${XVFB_RESOLUTION:-1920x1080x24}"

echo "=============================================="
echo "KI-Browser Standalone - CEF Edition"
echo "=============================================="
echo "Display: $DISPLAY"
echo "Resolution: $XVFB_RESOLUTION"
echo "=============================================="

# Function to cleanup on exit
cleanup() {
    echo "Shutting down..."
    if [ -n "$XVFB_PID" ]; then
        echo "Stopping Xvfb (PID: $XVFB_PID)"
        kill $XVFB_PID 2>/dev/null || true
    fi
    if [ -n "$BROWSER_PID" ]; then
        echo "Stopping KI-Browser (PID: $BROWSER_PID)"
        kill $BROWSER_PID 2>/dev/null || true
        wait $BROWSER_PID 2>/dev/null || true
    fi
    exit 0
}

# Set up signal handlers
trap cleanup SIGTERM SIGINT SIGQUIT

# Remove any existing lock files
rm -f /tmp/.X99-lock 2>/dev/null || true

# Start Xvfb virtual framebuffer
echo "Starting Xvfb on display $DISPLAY..."
Xvfb $DISPLAY -screen 0 $XVFB_RESOLUTION -ac -nolisten tcp &
XVFB_PID=$!

# Wait for Xvfb to be ready
sleep 1

# Verify Xvfb is running
if ! kill -0 $XVFB_PID 2>/dev/null; then
    echo "ERROR: Xvfb failed to start"
    exit 1
fi

echo "Xvfb started successfully (PID: $XVFB_PID)"

# Export display for CEF
export DISPLAY

# Set up shared memory for Chrome/CEF
# This is critical - Chrome needs at least 1GB of shared memory
if [ ! -d /dev/shm ]; then
    echo "WARNING: /dev/shm not available. Run with --shm-size=2gb"
fi

# CEF-specific environment variables
export CHROME_DEVEL_SANDBOX=/opt/cef/chrome-sandbox

# Disable sandbox in container (requires --privileged or specific capabilities)
# For production, configure proper sandboxing
export CEF_USE_SANDBOX=0

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
