# Network SMB support

Discover, browse, and mount SMB network shares. Works on macOS and Linux.

## Architecture

- **Discovery**: `mdns_discovery.rs` — Pure Rust mDNS using `mdns-sd` crate. Cross-platform.
- **Manual servers**: `manual_servers.rs` — User-added servers via "Connect to server..." dialog. Parses addresses, checks TCP reachability, persists to `manual-servers.json`, and injects synthetic `NetworkHost` entries with `source: Manual` into `DISCOVERY_STATE`. Loaded at startup.
- **E2E testing**: `virtual_smb_hosts.rs` — Injects 14 synthetic `NetworkHost` entries for smb2's consumer Docker containers. Ports come from `smb2::testing::*_port()` functions (configurable via `SMB_CONSUMER_*_PORT` env vars, default 10480+). Hosts configurable via `SMB_E2E_*_HOST` env vars (default `localhost`). Gated behind `smb-e2e` Cargo feature. Never enabled in production.
- **Share listing**: Split across multiple files:
  - `smb_client.rs` — Top-level share-listing entry point; orchestrates guest -> keychain -> prompt auth flow; tries smb2 first, falls back to smbutil (macOS only)
  - `smb_connection.rs` — TCP connection establishment and share listing via `smb2::SmbClient`
  - `smb_cache.rs` — 30-second in-memory cache for share lists, keyed by server address
  - `smb_smbutil.rs` — `smbutil view -G` fallback for older Samba/NAS servers (macOS); on Linux delegates to `smb_smbclient`
  - `smb_smbclient.rs` — `smbclient -L` fallback for Linux (requires `samba-client` package)
  - `linux_distro.rs` — Thin wrapper calling `crate::linux_distro::LinuxDistro` for smbclient install hints; `cfg(target_os = "linux")` gated
  - `smb_types.rs` — Shared types (`ShareInfo`, `AuthMode`, `ShareListError`, etc.)
  - `smb_util.rs` — Helpers: error classification (`classify_error`, `is_auth_error`) and `convert_shares` (maps `smb2::ShareInfo` to Cmdr's `ShareInfo`)
- **Mounting** (platform-specific via `#[path]` in `mod.rs`):
  - `mount.rs` — macOS `NetFSMountURLSync` for native `/Volumes/` mounts; also `unmount_smb_shares_from_host` (iterates `/Volumes/`, matches via `statfs`, unmounts via `diskutil`)
  - `mount_linux.rs` — Linux `gio mount` for GVFS-based user-space mounts
- **Auth** (platform-agnostic):
  - `keychain.rs` — SMB credential management. Delegates storage to `crate::secrets::store()` (see `secrets/CLAUDE.md` for backend details)
- **State**: `known_shares.rs` — Connection history in `known-shares.json` (usernames, last auth mode, timestamps).

## Platform strategy

| Component | macOS | Linux |
|-----------|-------|-------|
| mDNS discovery | `mdns-sd` (pure Rust) | `mdns-sd` (same) |
| SMB share listing | `smb2` crate (pure Rust) | `smb2` (same) |
| smbutil fallback | `smbutil view -G` | `smbclient -L` (from `samba-client` package) |
| Credential storage | `secrets` module (Keychain) | `secrets` module (Secret Service → encrypted file fallback) |
| Mounting | `NetFSMountURLSync` → `/Volumes/` | `gio mount` → `/run/user/<uid>/gvfs/` |

## Key decisions

### `NetFSMountURLAsync` for SMB mounting (not `mount_smbfs` CLI)

Non-blocking (UI stays responsive), credentials passed via secure API (not exposed in process list), native Keychain
integration, and structured error codes instead of parsing stderr. Requires custom Rust FFI bindings for NetFS.framework.
Linux uses `gio mount` (GVFS) instead.

### Custom auth UI with Keychain integration (not system dialog)

Full UX control (login form appears in-pane), smart defaults (pre-fill username from connection history), and
guest/credentials toggle. `keychain.rs` delegates to `crate::secrets::store()` for platform-agnostic credential storage
(macOS Keychain, Linux Secret Service, encrypted file fallback). Passwords never stored in our settings file.
`CMDR_SECRET_STORE=file` forces the plain file backend in dev mode (set by `tauri-wrapper.js`).

### `smb2` for SMB share enumeration (not `pavao`/libsmbclient, `smb-rs`, or `smbutil`)

MIT license (compatible with BSL, allows dual-licensing for enterprise), pure Rust (no C dependencies), async-native
(built on tokio), cross-platform, and typed errors (`smb2::Error` variants vs string pattern matching). David's own
crate — single dependency replaces the old `smb` + `smb-rpc` pair. `smb2::list_shares()` returns pre-filtered disk
shares with clean `String` fields (no NDR parsing needed). Fallback to `smbutil`/`smbclient` is available for older
Samba servers where smb2's RPC fails.

### Always use IP when available

smb2 uses the addr host component in UNC paths (`\\server\IPC$`). When hostname has a `.local` suffix, strip it
before passing as addr (some servers reject `.local` in UNC paths). Always pass resolved IP from mDNS discovery when
available. If IP unavailable, use derived hostname with `.local` stripped.

### Guest-first auth flow

1. Try anonymous/guest access first
2. On auth error → check stored credentials
3. If no stored creds → prompt user
4. Never assume "guest only" — always offer "Sign in for more access" when guest succeeds (can't distinguish guest-only from guest-or-creds at probe time)

### smbutil / smbclient fallback

`smb2` crate may fail on older Samba servers with RPC incompatibility. Classify error as `ProtocolError`, then try a platform-specific CLI fallback:
- **macOS:** `smbutil view -G` (built-in).
- **Linux:** `smbclient -L` (from `samba-client` package). If `smbclient` is not installed, returns a `MissingDependency` error with a distro-specific install command (detected via `/etc/os-release`). The `smb_smbutil.rs` Linux stubs delegate to `smb_smbclient.rs`.
- **Other platforms:** stubs return `ProtocolError`.

### No persistent connection pool

smb2 connections are lightweight (one `SmbClient` per connection) and created on-demand. Caching is at the share list level (30s TTL), not TCP connection level.

### In-memory credential cache

After first credential fetch, credentials cached in `CREDENTIAL_CACHE` (LazyLock + RwLock). Prevents repeated Keychain/secret-service round-trips during session. Cache keyed by `"smb://{server}/{share}"`.

### Credential storage via `secrets` module

All credential storage backends now live in `crate::secrets` (see `secrets/CLAUDE.md`). `keychain.rs` is platform-agnostic and delegates to `crate::secrets::store()`. The `is_file_backed()` check (used by the frontend to show a one-time info toast) delegates to `crate::secrets::is_file_backed()`.

### "Sneaky mount" for SmbVolume

When the user mounts an SMB share, we establish a parallel smb2 connection alongside the OS mount. The OS mount provides Finder/Terminal/drag-drop compatibility, while Cmdr's file operations use the smb2 session for better performance and fail-fast behavior. The `SmbVolume` is registered in `VolumeManager` before the FSEvents watcher fires, using `register` (overwrite). When the watcher fires, `register_if_absent` is a no-op since the SmbVolume is already registered. See `file_system/volume/smb.rs` for the implementation.

### Linux mounting via GVFS

`gio mount` is used for user-space SMB mounting on Linux. It requires the `gvfs-smb` package. If `gio` is not available, a helpful error message is returned. Mounts appear under `/run/user/<uid>/gvfs/`.

### `HostSource` enum on `NetworkHost`

`NetworkHost.source` distinguishes mDNS-discovered hosts (`Discovered`, default) from user-added ones (`Manual`). Defaults to `Discovered` via `#[serde(default)]` for backward compatibility with existing serialized data. The frontend uses this to determine which hosts show a "Remove" option and to skip mDNS resolution for manual hosts.

### Concurrency strategy for persistence stores

`known_shares.rs` uses an in-memory `Mutex<KnownSharesStore>` as single source of truth. Disk is a snapshot of the
in-memory state, so concurrent mutations are safe (the mutex serializes all in-memory updates).

`manual_servers.rs` uses a file-based read-modify-write pattern (no in-memory cache). A global `STORE_LOCK` mutex
protects the entire read-modify-write cycle to prevent TOCTOU races where two threads could read the same disk state
and one write clobbers the other.

### Manual server ID convention

Manual server IDs use the format `manual-{address}-{port}` with dots/colons replaced by dashes. This is deterministic (same address+port always produces the same ID), preventing duplicates. The `manual-` prefix avoids collision with mDNS-derived IDs.

### Mount path disambiguation for same-name shares

When two servers have a share with the same name (for example, two NAS devices both sharing `public`), the mount code
detects the collision before calling `NetFSMountURLSync`. `disambiguated_mount_path` checks if `/Volumes/{share}` is
already taken by a different server (via `statfs`), and if so picks `/Volumes/{share}-1`, `-2`, etc. (Finder's
convention) and passes it as an explicit mount point to `NetFSMountURLSync`. The volume switcher shows
`{share} on {server}` for SMB mounts so the user knows which server each volume belongs to.

## Gotchas

- **Don't hold mutex during DNS resolution**: `get_host_for_resolution` / `update_host_resolution` extract host info and release the mutex before blocking DNS, then re-acquire to update. Holding the mutex across network calls risks deadlock.
- **Auth mode is a guess**: `GuestAllowed` means "guest worked, creds might also work." `CredsRequired` means "guest failed, must have creds." Can't detect guest-only vs guest-or-creds without trying both.
- **NetFS error 17 (EEXIST) is success** (macOS): Share already mounted. Return existing mount path, set `already_mounted: true`. Not an error.
- **mDNS service type must include `.local.`**: `mdns-sd` requires full form `"_smb._tcp.local."` (trailing dot). Without it, browse() fails silently.
- **Account name is lowercase**: `make_account_name` lowercases server name for consistency. Prevents duplicate entries for "SERVER" vs "server".
- **Linux `gio mount` requires GVFS**: The `gvfs-smb` package must be installed. Standard on Ubuntu/Fedora GNOME desktops. KDE desktops may need it explicitly.
- **`ShareListError` uses internally tagged serde format** (`#[serde(tag = "type")]`) with struct variants. This keeps a flat JSON shape (`{ "type": "protocol_error", "message": "..." }`). The `MissingDependency` variant adds an optional `installCommand` field. When adding new variants, use struct syntax (not tuple).
- **macOS smbutil and NetFSMountURLSync fail with loopback IP + non-standard port**: `//127.0.0.1:10480` gives "Broken pipe", but `//localhost:10480` works. `build_smbutil_url` and `NetworkMountView.svelte` both fall back to hostname when IP is `127.0.0.1` or `::1`. This matters for E2E testing against Docker containers on localhost.
- **Mount URL must include port when non-standard**: `mount_share_sync` builds `smb://server:port/share` for non-445 ports. The port is passed as a separate parameter through `mount_share` → `mount_share_sync`, not embedded in the server string (embedding it would cause `build_smb_addr` to double the port: `localhost:10480:10480`). `SmbMountInfo.port` extracts the port from `statfs` mount source for upgrade paths.
- **Strip `.local` from addr for smb2**: `smb2::Connection::connect()` extracts `server_name` from the addr string and uses it in UNC paths. Passing `"foo.local:445"` creates `\\foo.local\IPC$` which some servers reject. The `build_addr` helper in `smb_connection.rs` handles this.
- **Manual hosts always set `hostname`**: The share listing pipeline guards on `host.hostname` being truthy. `create_network_host` always sets `hostname` (to the address, even for IPs) so manual hosts flow through the pipeline correctly.
