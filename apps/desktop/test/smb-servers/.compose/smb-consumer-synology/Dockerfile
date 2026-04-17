FROM alpine:3.21
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
