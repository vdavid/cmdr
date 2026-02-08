# mDNS discovery migration: NSNetServiceBrowser to mdns-sd

## Context

Cmdr's network host discovery (`bonjour.rs`, 532 lines) wraps Apple's deprecated `NSNetServiceBrowser`/`NSNetService`
APIs via `objc2` bindings. These APIs require main-thread execution, manual unsafe sockaddr parsing, ObjC delegate
lifecycle management, and are macOS-only. Apple has deprecated them in favor of `Network.framework`.

The `mdns-sd` crate (v0.17.2, pure Rust, Apache-2.0/MIT, 532 dependent crates, last commit Feb 7 2026) provides
cross-platform mDNS/DNS-SD with a clean channel-based API. Switching eliminates the deprecation risk, removes all unsafe
network code, drops the main-thread requirement, and opens the door to Linux/Windows support later.

**Scope**: Backend Rust only. No frontend changes — same 4 Tauri events, same commands, same data types.

## Files to modify

| File                                                   | Change                                                     |
|--------------------------------------------------------|------------------------------------------------------------|
| `apps/desktop/src-tauri/Cargo.toml`                    | Add `mdns-sd`, remove `NSNetServices`+`NSRunLoop` features |
| `apps/desktop/src-tauri/src/network/bonjour.rs`        | Delete (532 lines)                                         |
| `apps/desktop/src-tauri/src/network/mdns_discovery.rs` | Create (~170 lines)                                        |
| `apps/desktop/src-tauri/src/network/mod.rs`            | Change module decl + re-export (2 lines)                   |
| `apps/desktop/src-tauri/src/commands/settings.rs`      | Update import path (1 line)                                |
| `apps/desktop/src-tauri/src/lib.rs`                    | Add `stop_discovery()` to shutdown path (2 lines)          |

## Implementation

### Milestone 1: Add mdns-sd and create new module

**Cargo.toml** — add `mdns-sd` to macOS deps, clean up objc2-foundation features:

```toml
# Add to [target.'cfg(target_os = "macos")'.dependencies]:
mdns-sd = { version = "0.17", features = ["logging"] }

# Remove "NSNetServices" and "NSRunLoop" from objc2-foundation features
# (still needed: NSURL, NSString, NSDictionary, NSDate, NSArray, NSValue, NSError, NSFileManager
#  — used by volumes/mod.rs, file_system/sync_status.rs, file_system/macos_metadata.rs)
```

Run `cargo deny check licenses` to confirm compatibility.

**Create `mdns_discovery.rs`** with this structure:

- Constants: `SMB_SERVICE_TYPE = "_smb._tcp.local."` (mdns-sd needs the full dotted form), `SMB_DEFAULT_PORT = 445`,
  `DEFAULT_RESOLVE_TIMEOUT_MS = 5000`
- Global state: `DISCOVERY_MANAGER: OnceLock<Mutex<Option<MdnsDiscoveryManager>>>` holding `ServiceDaemon`, plus
  `APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>>`, `RESOLVE_TIMEOUT_MS: AtomicU64`
- No `unsafe impl Send`, no `MainThreadMarker`, no delegate classes

**Public API** (same signatures as `bonjour.rs`):

```rust
pub fn start_discovery(app_handle: AppHandle)
pub fn stop_discovery()
pub fn update_resolve_timeout(ms: u64)
```

**`start_discovery`**: Create `ServiceDaemon::new()`, call `daemon.browse(SMB_SERVICE_TYPE)` to get a
`Receiver<ServiceEvent>`, spawn a named thread (`"mdns-event-loop"`) to process events via `process_events(receiver)`.
Store daemon in global state.

**`process_events` loop** — maps mdns-sd events to existing `mod.rs` callbacks:

| mdns-sd event                 | Action                                                                                                                 |
|-------------------------------|------------------------------------------------------------------------------------------------------------------------|
| `SearchStarted(_)`            | `on_discovery_state_changed(Searching)`                                                                                |
| `ServiceFound(_, fullname)`   | Extract instance name → `on_host_found(host)` with unresolved host                                                     |
| `ServiceResolved(info)`       | Extract hostname, preferred IP, port → `on_host_resolved(...)`. On first resolve: `on_discovery_state_changed(Active)` |
| `ServiceRemoved(_, fullname)` | Extract instance name → `on_host_lost(id)`                                                                             |
| `SearchStopped(_)`            | `on_discovery_state_changed(Idle)`                                                                                     |

**Helper functions**:

- `extract_instance_name(fullname: &str) -> String` — extracts "David's MacBook" from "David's MacBook._smb._tcp.local."
  by splitting at first `"._"`
