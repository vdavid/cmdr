# Navigation

Browser-style back/forward history, path resolution, paged keyboard shortcuts, and the volume selector breadcrumb.

## Module map

- `navigation-history.ts`: purely functional immutable history stack (all ops return new objects).
- `path-navigation.ts` / `path-resolution.ts`: pick the initial path on volume switch; walk-up `resolveValidPath`.
- `keyboard-shortcuts.ts`: Home/End/PageUp/PageDown for file lists.
- `VolumeBreadcrumb.svelte` + `volume-grouping.ts` / `volume-space-manager.svelte.ts` /
  `volume-breadcrumb-handlers.svelte.ts` / `eject-predicate.ts`: the volume selector and its disk-space state machine.

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

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
