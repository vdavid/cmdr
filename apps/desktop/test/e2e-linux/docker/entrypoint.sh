#!/bin/bash
# E2E Test Entrypoint
# Starts Xvfb (virtual display) and dbus, then runs the provided command.

set -e

echo "Starting Xvfb on display :99..."
Xvfb :99 -screen 0 1920x1080x24 &
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

# Change to the app directory if we're mounted
if [ -d "/app/apps/desktop" ]; then
    cd /app/apps/desktop
fi

echo "Running command: $@"
exec "$@"
