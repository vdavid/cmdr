#!/bin/bash
# E2E Test Entrypoint
# Starts Xvfb (virtual display) and dbus, then runs the provided command.

set -e

echo "Starting Xvfb on display :99..."
Xvfb :99 -screen 0 1920x1080x24 2>/dev/null &
XVFB_PID=$!

# Wait for Xvfb to be ready (poll instead of fixed sleep)
for i in $(seq 1 20); do
  xdpyinfo -display :99 >/dev/null 2>&1 && break
  sleep 0.1
done

# Set display environment variables
export DISPLAY=:99
export GDK_BACKEND=x11

# Pin Playwright's host-platform tag to a 24.04 build that Playwright publishes
# (the 26.04 base image has no Playwright chromium build yet — see Dockerfile).
# Must be arch-aware AND carry the arch suffix: Playwright's registry keys are
# `ubuntu24.04-x64` and `ubuntu24.04-arm64` (see playwright-core
# registry/index.js). The bare `ubuntu24.04` (no suffix) matches no key, so
# `playwright install chromium` fails with "does not support chromium on
# ubuntu24.04" — that bare-amd64 value is what kept Linux CI red. arm64 is local
# on Apple Silicon; x64 is CI on x86_64 runners.
case "$(dpkg --print-architecture)" in
    arm64) export PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-arm64 ;;
    amd64) export PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=ubuntu24.04-x64 ;;
esac

# Start dbus (required for WebKitGTK)
echo "Starting dbus..."
eval "$(dbus-launch --sh-syntax)"
export DBUS_SESSION_BUS_ADDRESS

# Set XDG runtime directory (needed by some GTK apps and GVFS)
export XDG_RUNTIME_DIR=/run/user/$(id -u)
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# Start GVFS daemon (needed for gio mount, which Cmdr uses for SMB mounting on Linux)
echo "Starting GVFS daemon..."
/usr/libexec/gvfsd &
sleep 0.5

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

# Change to the app directory if we're mounted (skip in VNC mode: it runs from repo root)
if [ -d "/app/apps/desktop" ] && [ "${VNC:-}" != "1" ]; then
    cd /app/apps/desktop
fi

echo "Running command: $*"
exec "$@"
