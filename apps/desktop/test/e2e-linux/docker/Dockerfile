# E2E Testing Container for Cmdr
# This container provides a Linux environment for running Tauri E2E tests
# with tauri-driver and Playwright/WebDriverIO.

FROM ubuntu:24.04

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
    # WebKitWebDriver for tauri-driver
    webkit2gtk-driver \
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

# Install Rust (for tauri-driver)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install tauri-driver
RUN cargo install tauri-driver --locked

# Set up display for Xvfb
ENV DISPLAY=:99

# Create working directory
WORKDIR /app

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["pnpm", "test:e2e"]
