# The browser demo bundle, pinned by digest (the builder stage below says where it comes from). A
# global ARG, because `COPY --from` can't expand one. The placeholder default names no image, so a
# build that forgets the digest fails on the pull.
ARG DEMO_IMAGE_DIGEST=0000000000000000000000000000000000000000000000000000000000000000
FROM ghcr.io/getcmdr/cmdr-demo@sha256:${DEMO_IMAGE_DIGEST} AS demo-bundle

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
# keeps the old site up. See the script's header.
RUN node apps/website/scripts/fetch-live-manifest.ts

# Changelog and feature status are in repo root, needed at build time by the
# changelog page and the feature-status page / badge labels respectively
COPY CHANGELOG.md feature-status.json ./

# The browser demo bundle, which the build copies into dist/demo/ (src/plugins/demo.ts). CI builds
# it (Rust to WASM, then the desktop frontend) and publishes it as an image, so this box never
# compiles Rust. The deploy passes `DEMO_IMAGE_DIGEST`, the digest CI pushed and signed into the
# webhook payload (`infra/deploy-webhook/deploy-website.sh`); a local build passes a dir instead, with
# `--build-context ghcr.io/getcmdr/cmdr-demo=apps/desktop/build-demo`.
# `CMDR_WEBSITE_DEMO=1` makes an empty bundle fail too, so a deploy never drops /demo/ silently.
# `apps/website/DETAILS.md` § Browser demo.
COPY --from=demo-bundle / ./apps/desktop/build-demo/
ARG CMDR_WEBSITE_DEMO=1
ENV CMDR_WEBSITE_DEMO=${CMDR_WEBSITE_DEMO}

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
COPY apps/website/nginx-demo-precompressed.conf /etc/nginx/snippets/demo-precompressed.conf
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
