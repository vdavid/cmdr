# Flaky SMB server that cycles on/off
# Useful for testing connection health and timeout handling
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create share directory with sample files
RUN mkdir -p /share && \
    echo "Flaky content - catch me if you can!" > /share/flaky.txt

COPY smb.conf /etc/samba/smb.conf
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV UP_SECONDS=5
ENV DOWN_SECONDS=5

EXPOSE 445

CMD ["/entrypoint.sh"]
