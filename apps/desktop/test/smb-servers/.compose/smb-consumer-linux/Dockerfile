FROM alpine:3.21
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
