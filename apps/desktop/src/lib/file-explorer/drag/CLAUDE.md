# Drag and drop

Native drag-and-drop for files: dragging out to other apps, receiving drops from external sources, and pane-to-pane
drags. macOS only (backend commands + swizzle gated `#[cfg(target_os = "macos")]`).

## Module map

- `drag-drop.ts`: mouse tracking, threshold detection, drag initiation; records self-drag identity at drag start.
- `drop-target-hit-testing.ts` / `drop-target-validation.ts` / `drop-operation.ts`: pure drop-target resolution,
  source/descendant blocking, and move-vs-copy resolution.
- `drag-auto-scroll.ts`, `DragOverlay.svelte` + `drag-overlay.svelte.ts`, `drag-image-renderer.ts`: scroll bands, cursor
  label, canvas drag preview.
- Backend: `native_drag.rs` (+ `native_drag/type_plan.rs`), `commands/file_system/drag.rs`, `drag_image_detection.rs`,
  `drag_image_swap.rs`. Self-drag consumption lives in `pane/drag-drop-controller.svelte.ts`.

## Must-knows

- **Pasteboard layout is decided ONCE per drag session by the source volume's locality, never per item.** Local sessions
  match Finder: file-url + `NSFilenamesPboardType`, NO `public.utf8-plain-text` (the path-text item made some browser
  upload widgets treat the drop as text, not a file: [issue #28](https://github.com/vdavid/cmdr/issues/28); don't re-add
  it). Virtual sessions (MTP, direct SMB, search-results) advertise NOTHING external apps can materialize except an
  `NSFilePromiseProvider`. Don't mix per-item; the policy is pure in `native_drag/type_plan.rs::plan_pasteboard_items`.
- **In-app drops never trust the pasteboard round-trip.** Virtual-volume paths are volume-relative
  (`/photos/sunset.jpg`) and round-trip through wry looking like local absolute paths, so the resolver mis-resolves to
  local and the dialog reads 0 bytes. `drag-drop.ts::recordSelfDragIdentity` stamps the true
  `{ sourceVolumeId, sourcePaths }` at drag start; `drag-drop-controller.svelte.ts::handleDrop` consumes it via
  `consumableSelfDragIdentity()` ONLY when `getIsDraggingFromSelf()` is true AND the recorded `sourceVolumeId` is a
  REGISTERED backend-real volume. The registry-membership gate (not a virtual-id string compare) is what makes a
  search-results self-drag fall through to the resolver. `FullList`/`BriefList` need the `sourceVolumeId` prop for the
  stamp.
- **`clearSelfDragIdentity()` must NOT run on `'leave'`** (only `cancelDragTracking` and the webview `'drop'` branch).
  The self-drag flag is reset on leave; clearing the record too would break exit-and-re-enter self-drags (re-entry
  restores the flag via the fingerprint).
- **Always publish `NSFilenamesPboardType` explicitly on the first dragging item for local sessions.** Stock
  `wry-0.54.x::collect_paths` reads only that legacy type and `unwrap()`s; AppKit advertises it as available via
  auto-derivation but fails to produce the list, so self-drag re-entry panics without it. Drop only once wry ships the
  [wry#1723](https://github.com/tauri-apps/wry/pull/1723) fix.
- **Source op mask must be permissive (`Copy | Link | Generic | Move`).** The destination's `draggingEntered:` is
  constrained to the source mask; terminals only accept `Copy`, so a Move-only mask makes them reject the drop and
  animate it back. macOS modifier keys arbitrate the actual op.
- **`public.file-url` must be set with `setString:` (the URL's `absoluteString`), not `setPropertyList:`.** The
  property-list path produces a value AppKit can't parse ("An invalid URL was found on the pasteboard") and breaks
  downstream `NSFilenamesPboardType` derivation.
- **Drop runs the shared destination guard (`pane/transfer-entry.ts::checkTransferDestinationGuard`) FIRST**, before any
  stat / volume resolution, so F5/F6, paste, and drop reject read-only/search-results destinations identically.
- **`resolveSourceVolumeId` NEVER ships a knowingly-wrong id**: when sources span volumes or resolution fails it returns
  `root` (honest unknown). Don't reintroduce the `sourceVolumeId = destVolumeId` placeholder; it stat'd the wrong shape
  and reported 0 bytes / 0 files.
- **Self-drag op override**: the swizzle returns our resolved `NSDragOperation` only while `SELF_DRAG_ACTIVE`; without
  it macOS always draws the green "+" copy badge inside our window even on a Move.
- **Swizzle calls are wrapped in `catch_unwind` + `warn_once`**: a panic across the FFI boundary mid-drag would crash
  the app. Keep the guards if you touch `drag_image_detection.rs`.
- **`startDrag()` resolves before macOS delivers drag events.** Don't clear self-drag state (rich image path, active
  flag) from async JS after `startDrag`; only `endSelfDragSession()` on drop/leave clears it. Temp PNGs survive the
  whole session (`pendingImageCleanup`).

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
