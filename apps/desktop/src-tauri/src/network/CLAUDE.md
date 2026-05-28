# Network SMB support

Discover, browse, and mount SMB network shares. Works on macOS and Linux.

Frontend counterpart: [`apps/desktop/src/lib/file-explorer/network/CLAUDE.md`](../../../src/lib/file-explorer/network/CLAUDE.md) for the network browser, share picker, login form, and reconnect-manager state.

## Architecture

- **Discovery**: `mdns_discovery.rs`: Pure Rust mDNS using `mdns-sd` crate. Cross-platform.
- **Manual servers**: `manual_servers.rs`: User-added servers via "Connect to server..." dialog. Parses addresses, checks TCP reachability, persists to `manual-servers.json`, and injects synthetic `NetworkHost` entries with `source: Manual` into `DISCOVERY_STATE`. Loaded at startup.
- **E2E testing**: `virtual_smb_hosts.rs`: Injects 14 synthetic `NetworkHost` entries for smb2's consumer Docker containers. Hosts come from `SMB_E2E_{SVC}_HOST` (default `localhost`). Ports come from `SMB_E2E_{SVC}_PORT` when set, else `smb2::testing::*_port()` (which reads `SMB_CONSUMER_*_PORT`, default 10480+). `SMB_E2E_*_PORT` is the test-suite contract (same var the frontend fixture reads), so backend and fixture agree on which port to connect to. This matters inside Docker where containers listen on `:445` internally but `SMB_CONSUMER_*_PORT` would point at the host-side mapping. Gated behind `smb-e2e` Cargo feature. Never enabled in production.
- **Share listing**: Split across multiple files:
  - `smb_client.rs`: Top-level share-listing entry point; orchestrates guest -> keychain -> prompt auth flow; tries smb2 first, falls back to smbutil (macOS only)
  - `smb_connection.rs`: TCP connection establishment and share listing via `smb2::SmbClient`
  - `smb_cache.rs`: 30-second in-memory cache for share lists, keyed by server address
  - `smb_smbutil.rs`: `smbutil view -G` fallback for older Samba/NAS servers (macOS); on Linux delegates to `smb_smbclient`
  - `smb_smbclient.rs`: `smbclient -L` fallback for Linux (requires `samba-client` package)
  - `linux_distro.rs`: Thin wrapper calling `crate::linux_distro::LinuxDistro` for smbclient install hints; `cfg(target_os = "linux")` gated
  - `smb_types.rs`: Shared types (`ShareInfo`, `AuthMode`, `ShareListError`, etc.)
  - `smb_util.rs`: Helpers: error classification (`classify_error`, `is_auth_error`) and `convert_shares` (maps `smb2::ShareInfo` to Cmdr's `ShareInfo`)
  - `smb_upgrade.rs`: Upgrade OS-mounted SMB volumes to direct smb2 connections. Shared by three upgrade paths (startup, mount-time watcher, manual "Connect directly"). Contains `register_smb_volume`, `try_smb_upgrade`, `UpgradeResult`/`UpgradeError` types, address resolution (`resolve_server_address`, `resolve_ip_to_hostname`, `friendly_server_name`), and `get_keychain_password`.
- **Mounting** (platform-specific via `#[path]` in `mod.rs`):
  - `mount.rs`: macOS `NetFSMountURLSync` for native `/Volumes/` mounts; also `unmount_smb_shares_from_host` (iterates `/Volumes/`, matches via `statfs`, unmounts via `diskutil`)
  - `mount_linux.rs`: Linux `gio mount` for GVFS-based user-space mounts
- **Auth** (platform-agnostic):
  - `keychain.rs`: SMB credential management. Delegates storage to `crate::secrets::store()` (see `secrets/CLAUDE.md` for backend details)
- **State**: `known_shares.rs`: Connection history in `known-shares.json` (usernames, last auth mode, timestamps).

## Platform strategy

| Component | macOS | Linux |
|-----------|-------|-------|
| mDNS discovery | `mdns-sd` (pure Rust) | `mdns-sd` (same) |
| SMB share listing | `smb2` crate (pure Rust) | `smb2` (same) |
| smbutil fallback | `smbutil view -G` | `smbclient -L` (from `samba-client` package) |
| Credential storage | `secrets` module (Keychain) | `secrets` module (Secret Service → encrypted file fallback) |
| Mounting | `NetFSMountURLSync` → `/Volumes/` | `gio mount` → `/run/user/<uid>/gvfs/` |

## Key decisions

### Lazy mDNS startup gated on user toggle and first-trigger flag

`network::start_discovery()` no longer fires unconditionally in `lib.rs::setup`. Instead, two settings drive the
lifecycle:

- **`network.enabled`** (boolean, default `true`): top-level user toggle in `Settings > Network > SMB/Network shares`.
  When `false`, the picker shows "Network (disabled)", no mDNS daemon runs, and no proactive smb2 upgrades happen.
- **`network.firstTriggerDone`** (boolean, default `false`, hidden): tracks whether we've already performed a gated
  network action. Persisted across launches.

The runtime mirror of `network.enabled` lives in `network::NETWORK_ENABLED` (`AtomicBool`). `lib.rs::setup` seeds it
from the persisted settings; `commands::network::set_network_enabled` keeps it in sync with the live toggle.
`network::is_network_enabled()` is the runtime accessor; BE-side upgrade paths check this before kicking off mDNS or
waiting on hostname resolution.

At startup, mDNS starts only if `network.enabled && (firstTriggerDone || smb-e2e feature)`. On a fresh install,
`firstTriggerDone == false` so we stay quiet and the macOS "Cmdr wants to find devices on local networks" prompt
doesn't fire at app launch.

The frontend calls `ensure_network_discovery_started` (idempotent) when the user takes a network action: clicking
"Network" in the picker, opening "Connect to server…", or hitting the OS-mount → direct-smb2 upgrade indicator. That
first call is what triggers the OS prompt. We also flip `firstTriggerDone = true` so subsequent launches start mDNS
eagerly without surprising the user.

`set_network_enabled(false)` stops the daemon and clears `DISCOVERY_STATE.hosts`, emitting `network-host-lost` events
so the frontend store empties. `set_network_enabled(true)` is a no-op; the user must take a network action to
re-trigger discovery.

The E2E build feature (`smb-e2e`) bypasses both gates so virtual SMB hosts are populated before tests run.

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
crate, a single dependency replacing the old `smb` + `smb-rpc` pair. `smb2::list_shares()` returns pre-filtered disk
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
4. Never assume "guest only"; always offer "Sign in for more access" when guest succeeds (can't distinguish guest-only from guest-or-creds at probe time)

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

### `register_replacing_predecessor` cleans up the displaced volume before replacing it

Every `NSWorkspaceDidMountNotification` on an SMB share triggers a fresh `register_smb_volume` cycle, and the user can re-trigger the same path via manual "Connect directly" after an unmount/remount cycle. Without explicit cleanup, the new `SmbVolume` simply overwrites the old one's `Arc` slot in `VolumeManager`. The old volume's watcher would still exit eventually — its `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` drops with the `Arc`, the `Sender` drops, the watcher's `cancel_rx` resolves to `Err(Closed)` and the `select!` branch fires — but timing is non-deterministic ("whenever the last `Arc` ref goes away") and an in-flight `do_attempt_reconnect` on the displaced volume could install a fresh session into a volume that's no longer in the manager.

`register_replacing_predecessor` (in `smb_upgrade.rs`) closes both gaps: it looks up the predecessor via `manager.get(volume_id)`, calls `on_unmount` on it (which sets the `unmounted` flag, transitions state, pings the watcher cancel, and drops the smb2 session), then `register`s the new volume. Both `register_smb_volume` and `try_smb_upgrade` route through this helper.

**Gotcha**: `SmbVolume::on_unmount` uses `blocking_write()` / `blocking_lock()` because the FSEvents-thread call site (`volumes::watcher::handle_volume_unmounted`) is sync. Inside `register_replacing_predecessor` we're in an async context, so calling `on_unmount` directly would panic ("cannot block_on within a runtime"). The helper wraps the call in `tokio::task::spawn_blocking(...).await` so the lock acquisition runs on the blocking-thread pool. Don't switch back to a direct call.

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
- **SMB upgrade waits briefly for mDNS to warm**: When macOS auto-remounts an SMB share at login, FSEvents fires before
  mDNS has discovered the host, so `statfs` gives us an IP but the host map is empty. Stored Keychain credentials are
  keyed by mDNS hostname (`smb://naspolya/share`), not by IP, so a sync IP→hostname lookup misses and we'd prompt the
  user for credentials they already saved. The upgrade path now (a) kicks off mDNS via `network::ensure_mdns_started`
  before resolving and (b) calls `smb_upgrade::resolve_ip_to_hostname_with_wait` which polls the discovered-host map
  every 100ms up to 1500ms for private-range IPv4. Non-private IPs (Tailscale, public DNS) skip the wait — mDNS won't
  help there. The wait fails open: if mDNS never warms, the IP-only Keychain lookup still runs. Only relevant in dev,
  where `network.firstTriggerDone == false` keeps mDNS off at launch; prod users hit this once on the very first install
  but never afterwards. Both entry points are covered: `commands::network::upgrade_to_smb_volume` (manual "Connect
  directly") and `volumes::watcher::try_upgrade_smb_mount` (FSEvents auto-upgrade).
- **`statfs` can return mDNS service names instead of IPs**: When macOS auto-reconnects an SMB mount on login, `statfs.f_mntfromname` may contain `//user@Naspolya._smb._tcp.local/share` instead of `//user@192.168.1.111/share`. These service names are not DNS-resolvable. `resolve_server_address()` in `commands/network.rs` detects these (by checking for `._tcp`/`._udp`) and resolves them to IPs via `get_discovered_hosts()`. All upgrade paths (startup, mount-time, manual) go through this resolution. Similarly, `friendly_server_name()` extracts the display name (e.g., `Naspolya`) for UI display.
