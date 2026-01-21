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
#   ./scripts/e2e-linux.sh --clean   # Clean Linux build cache

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESKTOP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$DESKTOP_DIR/../.." && pwd)"
IMAGE_NAME="cmdr-e2e"

# Docker volume names for persistent caches
CARGO_VOLUME="cmdr-cargo-cache"
TARGET_VOLUME="cmdr-target-cache"
# Two node_modules volumes: one for monorepo root, one for apps/desktop
# This prevents Linux binaries from contaminating the host's node_modules
ROOT_NODE_MODULES_VOLUME="cmdr-root-node-modules-cache"
DESKTOP_NODE_MODULES_VOLUME="cmdr-desktop-node-modules-cache"

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
CLEAN=false

for arg in "$@"; do
    case $arg in
        --build)
            FORCE_BUILD=true
            ;;
        --shell)
            INTERACTIVE=true
            ;;
        --clean)
            CLEAN=true
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --build    Force rebuild of Docker image"
            echo "  --shell    Start interactive shell in container"
            echo "  --clean    Clean Linux build cache (forces rebuild)"
            echo "  --help     Show this help message"
            exit 0
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

# Check if a Linux binary exists in the Docker volume
# Override entrypoint to avoid Xvfb output
check_linux_build() {
    docker run --rm \
        --entrypoint /bin/bash \
        -v "$TARGET_VOLUME:/target" \
        "$IMAGE_NAME" \
        -c '
            # Check for either architecture marker
            if [ -f "/target/aarch64-unknown-linux-gnu/release/.linux-build" ]; then
                echo "aarch64-unknown-linux-gnu"
            elif [ -f "/target/x86_64-unknown-linux-gnu/release/.linux-build" ]; then
                echo "x86_64-unknown-linux-gnu"
            else
                echo ""
            fi
        ' 2>/dev/null || echo ""
}

LINUX_TARGET=$(check_linux_build)

if [ -n "$LINUX_TARGET" ]; then
    log_info "Found existing Linux build for: $LINUX_TARGET"
else
    log_warn "Linux Tauri binary not found, building inside Docker..."
    log_info "This may take a while on first run (compiling Rust code)..."

    # Build inside the container since host may be macOS
    # Use Docker volumes for cargo, target, and node_modules to avoid cross-platform issues
    # Both root and desktop node_modules are Docker volumes to prevent Linux binaries
    # from contaminating the host's node_modules
    docker run --rm \
        -v "$REPO_ROOT:/app" \
        -v "$CARGO_VOLUME:/root/.cargo" \
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
                patchelf

            # Install dependencies if needed (node_modules is a Docker volume)
            # Check root node_modules marker since that is where pnpm writes the marker
            if [ ! -f "/app/node_modules/.linux-installed" ]; then
                echo "Installing Linux node_modules..."
                pnpm install --frozen-lockfile
                touch /app/node_modules/.linux-installed
            fi

            echo "Building Tauri app for target: $LINUX_TARGET"
            # --no-bundle to skip creating .deb/.rpm/.appimage (not needed for E2E tests)
            pnpm tauri build --ci --target "$LINUX_TARGET" --no-bundle

            # Mark that we have a Linux build
            touch "/target/$LINUX_TARGET/release/.linux-build"
        '

    log_info "Linux build complete!"

    # Get the target that was built
    LINUX_TARGET=$(check_linux_build)
    if [ -z "$LINUX_TARGET" ]; then
        log_error "Build completed but marker file not found"
        exit 1
    fi
fi

log_info "Using Linux target: $LINUX_TARGET"

# The binary path inside the container (target volume is mounted at /target)
# The binary is named "Cmdr" (capital C) not "cmdr"
DOCKER_TAURI_BINARY="/target/$LINUX_TARGET/release/Cmdr"

if $INTERACTIVE; then
    log_info "Starting interactive shell in container..."
    log_info "Binary path: $DOCKER_TAURI_BINARY"
    docker run -it --rm \
        -v "$REPO_ROOT:/app" \
        -v "$CARGO_VOLUME:/root/.cargo" \
        -v "$TARGET_VOLUME:/target" \
        -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
        -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
        -w /app/apps/desktop \
        -e TAURI_BINARY="$DOCKER_TAURI_BINARY" \
        -e CI=true \
        "$IMAGE_NAME" \
        bash
else
    log_info "Running E2E tests in Docker..."
    log_info "Binary path: $DOCKER_TAURI_BINARY"
    docker run --rm \
        -v "$REPO_ROOT:/app" \
        -v "$TARGET_VOLUME:/target" \
        -v "$ROOT_NODE_MODULES_VOLUME:/app/node_modules" \
        -v "$DESKTOP_NODE_MODULES_VOLUME:/app/apps/desktop/node_modules" \
        -w /app/apps/desktop \
        -e TAURI_BINARY="$DOCKER_TAURI_BINARY" \
        -e CI=true \
        "$IMAGE_NAME" \
        bash -c '
            set -e

            # Install dependencies if needed (node_modules is a Docker volume)
            # Check root node_modules marker since that is where pnpm writes the marker
            if [ ! -f "/app/node_modules/.linux-installed" ]; then
                echo "Installing Linux node_modules..."
                pnpm install --frozen-lockfile
                touch /app/node_modules/.linux-installed
            fi

            # Run WebDriverIO tests with tauri-driver
            pnpm test:e2e:linux:native
        '
fi

log_info "Done!"
