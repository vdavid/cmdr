FROM alpine:3.21
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/public/Users/Administrator/Documents \
    && echo "Windows shared file" > /shares/public/shared.txt \
    && echo "Admin documents" > /shares/public/Users/Administrator/Documents/notes.txt \
    && chmod -R 777 /shares/public
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
