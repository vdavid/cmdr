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
- **SMB**: the host's address and port, on the HOST's edit (`update_saved_smb_host`). Its shares, password, and settings
  move at Save; each share's favorites and tabs follow at its first mount there. See § "SMB: a pending move".

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
   a pane reading "Not connected yet" would make them ask twice. A dial to the OLD address still out when Save lands is
   called off, and one that succeeds anyway is let go rather than remembered (it would save the old entry again): a
   landing check under a lock is the guarantee, cancellation only a courtesy. Mechanism and why:
   `apps/desktop/src-tauri/src/network/DETAILS.md` § "Moving a saved server to a new address".
8. **An operation on the place**: ❗ REFUSED (`SavedServerOutcome::OperationRunning`) while any copy, move, delete, or
   other write operation names one of the server's places as source or destination, asked before the secret is copied so
   a refusal touches nothing. Dropping the old session would stop it like Disconnect. It counts every operation the
   manager holds (`write_operations::operations_need_volume`): running, PAUSED (it still holds the session and resumes
   on it), and QUEUED (it names the old id and would set out for a session that's gone), plus a drag-out streaming off
   the place. That's wider than Eject's busy set, which skips queued operations because they haven't touched the device
   yet. An edit that keeps the address keeps the session, so it never asks.
9. **SFTP host keys** (`known-sftp-hosts.json`, keyed `(host, port, algorithm)`): untouched. Decision (David,
   2026-10-11): the old address's keys stay trusted after a move, consistent with Forget. They're facts about that
   address, not about the saved entry, and keeping them is the safer side: a DIFFERENT machine that later answers on the
   old IP meets a key-changed warning, where dropping them would turn it into a first-contact prompt that's easy to
   accept. The new address has no trusted key, so the first dial asks through the normal host-key step: ❌ never
   silently trusted because the server "is the same one", which is exactly the claim a host-key check exists to verify.
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

## SMB: a pending move

Decided 2026-10-11. Code: `apps/desktop/src-tauri/src/server_move_smb.rs` (the move and its completion),
`network/known_shares.rs` (`move_host_rows`, `pending_moves`, `remember_share` answering a `CompletedMove`).

**Why SMB can't move like the others.** An SMB share's volume id is not ours to mint: it comes off the MOUNT's `statfs`
(`smb_upgrade::identity_from_statfs`), which may spell the server as an IP, a hostname, or an mDNS service name, and
`KnownNetworkShare::volume_id` is "never re-derived from `server_name`" for exactly that reason. So a move can't know
the new ids up front: re-keying favorites and tabs to a guessed id would be overwritten by the first real mount
(`remember_mount` rewrites the stored id), and the guess would orphan them a second time.

**So the move is in two halves.**

1. **Save** (`server_move::smb::move_host`, from `update_saved_smb_host` when the typed address names another
   `(host, port)`): refusals first, then the passwords COPIED (the host's server-level entry and each saved share's,
   keyed by `smb_server(host, port)`), the manual entry relocated in one write
   (`manual_servers::relocate_manual_server`, which also saves a host only the share history knew, and swaps it in the
   discovery list), the host's rows pointed at the new address (`known_shares::move_host_rows`), and the old passwords
   deleted LAST. ❗ Each saved share KEEPS its old volume id, filed in `known_shares::pending_moves`. Favorites, tabs,
   and the share's `saved` switcher row keep naming that id, and it now dials the new address, so picking a favorite
   connects there and completes the move. ❌ Never `Forgotten`: the id still names a saved share.
2. **First mount** (`server_move::smb::complete`, from `smb_saved_shares::remember_mount` whenever `remember_share`
   answers a `CompletedMove`): the mount reports the real id, the pending entry goes, favorites re-key BY ID and rebase
   onto the new mount path (`favorites::store::follow_share_move`, ❌ never by path prefix: a share's paths are OS
   paths, and another server's same-named share may hold the old mount point now), and `ServerPlaceMoved` takes tabs,
   history, and `lastUsedPaths` along. ❗ The event carries the new id's LIVE `connection_state`, since the share just
   mounted: a row re-keyed as `saved` would make the following pane dial a share that's up. A mount that minted the same
   id ends the wait and re-keys nothing.

**What each holder does, beside the inventory above:**

- **The rows.** A row with an address dials the new one. Its `server_name` follows when it spelled the old address (same
  `credential_key`), and a row with no address (the host's sign-in history, a share an Add named) always takes the new
  discovery name, since its name is all that says which host it is. ❗ A Bonjour name (`Naspolya`) stays: it still names
  the machine, and the password filed under it stays reachable.
- **The passwords.** Only the entries keyed by the old address move. Two spellings of one key (`nas.local` → `nas`) are
  one entry, so nothing is copied or deleted. A port-less legacy entry (`note_found_under_portless`) is left alone, for
  the same reason Forget leaves it: it may be the 445 server's.
- **The direct-connection opt-out** is COPIED to the new address, ❌ not moved: an opt-out names a machine without a
  port, and another server on the old machine may share it.
- **Go to path's recents** stay: an SMB path is an OS path under the mount, which names no server.
- **The media index's per-volume choices** (`mediaIndex.networkVolumes`, the network opt-in, and
  `mediaIndex.alwaysIndexVolumes`): carried to the new id by the frontend's follow of the completing
  `server-place-moved` (`media-index/network-volume-prefs.ts::followVolumeMove`, persisted and live-applied, new id set
  before the old one clears). They're FE-owned settings, which is why the frontend carries them. Folder overrides
  (`alwaysIndexFolders`, `excludedFolders`) are OS paths under the mount, which a move normally keeps, so they stay.
- **The index stores** (`index-{id}.db`, `importance-{id}.db`, `media-{id}.db` with its vector index,
  `cmdr-index/src/volume_files.rs`): ❌ not re-keyed; the new id starts fresh. Decision (2026-10-11): renaming them
  isn't safe at completion. The new volume is registered (and its indexing may open `index-{new}.db`) BEFORE
  `remember_mount` learns the move completed, so a rename would race a live SQLite + WAL file set, and the media store
  has holders outside the lifecycle registry (`register_holder`). Rows also hold mount-rooted paths, which differ when
  the share remounts at `/Volumes/public-1`. A fresh drive scan is cheap; the old files stay on disk under the old id
  (the media store is never dropped for a merely absent volume). ❗ What's lost is the share's media enrichment (OCR,
  embeddings) and its folder-visit importance: worth a dedicated re-key if a moved NAS with a big photo library shows
  up.

**Refusals**, each writing nothing:

- `AddressTaken`: another saved SMB host is that server (`SmbServer::is`, same port and machine), or a manual entry
  holds the new id.
- `OperationRunning`: an operation names one of the host's share places.
- `ShareMounted` (SMB only): a saved share is still mounted from the old address, registered under its place id or in
  the kernel's mount table from the old server. Decision: ❌ Cmdr doesn't unmount it. It's an OS mount Finder or another
  app may be using, which Cmdr may not have made, and a share mounted at the old address keeps answering under the old
  id, so its first mount at the new one could never happen. The sentence names the share; the person ejects it and saves
  again.

**A dial to the old address that lands after Save** is let go, the same guarantee as the other protocols':
`connect_saved_share` files its place id with the `DialTicket` `connect_saved_place` took before reading the row, and
lands through `AttemptGuard::land`; a refused landing answers `Cancelled` and remembers nothing, so it can't save the
old address again as a second row. ❗ The kernel mount it made stays (NetFS can't take one back): the share is then
mounted from the old address, and its move completes at a later mount at the new one.

**Edits and Forget while a move is pending.** Editing again just moves the rows again: the pending entry still holds the
original id, so a move back to the old address completes like any other (a mount minting the same id ends the wait).
Forget drops the rows, and `forget_in` drops pending entries no row is left to complete; favorites then read
`Forgotten`, as for any forgotten share.

**Save redials a pane standing on a moved share.** Each moved share gets a `ServerPlaceMoved` with the same id on both
sides, so `place-moves.svelte.ts` bumps its move count and a pane on its `saved` row (reading "unreachable" at the old
address, say) dials the new one. Same reasoning as the other protocols' redial.

**Linux.** A GVFS mount's id comes off its folder name, which carries the server and port
(`volumes_linux::get_smb_mount_info`), so a mount at the new address mints a new id there too and completes the move the
same way. `ShareMounted` finds a GVFS mount through the registry only: `smb_mounts` reads CIFS, and GVFS's own gap
(#348, an outside unmount goes unnoticed) applies.
