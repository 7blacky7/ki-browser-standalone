#!/bin/bash
# KI-Browser Docker Entrypoint for CEF
# Starts Xvfb virtual display + ki-browser with CEF engine
#
# Environment variables:
#   DISPLAY          - X11 display (default: :99)
#   XVFB_RESOLUTION  - Virtual display resolution (default: 1920x1080x24)
#   KI_BROWSER_*     - Browser configuration options

set -e

echo "=============================================="
echo "KI-Browser Standalone - CEF Edition"
echo "=============================================="
echo "Display: ${DISPLAY:-:99}"
echo "Resolution: ${XVFB_RESOLUTION:-1920x1080x24}"
echo "CEF Resources: ${CEF_RESOURCES_DIR}"
echo "=============================================="

# Function to cleanup on exit
cleanup() {
    echo "Shutting down..."
    if [ -n "$BROWSER_PID" ]; then
        echo "Stopping KI-Browser (PID: $BROWSER_PID)"
        kill $BROWSER_PID 2>/dev/null || true
        wait $BROWSER_PID 2>/dev/null || true
    fi
    if [ -n "$XVFB_PID" ]; then
        echo "Stopping Xvfb (PID: $XVFB_PID)"
        kill $XVFB_PID 2>/dev/null || true
        wait $XVFB_PID 2>/dev/null || true
    fi
    exit 0
}

# Set up signal handlers
trap cleanup SIGTERM SIGINT SIGQUIT

# Set display if not set
export DISPLAY=${DISPLAY:-:99}
XVFB_RESOLUTION=${XVFB_RESOLUTION:-1920x1080x24}

# Set up shared memory for CEF
# CEF needs at least 1GB of shared memory
if [ ! -d /dev/shm ]; then
    echo "WARNING: /dev/shm not available. Run with --shm-size=2gb"
fi

# Detect GPU
if nvidia-smi &>/dev/null; then
    echo "NVIDIA GPU detected: $(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null || echo 'unknown')"
    echo "GPU rendering enabled"
else
    echo "No NVIDIA GPU detected, using software rendering"
fi

# Clean up stale Xvfb lock files from previous runs
DISPLAY_NUM=${DISPLAY#:}
rm -f /tmp/.X${DISPLAY_NUM}-lock /tmp/.X11-unix/X${DISPLAY_NUM} 2>/dev/null

# Start Xvfb virtual framebuffer
echo "Starting Xvfb on display ${DISPLAY}..."
Xvfb ${DISPLAY} -screen 0 ${XVFB_RESOLUTION} -ac +extension GLX +render -noreset &
XVFB_PID=$!

# Wait for Xvfb to be ready
echo "Waiting for Xvfb to start..."
sleep 2

# Verify Xvfb is running
if ! kill -0 $XVFB_PID 2>/dev/null; then
    echo "ERROR: Xvfb failed to start"
    exit 1
fi

echo "Xvfb started successfully (PID: $XVFB_PID)"

# Start DBus daemons (required by CEF)
if [ -x /usr/bin/dbus-daemon ]; then
    echo "Starting DBus daemons..."
    mkdir -p /run/dbus
    dbus-daemon --system --fork 2>/dev/null || true
    eval $(dbus-launch --sh-syntax) 2>/dev/null || true
fi

# Start KI-Browser
echo "Starting KI-Browser with CEF..."
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
