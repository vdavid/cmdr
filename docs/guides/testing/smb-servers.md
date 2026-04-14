# SMB test server farm

Docker-based SMB test servers for integration testing of network SMB features. Containers are provided by smb2's
consumer test harness — Cmdr doesn't maintain its own Dockerfiles.

## Overview

On first run, `start.sh` extracts Docker Compose files from smb2 (via
`cargo run --example smb_compose --features smb-e2e`) into `test/smb-servers/.compose/`, then starts containers.
Subsequent runs skip the extraction.

**Location**: `test/smb-servers/`

## Quick start

```bash
# Start core containers (guest, auth, both, readonly, flaky, slow)
./test/smb-servers/start.sh

# Start minimal set (just guest + auth)
./test/smb-servers/start.sh minimal

# Start all containers (14 total)
./test/smb-servers/start.sh all

# Stop everything
./test/smb-servers/stop.sh
```

## Container list

### Core authentication scenarios

| Container            | Port  | Purpose                     | Credentials                     |
| -------------------- | ----- | --------------------------- | ------------------------------- |
| `smb-consumer-guest` | 10480 | Guest access only           | None required                   |
| `smb-consumer-auth`  | 10481 | Credentials required        | `testuser` / `testpass`         |
| `smb-consumer-both`  | 10482 | Guest allowed, auth extends | None or `testuser` / `testpass` |

### Edge cases and stress tests

| Container               | Port  | Purpose               | Notes                            |
| ----------------------- | ----- | --------------------- | -------------------------------- |
| `smb-consumer-flaky`    | 10492 | 5s up / 5s down cycle | Tests connection health handling |
| `smb-consumer-50shares` | 10483 | 50 shares on one host | Tests share list UI scrolling    |
| `smb-consumer-slow`     | 10493 | 200ms+ latency        | Tests loading spinners, timeouts |
| `smb-consumer-readonly` | 10488 | Read-only share       | Tests write failure handling     |

### Name/path stress tests

| Container                | Port  | Purpose             | Notes                        |
| ------------------------ | ----- | ------------------- | ---------------------------- |
| `smb-consumer-unicode`   | 10484 | Unicode share names | CJK, emoji, accented chars   |
| `smb-consumer-longnames` | 10485 | 200+ char names     | Tests path truncation        |
| `smb-consumer-deepnest`  | 10486 | 50-level deep tree  | Tests navigation, breadcrumb |
| `smb-consumer-manyfiles` | 10487 | 10k+ files          | Tests listing performance    |

### Simulated server types

| Container               | Port  | Purpose               | Notes               |
| ----------------------- | ----- | --------------------- | ------------------- |
| `smb-consumer-windows`  | 10489 | Windows Server string | Tests OS detection  |
| `smb-consumer-synology` | 10490 | Synology NAS mimicry  | Tests NAS behaviors |
| `smb-consumer-linux`    | 10491 | Default Linux Samba   | Baseline comparison |

Ports are configurable via `SMB_CONSUMER_*_PORT` environment variables (for example, `SMB_CONSUMER_GUEST_PORT=9445`).

## Connection URLs

```bash
# Guest access (no auth)
smbclient -L localhost -p 10480 -N
smbclient //localhost/public -p 10480 -N

# Authenticated access
smbclient -L localhost -p 10481 -U testuser%testpass
smbclient //localhost/private -p 10481 -U testuser%testpass
```

## E2E testing

The `smb-e2e` Cargo feature injects all 14 containers as virtual hosts in the Network sidebar via
`virtual_smb_hosts.rs`. Ports come from `smb2::testing::*_port()` functions.

For Linux Docker E2E (`e2e-linux.sh`), containers are on the `smb-consumer_default` Docker network. The E2E container
joins this network and accesses containers by name on port 445 (no host port mapping needed).

See `test/e2e-playwright/smb.spec.ts` for the E2E test suite.

## Manual QA testing

Start all containers and run the app with `smb-e2e` enabled:

```bash
./test/smb-servers/start.sh all
cd apps/desktop && node scripts/tauri-wrapper.js dev -- --features smb-e2e
```

All 14 virtual SMB hosts appear in the Network sidebar. Click them to test share listing, mounting, file browsing, and
edge cases (unicode names, deep trees, 50 shares, etc.) against real Samba servers.

## Resource estimates

| Profile | Containers | RAM (idle) | RAM (active) |
| ------- | ---------- | ---------- | ------------ |
| minimal | 2          | ~60 MB     | ~100 MB      |
| core    | 6          | ~200 MB    | ~300 MB      |
| all     | 14         | ~400 MB    | ~600 MB      |

## Troubleshooting

### Container fails to start

```bash
# Check logs for a specific container
docker compose -p smb-consumer logs smb-consumer-guest

# Re-extract compose files (deletes .compose/ and re-runs extraction)
rm -rf test/smb-servers/.compose && ./test/smb-servers/start.sh
```

### Port already in use

```bash
# Check what's using the port
lsof -i :10480

# Clean up old containers
./test/smb-servers/stop.sh
docker container prune
```

### Using `connect_to_server` MCP tool

Docker SMB containers don't advertise via mDNS. With the `smb-e2e` feature, virtual hosts are injected automatically.
Without it, use the `connect_to_server` MCP tool:

```bash
curl -s http://localhost:9224/mcp -d '{
  "jsonrpc": "2.0", "id": 1, "method": "tools/call",
  "params": { "name": "connect_to_server", "arguments": { "address": "localhost:10480" } }
}'
```
