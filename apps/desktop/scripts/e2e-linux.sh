#!/bin/bash
# Run E2E tests in Docker (Linux environment)
#
# This script builds the Docker image if needed and runs the E2E tests
# inside the container with the Tauri app mounted.
#
# Usage:
#   ./scripts/e2e-linux.sh           # Run tests
#   ./scripts/e2e-linux.sh --build   # Force rebuild Docker image
#   ./scripts/e2e-linux.sh --shell   # Start interactive shell in container
#   ./scripts/e2e-linux.sh --vnc     # Interactive VNC mode with hot reload
#   ./scripts/e2e-linux.sh --clean   # Clean Linux build cache

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESKTOP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$DESKTOP_DIR/../.." && pwd)"
IMAGE_NAME="cmdr-e2e"

# Docker volume names for persistent caches.
# Each can be overridden via env var to a host path (starting with /) for CI,
# where `actions/cache` can only cache host paths, not Docker named volumes.
CARGO_VOLUME="${CARGO_VOLUME:-cmdr-cargo-cache}"
TARGET_VOLUME="${TARGET_VOLUME:-cmdr-target-cache}"
# Two node_modules volumes: one for monorepo root, one for apps/desktop
# This prevents Linux binaries from contaminating the host's node_modules
ROOT_NODE_MODULES_VOLUME="${ROOT_NODE_MODULES_VOLUME:-cmdr-root-node-modules-cache}"
DESKTOP_NODE_MODULES_VOLUME="${DESKTOP_NODE_MODULES_VOLUME:-cmdr-desktop-node-modules-cache}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
FORCE_BUILD=false
INTERACTIVE=false
VNC_MODE=false
CLEAN=false
GREP_FILTER=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --build)
            FORCE_BUILD=true
            shift
            ;;
        --shell)
            INTERACTIVE=true
            shift
            ;;
        --vnc)
            VNC_MODE=true
            shift
            ;;
        --clean)
            CLEAN=true
            shift
            ;;
        --grep)
            GREP_FILTER="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --build           Force rebuild of Docker image"
            echo "  --shell           Start interactive shell in container"
            echo "  --vnc             Interactive VNC mode with hot reload (pnpm dev)"
            echo "  --clean           Clean Linux build cache (forces rebuild)"
            echo "  --grep <pattern>  Filter tests by title pattern (passed to Playwright --grep)"
            echo "  --help            Show this help message"
            exit 0
            ;;
        *)
            shift
            ;;
    esac
done

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    log_error "Docker is not installed or not in PATH"
    exit 1
fi

# Clean build cache if requested
if $CLEAN; then
    log_info "Cleaning Linux build cache..."
    docker volume rm "$CARGO_VOLUME" 2>/dev/null || true
    docker volume rm "$TARGET_VOLUME" 2>/dev/null || true
    docker volume rm "$ROOT_NODE_MODULES_VOLUME" 2>/dev/null || true
    docker volume rm "$DESKTOP_NODE_MODULES_VOLUME" 2>/dev/null || true
    log_info "Cache cleaned."
    exit 0
fi

# Build Docker image if needed
if $FORCE_BUILD || ! docker image inspect "$IMAGE_NAME" &> /dev/null; then
    log_info "Building Docker image: $IMAGE_NAME"
    docker build -t "$IMAGE_NAME" -f "$DESKTOP_DIR/test/e2e-linux/docker/Dockerfile" "$DESKTOP_DIR/test/e2e-linux/docker"
fi

