# SMB server with artificial latency
# Useful for testing loading spinners and timeout UX
FROM alpine:3.21

RUN apk add --no-cache samba samba-client iproute2-tc

# Create share directory with sample files
RUN mkdir -p /share && \
    echo "Slow but steady" > /share/slow.txt

COPY smb.conf /etc/samba/smb.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV DELAY_MS=500

EXPOSE 445

CMD ["/entrypoint.sh"]
