# SMB server with 50 shares
# Useful for testing share list scrolling and UI performance
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create smb.conf with 50 shares
COPY smb.conf /etc/samba/smb.conf

# Create directories for all shares
RUN mkdir -p /shares && \
    for i in $(seq 1 50); do \
    mkdir -p "/shares/share_$(printf '%02d' $i)" && \
    echo "Content in share $i" > "/shares/share_$(printf '%02d' $i)/readme.txt"; \
    done

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
