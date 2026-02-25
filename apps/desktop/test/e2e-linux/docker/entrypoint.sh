#!/bin/bash
# E2E Test Entrypoint
# Starts Xvfb (virtual display) and dbus, then runs the provided command.

set -e

echo "Starting Xvfb on display :99..."
Xvfb :99 -screen 0 1920x1080x24 2>/dev/null &
XVFB_PID=$!

# Wait for Xvfb to start
sleep 2

# Set display environment variables
export DISPLAY=:99
export GDK_BACKEND=x11

# Start dbus (required for WebKitGTK)
echo "Starting dbus..."
eval "$(dbus-launch --sh-syntax)"
export DBUS_SESSION_BUS_ADDRESS

# Set XDG runtime directory (needed by some GTK apps)
export XDG_RUNTIME_DIR=/tmp/runtime-root
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Legacy test fixture for viewer.spec.ts (references /root/test-dir/test-file.txt directly)
mkdir -p /root/test-dir/sub-dir
echo "test content" > /root/test-dir/test-file.txt

# Main E2E fixtures are created by wdio.conf.ts via the shared fixture helper

# Verify environment
echo "Environment:"
echo "  DISPLAY=$DISPLAY"
echo "  GDK_BACKEND=$GDK_BACKEND"
echo "  DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS"
echo "  XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"

# Verify Xvfb is running
if ! kill -0 $XVFB_PID 2>/dev/null; then
    echo "ERROR: Xvfb failed to start!"
    exit 1
fi
echo "Xvfb is running (PID: $XVFB_PID)"

# Start VNC server if requested (for interactive debugging with --vnc mode)
if [ "${VNC:-}" = "1" ]; then
    echo "Starting VNC server..."
    x11vnc -display :99 -forever -nopw -shared -rfbport 5990 -q &
    /usr/share/novnc/utils/novnc_proxy --vnc localhost:5990 --listen 6090 &>/dev/null &
    echo ""
    echo "╔═══════════════════════════════════════════════════════════════════╗"
    echo "║  Open in browser: http://localhost:6090/vnc.html?autoconnect=true  ║"
    echo "║  Native VNC:      vnc://localhost:5990 (no password)               ║"
    echo "╚═══════════════════════════════════════════════════════════════════╝"
    echo ""
fi

# Change to the app directory if we're mounted (skip in VNC mode — it runs from repo root)
if [ -d "/app/apps/desktop" ] && [ "${VNC:-}" != "1" ]; then
    cd /app/apps/desktop
fi

echo "Running command: $*"
exec "$@"
