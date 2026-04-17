FROM alpine:3.21
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/public/docs \
    && echo "Hello from smb2 test server" > /shares/public/welcome.txt \
    && echo "Sample document content" > /shares/public/readme.txt \
    && echo "Test data for integration" > /shares/public/notes.txt \
    && echo "Nested file example" > /shares/public/docs/report.txt \
    && echo "Another nested file" > /shares/public/docs/summary.txt \
    && chmod -R 777 /shares/public
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
