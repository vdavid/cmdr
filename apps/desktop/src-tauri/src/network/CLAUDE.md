# Network SMB support

Discover, browse, and mount SMB shares on macOS + Linux: pure-Rust mDNS discovery, `smb2` share listing with an
`smbutil`/`smbclient` CLI fallback, mounting via `NetFSMountURLSync` (macOS) / `gio mount` (Linux).

Frontend: [`src/lib/file-explorer/network/CLAUDE.md`](../../../src/lib/file-explorer/network/CLAUDE.md). Auth-flow
background: `docs/notes/smb-auth-flow-redesign.md`.

## Module map

- Discovery + servers: `mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs` (`smb-e2e` only).
- Share listing: `smb_client.rs` (guest→keychain→prompt), `smb_connection.rs`, `smb_cache.rs`, `smb_smbutil.rs` /
  `smb_smbclient.rs` (CLI fallback), `smb_util.rs`, `smb_upgrade.rs`.
- Mount/auth/state: `mount.rs` / `mount_linux.rs`, `keychain.rs`, `known_shares.rs`, `server_identity.rs`.

## Must-knows

- **Credentials never go into argv** (never `ps aux` / `/proc/<pid>/cmdline`): `smbclient` via a 0o600 `-A` file, `gio
  mount` via child stdin, `build_smbutil_url` only passwordless `//host` URLs. Never a URL-embedded or argv password.
- **Compare servers by identity, never string** (`server_identity::same_server*` / `credential_key`): `statfs` may say
  `Naspolya._smb._tcp.local` where we mount `192.168.1.111`; a string compare splits one NAS in two (breaks session
  reuse, forces a dup mount, mis-keys creds).
- **mDNS is gated**: startup fires it only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install
  holds the macOS "find devices" prompt until a network action calls `ensure_network_discovery_started`. Runtime mirror
  `NETWORK_ENABLED`; check `is_network_enabled()` before mDNS.
- **Every NetFS mount sets `UIOption = NoUI`**: without it NetFS routes auth failures to NetAuthAgent (a system dialog
  pops, blocks, returns -6600 on dismiss) even with explicit creds. `NoUI` returns typed codes for our own form.
- **Re-register via `register_replacing_predecessor`, never a bare overwrite**: it cleans up the displaced volume
  (`on_unmount`, cancel old watcher, drop old session) then emits `volumes-changed`. A bare overwrite leaves the old
  watcher's lifetime non-deterministic and lets an in-flight reconnect install a session into an evicted volume. Keeps
  `on_unmount` in `spawn_blocking` (a direct call panics "cannot block_on within a runtime").
- **A direct-session install auto-resumes the drive index**: `register_smb_volume` / `try_smb_upgrade` call
  `indexing::resume_smb_index_if_enabled` after registering (no-op unless enabled); don't drop it.
- **All three upgrade paths share resolution**: the two auto paths route through `resolve_and_register_smb_volume`;
  manual "Connect directly" stays separate but reuses `resolve_ip_to_hostname_with_wait` + `get_keychain_password`.
  Don't drift to the one-shot resolver (misses hostname-keyed creds → guest → `STATUS_LOGON_FAILURE`).
- **A refused/unreachable TCP connect is NOT a protocol error**: `classify_error` maps those io kinds to
  `HostUnreachable`, so an offline server skips the CLI fallback.

Smaller gotchas, each covered in [DETAILS.md](DETAILS.md) § Gotchas: strip `.local` from the smb2 addr; pass the port as
a separate param (embedding doubles it); loopback IP + non-standard port fails (fall back to hostname); don't hold a
mutex across DNS/network calls; mDNS service type needs the trailing dot `"_smb._tcp.local."`; `ShareListError` uses
`#[serde(tag = "type")]` struct variants.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
