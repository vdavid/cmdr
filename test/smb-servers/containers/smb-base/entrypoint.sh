#!/bin/sh
set -e

# Default values
GUEST_OK="${GUEST_OK:-no}"
READ_ONLY="${READ_ONLY:-no}"
SERVER_SIGNING="${SERVER_SIGNING:-auto}"
SERVER_STRING="${SERVER_STRING:-Samba Test Server}"
SMB_SHARE_NAME="${SMB_SHARE_NAME:-share}"
MDNS_NAME="${MDNS_NAME:-}"

# Generate smb.conf from template
sed -e "s/__SERVER_STRING__/${SERVER_STRING}/" \
    -e "s/__SERVER_SIGNING__/${SERVER_SIGNING}/" \
    -e "s/__SHARE_NAME__/${SMB_SHARE_NAME}/" \
    -e "s/__READ_ONLY__/${READ_ONLY}/" \
    -e "s/__GUEST_OK__/${GUEST_OK}/" \
    /etc/samba/smb.conf.template > /etc/samba/smb.conf

# Create user if specified (format: username:password)
if [ -n "${CREATE_USER}" ]; then
    username=$(echo "${CREATE_USER}" | cut -d: -f1)
    password=$(echo "${CREATE_USER}" | cut -d: -f2)
    if ! id "$username" >/dev/null 2>&1; then
        adduser -D -H "$username"
        echo -e "${password}\n${password}" | smbpasswd -a -s "$username"
    fi
fi

# Create user from separate environment variables (runtime override)
if [ -n "$SMB_USER" ] && [ -n "$SMB_PASS" ]; then
    if ! id "$SMB_USER" >/dev/null 2>&1; then
        adduser -D -H "$SMB_USER"
        echo -e "$SMB_PASS\n$SMB_PASS" | smbpasswd -a -s "$SMB_USER"
    fi
fi

# Ensure share directory exists and has content
mkdir -p /share
if [ ! -f /share/test.txt ]; then
    echo "This is a test file" > /share/test.txt
    echo "Hello from SMB" > /share/hello.txt
    mkdir -p /share/subfolder
    echo "Nested content" > /share/subfolder/nested.txt
fi

# Set hostname for mDNS if specified
if [ -n "$MDNS_NAME" ]; then
    echo "$MDNS_NAME" > /etc/hostname
    hostname "$MDNS_NAME"
    # Update avahi hostname
    sed -i "s/^#host-name=.*/host-name=$MDNS_NAME/" /etc/avahi/avahi-daemon.conf 2>/dev/null || true
fi

# Start dbus (required by avahi)
echo "Starting dbus..."
mkdir -p /run/dbus
rm -f /run/dbus/pid
dbus-daemon --system --fork

# Start avahi-daemon for mDNS advertisement
echo "Starting avahi-daemon..."
avahi-daemon --daemonize --no-chroot

# Give avahi a moment to start
sleep 1

echo "Starting Samba..."
exec smbd --foreground --no-process-group --debug-stdout

