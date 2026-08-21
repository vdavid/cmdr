# Network SMB support

SMB's app-side half: mDNS discovery, `smb2` share listing behind an `smbutil`/`smbclient` CLI fallback, and mounting
via `NetFSMountURLSync` (macOS) / `gio mount` (Linux). The protocol layer under it is `crates/cmdr-smb/`, whose
`DETAILS.md` runs the boundary: what the protocol and its own types can answer belongs there.

Frontend: `apps/desktop/src/lib/file-explorer/network/CLAUDE.md`. Auth-flow
background: `docs/notes/smb-auth-flow-redesign.md`.

## Module map

- Discovery + servers: `mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs` (`smb-e2e` only).
- Share listing: `smb_client.rs` (guest→keychain→prompt), `smb_cache.rs`, `smb_smbutil.rs` / `smb_smbclient.rs` (CLI
  fallback), `smb_upgrade.rs`.
- Mount/auth/state: `mount.rs` / `mount_linux.rs`, `keychain.rs`, `known_shares.rs`, `server_identity.rs`,
  `credential_store.rs` (`KeychainCredentials`, the `CredentialStore` seam), `os_mount_notice.rs` (the fallback
  notice's once-per-server ledger).

## Must-knows

- **Credentials never go into argv** (never `ps aux` / `/proc/<pid>/cmdline`): `smbclient` via a 0o600 `-A` file, `gio
  mount` via child stdin, `build_smbutil_url` only passwordless `//host` URLs. Never a URL-embedded or argv password.
- **Compare servers by identity, never string** (`server_identity::same_server*` / `credential_key`): `statfs` may say
  `Naspolya._smb._tcp.local` where we mount `192.168.1.111`; a string compare splits one NAS in two (breaks session
  reuse, forces a dup mount, mis-keys creds).
- **mDNS is gated**: startup fires it only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install
  holds the macOS "find devices" prompt until `ensure_network_discovery_started`. Check `is_network_enabled()` first.
- **Every NetFS mount sets `UIOption = NoUI`**: without it NetFS routes auth failures to NetAuthAgent (a system dialog
  pops, blocks, returns -6600 on dismiss) even with explicit creds. `NoUI` returns typed codes for our own form.
- **Re-register via `register_replacing_predecessor`, never a bare overwrite**: it retires the displaced volume via
  `Volume::on_superseded`, then emits `volumes-changed`. ❌ NOT `on_unmount` — a replace isn't a disconnect, and the
  predecessor's smb2 session must stay up for the transfers still holding it
  (`file_system/volume/backends/DETAILS.md` § "Supersede vs. unmount"). Call it inside `spawn_blocking`: the trait
  DEFAULT falls through to `on_unmount`, which panics on a `block_on` within a runtime.
- **A direct-session install auto-resumes the drive index**: `register_smb_volume` / `try_smb_upgrade` call
  `indexing::resume_smb_index_if_enabled` after registering (no-op unless enabled); don't drop it.
- **All three upgrade paths share resolution**: the two auto paths route through `resolve_and_register_smb_volume`;
  manual "Connect directly" stays separate but reuses `resolve_ip_to_hostname_with_wait` + `get_keychain_password`.
  Don't drift to the one-shot resolver (misses hostname-keyed creds → guest → `STATUS_LOGON_FAILURE`).
- **Decide at ACT time, not trigger time**: every path waits 1.5–16.5 s for mDNS, so re-check `is_already_direct` right
  before connecting, and let the startup pass RE-SCAN after its wait (its pre-scan only gates the Local-Network
  prompt). One `UpgradePass` at a time. Acting on a stale decision replaced a healthy volume three times in 15 s, once
  mid-copy. `file_system/volume/backends/DETAILS.md` § "Every upgrade decides at ACT time".
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`: `smbutil`/`smbclient` never give up
  on a server gone quiet, and a spinner waits on them. ❌ Not a bare `Command::output()`, ❌ not `tokio::time::timeout`
  around `spawn_blocking` (leaks the child AND the pool thread).
- **Two `cmdr_smb` classifier calls carry behavior.** `log_direct_connect_failure` (target `smb_fallback`) asks
  `is_auth_error` itself, since `UpgradeFailure` has no auth variant and would print `Unexpected` for a rejected
  password; `classify_error` maps a refused/unreachable connect to `HostUnreachable`, so an offline server skips the
  CLI fallback.
- **A `network` type must not be constructible from a backend type**: a `From` impl silently welds the two into one
  cycle. § "The one edge that must not come back".

Smaller gotchas, each covered in `DETAILS.md` § Gotchas: pass the port as a separate param (embedding doubles it);
loopback IP + non-standard port fails (fall back to hostname); don't hold a mutex across DNS/network calls; mDNS service
type needs the trailing dot `"_smb._tcp.local."`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
