FROM alpine:3.24@sha256:be2656061b536c1883c4034790e21d0a09c5aaaf51968b89368a3c0371ea2c9a
RUN apk add --no-cache samba iproute2
COPY smb.conf /etc/samba/smb.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh \
    && mkdir -p /shares/public \
    && echo "Slow server sample file" > /shares/public/sample.txt \
    && echo "Another file for latency testing" > /shares/public/data.txt \
    && chmod -R 777 /shares/public
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=5s --retries=15 \
    CMD nc -z localhost 445
CMD ["/entrypoint.sh"]
