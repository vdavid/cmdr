# Network support

The app-side half of SMB, plus the saved-server stores and connect wiring SFTP, WebDAV, and S3 share. Protocol layers:
`crates/cmdr-smb/`, `crates/cmdr-sftp/`, `crates/cmdr-webdav/`, `crates/cmdr-s3/`. Frontend: `apps/desktop/src/lib/servers/CLAUDE.md`
and `apps/desktop/src/lib/file-explorer/network/CLAUDE.md`.

## Module map

SMB (discovery, share listing with CLI fallbacks, mounting through `mod.rs::mount_share`, OS-mount → direct upgrade at
launch, on mount, and on pane open),
SFTP, WebDAV, and S3 (host keys, a saved-server store each, ❗ one entry per PLACE for S3, a `*_volume_wiring.rs` that
dials and registers), and their
shared seams (`connect_wiring.rs`, `server_list_file.rs`, `saved_server_fields.rs`, `one_shot_credentials.rs`,
`credential_store.rs`). Every typed event: `events.rs`.

## Must-knows

- **A secret reaches a dial through `SecretOffer` or the store, ❌ never a `password` param.** `remember: false` writes
  nothing (`one_shot_credentials.rs`).
- **Key everything by `(host, port, username)`**, ❌ never the host alone; a trusted host KEY keys
  `(host, port, algorithm)`. ❌ Never write `~/.ssh/known_hosts`.
- **Credentials never go into argv**: `smbclient` via a 0o600 `-A` file, `gio mount` via stdin, `build_smbutil_url`
  passwordless only.
- **Compare servers by identity (`server_identity::same_server*`), ❌ never by string**, and ❌ never ship a server-keyed
  map over IPC: answer a lookup.
- **NFC-fold every SMB server and share name you send, key, or compare** (❌ never the password, ❌ never a path
  inside the share, which goes out byte-for-byte), or `TreeConnect` answers `STATUS_BAD_NETWORK_NAME`.
- **mDNS browses only under a `DiscoveryLease`** (`discovery_gate.rs`), and identity reads `fresh_discovered_hosts`,
  ❌ never the display-only `cached_discovered_hosts`: a stale pairing must never match two servers (`DETAILS.md`).
- **Every NetFS mount sets `UIOption = NoUI`**, or NetAuthAgent pops a dialog and blocks the mount.
- **Re-register via `register_replacing_predecessor` (SMB) or `install_retiring_incumbent`**, which retire through
  `on_superseded`; ❌ never a bare overwrite or `on_unmount`, which cuts in-flight transfers. An EDIT to a connected
  SFTP/WebDAV place shares its session instead: `live_server_edit.rs` (`DETAILS.md` § "Editing a connected place").
- **Drop a place's session through `commands::servers::disconnect_place_inner`, ❌ never a wiring's own `disconnect`**:
  it emits the `VolumeUnmounted` that sends a pane standing on the place home. Eject routes here too, so a server
  detaches from the chip, the switcher, and MCP the same way.
- **Decide at ACT time**: re-check `is_already_direct` and the per-share switch (`direct_connection_enabled`) under
  `lock_volume_upgrade`, right before connecting. ❗ Every auto upgrade dials through `register_smb_volume`, or the
  switch does nothing (`DETAILS.md` § "The per-share direct-connection switch"), and passes a `FallbackNotice`:
  `Announce` only when someone is watching the share.
- **A mount's volume ID and anchor come off ONE `statfs` row** (`identity_from_statfs`), ❌ never derived apart. Every
  mount read on an async path is bounded (`MOUNT_READ_LIMIT`).
- **Every SMB subprocess takes a deadline** via `crate::subprocess::output_within`, ❌ not a `timeout` around
  `spawn_blocking`.
- **Read a refusal by status AND step (`RefusedAt::of`), ❌ never off `is_auth_error` alone**: access denied at
  TreeConnect is the share turning an account away, not a wrong password.
- **❌ A `network` type must not be constructible from a backend type**: a `From` impl welds a module cycle.
- **Forgetting a server's SMB password takes only its own entries** (`keychain::forget_server_credentials`): every name
  on ITS port, plus a port-less entry only when a lookup noted finding its password there
  (`note_found_under_portless`). ❌ Never delete a port-less key blind: it's also the 445 server's key.
- **Only Cmdr's own mounts save an SMB share** (`smb_saved_shares.rs`), keyed by server + share, ❌ never by account,
  with the id the mount reported. An upgrade or watcher path that wrote one would invent history (`DETAILS.md` § "Saved
  SMB shares").

Architecture, flows, decisions, the per-server switches, and the smaller gotchas (ports, loopback addresses, the mDNS
trailing dot): `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
