# SMB server simulating a Synology NAS
# Includes TimeMachine share and typical Synology share structure
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create Synology-like share structure
RUN mkdir -p /shares/TimeMachine && \
    mkdir -p /shares/homes && \
    mkdir -p /shares/photo && \
    mkdir -p /shares/video && \
    mkdir -p /shares/music && \
    echo "TimeMachine backup target" > /shares/TimeMachine/readme.txt && \
    echo "User homes" > /shares/homes/readme.txt && \
    echo "Photos here" > /shares/photo/readme.txt && \
    echo "Videos here" > /shares/video/readme.txt && \
    echo "Music here" > /shares/music/readme.txt

COPY smb.conf /etc/samba/smb.conf

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
