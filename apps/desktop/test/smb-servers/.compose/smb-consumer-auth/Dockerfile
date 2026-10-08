FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/private/reports \
    && echo "Confidential document" > /shares/private/secret.txt \
    && echo "Private project notes" > /shares/private/project.txt \
    && echo "Quarterly results" > /shares/private/reports/q1.txt \
    && chmod -R 777 /shares/private \
    && adduser -D testuser \
    && printf "testpass\ntestpass\n" | smbpasswd -a -s testuser
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
