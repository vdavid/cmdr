# SMB server with Unicode share names
# Useful for testing encoding handling, display, and mounting
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create share directories with Unicode names
RUN mkdir -p "/shares/日本語" && \
    echo "Japanese content" > "/shares/日本語/日本語.txt" && \
    mkdir -p "/shares/émojis📁" && \
    echo "Emoji content" > "/shares/émojis📁/🎉.txt" && \
    mkdir -p "/shares/Ñoño" && \
    echo "Spanish content" > "/shares/Ñoño/español.txt" && \
    mkdir -p "/shares/中文" && \
    echo "Chinese content" > "/shares/中文/中文.txt" && \
    mkdir -p "/shares/Ελληνικά" && \
    echo "Greek content" > "/shares/Ελληνικά/ελληνικά.txt"

COPY smb.conf /etc/samba/smb.conf

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
