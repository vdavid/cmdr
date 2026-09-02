# Network support

Mostly SMB's app-side half: mDNS discovery, `smb2` share listing behind an `smbutil`/`smbclient` CLI fallback, and
mounting via `NetFSMountURLSync` (macOS) / `gio mount` (Linux). The protocol layer under it is `crates/cmdr-smb/`,
whose `DETAILS.md` runs the boundary.

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
- WebDAV: `webdav_known_servers.rs` + `webdav_volume_wiring.rs`, the same three steps. `commands/webdav.rs`.
- Shared by both: `connect_wiring.rs` (each backend's OWN cancelable `AttemptTable`, plus
  `install_retiring_incumbent`), `server_list_file.rs` (a saved list's durable read/write).

## Must-knows

- **SFTP's and WebDAV's two per-server switches are independent**: the Keychain entry IS "remember the secret", and
  `auto_reconnect` ❗ defaults to ON. `crates/cmdr-sftp/DETAILS.md`.
- **SFTP and WebDAV key everything by `(host, port, username)`** (volume id, saved server, secret store); ❌ never the
  host alone, or two accounts share a secret and a reconnect retries the wrong one. A trusted host KEY is keyed
  `(host, port, algorithm)`. ❌ Never write `~/.ssh/known_hosts`. `crates/cmdr-sftp/DETAILS.md`.
- **Credentials never go into argv** (`ps aux` / `/proc/<pid>/cmdline`): `smbclient` via a 0o600 `-A` file, `gio mount`
  via child stdin, `build_smbutil_url` only passwordless `//host` URLs.
- **Compare servers by identity, never string** (`server_identity::same_server*` / `credential_key`): `statfs` may say
  `Naspolya._smb._tcp.local` where we mount `192.168.1.111`, and a string compare splits one NAS in two. ❌ Never ship a
  keyed MAP over IPC either; answer a lookup (`get_username_hint`, `get_known_share`). `DETAILS.md`.
- **NFC-fold every SMB name you send, key, or compare** (never the password): `statfs` spells accented names
  decomposed, mDNS and the server composed. Unfolded, `TreeConnect` answers `STATUS_BAD_NETWORK_NAME` and one share
  gets two volume IDs and two Keychain entries. `DETAILS.md`.
- **mDNS is gated**: startup fires only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install holds
  the macOS "find devices" prompt until `ensure_network_discovery_started`.
- **Every NetFS mount sets `UIOption = NoUI`**: without it NetFS routes auth failures to NetAuthAgent (a dialog pops,
  blocks, returns -6600 on dismiss) even with explicit creds.
- **Re-register via `register_replacing_predecessor` (SMB) or `connect_wiring::install_retiring_incumbent` (SFTP,
  WebDAV), never a bare overwrite**: both retire the displaced volume via `on_superseded`, ❌ not `on_unmount`, which
  cuts the session out from under in-flight transfers.
- **A direct-session install auto-resumes the drive index.**
- **All three upgrade paths share one resolution** (`resolve_ip_to_hostname_with_wait` + `get_keychain_password`); the
  one-shot resolver misses hostname-keyed creds → guest → `LOGON_FAILURE`.
- **Decide at ACT time, under the lock**: every path waits 1.5–16.5 s for mDNS, so re-check `is_already_direct` right
  before connecting, holding `lock_volume_upgrade` so two paths can't both pass. A stale decision once replaced a
  healthy volume three times in 15 s, one mid-copy. `file_system/volume/backends/DETAILS.md` § "Every upgrade decides
  at ACT time".
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`: `smbutil`/`smbclient` never give up
  on a quiet server. ❌ Not a bare `Command::output()`, ❌ not `tokio::time::timeout` around `spawn_blocking`.
- **Both `cmdr_smb` classifier calls carry behavior, not just wording**: `is_auth_error` inside
  `log_direct_connect_failure`, and `classify_error`, deciding whether an offline server skips the CLI fallback.
- **A `network` type must not be constructible from a backend type**: a `From` impl silently welds the two into one
  cycle. `DETAILS.md`.

Architecture, flows, decisions, and smaller gotchas (ports, loopback addresses, the mDNS trailing dot): `DETAILS.md`.
Read it before any non-trivial work here.
