# Task list

All implementation tasks for network SMB support, organized by area.

See [index.md](./index.md) for an overview of this whole feature. It's a helpful starting point.

## Legend

- ⬜ Not started
- 🔄 In progress
- ✅ Complete
- 🔬 Spike/research needed
- ❌ Not needed

---

## 1. Host discovery

See [host-discovery.md](./host-discovery.md) for details.

### Backend (Rust)

- ✅ **1.1** Create `network` module in `src-tauri/src/`
- ✅ **1.2** Implement `NSNetServiceBrowserDelegate` using `objc2` `define_class!`
- ✅ **1.3** Create `BonjourDiscovery` struct that manages the browser lifecycle
- ✅ **1.4** Implement service resolution (lazy hostname generation + DNS lookup)
- ✅ **1.5** Create Tauri commands: `list_network_hosts`, `get_network_discovery_state`, `resolve_host`
- ✅ **1.6** Add event emission for real-time host updates to frontend
- ✅ **1.7** Start discovery at app initialization (in `lib.rs`)
- ✅ **1.8** Add unit tests for hostname conversion and serialization

### Frontend (Svelte)

- ✅ **1.9** Add "Network" section to volume selector
- ✅ **1.10** Subscribe to host discovery events from backend
- ✅ **1.11** Display discovered hosts with appropriate icons
- ✅ **1.12** Show "Searching..." indicator during initial discovery
- ✅ **1.13** Add frontend tests for network host display

---

## 2. Share listing

See [share-listing.md](./share-listing.md) for details. Decision: [ADR 013](../../adr/013-smb-rs-share-listing.md).

### Validation spike

- 🔄 **2.0** Validate `smb-rs` against real servers — [See results](./spike-2.0-results.md)
    - Test against macOS file sharing
    - Test against Synology/QNAP NAS
    - Test against Windows share
    - Test against Linux Samba
    - Test guest access and authenticated access
    - Measure latency
    - **Outcome**: smb-rs works with commercial NAS; has RPC compatibility issues with Samba. Implemented `smbutil`
      fallback for macOS.

### Backend (Rust)

- ✅ **2.1** Add `smb` crate to Cargo.toml dependencies
- ✅ **2.2** Create `smb_client` module in `src-tauri/src/network/`
- ✅ **2.3** Implement `list_shares` async function using smb-rs
- ✅ **2.4** Filter to show only disk shares (hide IPC$, printers, admin shares)
- ✅ **2.5** Create Tauri command: `list_shares_on_host`
- ✅ **2.6** Handle guest vs. authenticated enumeration
- ✅ **2.7** Implement `smbutil` fallback for edge cases
- ✅ **2.8** Add timeout handling (10–15 second limit)
- ❌ **2.9** Implement connection pool (60 sec TTL, max 20 connections)
- ✅ **2.10** Implement auth mode detection (try guest, detect `GuestAllowed` vs `CredsRequired`)
- ✅ **2.11** Add unit tests with mocked SMB responses

### Prefetching (Backend)

- ✅ **2.12** Prefetch shares for known hosts after discovery settles (parallel, cap 10)
- ✅ **2.13** Cache prefetched results for instant display

### Frontend (Svelte)

- ✅ **2.14** Display shares when user enters a network host
- ✅ **2.15** Show loading state with cancel option while enumerating
- ✅ **2.16** Implement brief caching (~30 seconds) for share lists
- ✅ **2.17** Handle errors (host unreachable, timeout, auth required)
- ✅ **2.18** Prefetch on hover (500 ms debounce)
- ✅ **2.19** Add frontend tests

---

## 3. Mounting

See [mounting.md](./mounting.md) for details.

### Backend (Rust)

