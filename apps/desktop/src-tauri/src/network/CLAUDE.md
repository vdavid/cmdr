# Network SMB support

Discover, browse, and mount SMB network shares on macOS and Linux. Discovery via pure-Rust mDNS; share listing via the
`smb2` crate with a `smbutil`/`smbclient` CLI fallback; mounting via `NetFSMountURLSync` (macOS) / `gio mount` (Linux).

Frontend counterpart: [`apps/desktop/src/lib/file-explorer/network/CLAUDE.md`](../../../src/lib/file-explorer/network/CLAUDE.md).

## Module map

- Discovery + servers: `mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs` (E2E, `smb-e2e` feature only).
- Share listing: `smb_client.rs` (orchestrates guest→keychain→prompt auth), `smb_connection.rs`, `smb_cache.rs` (30s
  TTL), `smb_smbutil.rs` / `smb_smbclient.rs` (CLI fallback), `smb_util.rs` (`classify_error`), `smb_upgrade.rs`.
- Mount/auth/state: `mount.rs` / `mount_linux.rs`, `keychain.rs` (delegates to `crate::secrets`), `known_shares.rs`,
  `server_identity.rs`.

## Must-knows

- **Credentials never go into argv.** `smbclient` gets creds via a 0o600 `-A <file>` (never `-U user%pass`); `gio mount`
  via child stdin (never `sh -c "echo PASS | …"`); Cmdr never shells out to `smbutil` with an explicit password
  (`build_smbutil_url` only builds passwordless `//host` URLs). `ps aux` / `/proc/<pid>/cmdline` must never see a
  password. Don't reintroduce a URL-embedded or argv-passed password on any path.
- **Compare servers by identity, never by string.** Use `server_identity::same_server` / `same_server_live` /
  `credential_key`. `statfs` may report `Naspolya._smb._tcp.local` while we mount by `192.168.1.111`; a string compare
  splits one NAS into two, breaks session reuse, forces a duplicate mount, and (for creds) saves under one key but looks
  up under another so a just-saved password is never found.
- **mDNS is gated, not unconditional.** Startup fires mDNS only if `network.enabled && (firstTriggerDone || smb-e2e)`.
  On a fresh install `firstTriggerDone == false`, so the macOS "find devices on local networks" prompt holds until the
  user takes a network action (which calls `ensure_network_discovery_started` and flips `firstTriggerDone`). The runtime
  mirror is `network::NETWORK_ENABLED` (`AtomicBool`); BE upgrade paths check `is_network_enabled()` before mDNS.
- **Every NetFS mount sets `UIOption = NoUI`** (`open_option_entries` in `mount.rs`). Without it, NetFS hands auth
  failures to NetAuthAgent even with explicit creds: a system dialog pops over Cmdr, blocks the call, and returns
  `kNetAuthErrorInternal` (-6600) on dismiss. `NoUI` makes failures return as typed codes so the frontend renders its
  own login form.
- **`register_replacing_predecessor` (in `smb_upgrade.rs`) must run on every re-register**, not a bare overwrite. It
  looks up the predecessor, calls `on_unmount` on it (cancels the old watcher, drops the old smb2 session), then
  registers the new volume, then emits `volumes-changed`. A bare overwrite leaves the old watcher's lifetime
  non-deterministic and lets an in-flight reconnect install a session into a volume no longer in the manager.
  **Gotcha**: it wraps `on_unmount` in `spawn_blocking` because `on_unmount` uses `blocking_*` locks and we're async;
  a direct call panics ("cannot block_on within a runtime"). Don't switch back to a direct call.
- **Strip `.local` from the addr for smb2** (`build_addr` in `smb_connection.rs`): `smb2` puts `server_name` into UNC
  paths and some servers reject `foo.local` in `\\foo.local\IPC$`. Always pass the resolved IP from mDNS when available.
- **All three SMB-upgrade paths share resolution.** Startup (`file_system::upgrade_existing_smb_mounts`) and mount-time
  (`volumes::watcher::try_upgrade_smb_mount`) both go through `smb_upgrade::resolve_and_register_smb_volume`; manual
  "Connect directly" stays separate (it surfaces `CredentialsNeeded`) but uses the same
  `resolve_ip_to_hostname_with_wait` + `get_keychain_password` pair. Don't let the resolver choice drift: the startup
  path once used the one-shot resolver, missed hostname-keyed creds, and fell back to guest → `STATUS_LOGON_FAILURE`.
- **A refused/unreachable TCP connect is NOT a protocol error.** `classify_error` maps those io kinds to
  `HostUnreachable` so an offline server skips the CLI fallback (the dead port refuses any client) instead of logging a
  fallback warn.
- **`ShareListError` uses `#[serde(tag = "type")]` with struct variants.** Add new variants in struct syntax, not tuple,
  to keep the flat JSON shape.
- **macOS loopback IP + non-standard port fails**: `//127.0.0.1:10480` gives "Broken pipe"; `//localhost:10480` works.
  `build_smbutil_url` and `NetworkMountView.svelte` fall back to hostname for `127.0.0.1` / `::1` (E2E vs Docker).
- **Pass the port as a separate parameter, never embedded in the server string**: embedding it makes `build_smb_addr`
  double it (`localhost:10480:10480`).
- **Don't hold a mutex across DNS resolution / network calls**: extract host info, release the lock, resolve, re-acquire
  (deadlock risk otherwise).
- **mDNS service type needs the trailing dot**: `mdns-sd` requires `"_smb._tcp.local."` or `browse()` fails silently.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
