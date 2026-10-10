# Moving a saved server to a new address

Design note for unlocking a saved server's address in the edit sheet (decided 2026-10-10). A server that MOVED (a NAS on
a new IP, `nas.local` → its Tailscale name, a new port) keeps its favorites, tabs, password, pin, and settings. The
protocol and the account stay locked: another account or protocol is another place, since two accounts on one server see
different files.

The mechanism's canonical home is `apps/desktop/src-tauri/src/server_move.rs` and the docs it points to; this note is
the inventory behind it, with the why of each decision.

## What counts as "the address", per protocol

- **SFTP**: host and port. The id is `(host, port, username)` (`cmdr_fs::volume::sftp_volume_id`), so a move changes it.
- **WebDAV**: the whole base URL (scheme, host, port, path). The id takes only `(host, port, username)`, but the base
  URL is what the client dials and what the store keys on (`webdav_known_servers::find(url, username)`), and a moved
  Nextcloud can change any part of it (`http` → `https` behind a proxy, a new host, a new path). Remote paths are
  relative to the base URL, so a tab's `webdav://…/Photos` still means the same folder after the base path moved. A move
  that keeps host and port keeps the id: the store and the secret still move, the session still redials, and every path
  stays as it is.
- **S3**: only the "Other S3-compatible" endpoint (its URL, region, and path-style switch), and only on the ACCOUNT's
  edit. A preset's region, account ID, or location names different STORAGE, not a new road to the same storage: an AWS
  bucket lives in one region (`region_mismatch` exists for that), and an R2 account ID is another Cloudflare account. A
  self-hosted MinIO or Garage on a new IP is the real case. The endpoint is the account's, so every place under the key
  moves together, with the one secret they share; a place's own edit keeps the endpoint locked.
- **SMB**: ❌ not in this change. See § "SMB, deferred".

## The inventory, and what each holder does

Everything that names a place by its id or spells a path with its address prefix:

1. **The saved-server store entry** (`known-sftp-servers.json`, `webdav_known_servers.json`, `s3_known_places.json` with
   its account record). ONE store write removes the old entry and inserts the new one, so no file ever holds both or
   neither. The new entry carries the old one's pin and `last_connected_at` (it's the same server); everything the sheet
   shows (name, root, start folder, key file, agent, "Reconnect automatically") comes from the edit, like any save.
2. **An address another saved server already holds**: ❗ REFUSED (`SavedServerOutcome::AddressTaken`), with a sentence
   under the address naming the other server. Merging would have to pick one entry's settings, pin, and secret and
   silently drop the other's, and the person can't see which. A refusal names the other server, so the person decides
   (open that one, or forget it first).
3. **The Keychain secret**, keyed by the address. Crash order: copy the secret to the new key FIRST, then the store
   move, then delete the old key LAST. A crash between any two steps leaves the password reachable from whichever entry
   the store holds; the worst leftover is an orphaned copy under a key nothing reads. A copy the store refuses (Deny, a
   locked keychain) refuses the whole move (`SecretNotMoved`): moving the server while its password stayed behind would
   turn a saved password into a sign-in prompt, which is the data loss this order exists to prevent.
4. **Favorites** (`favorites.json`, `FavoriteVolume { id, root, name }` plus a `path` in the place's namespace,
   `sftp://ada@nas.local:22/…`): re-keyed to the new id and the new label, and `root` and `path` rewritten from the old
   app prefix to the new one, by whole components. A legacy favorite with no volume yet but a path under the old prefix
   gets its path rewritten too, so the claim pass finds the new place. Reports nothing to analytics: the person edited a
   server, not a favorite.
5. **Go to path's recents** (`go-to-path-history.json`): paths under the old prefix are rewritten, since the old address
   names nothing saved any more and opening it would land on an Add sheet. Search and selection histories hold queries
   and patterns, not place paths, so they stay.
