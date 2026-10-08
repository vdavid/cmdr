# Build stage
FROM node:24-alpine@sha256:ebfe2f90462722a7a4de65e91990e97fe0d401c70e0e762c5b53302f905ec1c1 AS builder

WORKDIR /app

# Install pnpm
RUN corepack enable && corepack prepare pnpm@10.30.0 --activate

# Copy workspace config, lockfile, and root package.json (needed for overrides)
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./

# Copy the website package.json
COPY apps/website/package.json ./apps/website/

# Install dependencies for website only
RUN pnpm install --frozen-lockfile --filter "./apps/website"

# Copy website source
COPY apps/website/ ./apps/website/

# Changelog and feature status are in repo root, needed at build time by the
# changelog page and the feature-status page / badge labels respectively
COPY CHANGELOG.md feature-status.json ./

# Build static site
WORKDIR /app/apps/website
RUN pnpm run build

# Production stage - serve with nginx
FROM nginx:alpine@sha256:df221db836e1754089190208cee7eeda94f233197056426eda74a43ab1abeac2 AS production

# Copy built static files
COPY --from=builder /app/apps/website/dist /usr/share/nginx/html

# Copy nginx config
COPY apps/website/nginx.conf /etc/nginx/conf.d/default.conf
COPY apps/website/nginx-security-headers.conf /etc/nginx/snippets/security-headers.conf

# The access log dir (see nginx.conf). Production bind-mounts a host dir over it; this keeps a plain
# `docker run` writable too.
RUN mkdir -p /var/log/nginx/site && chown nginx:nginx /var/log/nginx/site

# Expose port 80
EXPOSE 80

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://127.0.0.1/ || exit 1

CMD ["nginx", "-g", "daemon off;"]
