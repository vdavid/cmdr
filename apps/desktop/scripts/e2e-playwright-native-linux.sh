#!/bin/bash
# Run Playwright E2E tests on native Linux (no Docker).
# Launches the Tauri app, waits for the playwright socket, runs tests.
#
# Requires:
#   - Tauri binary built with --features playwright-e2e
#   - TAURI_BINARY env var or the binary at the default release path
#
# Usage:
#   TAURI_BINARY=path/to/Cmdr ./scripts/e2e-playwright-native-linux.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESKTOP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$DESKTOP_DIR"

SOCKET_PATH="/tmp/tauri-playwright.sock"
BINARY="${TAURI_BINARY:-target/release/Cmdr}"

if [ ! -x "$BINARY" ]; then
    echo "ERROR: Tauri binary not found at $BINARY"
    echo "Build it with: pnpm test:e2e:playwright:build"
    exit 1
fi

# Create fixtures
export CMDR_E2E_START_PATH
CMDR_E2E_START_PATH=$(npx tsx -e "import { createFixtures } from './test/e2e-shared/fixtures.js'; console.log(createFixtures())")
echo "Fixtures at: $CMDR_E2E_START_PATH"

# Remove stale socket
rm -f "$SOCKET_PATH"

# Launch app
echo "Starting Tauri app..."
"$BINARY" &
APP_PID=$!
trap 'kill $APP_PID 2>/dev/null; wait $APP_PID 2>/dev/null' EXIT

# Wait for socket (30s timeout)
echo "Waiting for playwright socket..."
for i in $(seq 1 60); do
    [ -S "$SOCKET_PATH" ] && break
    if ! kill -0 $APP_PID 2>/dev/null; then
        echo "ERROR: Tauri app exited prematurely"
        exit 1
    fi
    sleep 0.5
done
if [ ! -S "$SOCKET_PATH" ]; then
    echo "ERROR: Socket did not appear within 30s"
    exit 1
fi
echo "Socket ready."

# Run tests
npx playwright test \
    --config test/e2e-playwright/playwright.config.ts \
    --project tauri \
    --reporter=list
