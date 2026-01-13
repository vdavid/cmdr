# Setting up SMB test containers on a Linux device

This guide explains how to run the SMB test containers on a Linux device (e.g., Raspberry Pi) so they are discoverable
via Bonjour/mDNS from your Mac.

## Why use a separate Linux device?

Running SMB containers on macOS has limitations:

1. **smb-rs + Samba incompatibility**: The smb-rs library uses NDR64 transfer syntax for RPC calls, which Samba doesn't
   support. This causes `list_shares` to fail.

2. **macOS Docker networking**: macOS's Docker runs in a VM, so SMB protocol negotiation sometimes breaks ("Broken pipe"
   errors with `smbutil`).

Running containers on a Linux device with macvlan networking gives each container a real LAN IP, making them
indistinguishable from real NAS devices. The `smbutil` fallback in the app works correctly over real network
connections.

## Prerequisites

- A Linux device on your LAN (Raspberry Pi, NUC, Linux VM, etc.)
- Docker and Docker Compose installed
- SSH access to the device
- Access to your router's DHCP settings (to reserve IPs)

## Setup steps

### 1. Reserve IP addresses on your router

Reserve IPs 192.168.1.200-215 for the Docker containers. This prevents DHCP conflicts.

The exact steps vary by router. Look for "DHCP Reservation" or "Static Leases" in your router's admin panel.

### 2. SSH into your Linux device

```bash
ssh pi@raspberrypi.local
# or
ssh user@192.168.1.x
```

### 3. Clone the repository

```bash
git clone https://github.com/your-org/cmdr.git
cd cmdr
```

Or sync your local changes:

```bash
rsync -avz --exclude node_modules --exclude target \
  ~/path/to/cmdr/ pi@raspberrypi.local:~/cmdr/
```

### 4. Configure network settings

Edit `test/smb-servers/docker-compose.pi.yml`:

```yaml
networks:
    smb_lan:
        driver: macvlan
        driver_opts:
            # Change to your device's network interface
            # Check with: ip link show
            parent: eth0 # or wlan0 for WiFi
        ipam:
            config:
                # Update to match your LAN
                - subnet: 192.168.1.0/24
                  gateway: 192.168.1.1
                  ip_range: 192.168.1.200/28
```

To find your network interface:

```bash
ip link show
# Look for eth0, wlan0, or similar
```

### 5. Start the containers

```bash
cd test/smb-servers
./start-pi.sh
```

This starts:

| Container    | IP            | mDNS name               | Purpose                           |
| ------------ | ------------- | ----------------------- | --------------------------------- |
| smb-guest    | 192.168.1.200 | smb-guest-test.local    | Guest access                      |
| smb-auth     | 192.168.1.201 | smb-auth-test.local     | Requires auth (testuser/testpass) |
| smb-both     | 192.168.1.202 | smb-both-test.local     | Guest + auth                      |
| smb-readonly | 192.168.1.203 | smb-readonly-test.local | Read-only share                   |

### 6. Verify from your Mac

```bash
# Check mDNS discovery
dns-sd -B _smb._tcp

# List shares
smbutil view -G -N //smb-guest-test.local

# The containers should also appear in the app's Network browser
```

## Stopping containers

```bash
./start-pi.sh stop
# or
docker compose -f docker-compose.pi.yml down
```

## Troubleshooting

### Containers not discoverable via mDNS

1. **Check Avahi is running**:

    ```bash
    docker exec smb-guest ps aux | grep avahi
    ```

2. **Check container has correct IP**:

    ```bash
    docker inspect smb-guest | grep IPAddress
    ```

3. **Check macvlan network exists**:
    ```bash
    docker network ls | grep smb_lan
    ```

### Can't connect from the Pi itself

With macvlan networking, the host can't directly communicate with containers. This is expected. Test from another device
(your Mac) instead.

### Permission denied / share not accessible

1. **Check container logs**:

    ```bash
    docker logs smb-guest
    ```

2. **Verify Samba is running**:
    ```bash
    docker exec smb-guest smbclient -L localhost -N
    ```

## Adding more containers

To add more containers, edit `docker-compose.pi.yml`:

1. Add a new service definition
2. Assign the next available IP (192.168.1.204, etc.)
3. Set a unique `MDNS_NAME` environment variable
4. Rebuild and restart

## Related documentation

- [SMB test server farm](../../testing/smb-servers.md) - Full container documentation
- [Docker server list](./test-docker-server-list.md) - Original planning document
- [Share listing](./share-listing.md) - Implementation details