- ✅ **3.1** Add NetFS.framework linking to Cargo.toml / build.rs
- ✅ **3.2** Create Rust bindings for `NetFSMountURLAsync`
- ✅ **3.3** Implement `mount_smb_share` async function
- ✅ **3.4** Create Tauri command: `mount_network_share`
- ✅ **3.5** Handle mount errors and map to user-friendly messages
- ✅ **3.6** Detect already-mounted shares (don't re-mount)
- ✅ **3.7** Add unit tests with mocked NetFS

### Frontend (Svelte)

- ✅ **3.8** Call mount command when user selects a share
- ✅ **3.9** Show mounting progress/spinner
- ✅ **3.10** Navigate to mounted path on success
- ✅ **3.11** Display error messages on failure
- ✅ **3.12** Add frontend tests

---

## 4. Authentication

See [authentication.md](./authentication.md) for details.

### Backend (Rust)

- ✅ **4.1** Add `security-framework` crate to dependencies
- ✅ **4.2** Implement `save_credentials_to_keychain` function
- ✅ **4.3** Implement `get_credentials_from_keychain` function
- ✅ **4.4** Implement auth options detection (guest/creds/both)
- ✅ **4.5** Create Tauri commands: `check_auth_required`, `save_smb_credentials`, `get_smb_credentials`
- ✅ **4.6** Add unit tests with mocked Keychain

### Frontend (Svelte)

- ✅ **4.7** Create `NetworkLoginForm.svelte` component
- ✅ **4.8** Integrate login form into `FilePane.svelte` (replaces file list when auth needed)
- ✅ **4.9** Implement guest vs. credentials toggle (when both available)
- ✅ **4.10** Pre-fill username from known shares store
- ✅ **4.11** Handle "Remember in Keychain" checkbox
- ✅ **4.12** Show contextual messages when auth options changed
- ✅ **4.13** Handle auth errors with re-prompt
- ✅ **4.14** Add frontend tests for all auth scenarios

---

## 5. Known shares store

See [known-shares-store.md](./known-shares-store.md) for details.

### Backend (Rust)

- ✅ **5.1** Add `KnownNetworkShare` struct to settings types
- ✅ **5.2** Add `known_network_shares` field to settings store
- ✅ **5.3** Implement `update_known_share` function
- ✅ **5.4** Implement `get_known_share` function
- ✅ **5.5** Create Tauri commands: `get_known_shares`, `update_known_share`
- ✅ **5.6** Add unit tests

### Frontend (Svelte)

- ✅ **5.7** Read known shares for username pre-fill (implemented in `NetworkLoginForm.svelte`)
- ✅ **5.8** Update known shares after successful connection (implemented in `ShareBrowser.svelte`)
- ✅ **5.9** Compare current auth options with stored to detect changes (implemented in `NetworkLoginForm.svelte`)
- ✅ **5.10** Add frontend tests (type and logic tests added)

---

## 6. Pre-mounted shares

Pre-mounted SMB shares (e.g., mounted via Finder) appear automatically in the volume selector because the existing
volume listing code at `/Volumes/*` picks them up. The macOS APIs return the correct network share icon.

### Backend (Rust)

- ✅ **6.1** Detect network mounts in existing volume listing code (uses `/Volumes/*` enumeration)
- ✅ **6.2** Categorize as `AttachedVolume` (works correctly, dedicated category not needed)
- ✅ **6.3** Add appropriate icon for network shares (uses `get_icon_for_path` which returns macOS system icon)
- ✅ **6.4** Unit tests (covered by existing volume listing tests)

### Frontend (Svelte)

- ✅ **6.5** Display pre-mounted network shares in volume selector (works automatically)
- ✅ **6.6** Frontend tests (covered by existing tests)

---

## 7. Integration & polish

- ⬜ **7.1** End-to-end manual testing with real SMB servers
- ⬜ **7.2** Performance testing with slow networks
- ⬜ **7.3** Error message review and polish
- ⬜ **7.4** Documentation review
- ⬜ **7.5** Accessibility review for login form

---

## 8. Connection health monitoring

Monitor mounted share health and handle disconnections gracefully.

### Backend (Rust)

- ⬜ **8.1** Implement `check_mount_health` function (lightweight `stat()` on mount root)
- ⬜ **8.2** Create health check scheduler with debouncing
- ⬜ **8.3** Emit events when mount becomes unreachable
- ⬜ **8.4** Add Tauri command: `check_network_mount_health`

### Frontend (Svelte)

- ⬜ **8.5** Trigger health check on window focus/blur
- ⬜ **8.6** Trigger health check on user keypresses/clicks (2 sec debounce)
- ⬜ **8.7** Implement 30-second periodic health check (debounced with other checks)
- ⬜ **8.8** Handle unreachable mount gracefully (show reconnection UI)

---

## Testing strategy

### Unit tests (fast, local)

All unit tests use mocks—no network or Docker needed. These run on every save/build.

### Integration tests (Docker SMB server)

For high-fidelity testing, we spin up a farm of Docker SMB test servers. See
[test-docker-server-list.md](./test-docker-server-list.md) for the container list and rationale, and
[SMB servers docs](../../testing/smb-servers.md) for setup and usage.

**Two deployment modes:**

1. **Local (macOS)**: Port-mapped containers at `localhost:PORT`. Limited due to Docker networking issues. Use
   `RUSTY_INJECT_TEST_SMB=1` to inject hosts into the app.

2. **Raspberry Pi (recommended)**: Macvlan networking with real LAN IPs. Containers advertise via mDNS/Bonjour and
   appear automatically in the app. See
   [Setting up SMB test containers on Linux](../../testing/setting-up-smb-test-containers-on-linux.md).

**Implementation**: `test/smb-servers/`

- Containers start once per test suite (not per test)
- Tests run against real SMB protocol
- Run via `--features integration-tests` flag
- In CI: Always run. Locally: Optional (developer can skip for faster iteration)

Quick start (local):

```bash
./test/smb-servers/start.sh          # Core containers
./test/smb-servers/start.sh minimal  # Just guest + auth
./test/smb-servers/start.sh all      # All 17 containers
```

Quick start (Raspberry Pi):

```bash
ssh pi@raspberrypi.local
cd cmdr/test/smb-servers
./start-pi.sh
```

### Manual testing checklist

_(To be expanded during implementation)_

- [ ] Discover hosts on network with File Sharing enabled
- [ ] Navigate into a discovered host and see shares
- [ ] Mount a guest-accessible share
- [ ] Mount a credentials-required share
- [ ] Test "Sign in for more access" flow
- [ ] Test timeout behavior (disconnect server during mount)
- [ ] Test host disappearance during browsing
- [ ] Test already-mounted share detection
- [ ] Test with slow network (throttle connection)
- [ ] Test with various server types (macOS, NAS, Windows, Linux)

---

## Dependencies between areas

```
1. Host discovery ──┐
                    ├──→ 2. Share listing ──→ 3. Mounting
                    │           │                  │
5. Known shares ────┼───────────┴──────────────────┤
                    │                              │
4. Authentication ──┴──────────────────────────────┘

6. Pre-mounted shares: Independent, can be done anytime
8. Connection health: After mounting works
```

Recommended implementation order:

1. **6. Pre-mounted shares** — Quick win, minimal work
2. **1. Host discovery** — Foundation for everything else
3. **2.0 Validation spike** — Confirm smb-rs works with real servers
4. **5. Known shares store** — Simple data structure
5. **2. Share listing** — Core enumeration with smb-rs
6. **3. Mounting** — Core functionality
7. **4. Authentication** — Auth flows and Keychain integration
8. **8. Connection health** — Monitoring after mount works
9. **7. Integration** — Final polish and testing
