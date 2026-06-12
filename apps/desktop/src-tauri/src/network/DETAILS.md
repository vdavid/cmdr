# Network SMB support details

Pull-tier docs for `src-tauri/src/network/`: architecture, flows, and decision rationale. Must-know invariants and
gotchas live in [CLAUDE.md](CLAUDE.md).

Discover, browse, and mount SMB network shares. Works on macOS and Linux.

Frontend counterpart: [`apps/desktop/src/lib/file-explorer/network/CLAUDE.md`](../../../src/lib/file-explorer/network/CLAUDE.md) for the network browser, share picker, login form, and reconnect-manager state.

Reference: [`benchmarks/smb/CLAUDE.md`](../../../../../benchmarks/smb/CLAUDE.md) is a standalone throughput benchmark of the third-party `smb` (smb-rs) crate, the alternative we measured before standardizing on the in-house `smb2`. It has its own `Cargo.toml` and isn't part of the app build.

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
  - `smb_upgrade.rs`: Upgrade OS-mounted SMB volumes to direct smb2 connections. Shared by three upgrade paths (startup, mount-time watcher, manual "Connect directly"). Contains `register_smb_volume`, `resolve_and_register_smb_volume` (the shared resolve+creds+register used by both fire-and-forget auto-upgrade paths), `try_smb_upgrade`, `UpgradeResult`/`UpgradeError` types, address resolution (`resolve_server_address`, `resolve_ip_to_hostname`, `friendly_server_name`), and `get_keychain_password`.
- **Mounting** (platform-specific via `#[path]` in `mod.rs`):
  - `mount.rs`: macOS `NetFSMountURLSync` for native `/Volumes/` mounts; also `unmount_smb_shares_from_host` (iterates `/Volumes/`, matches via `statfs`, unmounts via `diskutil`)
  - `mount_linux.rs`: Linux `gio mount` for GVFS-based user-space mounts
- **Server identity**: `server_identity.rs`: `same_server` / `same_server_live` equivalence over the names a server goes by (mDNS service name, `.local` hostname, IP), enriched from the discovery state. Used by the mount-path disambiguation and the already-mounted short-circuit so string-shape differences can't split one server into two.
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

