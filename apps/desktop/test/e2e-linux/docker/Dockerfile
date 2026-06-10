# E2E Testing Container for Cmdr — the thin, fast-changing layer.
# This container provides a Linux environment for running Tauri E2E tests
# via tauri-playwright (Unix socket bridge to the real Tauri webview).
#
# All slow system setup (apt packages, Node, pnpm, Rust) is baked into the
# base image (`Dockerfile.base`), which `e2e-linux.sh` builds and tags as
# `cmdr-e2e-base:<content-hash>` and passes in via BASE_IMAGE. This file only
# adds the entrypoint, so it rebuilds in ~1 s on every run — entrypoint.sh
# edits propagate automatically, no --build needed.

ARG BASE_IMAGE=cmdr-e2e-base:latest
FROM ${BASE_IMAGE}

# Create working directory
WORKDIR /app

# Copy entrypoint script
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["/entrypoint.sh"]
CMD ["pnpm", "test:e2e"]
