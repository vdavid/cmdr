# Browser demo stage: the desktop frontend on its fake backend, plus the listing engine built to
# WASM, which the site publishes at /demo/ (`apps/website/DETAILS.md` § Browser demo). It needs
# Rust, Go (the WASM build script), and Node. Go's image is the base because it already carries the
# C toolchain build scripts link with; Node comes from its official image, Rust from rustup.
FROM golang:1.27.3-bookworm@sha256:d9b6a1beb9474adc2a1ce842573639b44b2a0dd49b6669f51d3821baf9ec37c9 AS demo

COPY --from=node:26.11.1-bookworm-slim@sha256:86f07bc9c5dce4578cf37e5a418b7bfc7f817cda25cde66e2b66e95ed86c4567 /usr/local/bin/node /usr/local/bin/node
COPY --from=node:26.11.1-bookworm-slim@sha256:86f07bc9c5dce4578cf37e5a418b7bfc7f817cda25cde66e2b66e95ed86c4567 /usr/local/lib/node_modules/npm /usr/local/lib/node_modules/npm
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm && npm install --global pnpm@12.11.2

# rustup itself, checksum-pinned per arch (amd64 on the server, arm64 for a local build on a Mac).
# The toolchain it installs below is the one `rust-toolchain.toml` pins.
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo CARGO_INSTALL_ROOT=/opt/cargo-tools
ENV PATH=/opt/cargo-tools/bin:/opt/cargo/bin:$PATH
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
        amd64) triple=x86_64-unknown-linux-gnu; sum=5be191e8c70a5007074254cb74d78dbfdd25cd5a61c27de7b8e5410122ec24f4 ;; \
        arm64) triple=aarch64-unknown-linux-gnu; sum=a9c168dbea81a84671c6b7a5c1795cb0eb699020110d107ddfca427bd797e697 ;; \
        *) echo "No rustup checksum for $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    curl -fsSLo /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/1.29.2/$triple/rustup-init"; \
    echo "$sum  /tmp/rustup-init" | sha256sum -c -; \
    chmod +x /tmp/rustup-init; \
    /tmp/rustup-init -y --no-modify-path --profile minimal --default-toolchain none; \
    rm /tmp/rustup-init

WORKDIR /app
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
COPY vendor/ ./vendor/
COPY scripts/build-listing-wasm.sh ./scripts/
COPY scripts/build-listing-wasm/ ./scripts/build-listing-wasm/
COPY apps/desktop/ ./apps/desktop/
# The frontend's feature-status badges read it from the repo root
COPY feature-status.json ./

# Unchanged sources reuse this whole layer. Changed ones rebuild on cache mounts, so the toolchain,
# the crate registry, `target/`, the `wasm-bindgen` CLI, and the pnpm store survive between deploys
# and an unchanged engine costs seconds, not a cold Rust build. Both need the deploy to build
# WITHOUT `--no-cache`, which also drops cache mounts (verified on Docker 29.8 / BuildKit,
# 2026-11-04: a `--no-cache` rebuild recompiled all 245 crates and reinstalled `wasm-bindgen`), so
# the deploy busts only the `builder` stage, with `DEPLOY_ID`. `RUSTUP_TOOLCHAIN` pins the channel the
# toolchain file names while skipping the components and macOS targets it also lists (the demo
# needs neither). Two cargo jobs keep the peak memory polite on the shared server.
RUN --mount=type=cache,target=/opt/rustup \
    --mount=type=cache,target=/opt/cargo/registry \
    --mount=type=cache,target=/opt/cargo-tools \
    --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/root/.cache/go-build \
    --mount=type=cache,target=/root/.local/share/pnpm/store \
    set -eu; \
    channel="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"; \
    export RUSTUP_TOOLCHAIN="$channel" CARGO_BUILD_JOBS=2; \
    rustup toolchain install "$channel" --profile minimal --target wasm32-unknown-unknown; \
    pnpm install --frozen-lockfile --filter @cmdr/desktop; \
    pnpm build:demo

# Build stage
FROM node:26-alpine@sha256:143494b1da2945f061539253adc65e4f1569ddf07da2d384c022c791a9d90a4a AS builder

WORKDIR /app

# Install pnpm, the version `.mise.toml` pins (Node 25+ no longer ships corepack)
RUN npm install --global pnpm@12.11.2

# Copy workspace config, lockfile, and root package.json (needed for overrides)
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./

# Copy the website package.json
COPY apps/website/package.json ./apps/website/

# Install dependencies for website only
RUN pnpm install --frozen-lockfile --filter "./apps/website"

# Copy website source
COPY apps/website/ ./apps/website/

# The live update manifest, over the committed copy dev and the checks build with. It's the
# `latest.json` of the release GitHub marks "Latest", which only the release pipeline's `publish` job
# flips, so every deploy serves the current release. A failed fetch fails the build, and the deploy
# keeps the old site up. See the script's header. `DEPLOY_ID` (a fresh value per deploy, from
# `infra/deploy-webhook/deploy-website.sh`) makes every deploy miss the layer cache from here on,
# so the fetch never comes from a cached layer while the `demo` stage keeps its cache.
ARG DEPLOY_ID
RUN echo "Deploy ${DEPLOY_ID:-local}" && node apps/website/scripts/fetch-live-manifest.ts

# Changelog and feature status are in repo root, needed at build time by the
# changelog page and the feature-status page / badge labels respectively
COPY CHANGELOG.md feature-status.json ./

# The browser demo, which the build copies into dist/demo/ (src/plugins/demo.ts). With
# CMDR_WEBSITE_DEMO=1 a missing demo fails the build, so a deploy never drops /demo/ silently.
COPY --from=demo /app/apps/desktop/build-demo ./apps/desktop/build-demo
ENV CMDR_WEBSITE_DEMO=1

# Build static site
WORKDIR /app/apps/website
RUN pnpm run build

# Production stage - serve with nginx
FROM nginx:alpine@sha256:df221db836e1754089190208cee7eeda94f233197056426eda74a43ab1abeac2 AS production

# Copy built static files
COPY --from=builder /app/apps/website/dist /usr/share/nginx/html

# Copy nginx config
COPY apps/website/nginx.conf /etc/nginx/conf.d/default.conf
COPY apps/website/nginx-security-headers-base.conf /etc/nginx/snippets/security-headers-base.conf
COPY apps/website/nginx-security-headers.conf /etc/nginx/snippets/security-headers.conf
COPY apps/website/nginx-demo-security-headers.conf /etc/nginx/snippets/demo-security-headers.conf
# The inline-script hashes the build computed for the CSPs (src/plugins/csp-hashes.ts, src/plugins/demo.ts)
COPY --from=builder /app/apps/website/nginx-generated/csp-script-hashes.conf /etc/nginx/snippets/csp-script-hashes.conf
COPY --from=builder /app/apps/website/nginx-generated/csp-demo-script-hashes.conf /etc/nginx/snippets/csp-demo-script-hashes.conf

# The access log dir (see nginx.conf). Production bind-mounts a host dir over it; this keeps a plain
# `docker run` writable too.
RUN mkdir -p /var/log/nginx/site && chown nginx:nginx /var/log/nginx/site

# Expose port 80
EXPOSE 80

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://127.0.0.1/ || exit 1

CMD ["nginx", "-g", "daemon off;"]
