# SMB1/NT1 protocol only server
# Useful for testing legacy protocol fallback
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create share directory with sample files
RUN mkdir -p /share && \
    echo "Legacy content (SMB1 only)" > /share/ancient.txt

COPY smb.conf /etc/samba/smb.conf

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