- `extract_preferred_ip(addresses) -> Option<String>` — iterates the address set, prefers IPv4 over IPv6. Replaces 65
  lines of unsafe sockaddr parsing with ~10 lines of safe code.

**"Active" state heuristic**: The old code used NSNetServiceBrowser's `moreComing` flag to detect initial scan
completion. mdns-sd doesn't have this concept. We transition to `Active` on the first `ServiceResolved` event — the user
sees "Searching" briefly, then "Active" once we have a resolved host. This is a reasonable approximation.

**`stop_discovery`**: Call `daemon.stop_browse(SMB_SERVICE_TYPE)` then `daemon.shutdown()`. This cleanly terminates the
daemon thread and the event receiver loop (channel closes, `recv()` returns `Err`).

**`update_resolve_timeout`**: Same `AtomicU64` pattern as before. With mdns-sd, browse automatically resolves services (
no separate timeout). This timeout is only relevant for the manual DNS fallback path in `mod.rs::resolve_host_ip()`.

### Milestone 2: Wire up and switch over

**`network/mod.rs`** (lines 6, 26):

```rust
// pub mod bonjour;           →  pub mod mdns_discovery;
// pub use bonjour::start_discovery;  →  pub use mdns_discovery::start_discovery;
```

**`commands/settings.rs`** (line 11):

```rust
// use crate::network::bonjour::update_resolve_timeout;
// →
// use crate::network::mdns_discovery::update_resolve_timeout;
```

**`lib.rs`** — add explicit shutdown in `on_window_event` (line 559, alongside `ai::manager::shutdown()`):

```rust
#[cfg(target_os = "macos")]
network::mdns_discovery::stop_discovery();
```

Also add in the `Destroyed` handler (line 564). Currently `stop_discovery` is `#[allow(dead_code)]` — it becomes live.

### Milestone 3: Delete old code and test

- Delete `network/bonjour.rs` (532 lines)
- Write unit tests for `extract_instance_name` and `extract_preferred_ip` (edge cases: empty addresses, IPv6-only, no
  separator in fullname)
- Port `test_constants` from old module

**Run checks**:

```bash
./scripts/check.sh --check rustfmt --check clippy --check cargo-deny --check cargo-udeps --check jscpd-rust --check rust-tests
```

**Manual verification** with MCP:

1. `pnpm dev` → connect driver session
2. Verify hosts appear in the network sidebar
3. Verify resolved hostname + IP populate
4. Check `RUST_LOG=cmdr::network=debug pnpm dev` for event flow
5. If possible, toggle a network device off/on to test `ServiceRemoved`

## Task list

### Milestone 1: Add mdns-sd and create new module

- [ ] Add `mdns-sd = { version = "0.17", features = ["logging"] }` to macOS deps in Cargo.toml
- [ ] Remove `"NSNetServices"` and `"NSRunLoop"` from objc2-foundation features
- [ ] Run `cargo deny check licenses`
- [ ] Create `src/network/mdns_discovery.rs` with constants, global state, helpers, and public API
- [ ] Implement `start_discovery`, `stop_discovery`, `update_resolve_timeout`
- [ ] Implement `process_events` loop mapping all `ServiceEvent` variants
- [ ] Verify the new module compiles (both modules coexist temporarily)

### Milestone 2: Wire up and switch over

- [ ] Update `network/mod.rs` module declaration and re-export
- [ ] Update `commands/settings.rs` import path
- [ ] Add `stop_discovery()` to shutdown path in `lib.rs` (both `CloseRequested` and `Destroyed`)
- [ ] Verify the app compiles with only the new module wired in

### Milestone 3: Delete old code and test

- [ ] Delete `network/bonjour.rs`
- [ ] Write unit tests for `extract_instance_name`, `extract_preferred_ip`, and constants
- [ ] Run checks: rustfmt, clippy, cargo-deny, cargo-udeps, jscpd-rust, rust-tests
- [ ] Manual test with MCP: verify hosts appear, resolve, and disappear in the UI

## What changes for the user

Nothing. Same UI, same events, same timing (within a few hundred ms).

## Risks

| Risk                                                                                | Mitigation                                                                          |
|-------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------|
| "Active" state timing differs slightly                                              | Monitor during manual testing; can add debounce timer if needed                     |
| mdns-sd service type format differs (`_smb._tcp.local.` vs `_smb._tcp.` + `local.`) | Constant is set correctly; browse returns error on wrong format (caught at startup) |
| Daemon thread not shut down on crash                                                | Same as current — OS cleans up. Explicit shutdown handles normal exit               |
| Address type (`ScopedIp` vs `IpAddr`) may need `.into()` conversion                 | Check docs during implementation, straightforward either way                        |
