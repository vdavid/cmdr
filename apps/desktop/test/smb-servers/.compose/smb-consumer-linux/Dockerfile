FROM alpine:3.24@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6
RUN apk add --no-cache samba
COPY smb.conf /etc/samba/smb.conf
RUN mkdir -p /shares/public/projects \
    && echo "Linux server shared file" > /shares/public/welcome.txt \
    && echo "Project readme" > /shares/public/projects/readme.txt \
    && echo "Build output" > /shares/public/projects/build.log \
    && chmod -R 777 /shares/public
EXPOSE 445
HEALTHCHECK --interval=2s --timeout=3s --retries=10 \
    CMD nc -z localhost 445
CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
