#!/bin/bash
# KI-Browser Docker Entrypoint for CEF
# Starts an X server + ki-browser with CEF engine.
#
# X server selection (KI_BROWSER_X_SERVER=auto|xorg|xvfb, default: auto):
#   xorg - real Xorg with the NVIDIA X driver: hardware-backed GLX, required
#          for real GPU WebGL (CEF ANGLE-gl backend). Falls back to Xvfb when
#          GPU/driver/X server are unavailable.
#   xvfb - software X server (no NVIDIA GLX, GL lands on Mesa/llvmpipe).
#   auto - xorg when an NVIDIA GPU is visible, else xvfb.
#
# The NVIDIA X server modules (nvidia_drv.so, libglxserver_nvidia.so) must
# match the HOST driver version exactly. The NVIDIA container runtime mounts
# the GL *client* libraries only, so the X modules are downloaded once per
# driver version from download.nvidia.com and cached under /app/data.
#
# Environment variables:
#   DISPLAY                  - X11 display (default: :99)
#   XVFB_RESOLUTION          - Virtual display resolution (default: 1920x1080x24)
#   KI_BROWSER_X_SERVER      - auto|xorg|xvfb (default: auto)
#   KI_BROWSER_ANGLE_BACKEND - CEF GL backend; exported as "gl" by this
#                              script when Xorg+NVIDIA runs (unless preset)
#   KI_BROWSER_*             - Browser configuration options

set -e

echo "=============================================="
echo "KI-Browser Standalone - CEF Edition"
echo "=============================================="
echo "Display: ${DISPLAY:-:99}"
echo "Resolution: ${XVFB_RESOLUTION:-1920x1080x24}"
echo "X server mode: ${KI_BROWSER_X_SERVER:-auto}"
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
    if [ -n "$X_PID" ]; then
        echo "Stopping X server (PID: $X_PID)"
        kill $X_PID 2>/dev/null || true
        wait $X_PID 2>/dev/null || true
    fi
    exit 0
}

# Set up signal handlers
trap cleanup SIGTERM SIGINT SIGQUIT

# Set display if not set
export DISPLAY=${DISPLAY:-:99}
XVFB_RESOLUTION=${XVFB_RESOLUTION:-1920x1080x24}
X_SERVER_MODE=${KI_BROWSER_X_SERVER:-auto}
DISPLAY_NUM=${DISPLAY#:}
SCREEN_W=$(echo "${XVFB_RESOLUTION}" | cut -dx -f1)
SCREEN_H=$(echo "${XVFB_RESOLUTION}" | cut -dx -f2)

case "$X_SERVER_MODE" in
    auto|xorg|xvfb) ;;
    *)
        echo "WARNING: unknown KI_BROWSER_X_SERVER='${X_SERVER_MODE}', treating as 'auto'"
        X_SERVER_MODE=auto
        ;;
esac

# Set up shared memory for CEF
# CEF needs at least 1GB of shared memory
if [ ! -d /dev/shm ]; then
    echo "WARNING: /dev/shm not available. Run with --shm-size=2gb"
fi

# Detect GPU
GPU_AVAILABLE=false
if nvidia-smi &>/dev/null; then
    GPU_AVAILABLE=true
    echo "NVIDIA GPU detected: $(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)"
else
    echo "No NVIDIA GPU detected, using software rendering"
fi

# Clean up stale X lock files from previous runs
rm -f /tmp/.X${DISPLAY_NUM}-lock /tmp/.X11-unix/X${DISPLAY_NUM} 2>/dev/null

