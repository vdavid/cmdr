# Network support

Mostly SMB's app-side half: mDNS discovery, `smb2` share listing behind an `smbutil`/`smbclient` CLI fallback, and
mounting via `NetFSMountURLSync` (macOS) / `gio mount` (Linux). The protocol layer under it is `crates/cmdr-smb/`, whose
`DETAILS.md` runs the boundary: what the protocol and its types answer belongs there.

Frontend: `apps/desktop/src/lib/file-explorer/network/CLAUDE.md`. Auth-flow background:
`docs/notes/smb-auth-flow-redesign.md`.

## Module map

- Discovery + servers: `mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs` (`smb-e2e` only).
- Share listing: `smb_client.rs` (guest→keychain→prompt), `smb_cache.rs`, `smb_smbutil.rs` / `smb_smbclient.rs` (CLI
  fallback), `smb_upgrade.rs`.
- Mount/auth/state: `mount.rs` / `mount_linux.rs` (their `mount_share` wrappers share one timeout via
  `mod.rs::mount_within`), `keychain.rs`, `known_shares.rs`, `server_identity.rs`,
  `credential_store.rs` (`KeychainCredentials`, the `CredentialStore` seam), `os_mount_notice.rs` (the fallback
  notice: its once-per-server ledger AND the `AppHandle` it emits through).
- SFTP: `sftp_host_keys.rs` (the `HostKeys` seam over `known-sftp-hosts.json`), `sftp_known_servers.rs` (the
  saved-server list), `sftp_volume_wiring.rs` (dial → register → remember → disconnect). Commands: `commands/sftp.rs`.

## Must-knows

- **SFTP's two per-server switches are independent**: the Keychain entry IS "remember the secret", and
  `auto_reconnect` ❗ defaults to ON. `crates/cmdr-sftp/DETAILS.md` § "The two switches".
- **SFTP keys everything by `(host, port, username)`** — volume id, saved server, and the secret store's
  `service = "host:port"` + `scope = Some(username)`; ❌ never the host alone, or two accounts share a secret and a
  reconnect retries the wrong one into a lockout. A trusted host KEY is keyed `(host, port, algorithm)` instead, and
  re-recording replaces. ❌ Never write `~/.ssh/known_hosts`. Both, plus the frontend contract:
  `crates/cmdr-sftp/DETAILS.md`.
- **Credentials never go into argv** (never `ps aux` / `/proc/<pid>/cmdline`): `smbclient` via a 0o600 `-A` file, `gio
  mount` via child stdin, `build_smbutil_url` only passwordless `//host` URLs.
- **Compare servers by identity, never string** (`server_identity::same_server*` / `credential_key`): `statfs` may say
  `Naspolya._smb._tcp.local` where we mount `192.168.1.111`, and a string compare splits one NAS in two (breaks session
  reuse, forces a dup mount, mis-keys creds).
- **mDNS is gated**: startup fires it only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install
  holds the macOS "find devices" prompt until `ensure_network_discovery_started`. Check `is_network_enabled()`.
- **Every NetFS mount sets `UIOption = NoUI`**: without it NetFS routes auth failures to NetAuthAgent (a dialog pops,
  blocks, returns -6600 on dismiss) even with explicit creds. `NoUI` returns typed codes for our form.
- **Re-register via `register_replacing_predecessor` (SMB) or `sftp_volume_wiring::connect_and_register` (SFTP), never a
  bare overwrite**: both retire the displaced volume via `on_superseded`, ❌ not `on_unmount`, which would cut the
  session out from under in-flight transfers.
- **A direct-session install auto-resumes the drive index**: `register_smb_volume` / `try_smb_upgrade` call
  `indexing::resume_smb_index_if_enabled`.
- **All three upgrade paths share one resolution** (`resolve_ip_to_hostname_with_wait` + `get_keychain_password`); the
  one-shot resolver misses hostname-keyed creds → guest → `STATUS_LOGON_FAILURE`.
- **Decide at ACT time, not trigger time**: every path waits 1.5–16.5 s for mDNS, so re-check `is_already_direct` right
  before connecting, and let the startup pass RE-SCAN after its wait. One `UpgradePass` at a time; a stale decision
  once replaced a healthy volume three times in 15 s, one of them mid-copy.
  `file_system/volume/backends/DETAILS.md` § "Every upgrade decides at ACT time".
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`: `smbutil`/`smbclient` never give up
  on a server gone quiet. ❌ Not a bare `Command::output()`, ❌ not `tokio::time::timeout` around `spawn_blocking`
  (leaks the child AND the pool thread).
- **Both `cmdr_smb` classifier calls carry behavior, not just wording**: `is_auth_error` inside
  `log_direct_connect_failure`, and `classify_error`, which decides whether an offline server skips the CLI
  fallback.
- **A `network` type must not be constructible from a backend type**: a `From` impl silently welds the two into one
  cycle. `DETAILS.md` § "The one edge that must not come back".

Architecture, flows, decisions, and the smaller gotchas (port handling, loopback addresses, the mDNS trailing dot):
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
