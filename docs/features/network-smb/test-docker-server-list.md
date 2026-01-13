# Docker SMB test servers

This document describes the Docker-based SMB test server farm used for integration testing. These containers provide
deterministic, reproducible test environments covering various authentication modes, edge cases, and server behaviors.

## Architecture

All containers use **port-mapped networking** (`localhost:PORT`) for direct connection testing. This avoids Docker
networking complexity on macOS and provides reliable, consistent behavior across platforms.

> **Note**: Bonjour/mDNS discovery testing should use real network devices, as Docker's host networking doesn't properly
> bridge mDNS multicast on macOS.

## Container list

### Core authentication scenarios

| Container     | Port | Purpose                        | Key testing                                                |
| ------------- | ---- | ------------------------------ | ---------------------------------------------------------- |
| **smb-guest** | 9445 | Guest access only              | Anonymous browsing, no auth prompt                         |
| **smb-auth**  | 9446 | Credentials required           | Login form, Keychain save/retrieve                         |
| **smb-both**  | 9447 | Guest allowed but auth extends | "Sign in for more access" flow, auth mode detection (2.10) |

### Edge cases and stress tests

| Container        | Port | Purpose                   | Key testing                                                         |
| ---------------- | ---- | ------------------------- | ------------------------------------------------------------------- |
| **smb-flaky**    | 9448 | 5s up / 5s down cycle     | Connection health (section 8), timeout handling, host disappearance |
| **smb-50shares** | 9449 | 50 shares on one host     | Share list UI scrolling, prefetch caching, performance              |
| **smb-slow**     | 9450 | 500ms+ artificial latency | Loading spinners, timeout UX, cancel behavior                       |
| **smb-readonly** | 9451 | Read-only share           | Write failure handling, appropriate error messages                  |

### Protocol edge cases

| Container       | Port | Purpose                | Key testing                                        |
| --------------- | ---- | ---------------------- | -------------------------------------------------- |
| **smb-ancient** | 9452 | SMB1/NT1 protocol only | Legacy protocol fallback, `smbutil` fallback (2.7) |
| **smb-signing** | 9453 | Requires SMB signing   | Security negotiation                               |

### Name/path stress tests

| Container         | Port | Purpose                                   | Key testing                            |
| ----------------- | ---- | ----------------------------------------- | -------------------------------------- |
| **smb-unicode**   | 9454 | Share names: `日本語`, `émojis📁`, `Ñoño` | Encoding handling, display, mounting   |
| **smb-longnames** | 9455 | 200+ char share/path names                | Path truncation, breadcrumb overflow   |
| **smb-deepnest**  | 9456 | 50-level deep directory tree              | Navigation, breadcrumb, ".." handling  |
| **smb-manyfiles** | 9457 | 10k+ files in one share                   | Listing performance, virtual scrolling |

### Simulated server types

| Container             | Port | Purpose                            | Key testing                          |
| --------------------- | ---- | ---------------------------------- | ------------------------------------ |
| **smb-like-windows**  | 9458 | Windows-style server string        | OS detection display, icon selection |
| **smb-like-synology** | 9459 | Synology-style (TimeMachine share) | NAS-specific behaviors               |
| **smb-like-linux**    | 9460 | Default Linux Samba                | Baseline comparison                  |

## Resource estimates

| Metric           | Value                              |
| ---------------- | ---------------------------------- |
| Total containers | 16                                 |
| RAM (idle)       | ~800 MB                            |
| RAM (typical)    | ~500-700 MB (most containers idle) |
| Disk             | ~100-200 MB (shared base image)    |
| Ports            | 9445–9460                          |

## Optional containers

These can be deferred if resource-constrained:

- `smb-signing` — Niche security scenario
- `smb-like-*` trio — Mostly cosmetic differences in server identification
- `smb-deepnest` — Can test deep paths with any container

Removing these reduces count to **11 containers (~600-800 MB)**.

## Usage

**Implementation**: `test/smb-servers/` — See [SMB servers docs](../../guides/testing/smb-servers.md) for full details.

```bash
# Quick start with helper scripts
./test/smb-servers/start.sh          # Start core containers
./test/smb-servers/start.sh minimal  # Just smb-guest and smb-auth
./test/smb-servers/start.sh all      # Start all 16 containers
./test/smb-servers/stop.sh           # Stop all

# Or use docker compose directly
docker compose -f test/smb-servers/docker-compose.yml up -d
docker compose -f test/smb-servers/docker-compose.yml down
docker compose -f test/smb-servers/docker-compose.yml logs -f smb-flaky
```

## Connection URLs

For direct testing (bypassing Bonjour discovery):

```
smb://localhost:9445/public   # smb-guest
smb://localhost:9446/private  # smb-auth (user: testuser, pass: testpass)
smb://localhost:9447/mixed    # smb-both
# ... etc
```

## Related

- [Task list](./task-list.md) — Implementation tasks referencing these containers
- [Share listing](./share-listing.md) — How share enumeration works
- [Authentication](./authentication.md) — Auth flow details
