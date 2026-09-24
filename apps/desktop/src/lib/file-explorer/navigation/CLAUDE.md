# Navigation

Back/forward history, path resolution, paged keyboard shortcuts, and the pane's volume chip.

## Module map

- Paths and history: `navigation-history.ts` (immutable stack), `real-folder-history.ts` (the newest non-snapshot entry
  in one), `path-navigation.ts`, `path-resolution.ts`, `keyboard-shortcuts.ts`.
- `VolumeBreadcrumb.svelte` is the CHIP, hosting `VolumeChooserMenu.svelte` (the switcher) and `FavoritesMenu.svelte` +
  `favorites-menu.svelte.ts` (⌃D). Plus a helper per concern (grouping, disk space, connection state, eject, labels,
  badges) and the shared dots (`ConnectionDot`, `UsbSpeedDot`, `DetachButton`).
- `row-menu.ts`: the ONE list of a row's actions, whatever door opens it. `server-row-actions.ts` runs the server ones.

## Must-knows

- **History pushes on listing success AND failure.** Drop the `listing-error` branch and a TCC-restricted folder stays
  out, so `Cmd+[` jumps back two.
- **Callers holding per-entry resources need `push()`**: only it returns `droppedEntries` to release dropped refs.
- **`resolveValidPath` stops at a scheme path's floor and RETURNS it**, never `~`, `/`, or `null`: a remote path can
  answer no probe, so a plain walk lands the pane on the boot disk (`path-resolution.ts`, a cycle-breaker).
- **❗ Pass `volumeId` wherever the walk should stay on the pane's volume**: without it every probe asks the boot disk,
  which says "gone" for a phone's folders. Who passes it: `DETAILS.md` § `path-resolution.ts`.
- **A volume-switch correction has two gates**: ONE global `correctionGen` (❌ not one per pane), plus its pane's token
  and position, so it never moves a pane off a later navigation.
- **`containingVolumeId` comes from `resolvePathVolume(currentPath)`, ❌ not the `volumeId` prop** (a favorite's is
  virtual), so the checkmark tracks the real one.
- **Read `connectionState` through `connection-state.ts`'s predicates, ❌ never `!= null`**: four backends carry one,
  plus a `saved` row. `showsDisconnect` means REGISTERED, so both sign-in states are IN and only `saved` isn't.
- **`detachControlFor` decides EVERY detach control**: whether there is one, its word, its glyph, and the action it runs
  via `runDetach`. ❌ Never re-decide in a component: the chip once offered an Eject the backend can only refuse, on a
  server the switcher was disconnecting fine. It reads a SERVER by volume ID (`isServerPlaceRow`, ❌ never
  `category === 'network'`: a mounted SMB share is one) and a PHONE by `deviceReadiness`, ❌ never `isEjectable` or
  `connectionState`, carried unconditionally and not at all, else a greyed `unavailable` phone offers a live Disconnect.
- **`wordEjectRefusal(e)` words every eject refusal** from `errors.eject.*`; ❌ never `String(e)` or `diskutil` stderr.
  `wordUnmountRefusal` names the holders; ❗ `Unclassified` and BOTH empty `HolderScan` arms take the unnamed fallback,
  ❌ never "nothing is using this drive". Precedence: `DETAILS.md`.
- **The Network group's rows are the LISTING's**, filtered by `belongsInSwitcher`, plus the hub this dir synthesizes. ❗
  No `listSavedServers()` fetch in `volume-grouping.ts`; the row carries `pinned` already.
- **Favorites live in their OWN menu (⌃D), ❌ never in the switcher.** `volume-grouping.ts` groups the `favorite`
  category NOWHERE; the switcher's "See N favorites" row swaps the menus. Mutate ONLY through the
  `$lib/tauri-commands/favorites.ts` wrappers (pass bare ids using `stripFavoritePrefix`).
- **❗ The chip holds ONE `openMenu`**, so its two can't both be up: each reports through `onOpenChange`, and
  `isHeaderMenuOpen()` is the single answer the panes suppress keys on. ❌ No second source of truth.
- **Favorite inline editors own keystrokes**: preserve `isEditing()` and input `stopPropagation()` or pane shortcuts
  fire. The rename `<input>` holds four guards against leaking keystrokes to the panes; drop any one and it leaks again.
- **❗ EVERY menu here is the house `Menu`** (`$lib/ui/DETAILS.md` § Menu), row actions included: it owns keys, cursor,
  pointer, submenus, reorder, placement, and focus. ❌ Never a key handler, highlight index, or `getBoundingClientRect`
  here; a chip menu's `onKey` claims only the SWAP keys.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
