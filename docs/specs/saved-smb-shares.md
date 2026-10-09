# Saved SMB shares: a share and its account, as a row under its server

Tracks vdavid/cmdr-reports#7. Why: an SMB server in the Servers list is a bare host, so "user + server + share", which
is what a person means to save, has nowhere to live. `commands/servers/wire.rs` `SavedServer` names the missing piece: a
share-level writer at mount time. This builds it.

## The model

The account → place → pin model (`apps/desktop/src/lib/servers/DETAILS.md` § "The model") already fits SMB: the HOST is
the account, and each SHARE is a place under it. What changes is that a share place is now stored.

- **One row per share, keyed by server identity + share name.** ❗ Not by account: an SMB volume id is
  `smb_volume_id(server, port, share)` and carries no username, so two rows for one share under two accounts would be
  two saved places pointing at one volume id. The row remembers the account the share was last opened with. Server
  identity includes the PORT: `public` on two servers of one machine is two shares.
- **The row's volume id is the one the mount had** (read off `statfs`, like every other SMB id), ❌ never re-derived
  from the stored name. A share mounted by IP and one mounted by Bonjour name get different ids, and only the mount
  knows which one it got. Each mount through Cmdr refreshes it.
- **A share Cmdr never mounted has no volume id yet** (one named in `smb://user@host/share` at Add time). It is still a
  row in the hub; opening it goes through the host's share list, which mounts it and fills the id in.

## The store

`known-shares.json`, whose `KnownNetworkShare` already is "a share connected to, with its username". Rows with an empty
`shareName` stay what they are today: the host's sign-in history. Share rows (non-empty `shareName`) gain:

- `address`: what the mount dialed (an IP or hostname), where `serverName` is the name the person knows the host by (the
  hub's grouping key, and the Keychain's).
- `port` (absent means 445), `volumeId`, `mountPath` (where the last mount sat), `pinned`.

Writers, all upserts on the share's key:

1. **`mount_network_share`** (Cmdr's own mount, which is user intent) records the share after a mount that went through,
   guest included. It takes the host's name beside the dial address for that.
2. **Add with a share path** (`smb://sven@host/Container`) records the share with no volume id yet.
3. **Opening a saved share place** (below) mounts through the same writer.
4. **Favoriting a folder on a share nothing has saved** (a Finder or login mount) records the share from the live
   mount's source, with the mount's volume id, `pinned: false`, and only when no row holds the share yet, so an existing
   row's pin and account stay as the person set them. Favoriting is an explicit "remember this place", the same class of
   intent as 1 and 2, and without it a favorite on that share could never dial once it unmounts
   (`smb_saved_shares::remember_favorited_share`).

❌ Not written by the mount watcher, the startup adopter, the pane-open upgrade, or "Connect directly": those see mounts
nobody asked Cmdr to save (Finder's, macOS's at login), and minting rows for them would invent a history. Their volume
ids are the same `statfs` ids, so a saved row and a live mount of the same share dedupe by id.

## Migration

Nothing to rewrite. Every new field is `serde(default)`. Before this, the store's only writer wrote empty share names,
so no share rows exist yet; shares mounted earlier become rows the next time Cmdr mounts them. `manual-servers.json` got
its name and account fields in the two steps before this one.

## The hub

- A share row sits right under its server, indented: its name, "as sven" when it has an account, status Connected while
  it's mounted (off the volume list by id), else Saved. While it's connected, "as …" names the account the live mount
  signed in as (the mount table's user, `LocationInfo::mount_account`); the saved account is for the next connect.
- A server row names the SERVER-level account ("My NAS as testuser", "as guest"), the same one its share list's header
  names: the account its listing signed in as (`network-store.svelte.ts`'s `getListedAccount`), else the one it's set to
  be used with, else nothing. ❌ Never a share's mount (each share row says its own), and ❌ never a Keychain read. "My
  NAS as guest" from its one guest mount once disagreed with the header's "as testuser". The header offers "Sign in
  as…", the sign-in sheet with the username editable and no guest choice; a cancel stays on the list. The account signed
  in as becomes the host's preference (the typed-username one), so later listings, background ones included, sign in as
  it, and a guest listing never takes a known account back to guest. "Use guest" (offered where a guest listing worked
  this session) clears it first, then lists as guest.
- **Enter on a share row opens that share with that account.** With a volume id it goes the way an SFTP place does: the
  pane lands on the place, and a place that isn't mounted is brought to life in the pane (below). Without one, the
  host's share list opens and mounts it.
- **Enter on a server row opens its share list** and never mounts anything by itself (step 3 of this effort).
- Right-click on a share row: Open, Pin to switcher / Unpin, Forget share. F8 is Forget share.

## Pins and the switcher

- The volume list carries every share place that has a volume id and a mount path: the live mount row when it's mounted
  (annotated with the row's `pinned`), else a `saved` row at the last mount path. The switcher shows the pinned ones,
  like SFTP and WebDAV.
- A share is pinned on its first mount through Cmdr (rule 1 of the model), and Unpin sits in both row menus.

## Opening a saved share place

`connect_saved_place` gets an SMB arm, so a `saved` share row comes to life in the pane through the same `place-connect`
→ `connect-flow` path an SFTP place takes: mount the share with the recorded account's Keychain password (guest when it
has no account), connect the direct session the usual way, and answer `Connected`. No stored password for the account
answers `needs_credentials`, so the sheet asks for the password of that account (the place IS that account, so the
username is fixed). A remembered answer is written only after the mount went through. Cancel stops waiting; a kernel
mount already under way may still finish, and then the share simply shows up mounted.

## What Remove does

- **Forget share** (a share row): drops the row and its pin. The share stays mounted if it is, and no password is
  touched.
- **Forget server** (a server row): drops the manual entry, the host's sign-in history, and every share row under it.
  Nothing is unmounted. Where a password may be stored (the host is used with an account, or this session already read
  one; ❌ never a Keychain read to find out), its confirmation carries "Also forget the saved password", checked by
  default (a native alert's checkbox, `commands/confirm_dialog.rs`; Linux asks without it and keeps the password).
  Checked, it first deletes every password stored for that server (`forget_saved_smb_host_password`): the server-level
  and share-level entries under each name it goes by on ITS port (`SmbHostGroup::credential_names`), plus a port-less
  entry a lookup found its password under this session, and the in-memory cache. A port-less entry nobody found for it
  stays: it is also the key of the server on 445 of the same machine. SFTP and WebDAV servers get the same box
  (`forget_server_secret`).
- ❗ Both act on the row they were raised on, by its id alone, and take exactly the store rows that row showed. A host's
  identity is its address AND port, so a Forget never reaches another server on the same machine.

## Known limits

- One stored password per host (`keychain` keys SMB credentials by server), so a host holds one account's password at a
  time even though each share row remembers its own account.
- A saved place whose next mount lands on a different mount path is re-read at the new path; one whose server spelling
  changed gets a new volume id, and a pane standing on the old id lands home.
