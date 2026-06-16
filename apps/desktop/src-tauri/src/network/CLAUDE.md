# Network SMB support

Discover, browse, and mount SMB network shares on macOS and Linux. Discovery via pure-Rust mDNS; share listing via
`smb2` with a `smbutil`/`smbclient` CLI fallback; mounting via `NetFSMountURLSync` (macOS) / `gio mount` (Linux).

Frontend counterpart: [`src/lib/file-explorer/network/CLAUDE.md`](../../../src/lib/file-explorer/network/CLAUDE.md).
Auth-flow background: `docs/notes/smb-auth-flow-redesign.md`.

## Module map

- Discovery + servers: `mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs` (`smb-e2e` feature only).
- Share listing: `smb_client.rs` (guest→keychain→prompt auth), `smb_connection.rs`, `smb_cache.rs`, `smb_smbutil.rs` /
  `smb_smbclient.rs` (CLI fallback), `smb_util.rs`, `smb_upgrade.rs`.
- Mount/auth/state: `mount.rs` / `mount_linux.rs`, `keychain.rs`, `known_shares.rs`, `server_identity.rs`.

## Must-knows

- **Credentials never go into argv** (must never reach `ps aux` / `/proc/<pid>/cmdline`). `smbclient` gets creds via a
  0o600 `-A <file>` (never `-U user%pass`); `gio mount` via child stdin (never `sh -c "echo PASS | …"`);
  `build_smbutil_url` only builds passwordless `//host` URLs. Don't reintroduce a URL-embedded or argv password.
- **Compare servers by identity, never by string.** Use `server_identity::same_server` / `same_server_live` /
  `credential_key`. `statfs` may report `Naspolya._smb._tcp.local` while we mount by `192.168.1.111`; a string compare
  splits one NAS in two, breaking session reuse, forcing a duplicate mount, and keying creds inconsistently.
- **mDNS is gated, not unconditional.** Startup fires mDNS only if `network.enabled && (firstTriggerDone || smb-e2e)`, so
  on a fresh install the macOS "find devices on local networks" prompt holds until a network action calls
  `ensure_network_discovery_started` (flipping `firstTriggerDone`). Runtime mirror: `NETWORK_ENABLED` (`AtomicBool`),
  read via `is_network_enabled()`; BE upgrade paths check it before mDNS.
- **Every NetFS mount sets `UIOption = NoUI`** (`open_option_entries` in `mount.rs`). Without it, NetFS routes auth
  failures to NetAuthAgent even with explicit creds: a system dialog pops over Cmdr, blocks the call, and returns
  `kNetAuthErrorInternal` (-6600) on dismiss. `NoUI` returns failures as typed codes for the frontend's own form.
- **`register_replacing_predecessor` (in `smb_upgrade.rs`) must run on every re-register**, not a bare overwrite: it
  cleans up the displaced volume (`on_unmount`, cancel the old watcher, drop the old smb2 session) before registering the
  new one and emitting `volumes-changed`. A bare overwrite leaves the old watcher's lifetime non-deterministic and lets
  an in-flight reconnect install a session into an evicted volume. It wraps `on_unmount` in `spawn_blocking` (the
  `blocking_*` locks panic a direct call with "cannot block_on within a runtime"); don't revert to a direct call.
- **Strip `.local` from the addr for smb2** (`build_addr`): some servers reject `foo.local` in the UNC path smb2 builds
  from `server_name`. Pass the resolved mDNS IP when available.
- **All three SMB-upgrade paths share resolution.** The two auto paths (startup `upgrade_existing_smb_mounts`, mount-time
  `try_upgrade_smb_mount`) go through `resolve_and_register_smb_volume`; manual "Connect directly" stays separate (it
  surfaces `CredentialsNeeded`) but reuses `resolve_ip_to_hostname_with_wait` + `get_keychain_password`. Don't drift to
  the one-shot resolver: it misses hostname-keyed creds and falls back to guest → `STATUS_LOGON_FAILURE`.
- **A refused/unreachable TCP connect is NOT a protocol error.** `classify_error` maps those io kinds to
  `HostUnreachable` so an offline server skips the CLI fallback (the dead port refuses any client).
- **`ShareListError` uses `#[serde(tag = "type")]` with struct variants.** Add new variants in struct syntax, not tuple,
  for the flat JSON shape.
- **macOS loopback IP + non-standard port fails**: `//127.0.0.1:10480` gives "Broken pipe", `//localhost:10480` works.
  `build_smbutil_url` and `NetworkMountView.svelte` fall back to hostname for `127.0.0.1` / `::1` (E2E vs Docker).
- **Pass the port as a separate parameter, never embedded in the server string** (embedding makes `build_smb_addr`
  double it: `localhost:10480:10480`).
- **Don't hold a mutex across DNS resolution / network calls** (deadlock risk): extract host info, release, resolve,
  re-acquire.
- **mDNS service type needs the trailing dot**: `mdns-sd` needs `"_smb._tcp.local."` or `browse()` fails silently.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