6. **Open tabs, both panes, all tabs, their back/forward history, and `lastUsedPaths`**: the frontend's
   `server-place-moved` follow (`pane/server-move-follow.ts`), the twin of `volume-root-follow.ts`. The volume store's
   row is re-keyed first (the event isn't debounced and beats `volumes-changed`), each pane's ACTIVE tab moves through
   `navigate()` with no history push, a background tab is rewritten in place and saved, history entries are rewritten in
   place, and the remembered path moves to the new id. Persisted `leftVolumeId` / `leftPath` follow through the ordinary
   persistence of the active tab.
7. **A live session at the old address**: dropped without `VolumeUnmounted` (that event sends a pane home, and here the
   pane should follow), BEFORE the move event goes out, so a pane that follows to a place keeping its id (a WebDAV path
   move) can't dial into the session about to go. The pane that stood on it is now on the new `saved` row, and
   `place-connect` dials it on landing: that IS the redial at the new address, through the ordinary flow, so a new
   host's key, a missing password, or an unreachable address shows in the pane and the sheet exactly as a first open
   does. Chosen over leaving it disconnected because the person was looking at this server and just pressed Save on it:
   a pane reading "Not connected yet" would make them ask twice.
8. **An operation on the place**: ❗ REFUSED (`SavedServerOutcome::OperationRunning`) while any copy, move, delete, or
   other write operation names one of the server's places as source or destination, asked before the secret is copied so
   a refusal touches nothing. Dropping the old session would stop it like Disconnect. It counts every operation the
   manager holds (`write_operations::operations_need_volume`): running, PAUSED (it still holds the session and resumes
   on it), and QUEUED (it names the old id and would set out for a session that's gone), plus a drag-out streaming off
   the place. That's wider than Eject's busy set, which skips queued operations because they haven't touched the device
   yet. An edit that keeps the address keeps the session, so it never asks.
9. **SFTP host keys** (`known-sftp-hosts.json`, keyed `(host, port, algorithm)`): untouched. The old address's keys are
   facts about that address and stay, as they do on Forget. The new address has no trusted key, so the first dial asks
   through the normal host-key step: ❌ never silently trusted because the server "is the same one", which is exactly
   the claim a host-key check exists to verify.
10. **The drive index and media prefs**: nothing. SFTP, WebDAV, and S3 are never indexed (`cmdr-index` `handle/mod.rs`'s
    scheme gate), and the media-index network prefs are SMB-only.
11. **MCP, the hub, and the switcher**: they read the live listing, which `volumes-changed` republishes after the move.
    Nothing caches a server-keyed answer on the frontend for these protocols (`forgetShareListsOfMachine` is SMB's).

## Validation: Save doesn't dial

Save in edit mode never dials today ("Edit mode has nothing to try"), and a move keeps that: it writes and closes. Add
checks because a typo saved silently is found later, somewhere else; a moved server is found right away, because the
pane on it redials, and a wrong address shows there with Try again. Probing in the sheet would also pull the host-key
step and the password into the edit sheet, which has no attempt plumbing, and would refuse a move made while the server
is out of reach (editing at the office for the NAS at home). So there is no "Save anyway".

## The command

`update_saved_server(server, editing)`: `editing` is the id of the saved place the edit was raised on (`None` for Add
anyway). When the target's address differs from the edited entry's, it's a move; otherwise the ordinary edit path runs
unchanged (live check, `volume-root-changed`). The backend decides, so the frontend never compares addresses. A target
naming another protocol or account than the edited entry is refused (`AccountChanged`): the sheet locks both, so only a
broken caller sends one, and ❌ never a silent rewrite of the account.

S3's account edit (`update_saved_s3_account`) takes the account's endpoint beside its name for the same reason.

## SMB, deferred

An SMB share's volume id is not ours to mint: it comes off the MOUNT's `statfs` (`smb_upgrade::identity_from_statfs`),
which may spell the server as an IP, a hostname, or an mDNS service name, and `KnownNetworkShare::volume_id` is "never
re-derived from `server_name`" for exactly that reason. So a move can't know the new ids up front: re-keying favorites
and tabs to a guessed id would be overwritten by the first real mount (`remember_mount` rewrites the stored id), and the
guess would orphan them a second time. A kernel mount from the old address may also still be up and serving. The sound
shape is a pending move that the first mount at the new address completes (the mount reports its id, and the re-key runs
then), which belongs in `smb_saved_shares.rs` beside `remember_mount`. Until then the SMB address stays locked with its
own sentence.