# Provision the NVIDIA X server modules matching the host driver version.
# Only nvidia_drv.so + libglxserver_nvidia.so are extracted from the official
# .run installer — the installer itself is never executed, so the GL client
# libraries mounted by the NVIDIA container runtime stay untouched.
ensure_nvidia_x_driver() {
    local ver="$1"
    local drivers_dir=/usr/lib/xorg/modules/drivers
    local ext_dir=/usr/lib/xorg/modules/extensions
    local marker="${drivers_dir}/.nvidia-x-driver-version"

    if [ -f "$marker" ] && [ "$(cat "$marker")" = "$ver" ] && [ -f "${drivers_dir}/nvidia_drv.so" ]; then
        echo "NVIDIA X driver modules ${ver} already installed"
        return 0
    fi

    local cache_dir="/app/data/nvidia-x-driver/${ver}"
    if [ ! -f "${cache_dir}/nvidia_drv.so" ]; then
        echo "Fetching NVIDIA X driver modules ${ver} (one-time per driver version)..."
        local run=/tmp/nvidia-driver.run
        local extract=/tmp/nvidia-driver-extract
        curl -fSL --connect-timeout 15 -o "$run" \
            "https://us.download.nvidia.com/XFree86/Linux-x86_64/${ver}/NVIDIA-Linux-x86_64-${ver}.run" \
        || curl -fSL --connect-timeout 15 -o "$run" \
            "https://download.nvidia.com/XFree86/Linux-x86_64/${ver}/NVIDIA-Linux-x86_64-${ver}.run" \
        || { echo "WARNING: could not download NVIDIA driver ${ver}"; rm -f "$run"; return 1; }
        rm -rf "$extract"
        sh "$run" -x --target "$extract" >/dev/null 2>&1 \
            || { echo "WARNING: NVIDIA driver self-extract failed"; rm -rf "$run" "$extract"; return 1; }
        [ -f "${extract}/nvidia_drv.so" ] \
            || { echo "WARNING: nvidia_drv.so not found in installer"; rm -rf "$run" "$extract"; return 1; }
        mkdir -p "$cache_dir"
        cp "${extract}/nvidia_drv.so" "$cache_dir/"
        cp "${extract}"/libglxserver_nvidia.so.* "$cache_dir/" 2>/dev/null || true
        rm -rf "$run" "$extract"
    fi

    mkdir -p "$drivers_dir" "$ext_dir"
    cp "${cache_dir}/nvidia_drv.so" "$drivers_dir/"
    local glxserver
    glxserver=$(ls "${cache_dir}"/libglxserver_nvidia.so.* 2>/dev/null | head -1)
    if [ -n "$glxserver" ]; then
        cp "$glxserver" "$ext_dir/"
        ln -sf "$(basename "$glxserver")" "${ext_dir}/libglxserver_nvidia.so"
    fi
    echo "$ver" > "$marker"
    echo "NVIDIA X driver modules ${ver} installed"
}