# VNC mode: runs pnpm dev inside Docker with VNC for interactive debugging.
# Does NOT require a pre-built binary — uses Vite + Tauri hot reload instead.
if $VNC_MODE; then
    log_info "Starting VNC mode with hot reload..."

    # Temporarily clear .cargo/config.toml if present (local path patches don't exist in Docker)
    CARGO_CONFIG="$DESKTOP_DIR/src-tauri/.cargo/config.toml"
    CARGO_CONFIG_BAK=""
    if [ -f "$CARGO_CONFIG" ]; then
        CARGO_CONFIG_BAK="${CARGO_CONFIG}.docker-bak"
        cp "$CARGO_CONFIG" "$CARGO_CONFIG_BAK"
        > "$CARGO_CONFIG"
        trap 'mv "${CARGO_CONFIG_BAK}" "${CARGO_CONFIG}" 2>/dev/null || true' EXIT
    fi

    docker run -it --rm \
        -v "$REPO_ROOT:/app" \
        -v "$CARGO_VOLUME:/root/.cargo/registry" \
        -v "$TARGET_VOLUME:/target" \
        -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
        -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
        -w /app \
        -p 5990:5990 \
        -p 6090:6090 \
        -e VNC=1 \
        -e CARGO_TARGET_DIR=/target \
        -e TAURI_DEV_HOST=0.0.0.0 \
        -e RUST_LOG=info \
        "$IMAGE_NAME" \
        bash -c '
            set -e

            # Install dev packages needed for Tauri build
            apt-get update > /dev/null && apt-get install -y \
                libwebkit2gtk-4.1-dev \
                libayatana-appindicator3-dev \
                librsvg2-dev \
                libacl1-dev \
                patchelf \
                > /dev/null

            # Install dependencies if needed (node_modules is a Docker volume)
            # Compare lockfile hash to detect changes since last install
            LOCK_HASH=$(md5sum /app/pnpm-lock.yaml | cut -c1-32)
            if [ ! -f "/app/node_modules/.linux-installed" ] || [ "$(cat /app/node_modules/.linux-installed)" != "$LOCK_HASH" ]; then
                echo "Installing Linux node_modules..."
                pnpm install --frozen-lockfile
                echo "$LOCK_HASH" > /app/node_modules/.linux-installed
            fi

            echo "Starting Cmdr in dev mode (Vite HMR + Tauri)..."
            echo "Frontend edits on macOS will hot reload in ~1-3s."
            echo "Rust edits require Ctrl+C and re-run."
            echo ""
            pnpm dev
        '

    log_info "VNC session ended."
    exit 0
fi

# Always build — cargo handles incrementality (fast when nothing changed, correct when
# something did). Skipping based on binary existence caused stale-binary bugs.
log_info "Building Linux Tauri binary inside Docker..."

docker run --rm \
    -v "$REPO_ROOT:/app" \
    -v "$CARGO_VOLUME:/root/.cargo/registry" \
    -v "$TARGET_VOLUME:/target" \
    -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
    -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
    -w /app/apps/desktop \
    -e CI=true \
    -e CARGO_TARGET_DIR=/target \
    "$IMAGE_NAME" \
    bash -c '
        set -e

        # Detect architecture inside the container
        ARCH=$(uname -m)
        if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
            LINUX_TARGET="aarch64-unknown-linux-gnu"
        else
            LINUX_TARGET="x86_64-unknown-linux-gnu"
        fi
        echo "Detected architecture: $ARCH -> $LINUX_TARGET"

        # Install Tauri build dependencies (dev packages)
        apt-get update && apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libayatana-appindicator3-dev \
            librsvg2-dev \
            libacl1-dev \
            patchelf

        # Temporarily clear .cargo/config.toml if present -- it is a gitignored dev
        # override that patches mtp-rs to a local path which does not exist in Docker.
        # Uses trap to guarantee restore even if the build fails.
        CARGO_CONFIG="/app/apps/desktop/src-tauri/.cargo/config.toml"
        if [ -f "$CARGO_CONFIG" ]; then
            cp "$CARGO_CONFIG" "${CARGO_CONFIG}.docker-bak"
            > "$CARGO_CONFIG"
            trap "mv ${CARGO_CONFIG}.docker-bak $CARGO_CONFIG 2>/dev/null || true" EXIT
        fi

        # Install dependencies if needed (node_modules is a Docker volume)
        # Compare lockfile hash to detect changes since last install
        LOCK_HASH=$(md5sum /app/pnpm-lock.yaml | cut -c1-32)
        if [ ! -f "/app/node_modules/.linux-installed" ] || [ "$(cat /app/node_modules/.linux-installed)" != "$LOCK_HASH" ]; then
            echo "Installing Linux node_modules..."
            pnpm install --frozen-lockfile
            echo "$LOCK_HASH" > /app/node_modules/.linux-installed
        fi

        echo "Building Tauri app for target: $LINUX_TARGET"
        # --no-bundle to skip creating .deb/.rpm/.appimage (not needed for E2E tests)
        pnpm tauri build --ci --target "$LINUX_TARGET" --no-bundle -- --features playwright-e2e,virtual-mtp,smb-e2e

        # Write the target triple so the host script can find the binary
        echo "$LINUX_TARGET" > /target/.linux-target
    '

