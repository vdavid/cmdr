# File explorer › restic: details

Pull-tier docs for `restic/`. Must-knows live in `CLAUDE.md`.

## A pane inside the view

`capabilitiesForPane` gives a path at or below `<repo>/Browse snapshots` the `restic` row, a copy of the `git-portal`
row: a real backend listing, `..`, and copy-out, with every write refused up front. Paste-into, F7, ⇧F4, F2, delete, and
Multi-rename get the `fileExplorer.readOnly.restic*` alert; ⌘C/⌘X point at the live copy key
(`fileExplorer.restic.copyOutWithKey`, copy only, since a move out of a backup is refused; `copyOutWithoutKey` names the
Copy command once no key runs it). The repo folder and its real files keep the drive's row.

- **No deleted-folder poll** (`pollsForDeletedFolder: false`): the view has no inode, so a stat would evict the user.
- **`..` at the view's top** walks back to the repo folder: the listing reports `<repo>/Browse snapshots` as its root,
  so `../pane/has-parent.ts::effectiveVolumeRootFor` uses the drive's mount for every routed kind.
- **Not favoritable, not shareable, no editor** (`kindCanBeFavorited`, `rowIsOsVisible`): nothing in the view is on
  disk. The `Browse snapshots` row itself counts as inside the view.
- **Open terminal here** opens the repo folder (`#lib/open-terminal/terminal-target.ts`), the nearest folder on disk,
  and never the `Browse snapshots` row under the cursor.
- **Select all and invert leave the `Browse snapshots` row out**, like `..` and the git portal's category rows
  (`../pane/overlay-rows.ts`, one predicate: a reserved view icon id in a pane that isn't inside a view). "Select all,
  copy" in a repo folder means the repo's real files; picking the row on its own still restores through the view.
- **A session restored inside a view with no saved password** raises the unlock prompt at launch. Accepted, with no
  special case: Cancel leaves the pane on the listing error that names the backup.
- **The `Browse snapshots` row's icon** is Lucide `history`, drawn by `../selection/FileIcon.svelte` for the reserved
  `restic:snapshots` icon id the `cmdr-restic` hub puts on the row (`OVERLAY_ROW_ICON_ID`), the same way the git
  portal's `git:*` ids get their glyphs. It inherits the row's text color.

## Forgetting a saved password

"Forget backup password" (`restic.forgetPassword`, palette only; `forget-password.ts`) acts on the repo the focused pane
is inside, else the one whose `Browse snapshots` row is under the cursor, and otherwise says how to pick one. It calls
`forgetSavedContainerPassword` on `<repo>/Browse snapshots`, which locks the repo in the backend (password, opened repo,
index, and snapshot list all go: `crates/cmdr-restic/DETAILS.md` § "Passwords and the credential store"). Every pane
inside that repo's view then walks back to the repo folder with its cursor on `Browse snapshots` (only the focused pane
takes focus), so the next Enter raises the unlock prompt instead of a pane failing its next read. The toast says whether
Keychain let go (`storeKeptIt` is a warning: locked for this session, found again next launch). Not on MCP: the `unlock`
tool answers a prompt and an agent never saves a password, so there's nothing for one to forget.

## Index progress

Every folder inside a snapshot needs the repo's index first, which on a NAS takes seconds. The backend reports it as a
typed `ListingPhase::loadingIndex` (bytes and files, read so far and in all) on `listing-progress`
(`crates/cmdr-restic/DETAILS.md` § "The index lifecycle"). `../pane/listing-loader.ts` hands it to the pane
(`setLoadingPhase`, cleared at every load start and end, and by a progress event without one), and
`#lib/ui/LoadingIcon.svelte` words it: "Loading the backup index: 48 MB of 312 MB" over a bar. Escape cancels it like
any load; the backend drops the index load when its last waiter leaves.

- **QA repro**: Debug window (⌘⇧D) → Components → Loading has a static "48 of 312 MB" cell and a scripted 20 s load on a
  loop. Judge the static one: a moving bar can't be judged from a frame.
