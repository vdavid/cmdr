# E2E Testing Container for Cmdr
# This container provides a Linux environment for running Tauri E2E tests
# via tauri-playwright (Unix socket bridge to the real Tauri webview).

FROM ubuntu:26.04

# Avoid prompts during package installation
ENV DEBIAN_FRONTEND=noninteractive

# Install Tauri dependencies (matching official prerequisites)
# See: https://tauri.app/start/prerequisites/#linux
RUN apt-get update && apt-get install -y \
    # Official Tauri prerequisites for Debian
    libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    # ACL support for file operations
    libacl1-dev \
    # X11 virtual framebuffer for headless GUI
    xvfb \
    dbus-x11 \
    # X11 libraries needed for GTK
    libx11-6 \
    libxcomposite1 \
    libxdamage1 \
    libxext6 \
    libxfixes3 \
    libxrandr2 \
    libxcursor1 \
    libxi6 \
    libxtst6 \
    libxrender1 \
    # GLib/GTK additional runtime
    libglib2.0-0 \
    libpango-1.0-0 \
    libcairo2 \
    libatk1.0-0 \
    libgdk-pixbuf-2.0-0 \
    # Chromium runtime libs for Playwright's headless_shell. The tauri-playwright
    # fixture launches a real headless chromium even though no spec drives a
    # browser directly, so the binary needs these to start. We install them here
    # (plain apt on 26.04) instead of via `playwright install --with-deps`, which
    # fails on 26.04 because Playwright 1.59 only knows ubuntu <=24.04. Keep this
    # list in sync with Playwright's chromium deps when bumping the version (see
    # node_modules/playwright-core/.../registry/nativeDeps.js, ubuntu24.04-x64).
    libnss3 \
    libnspr4 \
    libgbm1 \
    libdrm2 \
    libcups2t64 \
    libxkbcommon0 \
    libatspi2.0-0 \
    libatk-bridge2.0-0 \
    libasound2t64 \
    libxcb1 \
    # Go (needed by beforeBuildCommand for llama-server placeholder)
    golang-go \
    # VNC for interactive debugging (--vnc mode)
    x11vnc \
    novnc \
    python3-websockify \
    # SMB support for network E2E tests
    smbclient \
    cifs-utils \
    # Git for tests that synthesize repository fixtures (git-portal.spec.ts)
    git \
    # GVFS for gio mount (used by Cmdr's mount_linux.rs for user-space SMB mounting)
    gvfs \
    gvfs-backends \
    gvfs-daemons \
    gvfs-fuse \
    gvfs-common \
    gvfs-libs \
    # Useful utilities
    ca-certificates \
    # Clean up
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 22 LTS
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install pnpm
RUN npm install -g pnpm

# Install Rust (needed to compile the Tauri binary with --features playwright-e2e)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Set up display for Xvfb
ENV DISPLAY=:99

# Playwright doesn't publish chromium binaries for ubuntu26.04 yet, but the
# ubuntu24.04 build runs fine on 26.04 (libc-compatible). Without an override,
# `npx playwright install chromium` errors with "Playwright does not support
# chromium on ubuntu26.04[-arm64]". The override has to be arch-aware (arm64
# locally on macOS, x64 in CI on GitHub-hosted Linux runners) — set it in
# entrypoint.sh based on `dpkg --print-architecture`, not as a static ENV here.
# Drop this once Playwright adds 26.04 to its supported list.

# Create working directory
WORKDIR /app

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["pnpm", "test:e2e"]
