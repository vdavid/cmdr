# Network support

The app-side half of SMB, plus the saved-server stores and connect wiring SFTP and WebDAV share. Protocol layers:
`crates/cmdr-smb/`, `crates/cmdr-sftp/`, `crates/cmdr-webdav/`. Frontend: `apps/desktop/src/lib/servers/CLAUDE.md`
and `apps/desktop/src/lib/file-explorer/network/CLAUDE.md`.

## Module map

SMB (discovery, share listing with CLI fallbacks, mounting through `mod.rs::mount_share`, OS-mount → direct upgrade),
SFTP and WebDAV (host keys, a saved-server store each, a `*_volume_wiring.rs` that dials and registers), and their
shared seams (`connect_wiring.rs`, `server_list_file.rs`, `saved_server_fields.rs`, `one_shot_credentials.rs`,
`credential_store.rs`).

## Must-knows

- **A secret reaches a dial through `SecretOffer` or the store, ❌ never a `password` param.** `remember: false` writes
  nothing (`one_shot_credentials.rs`).
- **Key everything by `(host, port, username)`**, ❌ never the host alone; a trusted host KEY keys
  `(host, port, algorithm)`. ❌ Never write `~/.ssh/known_hosts`.
- **Credentials never go into argv**: `smbclient` via a 0o600 `-A` file, `gio mount` via stdin, `build_smbutil_url`
  passwordless only.
- **Compare servers by identity (`server_identity::same_server*`), ❌ never by string**, and ❌ never ship a server-keyed
  map over IPC: answer a lookup.
- **NFC-fold every SMB name you send, key, or compare** (❌ never the password), or `TreeConnect` answers
  `STATUS_BAD_NETWORK_NAME`.
- **mDNS is gated**: startup fires only if `network.enabled && (firstTriggerDone || smb-e2e)`, so a fresh install holds
  the macOS "find devices" prompt until `ensure_network_discovery_started`.
- **Every NetFS mount sets `UIOption = NoUI`**, or NetAuthAgent pops a dialog and blocks the mount.
- **Re-register via `register_replacing_predecessor` (SMB) or `install_retiring_incumbent`**, which retire through
  `on_superseded`; ❌ never a bare overwrite or `on_unmount`, which cuts in-flight transfers. An EDIT to a connected
  SFTP/WebDAV place shares its session instead: `live_server_edit.rs` (`DETAILS.md` § "Editing a connected place").
- **Decide at ACT time**: re-check `is_already_direct` under `lock_volume_upgrade`, right before connecting.
- **A mount's volume ID and anchor come off ONE `statfs` row** (`identity_from_statfs`), ❌ never derived apart. Every
  mount read on an async path is bounded (`MOUNT_READ_LIMIT`).
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`, ❌ not a `timeout` around
  `spawn_blocking`.
- **Read a refusal by status AND step (`RefusedAt::of`), ❌ never off `is_auth_error` alone**: access denied at
  TreeConnect is the share turning an account away, not a wrong password.
- **❌ A `network` type must not be constructible from a backend type**: a `From` impl welds a module cycle.

Architecture, flows, decisions, the per-server switches, and the smaller gotchas (ports, loopback addresses, the mDNS
trailing dot): `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
