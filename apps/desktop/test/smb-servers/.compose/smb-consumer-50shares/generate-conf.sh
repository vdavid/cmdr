#!/bin/sh
# Generate smb.conf with 50 shares, each containing a sample file.
cat > /etc/samba/smb.conf <<'HEADER'
[global]
server role = standalone server
server min protocol = SMB2_02
server max protocol = SMB3_11
map to guest = Bad User
log level = 1
HEADER

for i in $(seq 1 50); do
    name="share_$(printf '%02d' "$i")"
    dir="/shares/$name"
    mkdir -p "$dir"
    echo "Hello from $name" > "$dir/sample.txt"
    chmod 777 "$dir"
    cat >> /etc/samba/smb.conf <<SHARE

[$name]
path = $dir
read only = no
guest ok = yes
browseable = yes
SHARE
done
