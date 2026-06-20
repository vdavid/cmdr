# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume selector breadcrumb.

## Module map

- `navigation-history.ts`: purely functional immutable history stack (all ops return new objects).
- `path-navigation.ts` / `path-resolution.ts`: pick the initial path on volume switch; walk-up `resolveValidPath`.
- `keyboard-shortcuts.ts`: Home/End/PageUp/PageDown.
- `VolumeBreadcrumb.svelte` + `volume-grouping.ts` / `volume-space-manager.svelte.ts` /
  `volume-breadcrumb-handlers.svelte.ts` / `favorites-controller.svelte.ts` / `eject-predicate.ts`: the volume selector,
  its disk-space state machine, and the favorites interaction layer.

## Must-knows

- **History is pushed on listing success AND listing failure.** `FilePane.svelte`'s `onPathChange?.(loadPath)` fires
  from both `handleListingComplete` and the `listing-error` handler when the path still exists. Drop the failure branch
  and a folder that fails to list (TCC-restricted) stays absent from history, so `Cmd+[` jumps back two steps. The
  deleted-path auto-fallback does NOT push here (it relies on its own `commitPathFromListing` push).
- **`push()` vs `pushPath()` and released resources.** Callers holding per-entry resources (the search-results snapshot
  store, via `pushHistoryEntry`) must use `push()`: it returns `droppedEntries` so they can release dropped refs;
  `pushPath` discards them. A no-op push returns the same `history` ref, so `===` dedup still works.
- **`MAX_HISTORY_PER_TAB = 100`, every volume uniformly.** Don't tighten (hurts deep-navigating power users) or bump
  (each entry is three strings).
- **`path-resolution.ts` is a separate module to break a cycle**: `app-status-store.ts` imports `resolveValidPath`, and
  `path-navigation.ts` imports `getLastUsedPathForVolume` from it. Keep `resolveValidPath` here.
- **Two-layer timeout on every `pathExists`**: Rust `blocking_with_timeout` (2 s) plus frontend `withTimeout` (500 ms
  for `determineNavigationPath`, 1 s for `resolveValidPath`); the faster wins, so a hung mount never blocks runtime.
- **Stale volume-switch corrections are gated by a single GLOBAL `correctionGen`** (shared by both panes): a later
  volume change on either pane bumps it and drops a superseded background `determineNavigationPath` correction. Not
  per-pane.
- **`containingVolumeId` is derived via `resolvePathVolume(currentPath)`, not the `volumeId` prop** (a favorite's
  virtual id), so the active checkmark tracks the real containing volume.
- **The drive-index freshness badge (`DriveIndexBadge.svelte`) renders only on real DRIVE rows** (`isDriveRow`: not
  favorites or the synthetic `network` / `search-results` entries). State→color/menu is the pure
  `drive-index-status.ts`; status stays live via the manager's event subscriptions, not polling. A scanning badge shows
  a LIVE count off `index-scan-progress`, per-volume via `getScanProgress` (never bleed progress across drives). The
  badge is a `<button>` (axe rejects `role="img"`). Refused enable/rescan is classified by typed
  `SmbIndexGateReason`, never text. Full contract: DETAILS § Drive index freshness badge.
- **Favorites: mutate ONLY via the `commands.*` wrappers, ALWAYS stripping the `fav-` prefix** (the switcher id is
  `fav-<favoriteId>`; the commands take the bare id via `stripFavoritePrefix`). The `volume-grouping.ts` favorites group
  always renders even when empty (the placeholder row), so don't tidy it into a hide-when-empty branch. "Add to
  favorites" is in Rust (`FAVORITES_ADD_CONTEXT_ID`), not `favorites.add`.
- **The favorites interaction layer is `favorites-controller.svelte.ts`** (`createFavoritesController(deps)`, instanced
  as `fav`). State is getter-exposed, so template reads MUST go through `fav.*` (a snapshot won't stay reactive).
- **The favorite-rename `<input>` must not leak keystrokes to the panes.** Don't remove any of the four guards:
  `fav.handleRenameKeyDown` `stopPropagation()`s every key; `VolumeBreadcrumb.handleKeyDown` bails while
  `fav.renamingFavoriteId !== null`; `routeToVolumeChooser` swallows keys behind an open dropdown; `+page.svelte`'s
  `isModalDialogOpen()` (via `explorerRef.isVolumeChooserOpen()`) suppresses central dispatch.
- **Favorite reorder is POINTER-based and LOCAL-FIRST, not HTML5 drag** (the OS intercepts drag gestures under Tauri's
  `dragDropEnabled`, so `draggable`/`ondrop` never fire; don't reintroduce them). Keyboard reorder (Alt+↑ / Alt+↓) runs
  before `handleDropdownKey` consumes the bare arrows. Both paths set `optimisticFavoriteIds` synchronously, then persist
  the FULL order via `reorderFavorites(bareIds)` in the background (don't await the IPC before updating the UI). Full
  flow, the four-guard rationale, and the reorder mechanism: DETAILS § Editable favorites.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