To make this hold, every NetFS mount sets `UIOption = NoUI` (`open_option_entries` in `mount.rs`). Without it, NetFS
hands auth *failures* to NetAuthAgent even when we pass explicit credentials: the agent pops a system dialog ("You
entered an invalid username or password...") on top of Cmdr, blocks the mount call while open, and returns
`kNetAuthErrorInternal` (-6600) when dismissed. With `NoUI`, the same failure returns immediately as a typed code
(`error_from_code` maps -6600 → `AuthFailed`, -6004 `kNetAuthErrorGuestNotSupported` → `AuthRequired`) and the frontend
renders its own login form.

### `smb2` for SMB share enumeration (not `pavao`/libsmbclient, `smb-rs`, or `smbutil`)

MIT license (compatible with BSL, allows dual-licensing for enterprise), pure Rust (no C dependencies), async-native
(built on tokio), cross-platform, and typed errors (`smb2::Error` variants vs string pattern matching). David's own
crate, a single dependency replacing the old `smb` + `smb-rpc` pair. `smb2::list_shares()` returns pre-filtered disk
shares with clean `String` fields (no NDR parsing needed). Fallback to `smbutil`/`smbclient` is available for older
Samba servers where smb2's RPC fails.

### Fix share-enumeration gaps in `smb2`, not via native macOS SMB SPI

The dominant trigger for the smbutil/smbclient fallback was an `smb2` bug: it failed to reassemble a `NetShareEnum`
srvsvc reply that a server split across multiple DCE/RPC fragments (older Samba / NAS firmware with many shares or long
comments returned `STATUS_BUFFER_OVERFLOW`, which smb2 treated as fatal). `smb2 0.11.3` fixes this (fragment reassembly
+ `STATUS_BUFFER_OVERFLOW` follow), so those servers now enumerate over the pure-Rust path and never reach the fallback.
The end-to-end regression test is `smb_client.rs::integration_tests::smb_integration_many_shares_enumerate_via_smb2`
(lists 50 guest shares through Cmdr's own `list_shares` entry point).

We considered a native macOS SMB-enumeration API to drop the smbutil shell-out entirely, and rejected it. The auth half
exists only as **private SPI** (`SMBClient.framework`'s `SMBOpenServerEx`, no public headers), and the enumeration half
(`NetShareEnum` srvsvc + `RapNetShareEnum` legacy RAP — the exact path old servers need) is **not a framework API at
all**: it lives inside the `/usr/bin/smbutil` binary, so we'd link a fragile private framework for auth and still
reimplement enumeration ourselves. Since smb2 already owns that domain (in supported, cross-platform Rust), fixing the
root cause there is the cleaner path. Full evidence (disassembly, SDK header grep, in-memory-auth probe against the
Docker containers, effort estimate): [`docs/notes/spike-native-smb-share-enumeration.md`](../../../../../docs/notes/spike-native-smb-share-enumeration.md).

Landing the smb2 fix also let us **drop the leaky macOS authed-smbutil path** (the `//user:password@host` URL leaked the
cleartext password into `ps`-readable argv). See the "smbutil / smbclient fallback" credential-channel note below.

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

`smb2` crate may fail on older Samba servers with RPC incompatibility. Classify error as `ProtocolError`, then try a platform-specific CLI fallback. A refused/unreachable TCP connect is NOT a protocol error: `classify_error` (in `smb_util.rs`) maps `smb2::Error::Io` with `ConnectionRefused` / `HostUnreachable` / `NetworkUnreachable` io kinds to `ShareListError::HostUnreachable`, so an offline server skips the fallback (the same dead port refuses any client) and doesn't log the fallback warn. The fallback paths:
- **macOS:** `smbutil view -G -N` (guest) or `smbutil view -N` (Keychain-backed; smbutil reads the system Keychain itself). **No authenticated smbutil fallback** — see the credential-channel note below.
- **Linux:** `smbclient -L` (from `samba-client` package), guest or authenticated. If `smbclient` is not installed, returns a `MissingDependency` error with a distro-specific install command (detected via `/etc/os-release`). The `smb_smbutil.rs` Linux stubs delegate to `smb_smbclient.rs`.
- **Other platforms:** stubs return `ProtocolError`.

When smb2's authenticated listing returns empty or errors, the fallback diverges by platform (`smb_client.rs::list_shares_smb2`): **Linux** retries via `smbclient -A` (safe authfile); **macOS** surfaces the underlying smb2 failure (classified via `classify_error`, or `AuthFailed` on an empty result) so the user gets a real error and can still mount through the secure NetFS path.

**Credential channel (keeping the password out of argv):** `smbclient` gets credentials via a 0o600 temp
authentication file passed as `-A <file>` (`smb_smbclient.rs::write_smbclient_auth_file`), never `-U user%pass`, so the
password never lands in the world-readable process argument list (`ps aux` / `/proc/<pid>/cmdline`). The temp file is
created inside the blocking task and dropped (unlinked) the moment the call returns, success or error.

`smbutil` has **no argv-free channel** for an explicit password: `smbutil view` only accepts the password embedded in the
`//user:password@host` URL (per `man smbutil`), `nsmb.conf`/`~/.nsmbrc` has no password keyword (per `man nsmb.conf`),
there's no password env var, and the interactive prompt (omit `-N`) reads via `getpass()`/`/dev/tty` which a TTY-less
spawned child can't feed reliably. So Cmdr **never shells out to smbutil with an explicit password**: `build_smbutil_url`
only ever builds passwordless `//host` / `//host:port` URLs, used by the guest (`-G -N`) and Keychain (`-N`) paths. The
old URL-embedded-password leak is closed. The primary macOS mount path (`NetFSMountURLSync`) and smb2 share enumeration
also never expose the password.

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

`register_replacing_predecessor` (in `smb_upgrade.rs`) closes both gaps: it looks up the predecessor via `manager.get(volume_id)`, calls `on_unmount` on it (which sets the `unmounted` flag, transitions state, pings the watcher cancel, and drops the smb2 session), then `register`s the new volume. Both `register_smb_volume` and `try_smb_upgrade` route through this helper. It also emits `volumes-changed` after registering: the after-sign-in and already-mounted upgrade paths have no FSEvents mount event to ride, so without the explicit broadcast the frontend keeps the stale `os_mount` dot on a volume that's already `direct`.

**Gotcha**: `SmbVolume::on_unmount` uses `blocking_write()` / `blocking_lock()` because the FSEvents-thread call site (`volumes::watcher::handle_volume_unmounted`) is sync. Inside `register_replacing_predecessor` we're in an async context, so calling `on_unmount` directly would panic ("cannot block_on within a runtime"). The helper wraps the call in `tokio::task::spawn_blocking(...).await` so the lock acquisition runs on the blocking-thread pool. Don't switch back to a direct call.

### Linux mounting via GVFS

`gio mount` is used for user-space SMB mounting on Linux. It requires the `gvfs-smb` package. If `gio` is not available, a helpful error message is returned. Mounts appear under `/run/user/<uid>/gvfs/`.

The password is fed to `gio mount` through the child's **stdin** (`run_gio_mount` spawns `gio` directly with a piped stdin), never via a shell command line. An earlier `sh -c "echo 'PASS' | gio mount …"` shape leaked the cleartext password into the process argument list (`ps` / `/proc/<pid>/cmdline`) — the same argv exposure the macOS smbutil path is careful to avoid. The already-mounted check (`find_existing_mount` → `match_existing_smb_mount`) parses `gio mount -l` and compares servers by identity (`server_identity::same_server`), so a share mounted under one name (for example by Nautilus using the hostname) is recognized when we look it up by another (the IP).

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

"Different server" is an identity comparison (`server_identity::same_server_live`), never a string compare: `statfs`
may report the existing mount as `Naspolya._smb._tcp.local` while we mount by `192.168.1.111`, and a string mismatch
would treat one NAS as two, force a second mount with `ForceNewSession`, and break session reuse. For the same reason,
`mount_share_sync` returns early with `already_mounted: true` when `find_mount_path_for_share` finds the same
server+share+port already mounted, skipping NetFS entirely.

## Gotchas

- **Don't hold mutex during DNS resolution**: `get_host_for_resolution` / `update_host_resolution` extract host info and release the mutex before blocking DNS, then re-acquire to update. Holding the mutex across network calls risks deadlock.
- **Auth mode is a guess**: `GuestAllowed` means "guest worked, creds might also work." `CredsRequired` means "guest failed, must have creds." Can't detect guest-only vs guest-or-creds without trying both.
- **NetFS error 17 (EEXIST) is success** (macOS): Share already mounted. Return existing mount path, set `already_mounted: true`. Not an error.
- **mDNS service type must include `.local.`**: `mdns-sd` requires full form `"_smb._tcp.local."` (trailing dot). Without it, browse() fails silently.
- **Account name is keyed by server identity, not the raw string**: `make_account_name` runs the server through `server_identity::credential_key` (lowercase + strip the mDNS service suffix / `.local` down to the bare instance name), so `Naspolya`, `naspolya.local`, and `Naspolya._smb._tcp.local` all key the same entry. Without this the frontend saved under the mDNS instance name while the OS-mount upgrade path looked up by the `statfs` service name, so a just-saved password was never found on the next connect (the picker kept showing the `os_mount` dot and re-prompted). IP literals have no bare form and pass through unchanged.
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
  but never afterwards. **All three upgrade paths are covered.** The two fire-and-forget paths — startup
  (`file_system::upgrade_existing_smb_mounts`) and mount-time (`volumes::watcher::try_upgrade_smb_mount`) — both go
  through the shared `smb_upgrade::resolve_and_register_smb_volume`, so the resolver choice can't drift between them
  again (the startup copy previously used the one-shot `resolve_ip_to_hostname`, looked creds up by LAN IP, missed
  hostname-keyed creds, and fell back to guest → `STATUS_LOGON_FAILURE`). The manual "Connect directly" path
  (`commands::network::upgrade_to_smb_volume`) stays separate because it surfaces `CredentialsNeeded` to prompt the
  user, but uses the same `resolve_ip_to_hostname_with_wait` + `get_keychain_password` pair.
- **`statfs` can return mDNS service names instead of IPs**: When macOS auto-reconnects an SMB mount on login, `statfs.f_mntfromname` may contain `//user@Naspolya._smb._tcp.local/share` instead of `//user@192.168.1.111/share`. These service names are not DNS-resolvable. `resolve_server_address()` in `commands/network.rs` detects these (by checking for `._tcp`/`._udp`) and resolves them to IPs via `get_discovered_hosts()`. All upgrade paths (startup, mount-time, manual) go through this resolution. Similarly, `friendly_server_name()` extracts the display name (e.g., `Naspolya`) for UI display.
