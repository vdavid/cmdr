# SMB server with 50-level deep directory tree
# Useful for testing navigation, breadcrumb, and ".." handling
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create 50-level deep directory structure
RUN mkdir -p /share && \
    current="/share" && \
    for i in $(seq 1 50); do \
    current="${current}/level_$(printf '%02d' $i)"; \
    mkdir -p "$current"; \
    echo "You are at depth $i" > "$current/depth_$i.txt"; \
    done && \
    echo "You reached the bottom!" > "$current/congratulations.txt"

COPY smb.conf /etc/samba/smb.conf

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