LINUX_TARGET=$(docker run --rm --entrypoint cat -v "$TARGET_VOLUME:/target" "$IMAGE_NAME" /target/.linux-target 2>/dev/null)
if [ -z "$LINUX_TARGET" ]; then
    log_error "Build completed but target marker not found"
    exit 1
fi

log_info "Using Linux target: $LINUX_TARGET"

# The binary path inside the container (target volume is mounted at /target)
# The binary is named "Cmdr" (capital C) not "cmdr"
DOCKER_TAURI_BINARY="/target/$LINUX_TARGET/release/Cmdr"

# ── SMB container management ────────────────────────────────────────────────
# Start Docker SMB containers for network E2E tests. The E2E test container
# joins the smb-consumer_default network so it can reach smb-consumer-guest:445
# and smb-consumer-auth:445 by container name (no host port mapping needed).
# Containers come from smb2's consumer test harness.

SMB_SERVERS_DIR="$DESKTOP_DIR/test/smb-servers"
SMB_NETWORK="smb-consumer_default"

start_smb_containers() {
    # Check that ALL four required containers are running. A prior `minimal` or
    # `core` invocation leaves guest+auth up but not 50shares/unicode, so a
    # guest-only check falsely reports "already running" and tests that need
    # the other two fail with "Cannot reach smb-consumer-50shares".
    local running
    running=$(docker compose -p smb-consumer ps --services --filter status=running 2>/dev/null || true)
    if echo "$running" | grep -q '^smb-consumer-guest$' \
        && echo "$running" | grep -q '^smb-consumer-auth$' \
        && echo "$running" | grep -q '^smb-consumer-50shares$' \
        && echo "$running" | grep -q '^smb-consumer-unicode$'; then
        log_info "SMB containers already running"
    else
        log_info "Starting SMB containers (e2e)..."
        "$SMB_SERVERS_DIR/start.sh" e2e
    fi

    # Wait for the network to exist (docker compose creates it)
    for i in $(seq 1 10); do
        docker network inspect "$SMB_NETWORK" > /dev/null 2>&1 && break
        sleep 1
    done
    if ! docker network inspect "$SMB_NETWORK" > /dev/null 2>&1; then
        log_error "SMB network '$SMB_NETWORK' not found after starting containers"
        exit 1
    fi
}

start_smb_containers

# SMB env vars: inside the Docker network, containers are addressable by name on port 445
# CMDR_MCP_ENABLED: release builds disable MCP by default — tests need it
# --privileged: needed for mount -t cifs inside the container (SYS_ADMIN alone is
# blocked by Docker's default seccomp profile which denies the mount syscall)
SMB_ENV_ARGS="-e SMB_E2E_GUEST_HOST=smb-consumer-guest -e SMB_E2E_GUEST_PORT=445 -e SMB_E2E_AUTH_HOST=smb-consumer-auth -e SMB_E2E_AUTH_PORT=445 -e SMB_E2E_50SHARES_HOST=smb-consumer-50shares -e SMB_E2E_50SHARES_PORT=445 -e SMB_CONSUMER_50SHARES_PORT=445 -e SMB_E2E_UNICODE_HOST=smb-consumer-unicode -e SMB_E2E_UNICODE_PORT=445 -e SMB_CONSUMER_UNICODE_PORT=445 -e CMDR_MCP_ENABLED=true"
SMB_DOCKER_ARGS="--privileged"

