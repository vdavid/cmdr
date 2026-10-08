FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/shared/documents /shares/timemachine/backups \
    && echo "Shared NAS file" > /shares/shared/readme.txt \
    && echo "Meeting notes" > /shares/shared/documents/notes.txt \
    && echo "TimeMachine backup marker" > /shares/timemachine/backups/backup_2024.txt \
    && chmod -R 777 /shares/shared /shares/timemachine
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
