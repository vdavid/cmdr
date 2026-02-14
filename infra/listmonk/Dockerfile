ARG LISTMONK_VERSION=6.0.0

# Stage 1: Fetch default static files (public pages, assets) from the matching release.
# Our custom email templates override only the email-templates/ subdirectory.
FROM alpine:3.21 AS defaults
ARG LISTMONK_VERSION
RUN wget -qO- "https://github.com/knadh/listmonk/archive/refs/tags/v${LISTMONK_VERSION}.tar.gz" \
    | tar xz -C /tmp \
    && mv /tmp/listmonk-*/static /tmp/static

# Stage 2: Listmonk with branded email templates
FROM listmonk/listmonk:v${LISTMONK_VERSION}
COPY --from=defaults /tmp/static /listmonk/custom-static
COPY email-templates/ /listmonk/custom-static/email-templates/