# xorg.conf for a headless NVIDIA screen. BusID must be the DECIMAL PCI
# triplet (nvidia-smi prints hex). AllowEmptyInitialConfiguration +
# UseDisplayDevice None let the server start without a connected monitor.
write_xorg_conf() {
    local bus_id_hex bus dev func
    bus_id_hex=$(nvidia-smi --query-gpu=pci.bus_id --format=csv,noheader 2>/dev/null | head -1)
    if [ -z "$bus_id_hex" ]; then
        echo "WARNING: could not query GPU PCI BusID"
        return 1
    fi
    # Format: 00000000:09:00.0 (domain:bus:device.function, hex)
    bus=$((16#$(echo "$bus_id_hex" | cut -d: -f2)))
    dev=$((16#$(echo "$bus_id_hex" | cut -d: -f3 | cut -d. -f1)))
    func=$(echo "$bus_id_hex" | cut -d. -f2)
    echo "GPU BusID: ${bus_id_hex} -> PCI:${bus}:${dev}:${func}"

    cat > /tmp/xorg.conf <<EOF
Section "ServerLayout"
    Identifier "Layout0"
    Screen 0 "Screen0" 0 0
EndSection

Section "ServerFlags"
    Option "AutoAddDevices" "false"
    Option "AutoAddGPU" "false"
EndSection

Section "Device"
    Identifier "NVIDIA GPU"
    Driver "nvidia"
    BusID "PCI:${bus}:${dev}:${func}"
    Option "AllowEmptyInitialConfiguration" "true"
    Option "HardDPMS" "false"
EndSection

Section "Screen"
    Identifier "Screen0"
    Device "NVIDIA GPU"
    DefaultDepth 24
    Option "UseDisplayDevice" "None"
    SubSection "Display"
        Depth 24
        Virtual ${SCREEN_W} ${SCREEN_H}
    EndSubSection
EndSection
EOF
}

# Wait until the X socket exists and the display answers (max ~10s)
wait_for_x() {
    local pid="$1" name="$2"
    local i
    for i in $(seq 1 20); do
        if [ -S "/tmp/.X11-unix/X${DISPLAY_NUM}" ] && xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then
            echo "${name} started successfully (PID: ${pid})"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            return 1
        fi
        sleep 0.5
    done
    return 1
}

start_xvfb() {
    echo "Starting Xvfb on display ${DISPLAY}..."
    Xvfb ${DISPLAY} -screen 0 ${XVFB_RESOLUTION} -ac +extension GLX +render -noreset &
    X_PID=$!
    if ! wait_for_x "$X_PID" "Xvfb"; then
        echo "ERROR: Xvfb failed to start"
        exit 1
    fi
    X_SERVER_KIND=xvfb
}

start_xorg() {
    write_xorg_conf || return 1
    echo "Starting Xorg (NVIDIA) on display ${DISPLAY}..."
    Xorg ${DISPLAY} -config /tmp/xorg.conf -logfile /tmp/xorg.log \
        -noreset -nolisten tcp -novtswitch -sharevts +extension GLX &
    X_PID=$!
    if wait_for_x "$X_PID" "Xorg (NVIDIA)"; then
        X_SERVER_KIND=xorg
        return 0
    fi
    echo "WARNING: Xorg (NVIDIA) failed to start — falling back to Xvfb. Log tail:"
    tail -n 25 /tmp/xorg.log 2>/dev/null || true
    kill "$X_PID" 2>/dev/null || true
    wait "$X_PID" 2>/dev/null || true
    X_PID=""
    rm -f /tmp/.X${DISPLAY_NUM}-lock /tmp/.X11-unix/X${DISPLAY_NUM} 2>/dev/null
    return 1
}

# Start the X server: real Xorg+NVIDIA when possible, Xvfb otherwise
X_SERVER_KIND=""
if [ "$X_SERVER_MODE" != "xvfb" ] && [ "$GPU_AVAILABLE" = "true" ]; then
    DRIVER_VERSION=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -1)
    echo "Host NVIDIA driver: ${DRIVER_VERSION:-unknown}"
    if [ -n "$DRIVER_VERSION" ] && ensure_nvidia_x_driver "$DRIVER_VERSION"; then
        start_xorg || true
    fi
fi
if [ -z "$X_SERVER_KIND" ]; then
    if [ "$X_SERVER_MODE" = "xorg" ]; then
        echo "WARNING: KI_BROWSER_X_SERVER=xorg requested but Xorg is unavailable — using Xvfb"
    fi
    start_xvfb
fi

# Match the CEF GL backend to the X server: a real Xorg+NVIDIA server exposes
# hardware GLX which CEF reaches via ANGLE's OpenGL backend (use-angle=gl —
# verified: hardware RTX 2070; CEF 144 rejects --use-gl=desktop with SIGTRAP).
# Under Xvfb the stable ANGLE gl-egl (software) default stays active.
if [ "$X_SERVER_KIND" = "xorg" ] && [ -z "$KI_BROWSER_ANGLE_BACKEND" ]; then
    export KI_BROWSER_ANGLE_BACKEND=gl
fi
echo "X server: ${X_SERVER_KIND} | CEF GL backend: ${KI_BROWSER_ANGLE_BACKEND:-gl-egl (default)}"

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
