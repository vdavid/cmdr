FROM alpine:3.21
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/public/docs /shares/private/reports \
    && echo "Public welcome file" > /shares/public/welcome.txt \
    && echo "Public documentation" > /shares/public/docs/guide.txt \
    && echo "Private credentials" > /shares/private/credentials.txt \
    && echo "Private quarterly report" > /shares/private/reports/q1.txt \
    && chmod -R 777 /shares/public /shares/private \
    && adduser -D testuser \
    && printf "testpass\ntestpass\n" | smbpasswd -a -s testuser
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
