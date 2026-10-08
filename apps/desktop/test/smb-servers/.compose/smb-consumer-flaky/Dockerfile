FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
COPY cycle.sh /cycle.sh
RUN chmod +x /cycle.sh \
    && mkdir -p /shares/public \
    && echo "Flaky server sample file" > /shares/public/sample.txt \
    && chmod -R 777 /shares/public
EXPOSE 445
# No healthcheck — the whole point is the server goes up and down.
CMD ["/cycle.sh"]
