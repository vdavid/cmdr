# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume selector breadcrumb.

## Module map

- `navigation-history.ts`: purely functional immutable history stack (all ops return new objects).
- `path-navigation.ts` / `path-resolution.ts`: pick the initial path on volume switch; walk-up `resolveValidPath`.
- `keyboard-shortcuts.ts`: Home/End/PageUp/PageDown for file lists.
- `VolumeBreadcrumb.svelte` + `volume-grouping.ts` / `volume-space-manager.svelte.ts` /
  `volume-breadcrumb-handlers.svelte.ts` / `favorites-controller.svelte.ts` / `eject-predicate.ts`: the volume selector,
  its disk-space state machine, and the favorites interaction layer (rename, reorder, remove).

## Must-knows

- **History is pushed on listing success AND listing failure.** `FilePane.svelte`'s `onPathChange?.(loadPath)` fires
  from both `handleListingComplete` and the `listing-error` handler when the path still exists. Drop the failure branch
  and a folder that fails to list (TCC-restricted, mode 0700) renders `ErrorPane` while staying absent from history, so
  `Cmd+[` visually jumps back two steps. The deleted-path auto-fallback does NOT push here (it relies on the fallback
  navigation's own `commitPathFromListing` push).
- **`push()` vs `pushPath()` and released resources.** `push` returns `{ history, droppedEntries }`; callers holding
  per-entry resources (the search-results snapshot store, via the tab-state manager's `pushHistoryEntry`) must use
  `push()` to release refs for dropped entries. `pushPath` discards them. When the new entry equals the current one,
  `push()` returns the same `history` reference with empty `droppedEntries`, so `===` dedup still works.
- **`MAX_HISTORY_PER_TAB = 100` applies to every volume uniformly.** Don't tighten it (hurts deep-navigating power
  users) or bump it (each entry is three string fields, headroom is fine).
- **`path-resolution.ts` is its own module to break a cycle**: `app-status-store.ts` imports `resolveValidPath`, and
  `path-navigation.ts` imports `getLastUsedPathForVolume` from `app-status-store.ts`. Keep `resolveValidPath` here.
- **Two-layer timeout on every `pathExists`**: Rust `blocking_with_timeout` (2 s) plus frontend `withTimeout` (500 ms
  for `determineNavigationPath`, 1 s for `resolveValidPath`); the faster wins. Hung network mounts must never block the
  Tauri runtime.
- **Stale volume-switch corrections are gated by a single GLOBAL `correctionGen`** (shared by both panes): a later
  volume change on either pane bumps it and drops a superseded background `determineNavigationPath` correction. Don't
  make it per-pane.
- **`containingVolumeId` is derived via `resolvePathVolume(currentPath)`, not the `volumeId` prop** (which may be a
  favorite's virtual id), so the active checkmark tracks the real containing volume.
- **Favorites: mutate ONLY via the `commands.*` wrappers and ALWAYS strip the `fav-` prefix.** The switcher's
  `LocationInfo.id` is `fav-<favoriteId>`; `removeFavorite` / `renameFavorite` / `reorderFavorites` take the bare id
  (`stripFavoritePrefix`). The favorites group in `volume-grouping.ts` always renders (even empty, for the placeholder),
  so don't "tidy" it back into a hide-when-empty branch. "Add to favorites" is handled in Rust
  (`FAVORITES_ADD_CONTEXT_ID`), not the `favorites.add` command. Full flow: DETAILS.md § Editable favorites.
- **The favorites interaction layer lives in `favorites-controller.svelte.ts`** (`createFavoritesController(deps)`,
  instantiated as `fav`): rename, reorder, remove, and the local-first `optimisticFavoriteIds` override. State is
  exposed via getters, so template reads MUST go through `fav.*` (a snapshot won't stay reactive).
- **The favorite-rename `<input>` must not leak keystrokes to the panes.** Four guards work together; don't remove any:
  `fav.handleRenameKeyDown` `stopPropagation()`s every key; `VolumeBreadcrumb.handleKeyDown` bails while
  `fav.renamingFavoriteId !== null`; `routeToVolumeChooser` swallows keys from the pane behind an open switcher dropdown;
  and `+page.svelte`'s `isModalDialogOpen()` reads `explorerRef.isVolumeChooserOpen()` to suppress centralized dispatch.
  Why each: DETAILS § Editable favorites.
- **Favorite reorder is POINTER-based and LOCAL-FIRST, not HTML5 drag.** HTML5 `draggable`/`ondragstart`/`ondrop` never
  fire under Tauri's `dragDropEnabled` (the OS intercepts drag gestures first). Keyboard reorder (Alt+↑ / Alt+↓) must
  run before `handleDropdownKey` consumes the bare arrows. Both paths set `optimisticFavoriteIds` synchronously, then
  persist the FULL order via `reorderFavorites(bareIds)` in the background. Don't reintroduce HTML5 drag; don't await
  the IPC before updating the UI. Full mechanism: DETAILS.md § Editable favorites.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
