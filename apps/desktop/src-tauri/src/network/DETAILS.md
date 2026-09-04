# Network SMB support details

Pull-tier docs for `src-tauri/src/network/`: architecture, flows, and decision rationale. Must-know invariants and
gotchas live in `CLAUDE.md`.

Discover, browse, and mount SMB network shares. Works on macOS and Linux.

Frontend counterpart: `apps/desktop/src/lib/file-explorer/network/CLAUDE.md` for the network browser, share picker,
login form, and reconnect-manager state.

Reference: `benchmarks/smb/CLAUDE.md` is a standalone throughput benchmark of the third-party `smb` (smb-rs) crate,
the alternative we measured before standardizing on the in-house `smb2`. It has its own `Cargo.toml` and isn't part
of the app build.

## Architecture

- **Discovery**: `mdns_discovery.rs`: Pure Rust mDNS using `mdns-sd` crate. Cross-platform.
- **Manual servers**: `manual_servers.rs`: User-added servers via "Connect to server..." dialog. Parses addresses, checks TCP reachability, persists to `manual-servers.json`, and injects synthetic `NetworkHost` entries with `source: Manual` into `DISCOVERY_STATE`. Loaded at startup.
- **E2E testing**: `virtual_smb_hosts.rs`: Injects 14 synthetic `NetworkHost` entries for smb2's consumer Docker containers. Hosts come from `SMB_E2E_{SVC}_HOST` (default `localhost`). Ports come from `SMB_E2E_{SVC}_PORT` when set, else `smb2::testing::*_port()` (which reads `SMB_CONSUMER_*_PORT`, default 10480+). `SMB_E2E_*_PORT` is the test-suite contract (same var the frontend fixture reads), so backend and fixture agree on which port to connect to. This matters inside Docker where containers listen on `:445` internally but `SMB_CONSUMER_*_PORT` would point at the host-side mapping. Gated behind `smb-e2e` Cargo feature. Never enabled in production.
- **Share listing**: Split across multiple files:
  - `smb_client.rs`: Top-level share-listing entry point; orchestrates guest -> keychain -> prompt auth flow; tries smb2 first, falls back to smbutil (macOS only)
  - `smb_cache.rs`: 30-second in-memory cache for share lists, keyed by server address
  - `smb_smbutil.rs`: `smbutil view -G` fallback for older Samba/NAS servers (macOS); on Linux delegates to `smb_smbclient`
  - `smb_smbclient.rs`: `smbclient -L` fallback for Linux (requires `samba-client` package)
  - `linux_distro.rs`: Thin wrapper calling `crate::linux_distro::LinuxDistro` for smbclient install hints; `cfg(target_os = "linux")` gated
  - The protocol layer under all of them is the `cmdr-smb` crate: the addr builder, the guest / authenticated `smb2::SmbClient` listing calls, the `classify_*` / `is_auth_error` classification, and the `ShareInfo` / `AuthMode` / `ShareListResult` / `ShareListError` vocabulary. `crates/cmdr-smb/DETAILS.md` says what belongs there and what stays here
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
`CMDR_SECRET_STORE=file` forces the plain file backend in dev mode (set by `tauri-wrapper.ts`).

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
Docker containers, effort estimate): `docs/notes/spike-native-smb-share-enumeration.md`.

Landing the smb2 fix also let us **drop the leaky macOS authed-smbutil path** (the `//user:password@host` URL leaked the
cleartext password into `ps`-readable argv). See the "smbutil / smbclient fallback" credential-channel note below.

### Always use IP when available

Always pass the resolved IP from mDNS discovery when one is available; fall back to the hostname otherwise. The
`.local` strip that fallback needs is `cmdr_smb::build_smb_addr`'s, and the reason for it lives with that code.

### Guest-first auth flow

