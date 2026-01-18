# SMB test server farm

This document describes the Docker-based SMB test server infrastructure for integration testing of network SMB features.

## Overview

We maintain a farm of Docker containers running various SMB server configurations. These provide deterministic,
reproducible test environments covering authentication modes, edge cases, and server behaviors.

**Two deployment modes are supported:**

1. **Local (macOS)**: Port-mapped networking (`localhost:PORT`). Requires test host injection to appear in the app.
   Limited by macOS Docker networking issues with SMB (see [Known limitations](#known-limitations)).

2. **Raspberry Pi**: Macvlan networking with real LAN IPs. Containers advertise via mDNS/Bonjour and appear
   automatically in the app's network browser. **Recommended for realistic testing.**

**Location**: `test/smb-servers/`

## Quick start (local)

```bash
# Start core containers (recommended for most development)
./test/smb-servers/start.sh

# Start minimal set (just guest + auth)
./test/smb-servers/start.sh minimal

# Start all containers (16 total)
./test/smb-servers/start.sh all

# Stop everything
./test/smb-servers/stop.sh
```

## Quick start (Raspberry Pi) – recommended

Run containers on a Pi for real network testing with Bonjour discovery:

```bash
# SSH into your Pi
ssh pi@raspberrypi.local

# Clone the repo (or sync it)
git clone <repo-url>
cd cmdr

# Edit network settings to match your LAN
vi test/smb-servers/docker-compose.pi.yml
# Update: parent (eth0 or wlan0), subnet, gateway, ip_range

# Reserve IPs 192.168.1.200-215 on your router's DHCP settings

# Start containers
./test/smb-servers/start-pi.sh

# Test from your Mac
smbutil view -G -N //smb-guest-test.local
```

The containers will appear in the app's Network browser via Bonjour as:

- `smb-guest-test.local` (192.168.1.200)
- `smb-auth-test.local` (192.168.1.201)
- `smb-both-test.local` (192.168.1.202)
- `smb-readonly-test.local` (192.168.1.203)

## Using test hosts in the app (local mode)

To see Docker SMB hosts in the app's Network section (alongside real Bonjour-discovered hosts), enable test host
injection:

```bash
# Start Docker containers first
./test/smb-servers/start.sh

# Then run the app with injection enabled
RUSTY_INJECT_TEST_SMB=1 pnpm tauri dev
```

This injects all 16 Docker hosts into the network discovery list with names like "SMB Guest (Docker)". They're
pre-resolved to `127.0.0.1` with the correct port, so they work immediately for browsing shares.

> **Note**: This only works in dev builds (`debug_assertions`). Production builds ignore this env var.

## Container list

### Core authentication scenarios

| Container   | Port | Purpose                     | Credentials                     |
| ----------- | ---- | --------------------------- | ------------------------------- |
| `smb-guest` | 9445 | Guest access only           | None required                   |
| `smb-auth`  | 9446 | Credentials required        | `testuser` / `testpass`         |
| `smb-both`  | 9447 | Guest allowed, auth extends | None or `testuser` / `testpass` |

### Edge cases and stress tests

| Container      | Port | Purpose               | Notes                            |
| -------------- | ---- | --------------------- | -------------------------------- |
| `smb-flaky`    | 9448 | 5s up / 5s down cycle | Tests connection health handling |
| `smb-50shares` | 9449 | 50 shares on one host | Tests share list UI scrolling    |
| `smb-slow`     | 9450 | 500ms+ latency        | Tests loading spinners, timeouts |
| `smb-readonly` | 9451 | Read-only share       | Tests write failure handling     |

### Protocol edge cases

| Container     | Port | Purpose          | Notes                          |
| ------------- | ---- | ---------------- | ------------------------------ |
| `smb-ancient` | 9452 | SMB1/NT1 only    | Tests legacy protocol fallback |
| `smb-signing` | 9453 | Requires signing | Tests security negotiation     |

### Name/path stress tests

| Container       | Port | Purpose             | Notes                        |
| --------------- | ---- | ------------------- | ---------------------------- |
| `smb-unicode`   | 9454 | Unicode share names | `日本語`, `émojis📁`, etc.   |
| `smb-longnames` | 9455 | 200+ char names     | Tests path truncation        |
| `smb-deepnest`  | 9456 | 50-level deep tree  | Tests navigation, breadcrumb |
| `smb-manyfiles` | 9457 | 10k+ files          | Tests listing performance    |

### Simulated server types

| Container           | Port | Purpose                    | Notes               |
| ------------------- | ---- | -------------------------- | ------------------- |
| `smb-like-windows`  | 9458 | Windows Server string      | Tests OS detection  |
| `smb-like-synology` | 9459 | Synology NAS (TimeMachine) | Tests NAS behaviors |
| `smb-like-linux`    | 9460 | Default Linux Samba        | Baseline comparison |

## Connection URLs

```bash
# Guest access (no auth)
smbclient -L localhost -p 9445 -N
smbclient //localhost/public -p 9445 -N

# Authenticated access
smbclient -L localhost -p 9446 -U testuser%testpass
smbclient //localhost/private -p 9446 -U testuser%testpass

# macOS Finder (use smb:// URLs)
open "smb://localhost:9445/public"
open "smb://testuser:testpass@localhost:9446/private"
```

## Usage contexts

### CI integration

In CI, we start the full test farm before running integration tests:

```yaml
# .github/workflows/ci.yml
- name: Start SMB test servers
  run: ./test/smb-servers/start.sh all

- name: Run integration tests
  run: cargo nextest run --features integration-tests

- name: Stop SMB test servers
  if: always()
  run: ./test/smb-servers/stop.sh
```

### Integration tests

Rust integration tests use the `integration-tests` feature flag:

```rust
#[cfg(feature = "integration-tests")]
mod integration {
    #[tokio::test]
    async fn test_guest_share_listing() {
        let shares = list_shares("localhost", 9445, None).await.unwrap();
        assert!(shares.iter().any(|s| s.name == "public"));
    }

    #[tokio::test]
    async fn test_auth_share_listing() {
        let creds = Credentials::new("testuser", "testpass");
        let shares = list_shares("localhost", 9446, Some(creds)).await.unwrap();
        assert!(shares.iter().any(|s| s.name == "private"));
    }
}
```

Run integration tests locally:

```bash
# Start servers first
./test/smb-servers/start.sh

# Run integration tests
cd src-tauri && cargo nextest run --features integration-tests && cd ..

# Stop servers when done
./test/smb-servers/stop.sh
```

### Manual testing

For manual testing during development:

```bash
# Start minimal set for quick iteration
./test/smb-servers/start.sh minimal

# Then test via CLI:
smbclient -L localhost -p 9445 -N
smbclient //localhost/public -p 9445 -N -c 'ls'
```

## Resource estimates

| Profile | Containers | RAM (idle) | RAM (active) |
| ------- | ---------- | ---------- | ------------ |
| minimal | 2          | ~100 MB    | ~150 MB      |
| core    | 6          | ~300 MB    | ~400 MB      |
| all     | 16         | ~800 MB    | ~1.2 GB      |

## Troubleshooting

### Container fails to start

```bash
# Check logs for a specific container
docker compose -f test/smb-servers/docker-compose.yml logs smb-guest

# Rebuild a specific container
docker compose -f test/smb-servers/docker-compose.yml build smb-guest
```

### Port already in use

```bash
# Check what's using the port
lsof -i :9445

# If it's an old container, clean up
docker compose -f test/smb-servers/docker-compose.yml down
docker container prune
```

### Can't connect from macOS

macOS Finder may have trouble with non-standard SMB ports. Use `smbclient` from the command line:

```bash
# List shares
smbclient -L localhost -p 9445 -N

# Mount manually (might need sudo)
mkdir -p /tmp/smb-test
mount_smbfs -o port=9445 //guest@localhost/public /tmp/smb-test
```

## Known limitations

### smb-rs and Samba RPC compatibility

The `smb-rs` crate has a known compatibility issue with Samba's DCE-RPC implementation for the `list_shares` operation.
Specifically, smb-rs uses NDR64 transfer syntax which Samba may not support for SRVSVC (Server Service) RPC calls.

**Symptoms:**

- Docker SMB containers show as "Reachable" but fail to list shares
- Error:
  `BindAck result for syntax (71710533-beba-4937-8319-b5dbef9ccc36/1) was not acceptance: ProviderRejection, reason: ProposedTransferSyntaxesNotSupported`

**Impact:**

- Docker-based Samba containers cannot be used for share listing tests
- Real NAS devices (QNAP, Synology, Windows) typically work fine as they use different RPC implementations

**Workarounds:**

1. Test against real SMB servers on your network (QNAP, Synology, Windows)
2. Use the Docker containers for connection/authentication testing only
3. Follow [smb-rs GitHub issues](https://github.com/afiffon/smb-rs/issues) for updates

This doesn't affect production use with real network devices, only Docker-based testing.

## Related

- [SMB feature documentation](../../features/network-smb/index.md)
- [Test docker server list](../../features/network-smb/test-docker-server-list.md) (original planning doc)
- [Manual test checklist](manual-checklist.md)
