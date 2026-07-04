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

- **History is pushed on listing success AND failure.** `FilePane.svelte`'s `onPathChange?.(loadPath)` fires from both
  `handleListingComplete` and the `listing-error` handler when the path still exists. Drop the failure branch and a
  TCC-restricted folder that fails to list stays absent from history, so `Cmd+[` jumps back two steps. (The deleted-path
  auto-fallback pushes via its own `commitPathFromListing`, not here.)
- **`push()` vs `pushPath()`.** Callers holding per-entry resources (the search-results snapshot store, via
  `pushHistoryEntry`) must use `push()` — it returns `droppedEntries` to release dropped refs; `pushPath` discards them.
  A no-op push returns the same `history` ref, so `===` dedup still works.
- **`MAX_HISTORY_PER_TAB = 100`, every volume uniformly.** Don't tighten (hurts deep-navigating power users) or bump
  (each entry is three strings).
- **`path-resolution.ts` is a separate module to break a cycle**: `app-status-store.ts` imports `resolveValidPath`, and
  `path-navigation.ts` imports `getLastUsedPathForVolume` from it. Keep `resolveValidPath` here.
- **Two-layer timeout on every `pathExists`**: Rust `blocking_with_timeout` (2 s) plus a frontend `withTimeout`
  (per-call values in DETAILS); the faster wins, so a hung mount never blocks runtime.
- **Stale volume-switch corrections are gated by a single GLOBAL `correctionGen`** (shared by both panes): a later
  volume change on either pane bumps it and drops a superseded background `determineNavigationPath` correction. Not
  per-pane.
- **`containingVolumeId` is derived via `resolvePathVolume(currentPath)`, not the `volumeId` prop** (a favorite's
  virtual id), so the active checkmark tracks the real containing volume.
- **The drive-index freshness badge (`DriveIndexBadge.svelte`) renders only on real DRIVE rows** (`isDriveRow`: not
  favorites, `network` / `search-results`, or disk images). State→color/menu is the pure `drive-index-status.ts`;
  freshness stays live via `drive-index-manager`'s event subscriptions (NOT polling). The manager owns ONLY
  freshness/menu facts (dot color + last-scan facts); LIVE scan progress comes from `$lib/indexing`'s `index-state` (the
  single live-activity source — see its docs), read per-volume via `getVolumeActivity`; don't reintroduce a manager-side
  progress map. The badge is a `<button>` (axe rejects `role="img"`), and a refused enable/rescan is classified by typed
  `SmbIndexGateReason`, never text. Full contract: DETAILS § Drive index freshness badge.
- **Favorites: mutate ONLY via the `commands.*` wrappers, always stripping the `fav-` prefix** (`stripFavoritePrefix`;
  the switcher id is `fav-<favoriteId>`, the commands take the bare id). The `volume-grouping.ts` favorites group always
  renders even when empty (the placeholder row) — don't tidy it into a hide-when-empty branch. "Add to favorites" is in
  Rust (`FAVORITES_ADD_CONTEXT_ID`), not `favorites.add`.
- **The favorites interaction layer is `favorites-controller.svelte.ts`** (`createFavoritesController(deps)`, instanced
  as `fav`). State is getter-exposed, so template reads MUST go through `fav.*` (a snapshot won't stay reactive).
- **The favorite-rename `<input>` must not leak keystrokes to the panes.** Don't remove any of the four guards:
  `fav.handleRenameKeyDown` `stopPropagation()`s every key; `VolumeBreadcrumb.handleKeyDown` bails while
  `fav.renamingFavoriteId !== null`; `routeToVolumeChooser` swallows keys behind an open dropdown; `+page.svelte`'s
  `isModalDialogOpen()` suppresses central dispatch.
- **Favorite reorder is POINTER-based and LOCAL-FIRST, not HTML5 drag** (the OS intercepts drag under Tauri's
  `dragDropEnabled`, so `draggable`/`ondrop` never fire; don't reintroduce them). Keyboard reorder (Alt+↑ / Alt+↓) runs
  before `handleDropdownKey` consumes the bare arrows. Both paths set `optimisticFavoriteIds` synchronously, then
  persist the full order via `reorderFavorites(bareIds)` in the background (don't await the IPC first). Full flow + the
  reorder mechanism: DETAILS § Editable favorites.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