1. Try anonymous/guest access first
2. On auth error → check stored credentials
3. If no stored creds → prompt user
4. Never assume "guest only"; always offer "Sign in for more access" when guest succeeds (can't distinguish guest-only from guest-or-creds at probe time)

### smbutil / smbclient fallback

`smb2` crate may fail on older Samba servers with RPC incompatibility. Classify error as `ProtocolError`, then try a platform-specific CLI fallback. Two error classes are NOT protocol errors and must not trigger the fallback (both kept the fallback warn crying wolf when they misclassified):

- A refused/unreachable TCP connect: `cmdr_smb::classify_error` maps `smb2::Error::Io` with `ConnectionRefused` / `HostUnreachable` / `NetworkUnreachable` io kinds to `ShareListError::HostUnreachable`, so an offline server skips the fallback (the same dead port refuses any client).
- A guest/anonymous SessionSetup the server rejects on auth grounds: macOS smbd answers with `STATUS_ACCOUNT_RESTRICTION` (0xC000006E), and smb2 ≥0.13.1 classifies the whole logon-rejection NTSTATUS family as `ErrorKind::AuthRequired`, so `is_auth_error` routes it to the credentials path (keychain → prompt) instead of the CLI fallback. Pinned by `crates/cmdr-smb/src/errors.rs::test_guest_rejection_status_is_auth_error`.

The fallback paths:
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

When the user mounts an SMB share, we establish a parallel smb2 connection alongside the OS mount. The OS mount provides Finder/Terminal/drag-drop compatibility, while Cmdr's file operations use the smb2 session for better performance and fail-fast behavior. The `SmbVolume` is registered in `VolumeManager` before the FSEvents watcher fires, using `register` (overwrite). When the watcher fires, `register_if_absent` is a no-op since the SmbVolume is already registered. See `crates/cmdr-smb/src/volume/mod.rs` for the implementation.

### `register_replacing_predecessor` retires the displaced volume; it never unmounts it

Every `NSWorkspaceDidMountNotification` on an SMB share triggers a fresh `register_smb_volume` cycle, the user can re-trigger the same path via manual "Connect directly", and the startup upgrade pass can land on an already-direct volume. `register_replacing_predecessor` (in `smb_upgrade.rs`) is the one place a new `SmbVolume` takes an occupied slot: it looks up the predecessor via `manager.get(volume_id)`, calls `Volume::on_superseded` on it, then `register`s the new volume. Both `register_smb_volume` and `try_smb_upgrade` route through it. It also emits `volumes-changed` after registering: the after-sign-in and already-mounted upgrade paths have no FSEvents mount event to ride, so without the explicit broadcast the frontend keeps the stale `os_mount` dot on a volume that's already `direct`.

**A replace is not a disconnect, and the predecessor's session must survive it.** The full lifecycle contract, the in-flight holders it protects, and the id-scoped parts that do retire live in `file_system/volume/backends/DETAILS.md` § "Supersede vs. unmount" — the canonical doc, next to the `SmbVolume` code that implements it.

**Gotcha**: the `Volume::on_superseded` DEFAULT delegates to `on_unmount`, which uses `blocking_write()` / `blocking_lock()` because its FSEvents-thread call site (`volumes::watcher::handle_volume_unmounted`) is sync. Inside `register_replacing_predecessor` we're in an async context, so a direct call would panic ("cannot block_on within a runtime") for any backend that hasn't overridden the hook. The helper wraps the call in `tokio::task::spawn_blocking(...).await` so the lock acquisition runs on the blocking-thread pool. Don't switch back to a direct call.

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

### TCP reachability check runs in the dialog, before the host is added

`add_manual_server` does a TCP connect to `host:port` and fails up front if the port is closed, so the dialog shows the error inline and the host is never added on an unreachable address. Discovered hosts can sit in a "Resolving…" state because mDNS guarantees they exist; a typed address has no such guarantee, so without the up-front check a typo or dead host would clutter the list with an entry that never works. The check proves only that the port is open, not that SMB is healthy — protocol/auth failures still surface later through the normal share-listing pipeline once the host is in the list.

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

## The mount URL is built, escaped, and NFC-normalized

`mount.rs::build_smb_mount_url` assembles `smb://host[:port]/share` from percent-encoded halves.
`CFURLCreateWithString` PARSES, it never escapes: hand it a string that isn't already a valid RFC 3986 URL and it
returns NULL, so `café` and `公開` couldn't be mounted at all while `public` on the same host mounted fine. Escaping the
finished URL string instead would eat the scheme, the `//`, the port colon, and the share separator, which is why the
two data halves are escaped separately and the structure assembled around them.

- **The escape set is RFC 3986 `unreserved`** (`urlencoding::encode`: keeps `A-Za-z0-9-._~`). Over-escaping is free
  (a reader decodes back to the same bytes); under-escaping is not — an unescaped `%` in a share named `100%` reads as
  a truncated escape and the URL is rejected outright, and `#` or `?` would silently cut the name short.
- **NFC first, both halves.** macOS hands out decomposed strings while SMB servers store and answer with composed
  ones, so one visible name is two byte strings and two different escapes, and the server only recognizes the NFC one.
  Same normalization `cmdr_smb::volume::paths` applies to every path it sends. The fixture host's `smb.conf` spells
  `café` NFC, matching what a real Samba server stores.
- **An IPv6 literal is the one host that isn't escaped**: it goes in brackets (`smb://[fe80::1]/public`, zone id as
  `%25` per RFC 6874) so its colons can't read as the port separator. mDNS hands us one whenever a host advertises no
  IPv4 address (`mdns_discovery::extract_preferred_ip`).

