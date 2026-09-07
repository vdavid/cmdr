# Pinnable SMB shares

An SFTP or WebDAV place can be pinned: it keeps a greyed row in the volume switcher's Network group while disconnected,
and activating that row dials it. An SMB share cannot. It reaches the switcher only while it is mounted, so an unmounted
share is invisible until the user walks the servers hub down to it again.

Closing that gap is a share-level writer plus one id rule, and both are cheap. The reason it was left out of
`docs/specs/servers-hub-plan.md` is in that plan's D4: half-building it would have shipped pins that silently never
match a mounted share.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## Why the store can't answer today

Three facts, each verified against the code (2026-09-07):

- **The known-shares store holds no share rows.** `KnownNetworkShare` has a `share_name` field, but its one writer,
  `PlacesBrowser.svelte` through `update_known_share`, always passes `''`: it records the SERVER a user signed in to, so
  the next sign-in can pre-fill a username. There is nothing per-share to pin.
- **The store holds no port.** `KnownNetworkShare` is
  `(server_name, share_name, protocol, last_connected_at, last_connection_mode, last_known_auth_options, username)`.
- **The two names disagree.** A mounted share's id is minted by `smb_volume_id(server, port, share)` from what `statfs`
  reports (`file_system/index_provider.rs::smb_volume_id_for_path`), which normalizes an mDNS name to an IP address. The
  store holds whatever the host list published, which is usually the mDNS name. So no id derivable from a stored row
  matches the mounted volume's, and a pin keyed on one would sit beside its own share forever.

## The shape of the fix

1. **A share-level writer at MOUNT time**, storing the server exactly as `statfs` spells it, plus the port and the
   share. That is the only moment both halves of the identity are known, and it is what makes
   `smb_volume_id(stored_server, stored_port, stored_share)` reproduce the mounted volume's id. The mount sites are in
   `apps/desktop/src-tauri/src/network/DETAILS.md`; the id rule is `crates/cmdr-fs/src/volume/ids.rs::smb_volume_id` (it
   NFC-folds and lowercases both halves, so the writer does not have to).
2. **A `pinned` field on the share row**, `#[serde(default)]` false, set true on first successful mount and preserved on
   every later write. The two SFTP and WebDAV stores already do exactly this; copy their "field-absent file" cell.
3. **Share rows in `list_saved_servers()`**, so an SMB account stops answering with an empty `places` list. The shape is
   `apps/desktop/src/lib/servers/DETAILS.md`; the switcher's own rule for which rows appear is
   `apps/desktop/src/lib/file-explorer/navigation/DETAILS.md`.
4. **A `saved` SMB row that dials on activation.** SMB's connect is a share MOUNT rather than a session, so it does not
   go through `connect_saved_place`; the arm it needs is the mount path the hub already drives. Deciding that is the
   real design work in this item.

## What it buys, and what it costs

**Buys**: the NAS someone uses every day sits in their switcher whether or not it is mounted, which is the whole point
of pins. It also lets a restored tab on an unmounted share resolve to something, rather than falling back.

**Costs**: a store migration (existing rows have no port and a possibly-wrong server spelling, so they are server-level
rows that stay unpinnable until the share is mounted once more), and the mount-arm decision in step 4.

**Trigger**: a user asking why their NAS share does not stay in the list the way an SFTP server does. Until then, the
hub is one keystroke away and shows every SMB host.
