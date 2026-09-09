# Network support

The app-side half of SMB, plus the saved-server stores and connect wiring SFTP and WebDAV share. Protocol layers:
`crates/cmdr-smb/`, `crates/cmdr-sftp/`, `crates/cmdr-webdav/`. Frontend: `apps/desktop/src/lib/servers/CLAUDE.md`
and `apps/desktop/src/lib/file-explorer/network/CLAUDE.md`.

## Module map

Three families: SMB (discovery, share listing with CLI fallbacks, mounting, OS-mount → direct upgrade), SFTP and
WebDAV (host keys, a saved-server store each, a `*_volume_wiring.rs` that dials and registers), and their shared seams
(`connect_wiring.rs`, `server_list_file.rs`, `one_shot_credentials.rs`, `credential_store.rs`).

## Must-knows

- **The per-server switches are independent**: the Keychain entry IS "remember the secret", `auto_reconnect` defaults
  ON, and `pinned` defaults OFF and survives a reconnect (`crates/cmdr-sftp/DETAILS.md`).
- **A secret reaches a dial through `SecretOffer` or the store, ❌ never a `password` param.** `remember: false` wraps
  the store for one attempt and drops it, so nothing is written (`one_shot_credentials.rs`).
- **Key everything by `(host, port, username)`** (volume id, saved server, secret store); ❌ never the host alone, or
  two accounts share a secret. A trusted host KEY keys `(host, port, algorithm)`. ❌ Never write `~/.ssh/known_hosts`.
- **Credentials never go into argv** (`ps aux` reads it): `smbclient` via a 0o600 `-A` file, `gio mount` via child
  stdin, `build_smbutil_url` only passwordless `//host` URLs.
- **Compare servers by identity, ❌ never by string** (`server_identity::same_server*`): `statfs` says
  `Naspolya._smb._tcp.local` where we mount `192.168.1.111`, so a string compare splits one NAS in two. ❌ Never ship a
  keyed MAP over IPC: answer a lookup.
- **NFC-fold every SMB name you send, key, or compare** (❌ never the password): `statfs` spells accented names
  decomposed, and unfolded, `TreeConnect` answers `STATUS_BAD_NETWORK_NAME`.
- **mDNS is gated**: startup fires only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install holds
  the macOS "find devices" prompt until `ensure_network_discovery_started`.
- **Every NetFS mount sets `UIOption = NoUI`**: without it NetFS routes auth failures to NetAuthAgent, which pops a
  dialog, blocks the mount, and returns -6600 on dismiss.
- **Re-register via `register_replacing_predecessor` (SMB) or `install_retiring_incumbent` (SFTP, WebDAV), ❌ never a
  bare overwrite**: both retire the displaced volume via `on_superseded`, ❌ not `on_unmount`, which cuts the session
  out from under in-flight transfers.
- **Decide at ACT time, under the lock**: re-check `is_already_direct` right before connecting, holding
  `lock_volume_upgrade`. A stale decision once replaced a healthy volume three times in 15 s, one mid-copy.
- **A mount's volume ID and its anchor inside the share come off ONE `statfs` row** (`identity_from_statfs`): the caller
  knows which share it ASKED for, only the mount knows where the OS put it (a DFS referral lands a second mount a
  directory inside the namespace root). ❌ Never derive the two apart, or a mount gets keyed as one share and addressed
  as another (ERR-48RZX).
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`: `smbutil` / `smbclient` never give
  up on a quiet server. ❌ Not a bare `Command::output()`, ❌ not a `timeout` around `spawn_blocking`.
- **❌ A `network` type must not be constructible from a backend type**: a `From` impl silently welds the two into one
  module cycle.

Architecture, flows, decisions, and the smaller gotchas (ports, loopback addresses, the mDNS trailing dot):
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
