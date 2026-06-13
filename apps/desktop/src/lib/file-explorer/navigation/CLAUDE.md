# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume selector breadcrumb.

## Module map

- `navigation-history.ts`: purely functional immutable history stack (all ops return new objects).
- `path-navigation.ts` / `path-resolution.ts`: pick the initial path on volume switch; walk-up `resolveValidPath`.
- `keyboard-shortcuts.ts`: Home/End/PageUp/PageDown for file lists.
- `VolumeBreadcrumb.svelte` + `volume-grouping.ts` / `volume-space-manager.svelte.ts` /
  `volume-breadcrumb-handlers.svelte.ts` / `favorites-controller.svelte.ts` / `eject-predicate.ts`: the volume selector,
  its disk-space state machine, and the favorites interaction layer (rename, reorder, remove). The favorites interaction
  logic lives in `favorites-controller.svelte.ts` (`createFavoritesController`), not inline in the component.

## Must-knows

- **History is pushed on listing success AND listing failure.** `FilePane.svelte`'s `onPathChange?.(loadPath)` fires
  from both `handleListingComplete` and the `listing-error` handler when the path still exists (error pane renders).
  Drop the failure branch and a folder that fails to list (TCC-restricted, mode 0700) renders `ErrorPane` while staying
  absent from history, so `Cmd+[` visually jumps back two steps. The deleted-path auto-fallback does NOT push here; it
  relies on the fallback navigation's own `commitPathFromListing` push.
- **`push()` vs `pushPath()` and released resources.** `push` returns `{ history, droppedEntries }`; callers that hold
  per-entry resources (the search-results snapshot store, via the tab-state manager's `pushHistoryEntry`) must use
  `push()` (or that wrapper) to release refs for dropped entries. `pushPath` discards `droppedEntries`. When the new
  entry equals the current one, `push()` returns the **same `history` reference** with empty `droppedEntries`, so `===`
  dedup still works.
- **`MAX_HISTORY_PER_TAB = 100` applies to every volume uniformly.** Don't tighten it (hurts deep-navigating power users
  who rely on `⌘[`); don't bump it (each entry is three string fields, headroom is fine).
- **`path-resolution.ts` is its own module to break a cycle**: `app-status-store.ts` imports `resolveValidPath`, and
  `path-navigation.ts` imports `getLastUsedPathForVolume` from `app-status-store.ts`. Keep `resolveValidPath` here.
- **Two-layer timeout on every `pathExists`**: Rust `blocking_with_timeout` (2 s) plus frontend `withTimeout` (500 ms
  for `determineNavigationPath`, 1 s for `resolveValidPath`); the faster wins. Hung network mounts must never block the
  Tauri runtime.
- **Stale volume-switch corrections are gated by a single GLOBAL `correctionGen`** (shared by both panes). A later
  volume change on either pane bumps it and drops a superseded background `determineNavigationPath` correction. Don't
  make it per-pane.
- **`containingVolumeId` is derived via `resolvePathVolume(currentPath)`, not the `volumeId` prop** (which may be a
  favorite's virtual id), so the active checkmark tracks the real containing volume.
- **Favorites are user-editable; mutate only via the `commands.*` wrappers and ALWAYS strip the `fav-` prefix.** The
  switcher's `LocationInfo.id` is `fav-<favoriteId>`; `removeFavorite` / `renameFavorite` / `reorderFavorites` take the
  bare id (`stripFavoritePrefix`). Don't pass the `fav-…` id straight through. The favorites group in
  `volume-grouping.ts` always renders (even empty, for the placeholder) — don't "tidy" it back into the hide-when-empty
  branch. Context-menu "Add to favorites" is handled in Rust (`FAVORITES_ADD_CONTEXT_ID`), not the `favorites.add`
  command. Full flow in [DETAILS.md](DETAILS.md) § Editable favorites.
- **The favorites interaction layer lives in `favorites-controller.svelte.ts`** (`createFavoritesController(deps)`,
  instantiated as `fav` in `VolumeBreadcrumb.svelte`): rename, pointer-drag + keyboard reorder, remove, and the
  local-first `optimisticFavoriteIds` override + its reconciliation `$effect`. The component keeps the template, the
  shared `highlightedIndex`, and the `effectiveVolumes` / `favorites` deriveds (which read `fav.optimisticFavoriteIds`).
  Controller state is exposed via getters, so template reads MUST go through `fav.*` (a snapshot won't stay reactive).
- **The favorite-rename `<input>` must not leak keystrokes to the panes.** Four guards work together; don't remove any:
  `fav.handleRenameKeyDown` calls `e.stopPropagation()` for EVERY key (the focused input owns its keystrokes; the pane's
  Space-selection DOM listener isn't covered by the dispatch-level guard, so without this a Space typed into the box
  also selects the file under the cursor); `VolumeBreadcrumb.handleKeyDown` bails while
  `fav.renamingFavoriteId !== null`; `DualPaneExplorer.routeToVolumeChooser` swallows keys from the pane behind ANY open
  switcher dropdown (returns true even when the dropdown ignores the key); and `+page.svelte`'s `isModalDialogOpen()`
  reads `explorerRef.isVolumeChooserOpen()` to suppress centralized dispatch.
- **Favorite reorder is POINTER-based, not HTML5 drag.** HTML5 `draggable`/`ondragstart`/`ondrop` never fire under
  Tauri's `dragDropEnabled` (the OS intercepts drag gestures before the WKWebView sees them), so the reorder uses
  `onmousedown` + window `mousemove`/`mouseup` listeners with a small move threshold (below it, a mouseup is a plain
  click → navigate). Don't reintroduce HTML5 drag here. Keyboard reorder (Alt+↑ / Alt+↓) lives in the exported
  `handleKeyDown` and acts on the virtual `highlightedIndex` (the rows aren't DOM-focused), so it must run before
  `handleDropdownKey` consumes the bare arrows. Both paths persist the FULL order via `reorderFavorites(bareIds)` using
  the pure `favorites-reorder.ts` helpers. Reorders are LOCAL-FIRST: the controller sets its `optimisticFavoriteIds`
  override (which `effectiveVolumes` / `favorites` derive from via `fav.optimisticFavoriteIds`) synchronously, so the
  list re-renders instantly and rapid Alt+↑/↓ presses compute against fresh state instead of racing the backend
  `volumes-changed` round-trip; a reconciliation `$effect` clears the override once the store catches up (or the
  favorite set changes). Don't make the reorder await the IPC before updating the UI, or fast repeats move the wrong
  item.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
