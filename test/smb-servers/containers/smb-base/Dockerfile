# Base SMB server using Samba with mDNS/Bonjour advertisement
# Configurable via build args for different authentication modes
FROM alpine:3.21

# Install Samba + Avahi for mDNS advertisement
RUN apk add --no-cache samba samba-client avahi dbus

# Build arguments for customization
ARG GUEST_OK=no
ARG CREATE_USER=""
ARG READ_ONLY=no
ARG SERVER_SIGNING=disabled
ARG SERVER_STRING="Samba Test Server"

# Create share directory with sample files
RUN mkdir -p /share && \
    echo "This is a test file" > /share/test.txt && \
    echo "Hello from SMB" > /share/hello.txt && \
    mkdir -p /share/subfolder && \
    echo "Nested content" > /share/subfolder/nested.txt

# Create directories for dbus and avahi
RUN mkdir -p /run/dbus /var/run/avahi-daemon

# Copy template, avahi service, and entrypoint
COPY smb.conf.template /etc/samba/smb.conf.template
COPY smb.service /etc/avahi/services/smb.service
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Store build args as env vars for entrypoint to use
ENV GUEST_OK=${GUEST_OK}
ENV CREATE_USER=${CREATE_USER}
ENV READ_ONLY=${READ_ONLY}
ENV SERVER_SIGNING=${SERVER_SIGNING}
ENV SERVER_STRING=${SERVER_STRING}

# SMB port + mDNS multicast
EXPOSE 445 5353/udp

CMD ["/entrypoint.sh"]
