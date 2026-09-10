# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume switcher breadcrumb.

## Module map

- Paths and history: `navigation-history.ts` (immutable stack), `path-navigation.ts`, `path-resolution.ts`,
  `keyboard-shortcuts.ts`.
- The switcher: `VolumeBreadcrumb.svelte` plus a helper per concern (grouping, disk space, favorites, connection state,
  eject, labels), and `server-row-actions.ts` for a SERVER row's menu, shared with the hub and the palette.

## Must-knows

- **History pushes on listing success AND failure.** Drop the `listing-error` branch and a TCC-restricted folder stays
  out of history, so `Cmd+[` jumps back two steps.
- **Callers holding per-entry resources need `push()`**: only it returns `droppedEntries` to release dropped refs.
- **`resolveValidPath` stops at a scheme path's floor and RETURNS it**, never `~`, `/`, or `null`: a remote path can
  answer no probe, so a plain walk lands the pane on the boot disk. It stays in `path-resolution.ts`, a module that only
  exists to break a cycle.
- **❗ Pass `volumeId` wherever the walk should stay on the pane's volume**: without it every probe asks the boot disk,
  which says "gone" for a phone's or server's folders. Who passes it and who doesn't: `DETAILS.md` §
  `path-resolution.ts`.
- **ONE global `correctionGen` gates stale volume-switch corrections**, ❌ not one per pane: a change on either pane
  drops a superseded one.
- **`containingVolumeId` comes from `resolvePathVolume(currentPath)`, ❌ not the `volumeId` prop** (a favorite's is
  virtual), so the checkmark tracks the real containing volume.
- **Read `connectionState` through `connection-state.ts`'s predicates, ❌ never `!= null`**: four backends carry one,
  plus a `saved` row, so "has a value" answers nothing a caller asks. `showsDisconnect` is "a volume is REGISTERED under
  this id", so both sign-in states are IN; only `saved` has no subject.
- **❗ A PHONE's detach control is decided by `deviceReadiness`, ❌ never by `isEjectable` or `connectionState`.** A
  device row carries `isEjectable: true` unconditionally and no `connectionState` at all (readiness is presence, never
  session health), so `isVolumeEjectable` reads readiness for one and every other row keeps the two session predicates.
  Without that branch a greyed `unavailable` phone nobody can open still offered a live Disconnect.
- **A SERVER row says Disconnect, never Eject**, and is claimed by VOLUME ID (`isServerPlaceRow`), ❌ never by
  `category === 'network'`: a mounted SMB share is one of those, and `disconnectPlace` can't speak its OS mount.
- **`wordEjectRefusal(e)` words every eject refusal** from `errors.eject.*`; ❌ never toast `String(e)` or `diskutil`'s
  stderr.
- **The Network group's rows are the LISTING's**, filtered by `belongsInSwitcher`, plus the one row this dir
  synthesizes: the hub. ❗ No `listSavedServers()` fetch in `volume-grouping.ts`; the row already carries `pinned`.
- **Favorites: mutate ONLY through the `$lib/tauri-commands/favorites.ts` wrappers, stripping the `fav-` prefix.** The
  group renders even when empty (the placeholder row), so ❌ no hide-when-empty branch. `favorites-controller.svelte.ts`
  is getter-exposed, so template reads go through `fav.*` or lose reactivity.
- **The favorite-rename `<input>` must not leak keystrokes to the panes**: four guards hold that line, and removing any
  one reopens it. **Reorder is POINTER-based**, ❌ never HTML5 drag (the OS intercepts drag under `dragDropEnabled`).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