macOS keeps the ESCAPED form in `statfs`'s `f_mntfromname`, so the round trip closes in
`volumes/DETAILS.md` § "SMB mount sources are percent-escaped". `smb_integration_mount_non_ascii_share` covers the
whole path against the `unicode` fixture host, which is why the Rust integration lane brings that container up.

## One SMB name, two spellings: every use of it folds NFC

The mount URL is one instance of a rule that holds everywhere an SMB server or share name is sent, keyed, or compared.
A name reaches us through two pipes that disagree: `statfs` (and the mount paths under it) spells an accented name
DECOMPOSED, while mDNS, the server's own share list, and the frontend spell it COMPOSED. One visible share is
therefore two byte strings, and every byte-exact use of it is a latent split.

The failure is not theoretical and not cosmetic. On a Synology share named `Régi NAS` the decomposed spelling reached
`TreeConnect` and came back `STATUS_BAD_NETWORK_NAME` while the composed one connected in the same second; the share
sat on the kernel mount and "Connect directly" could never fix it, because that path reads the name from `statfs`
(ERR-ABXW4). The same split, one layer over, hands one share two volume IDs and two Keychain entries.

Every one of these folds NFC, and a new use of a name has to join them:

- **The wire.** `cmdr_smb::SmbConnectionParams::new` folds `server` and `share_name`, which is what both `connect_share`
  calls read (the session and the watcher's own session). ❌ Never build those params by struct literal from a raw
  `statfs` name. Paths under the share are folded separately by `cmdr_smb::volume::paths`.
- **Identity.** `cmdr_fs::volume::ids::smb_volume_id` folds both halves before it case-folds, so a share gets one
  `index-{id}.db`, one set of `lastUsedPaths`, and one `volumeId` however it was registered.
- **Credentials.** `keychain::make_account_name` folds the share half; `server_identity::normalize` folds the server
  half for every `credential_key` / `same_server` answer.
- **Stores and comparisons.** `known_shares::share_key`, `mount::same_share_name`, `mount::same_server_name`.
- **Never the password.** It's bytes the user typed; folding it would change the secret.

Two places deliberately stay byte-exact: `path_volume_id` (the kernel is self-consistent about how it spells a mount
point) and `mount_linux::derive_gvfs_path` (it has to match the path GVFS actually created, which is GVFS's convention
to set, not ours).

## Server-keyed answers are lookups, never maps

`known_shares` answers "what username should this login form pre-fill" with
`get_username_hint(server_name) -> Option<String>`, and "what do we know about this share" with
`get_known_share(server_name, share_name)`. Both take raw names and do the keying HERE.

The alternative is what this used to be: a command returning `HashMap<server_key, username>` for every server at once,
which puts the KEY in the IPC contract. The caller then has to rebuild that key to read its own answer, so the rule
exists twice, in two languages, and only one of them is the real one. It had already drifted. Rust keyed on
`server_name.to_lowercase()` while `NetworkLoginForm.svelte` looked up `host.name.toLowerCase()`, and neither is
`credential_key`, so a server saved as `Naspolya` was invisible to a form opened on `Naspolya._smb._tcp.local`, which
is exactly the case a hint exists for. The failure is silent (no pre-fill, no error), which is why it sat there.

Keying on [`server_identity::credential_key`] is what pairs the name forms, and it is deliberately the SAME identity the
stored password uses: a hint and a password answer the same question ("who did this person sign in as here"), so they
must agree about which server is which. Last match wins, because shares are appended in connect order.

The general rule: a command whose result is keyed by something the caller must reconstruct is a command with the wrong
signature. Take the identifier as an argument and answer for it.

## Every SMB subprocess runs under a deadline

`smbutil view` (macOS) and `smbclient -L` (Linux) are the last thing tried when smb2 can't list a host's shares, and
the share browser's spinner waits on them. Neither has a timeout flag, and neither gives up against a server that
accepts the connection and then goes quiet. `list_shares`'s `timeout_ms` only ever reached the smb2 attempt, so the
CLI fallback had no bound at all.

Both run under `crate::subprocess::output_within` with a 20 s limit (`SMBUTIL_VIEW_LIMIT` / `SMBCLIENT_LIST_LIMIT`),
set well above the ~1 s a working tool takes on a slow LAN: the number only decides how long a dead server is allowed
to look alive.

❌ Don't "fix" a future one with `tokio::time::timeout` around `spawn_blocking`. That releases the caller and leaks
both the child and the blocking-pool thread parked in `wait()`; the pool caps at 512 threads and is shared with every
directory listing in the app. `output_within` bounds a `tokio::process` child with `kill_on_drop`, so expiry ends the
process. Rationale and the stdio trap it closes: the module doc on `subprocess.rs`.

## The direct-connect fallback names its cause

When smb2 can't connect, the share stays on the macOS kernel mount for the session: slower, and without the direct
session's control surface. Both upgrade paths log that through `smb_upgrade::log_direct_connect_failure`, on the
`smb_fallback` target.

It asks `is_auth_error` itself rather than describing the failure with `UpgradeFailure`. **`UpgradeFailure` has no auth
variant** and folds a rejected password into `Unexpected`: it crosses IPC to pick the network-error copy, and the auth
case reaches `CredentialsNeeded` instead. Describing an auth failure with it makes a stale Keychain password read as a
flaky server.

Level follows what happens to the user, since that decides whether anything else will mention it: a share that
silently stays on the kernel mount is a WARN even for auth (nobody will be asked anything), while the manual "Connect
directly" path's auth failure is an INFO (a credentials prompt follows immediately).

## Telling the user about a kernel-mount fallback

The log above answers "why is this share slow" for whoever reads logs. `os_mount_notice.rs` answers it for the person
using the app: `announce_os_mount_fallback` emits `SmbFellBackToOsMount { volume_id, share }`, and the frontend raises
a notice with a "Try connecting directly" button (`src/lib/file-explorer/network/DETAILS.md` § "The OS-mount fallback
notice").

**The module holds the `AppHandle` for it**, stashed from `lib.rs::setup`. It's the only `AppHandle` this corner of the
app needs: a share's session-state transitions go out through the volume host's event seam instead, so the SMB backend
names no `tauri` type at all.

**Only the auto paths speak.** `register_smb_volume` is the fallback that nobody asked for and nothing else announces:
the startup pass over existing mounts and the FSEvents mount watcher both land there. `try_smb_upgrade` (the manual
"Connect directly") returns its failure to the caller, who is a person watching a spinner, so a notice there would say
the same thing twice.

**Once per SERVER per run, not once per share.** Both auto paths call `register_smb_volume` once per MOUNTED SHARE, and
what failed is a property of the connection to the server: a stale password or a sleeping host rejects every share on
it identically. A NAS whose shares all remount at login would raise one notice per share, which is worse than the
silence it replaces.

**The ledger asks `server_identity::same_server`, not a string key.** `statfs` echoes back whichever name form each
mount used, so one NAS arrives as `192.168.1.111` on one mount and `Naspolya._smb._tcp.local` on the next. That's also
why the ledger is a `Vec` rather than a `HashSet`: identity here is an equivalence relation over the live mDNS state,
not a value to hash. Its one weak spot is the same one `same_server` documents — before discovery warms, an IP and a
name look like two servers, so the worst case is two notices rather than one, never a missed one.

**An E2E run never gets here at all.** The notice's one trigger is the auto-upgrade of a mount the app didn't make, and
under E2E every such mount is the developer's own — on this machine, reliably `/Volumes/naspi`. So the startup adopter
returns early (`test_mode::may_adopt_preexisting_network_mounts`, checked in
`file_system::upgrade_existing_smb_mounts` BEFORE the scan), and no E2E run waits on mDNS for a real NAS, reaches for
its Keychain entry, opens a session to it, or raises a toast about it that then fails whichever spec is running. The
FSEvents mount-time path is deliberately NOT gated: a mount that appears DURING a run is the test's own fixture.

**One upgrade at a time per volume, so the notice can't be raised by a loser.** Every path re-checks
`is_already_direct` before connecting, but that alone is a check-then-act: the mount-time and startup paths both looked,
both saw "not direct", and both connected on one mount 20 ms apart. The attempt that failed announced a fallback that
the other attempt's session disproved 55 ms later, and nothing retracts a notice the frontend already has, so the user
was told they were on the slow path while a direct session served their files (ERR-ABXW4). `smb_upgrade`'s
`lock_volume_upgrade` holds a per-volume-id lock across the whole attempt and the re-check happens under it, turning
the sequence into lock-check-act: the second path waits, sees the first one's `Direct` volume, and skips. The redundant
session, auth round-trip, and volume replacement stop happening too. The lock is per volume, not global, so a NAS
remounting every share at login still warms them in parallel.

**A landed direct session clears the server's entry** (`clear_os_mount_notice`, called from both the auto and the
manual install paths). A notice describes a situation, not an event: once the server is off the slow path, the next
genuine regression is worth saying out loud again. Without the clear, one bad startup would mute the notice for the
rest of the run.

## The two SFTP stores, and why neither is a widened SMB one

`sftp_host_keys.rs` holds what this machine TRUSTS (`known-sftp-hosts.json`), `sftp_known_servers.rs` holds what the
user has CONNECTED TO (`known-sftp-servers.json`), and the secret store holds the passwords. Three files because they
answer three different questions and have three different lifetimes: forgetting a server from a list is not revoking its
password, and neither is deciding its identity changed. The commands keep that split (`forget_known_sftp_server`,
`delete_sftp_credentials`, `forget_sftp_host_key`), so the UI can ask exactly what it means.

❗ **A server entry is keyed `(host, port, username)`** — the same triple `cmdr_fs::volume::sftp_volume_id` derives from,
with the same case rules (the host folds, the account doesn't). A drift there files one volume under two entries. A host
key is keyed `(host, port, algorithm)` instead, because a server may hold several key types and present any of them;
that is `crates/cmdr-sftp/DETAILS.md` § "Host-key trust".

❌ **Not a widened `KnownNetworkShare`.** That type carries a `share_name` an SFTP server has no equivalent of, and an
`AuthOptions` that can only say guest-or-credentials — which expresses neither a key file nor an ssh-agent. Bending it
would leave two backends sharing fields that mean different things in each.

**The two per-server switches live in two different places, on purpose.** "Remember the secret" IS the Keychain entry
(`save_sftp_credentials` writes it, `has_sftp_credentials` reads it, `delete_sftp_credentials` clears it), so there is
❌ no second flag anywhere that could disagree with the store — a user who deletes the entry through Keychain Access has
turned the switch off. "Reconnect automatically" is `KnownSftpServer::auto_reconnect`, which is per-server user intent
rather than a fact about a secret, so the saved-server list is its home. ❗ It reads as `true` when a stored entry
doesn't name it: SFTP has always come back on its own, and a missing field must not switch that off under servers saved
before the setting existed.

`update_known_sftp_server` moves both copies: the saved entry, and (through `sftp_volume_wiring::apply_auto_reconnect`)
a volume that happens to be mounted, so the switch takes effect now rather than on the next connect. What the two mean
together, what the backend answers when one is on and can't work, and what a UI shows:
`crates/cmdr-sftp/DETAILS.md` § "The two switches".

`sftp_volume_wiring.rs` is the only path a volume gets registered on, and it does three things in one order: dial
through `cmdr_sftp::connect_sftp_volume` (an ABANDONED connect leaves the server nothing, because the SFTP hello's
teardown runs from a guard's `Drop`: `crates/cmdr-sftp/DETAILS.md` § "2. An abandoned `Sftp::new`"), register while
retiring any predecessor with `on_superseded`, and remember the server. `disconnect`
downcasts through `Volume::as_any` rather than guessing at the id's shape, then DROPS the session — ❌ never
`Sftp::close()`, which hangs forever over an SSH channel.

### The attempt table, and why the id is the caller's

`connect_and_register` takes an `attempt_id`, files a `CancellationToken` under it in the module's `ATTEMPTS` map, and
`cancel_connect(attempt_id)` is what a dialog's cancel button reaches. What the token then does inside the dial is the
backend's, and `crates/cmdr-sftp/DETAILS.md` § "2b. Calling a connect off" owns it.

❗ **The id comes from the caller, and there is no version where the backend hands one back.** `connect_sftp_volume`
doesn't answer until the connect is over — up to 30 s — so an id it returned would arrive at exactly the moment a
cancel stopped being useful. A second command to allocate one first would be a round trip plus a state to leak whenever
the connect never followed.

Two details keep the table honest:

- **An RAII `AttemptGuard` removes the entry**, however the connect ends. A connect leaves through eight arms, and the
  one that forgot would be a token nobody ever collects.
- **A serial beside each token**, so a repeated id can't strand one: the guard only takes its OWN entry out, and a
  second connect under the same id stays cancelable.

### The WebDAV twin

WebDAV has one store, not two: `webdav_known_servers.rs` holds what the user has connected to (`webdav_known_servers.json`,
keyed by the `(host, port, username)` triple `cmdr_fs::volume::webdav_volume_id` derives from) and the secret store holds
the passwords; there is no trusted-host file because TLS trust comes from the system roots (a self-signed certificate
answers `certificate_untrusted`, and pinning is a follow-up in `docs/specs/webdav-backend-follow-ups.md`).
`webdav_volume_wiring.rs` is the same three steps in the same order (dial, register while retiring the incumbent via
`on_superseded`, remember) with the same caller-owned attempt table. The connection states, the one unattended re-probe,
and every connect outcome: `crates/cmdr-webdav/DETAILS.md` § "Connecting from the frontend".

❗ **`cancel_connect` on an id nobody is running answers `false`** rather than raising: a click landing just after a
connect finished is ordinary. And a `Cancelled` outcome never reaches `register` or `remember`, so a cancelled connect
leaves no volume, no saved server, and no secret.

## The one edge that must not come back

`network/` and the SMB backend (`crates/cmdr-smb/src/volume/`) sat in a single nine-module dependency cycle for a
long time, and the edge that closed it was invisible to grep: no line under `network/` named `backends::smb`. It was an
`impl From<ConnectionState> for network::VolumeConnection` living in `crates/cmdr-smb/src/volume/state.rs`. `cargo-modules` attributes an impl
to the module that defines the type it PRODUCES, so the edge printed as `network → backends::smb::state` and looked
like `network/` reaching into the backend.

The cut: the backend converts into `cmdr_fs::volume::host::events::VolumeConnection`, the backend-facing enum every
connecting backend reports in, and `events::volume_mapping` widens that into the wire enum for the frontend. That
adapter already existed for backends on a `VolumeHost`; SMB now shares it instead of hand-rolling a second emit. With
the edge gone, `network` is in no cycle but its own parent ↔ `mdns_discovery` pair, and the backend's component is six
modules of parent ↔ child.

**So: a type in `network/` must never be constructible from a backend type.** The direction that stays legal is the
backend naming `cmdr_fs` and `cmdr_smb` types, plus `network/` calling into the backend explicitly the way
`smb_upgrade.rs` does. The trap and its four siblings are catalogued in `scripts/check/checks/DETAILS.md` § "Rust module
cycles"; re-measure there before trusting any number.

## Gotchas

- **Don't hold mutex during DNS resolution**: `get_host_for_resolution` / `update_host_resolution` extract host info and release the mutex before blocking DNS, then re-acquire to update. Holding the mutex across network calls risks deadlock.
- **Auth mode is a guess**: `GuestAllowed` means "guest worked, creds might also work." `CredsRequired` means "guest failed, must have creds." Can't detect guest-only vs guest-or-creds without trying both.
- **NetFS error 17 (EEXIST) is success** (macOS): Share already mounted. Return existing mount path, set `already_mounted: true`. Not an error.
- **mDNS service type must include `.local.`**: `mdns-sd` requires full form `"_smb._tcp.local."` (trailing dot). Without it, browse() fails silently.
- **Account name is keyed by server identity, not the raw string**: `make_account_name` runs the server through `server_identity::credential_key` (lowercase + strip the mDNS service suffix / `.local` down to the bare instance name), so `Naspolya`, `naspolya.local`, and `Naspolya._smb._tcp.local` all key the same entry. Without this the frontend saved under the mDNS instance name while the OS-mount upgrade path looked up by the `statfs` service name, so a just-saved password was never found on the next connect (the picker kept showing the `os_mount` dot and re-prompted). IP literals have no bare form and pass through unchanged.
- **Linux `gio mount` requires GVFS**: The `gvfs-smb` package must be installed. Standard on Ubuntu/Fedora GNOME desktops. KDE desktops may need it explicitly.
- **macOS smbutil and NetFSMountURLSync fail with loopback IP + non-standard port**: `//127.0.0.1:10480` gives "Broken pipe", but `//localhost:10480` works. `build_smbutil_url` and `NetworkMountView.svelte` both fall back to hostname when IP is `127.0.0.1` or `::1`. This matters for E2E testing against Docker containers on localhost.
- **Mount URL must include port when non-standard**: `mount_share_sync` builds `smb://server:port/share` for non-445 ports. The port is passed as a separate parameter through `mount_share` → `mount_share_sync`, not embedded in the server string (embedding it would cause `cmdr_smb::build_smb_addr` to double the port: `localhost:10480:10480`). `SmbMountInfo.port` extracts the port from `statfs` mount source for upgrade paths.
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
