# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume selector breadcrumb.

## Module map

- `navigation-history.ts`: purely functional immutable history stack (all ops return new objects).
- `path-navigation.ts` / `path-resolution.ts`: pick the initial path on volume switch; walk-up `resolveValidPath`.
- `keyboard-shortcuts.ts`: Home/End/PageUp/PageDown.
- `VolumeBreadcrumb.svelte` + `volume-grouping.ts` / `volume-space-manager.svelte.ts` /
  `volume-breadcrumb-handlers.svelte.ts` / `favorites-controller.svelte.ts` / `eject-predicate.ts`: the volume selector,
  its disk-space state machine, and the favorites interaction layer.
- `favorites-analytics.ts`: `favorite_opened`, from the two `category === 'favorite'` branches (`navigate()` below them
  sees only the containing volume).
- `server-row-actions.ts`: what a SERVER row's menu offers and what each item does (the menu, Disconnect, Pin/unpin, the
  two Forgets), shared by the switcher, the hub, and the palette's server commands.
- `filesystem-label.ts`: the `fsType` → label maps. A NETWORK row answers with its PROTOCOL (`protocolLabel`, which the
  servers hub's Type column reads too), because a place with no local mount has no filesystem to name.

## Must-knows

- **History is pushed on listing success AND failure.** Drop the `listing-error` branch and a TCC-restricted folder
  stays absent from history, so `Cmd+[` jumps back two steps. DETAILS § Gotcha.
- **`push()` vs `pushPath()`.** Callers holding per-entry resources must use `push()`: it returns `droppedEntries` to
  release dropped refs; `pushPath` discards them. A no-op push returns the same `history` ref, so `===` dedup works.
- **`MAX_HISTORY_PER_TAB = 100`, every volume uniformly.** Don't tighten (hurts power users) or bump.
- **`path-resolution.ts` is a separate module to break a cycle**: `app-status-store.ts` imports `resolveValidPath`,
  `path-navigation.ts` imports `getLastUsedPathForVolume` from it. Keep `resolveValidPath` here.
- **Two-layer timeout on every `pathExists`**: Rust `blocking_with_timeout` (2 s) plus a frontend `withTimeout`; the
  faster wins, so a hung mount never blocks runtime.
- **Stale volume-switch corrections are gated by a single GLOBAL `correctionGen`**: a later volume change on either pane
  bumps it and drops a superseded `determineNavigationPath` correction. Not per-pane.
- **`containingVolumeId` is derived via `resolvePathVolume(currentPath)`, not the `volumeId` prop** (a favorite's
  virtual id), so the checkmark tracks the real containing volume.
- **The drive-index badge renders only on real DRIVE rows** (`isDriveRow`), stays live through `drive-index-manager`'s
  event subscriptions (❌ never polling), and owns freshness facts only — LIVE progress comes from `index-state`. While
  the MASTER switch is off, `driveIndexMenuActions` returns NOTHING, because the backend refuses every start then.
  DETAILS § Drive index freshness badge.
- **Eject refusals are worded by `wordEjectRefusal(e)` from `errors.eject.*`**; ❌ never toast `String(e)` or
  `diskutil`'s stderr.
- **A SERVER row says Disconnect, never Eject**, and is claimed by VOLUME ID (`isServerPlaceRow`), ❌ never by
  `category === 'network'`: a mounted SMB share is one of those, and `disconnectPlace` doesn't speak its OS mount.
- **The Network group's own rows are the LISTING's**, filtered by `belongsInSwitcher`, plus exactly one row this dir
  synthesizes: the hub. The listing carries every saved place and its `pinned`; hiding the unpinned ones is this dir's
  job. ❗ Don't add a `listSavedServers()` fetch to `volume-grouping.ts`: the row already says. DETAILS § "The
  three-things rule".
- **`resolveValidPath` stops at a scheme path's floor and RETURNS it**, never `~`, `/`, or `null`: a remote path answers
  no probe, so the plain walk lands the pane on the boot disk. DETAILS § "Restoring a remote path".
- **Favorites: mutate ONLY via the `commands.*` wrappers, always stripping the `fav-` prefix.** The favorites group
  renders even when empty (the placeholder row); don't tidy that into a hide-when-empty branch. The interaction layer is
  `favorites-controller.svelte.ts`, getter-exposed, so template reads go through `fav.*` or lose reactivity.
- **The favorite-rename `<input>` must not leak keystrokes to the panes**: four guards hold that line, and removing any
  one reopens it. **Favorite reorder is POINTER-based and LOCAL-FIRST**, ❌ never HTML5 drag (the OS intercepts drag
  under Tauri's `dragDropEnabled`). Both: DETAILS § Editable favorites.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