if $INTERACTIVE; then
    log_info "Starting interactive shell in container..."
    log_info "Binary path: $DOCKER_TAURI_BINARY"
    docker run -it --rm \
        --network "$SMB_NETWORK" \
        $SMB_DOCKER_ARGS \
        -v "$REPO_ROOT:/app" \
        -v "$CARGO_VOLUME:/root/.cargo/registry" \
        -v "$TARGET_VOLUME:/target" \
        -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
        -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
        -w /app/apps/desktop \
        -p 5900:5900 \
        -e TAURI_BINARY="$DOCKER_TAURI_BINARY" \
        -e CI=true \
        -e "E2E_GREP=${GREP_FILTER:-}" \
        $SMB_ENV_ARGS \
        "$IMAGE_NAME" \
        bash
else
    log_info "Running E2E tests in Docker..."
    log_info "Binary path: $DOCKER_TAURI_BINARY"
    docker run --rm \
        --network "$SMB_NETWORK" \
        $SMB_DOCKER_ARGS \
        -v "$REPO_ROOT:/app" \
        -v "$TARGET_VOLUME:/target" \
        -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
        -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
        -w /app/apps/desktop \
        -e TAURI_BINARY="$DOCKER_TAURI_BINARY" \
        -e CI=true \
        -e "E2E_GREP=${GREP_FILTER:-}" \
        $SMB_ENV_ARGS \
        "$IMAGE_NAME" \
        bash -c '
            set -e

            # Install dependencies if needed (node_modules is a Docker volume)
            # Compare lockfile hash to detect changes since last install
            LOCK_HASH=$(md5sum /app/pnpm-lock.yaml | cut -c1-32)
            if [ ! -f "/app/node_modules/.linux-installed" ] || [ "$(cat /app/node_modules/.linux-installed)" != "$LOCK_HASH" ]; then
                echo "Installing Linux node_modules..."
                pnpm install --frozen-lockfile
                echo "$LOCK_HASH" > /app/node_modules/.linux-installed
            fi

            # Playwright browsers live in /root/.cache (ephemeral), not in the
            # node_modules volume, so they must be reinstalled each container run.
            npx playwright install --with-deps chromium

            SOCKET_PATH="/tmp/tauri-playwright.sock"

            # Create fixtures via the shared helper
            export CMDR_E2E_START_PATH
            CMDR_E2E_START_PATH=$(npx tsx -e "import { createFixtures } from \"./test/e2e-shared/fixtures.js\"; console.log(createFixtures())" | tail -1)
            echo "Fixtures at: $CMDR_E2E_START_PATH"

            # Remove stale socket from a previous run
            rm -f "$SOCKET_PATH"

            # Launch the Tauri app in the background
            echo "Starting Tauri app..."
            "$TAURI_BINARY" &
            APP_PID=$!
            trap "kill $APP_PID 2>/dev/null; wait $APP_PID 2>/dev/null || true" EXIT

            # Wait for the Unix socket to appear (timeout 30s)
            echo "Waiting for playwright socket at $SOCKET_PATH..."
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

            # Run Playwright tests
            if [ -n "${E2E_GREP:-}" ]; then
                npx playwright test \
                    --config test/e2e-playwright/playwright.config.ts \
                    --project tauri \
                    --reporter=list \
                    --grep "$E2E_GREP"
            else
                npx playwright test \
                    --config test/e2e-playwright/playwright.config.ts \
                    --project tauri \
                    --reporter=list
            fi
        '
fi

log_info "Done!"
