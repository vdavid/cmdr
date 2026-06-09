# Drag and drop

Handles native drag-and-drop for files: dragging out to other apps, receiving drops from external sources, and
pane-to-pane drags.

## Architecture

### Drag-out (initiating drags)

Two backend commands, both routed through `native_drag.rs` so the pasteboard payload is identical:

- **Single file** → `start_drag_paths` (frontend passes the file path directly)
- **Multi-file selection** → `start_selection_drag` (backend resolves paths from the listing cache, avoiding IPC
  overhead for large selections)
- **Multi-file paths-by-value** → `start_drag_paths` (the same single-file command, just with > 1 path). Used by the
  search-results pane: `staticEntries` panes have no backend listing for `start_selection_drag` to resolve indices
  against, but the FE already has absolute paths on each row, so we send them directly. Wired via the `PathsDragContext`
  variant in `drag-drop.ts`; FullList picks this branch automatically when `usingStaticEntries`.

Pasteboard layout is decided ONCE per drag session by the source volume's locality, never per item (a single drag can't
mix local and virtual items — single-pane selections, single-volume panes). The locality-aware composition is a pure,
unit-tested policy in `native_drag/type_plan.rs::plan_pasteboard_items`; `native_drag/mod.rs` is a thin executor of the
plan. The backend derives locality from `Volume::supports_local_fs_access()`: `start_selection_drag` reads it off the
listing's volume; `start_drag_paths` takes an optional `sourceVolumeId` (threaded from the FE drag-start path, which has
it since the recorded-identity work; `null` defaults to local).

- **Local sessions** (real local FS or OS-mounted shares — `file://` URLs are real) keep the legacy layout
  byte-for-byte, one `NSPasteboardItem` per file:
  - Every item: `public.file-url` (the URL's `absoluteString`): Finder, IntelliJ, etc. iterate items reading this.
  - First item only: `public.utf8-plain-text` with the full shell-escaped path list joined by spaces (terminals like
    Warp etc.) read this via `pasteboard.string(forType:)` and insert the text at the cursor.
  - Later items: `public.utf8-plain-text` with their own escaped path (so item-iterating consumers don't see
    duplicates).
  - First item only: `NSFilenamesPboardType` (legacy `NSArray<NSString>` of all paths). Required for stock wry's
    `collect_paths`, which reads only this type and `unwrap()`s if absent; see
    [wry#1723](https://github.com/tauri-apps/wry/pull/1723) for the upstream fix. Drop this once wry ships a release
    containing the fix and we bump our `tauri-runtime-wry`.
- **Virtual sessions** (MTP, direct SMB, search-results — paths with no local backing) advertise NOTHING external apps
  can materialize: no file-url, no text, no filenames, across EVERY item. A virtual path's `file://` URL is bogus and
  the legacy types are exactly what Finder turned into the `.textClipping` junk file. Promise-only items still fire
  wry's drop event with empty paths (no panic), so in-app self-drags keep working via recorded identity (below); an
  external drop fulfills the promise instead. The drag image and count badge are unaffected (one `NSDraggingItem` per
  file regardless; `setDraggingFrame:contents:` is writer-agnostic).

Source operation mask: permissive (`Copy | Link | Generic | Move`). macOS arbitrates the actual operation via modifier
keys (Alt → Copy, Cmd → Move, Ctrl-Alt → Link) and destination preference. Restricting the mask to a single op breaks
terminals (which only accept Copy).

Key files:

- `drag-drop.ts`: Mouse tracking, threshold detection, drag initiation
- `drag-out-event-bridge.ts`: Subscribes to the backend's `drag-out-session-started` / `drag-out-session-complete`
  events and turns each drag SESSION into one signs-of-life → completion toast (mounted from `(main)/+page.svelte`)
- `drag-out-toast.ts`: Pure `composeDragOutCompleteToast(payload)` — success counts via the shared transfer-toast
  composer, failure toast naming the file(s). Unit-tested
- `drag-image-renderer.ts`: Canvas rendering for rich OS drag preview (retina-aware, fading edges)
- `commands/file_system/drag.rs`: `start_drag_paths` and `start_selection_drag` Tauri commands; both hop to the AppKit
  main thread before calling into `native_drag::start_drag`
- `native_drag.rs`: Builds `NSPasteboardItem`s as above, wraps each in an `NSDraggingItem`, and begins the dragging
  session via a custom `CmdrDragSource` that returns the permissive op mask

### Drop-in (receiving drops)

Uses Tauri 2's `onDragDropEvent` (window-level) with DOM hit-testing to resolve target pane/folder.

Key files:

- `drop-target-hit-testing.ts`: Pure logic: `document.elementFromPoint()` + `data-drop-target-path` walk
- `drop-target-validation.ts`: Pure logic: blocks drops onto the source itself or into a descendant
- `drag-auto-scroll.ts`: Pure edge-band math for native drag auto-scroll. Full mode passes `axis: 'vertical'`; Brief
  mode passes `axis: 'horizontal'`.
- `DragOverlay.svelte` + `drag-overlay.svelte.ts`: Floating label near cursor
- `../modifier-key-tracker.svelte.ts`: Alt/Cmd/Shift state (DragDropEvent doesn't include modifiers; lives in parent
  `file-explorer/` directory)
- `drop-operation.ts`: Pure logic: resolves the `'move' | 'copy'` operation from source/target paths, the volumes list,
  and the current modifier state. Same function feeds the overlay label and the actual drop, so the displayed and
  executed operation can never disagree. `findVolumeIdForPath` is a raw longest-prefix matcher; `isSameVolume` (which
  `pickDropOperation` uses for the same-volume Move default) filters FAVORITES (`category === 'favorite'`) out of the
  candidate set first — a favorite's root is a real local path, so a Desktop→Documents drag is local→local (Move), not
  cross-favorite (Copy). Same favorites-blindness fix as `resolveSourceVolumeId`, kept at the affinity layer so the raw
  matcher stays pure.
- `drag-position.ts`: Corrects Tauri coords for docked DevTools (dev-only, zero overhead in prod)
- Integration in `DualPaneExplorer.svelte`

### Self-drag identity (in-app drops never trust the pasteboard round-trip)

An in-app drag puts the source listing's paths on the pasteboard. For a virtual volume (MTP, smb2-native SMB) those
paths are **volume-relative** (`/photos/sunset.jpg`), and after a round-trip through wry's native drop event they look
exactly like a local absolute path. The drop resolver can't tell them apart, mis-resolves the source to the local
volume, and the transfer dialog reports 0 bytes / 0 files (and the move-vs-copy badge can read wrong).

The fix: record the drag's **true identity** at drag start and consume it on our own drop, sidestepping the lossy
round-trip entirely.

- **Record (drag start).** Each `perform{SingleFile,Selection,Paths}Drag` in `drag-drop.ts` calls
  `recordSelfDragIdentity(sourceVolumeId, sourcePaths)`, storing `{ sourceVolumeId, sourcePaths, startedAt }` in module
  state. `sourcePaths` are the paths AS THE SOURCE VOLUME KNOWS THEM: the single path for a `single` drag, the
  FE-resolved paths for a `paths` drag, and the backend-resolved paths (`get_paths_at_indices`, the SAME set the drag
  itself uses) for a `selection` drag. `sourceVolumeId` is threaded as a prop into `FullList` / `BriefList` (`'root'`
  for a local pane, the `mtp-*` / SMB id for a virtual one, `'search-results'` for a snapshot pane).
- **Consume (drop).** `drag-drop-controller.svelte.ts::handleDrop` reads the recorded identity via
  `consumableSelfDragIdentity()` — but ONLY when `getIsDraggingFromSelf()` is true (the existing self-drag detection,
  NOT a parallel one) AND the recorded `sourceVolumeId` is a REGISTERED backend-real volume (present in `getVolumes()`).
  When trusted, `handleFileDrop` builds the transfer from `{ sourceVolumeId, sourcePaths }` and skips both the resolver
  and the local kind probe (a volume-relative path can't be stat'd locally — the approximate count shape is the honest
  fallback). The recorded source-volume ROOT also feeds `pickDropOperation`, so a same-volume MTP/SMB self-drag resolves
  to Move, not Copy.
- **Why the registry-membership gate.** It's what makes a `search-results` self-drag (virtual `'search-results'` id, but
  real ABSOLUTE paths that span volumes, with no single dispatchable source id) fall through to the resolver — which
  matches each absolute path to its real volume. That's a membership check, not a virtual-id string compare.
- **Clear (every termination).** `clearSelfDragIdentity()` runs on `cancelDragTracking` (ESC / mouseup-before-threshold)
  and on the webview 'drop' branch. It does NOT run on 'leave': the self-drag flag is reset there, so a lingering record
  can't be consumed unless re-entry restores the flag via the fingerprint — same lifecycle as the fingerprint, which
  keeps an exit-and-re-enter self-drag working.

External drops (no in-flight self-drag) keep the resolver path: their paths are genuine local absolute paths from Finder
et al.

### Drag-out of a non-local pane to external apps (file promises — downloads on drop)

Dragging a file or folder from an MTP / SMB pane OUT to Finder or the Desktop **downloads it there**, under the exact
filename the destination chose (Finder uniquifies collisions, "sunset 2.jpg", and we honor that leaf). Each dragged item
carries an `NSFilePromiseProvider` instead of a `file://` URL: the provider promises Finder "I'll produce this file's
bytes when you ask," and Finder asks by calling our delegate's `writePromiseToURL:` on drop, which streams the real
bytes off the device into the Finder-chosen destination. Multi-select works; folders download recursively. The drag
image and count badge are unaffected (one `NSDraggingItem` per item; `setDraggingFrame:contents:` is writer-agnostic).
The promise machinery, delegate, fulfillment service, and the completion toasts live in
[`src-tauri/src/native_drag/CLAUDE.md`](../../../../src-tauri/src/native_drag/CLAUDE.md). The pasteboard-layout and
folder-promise verdicts behind the design are recorded in
[`docs/notes/drag-out-promises-spike.md`](../../../../../../docs/notes/drag-out-promises-spike.md).

- **No more `.textClipping` junk file.** A virtual session advertises NO materializable representations besides the
  promise (no file-url, no text, no filenames), so an app that accepts neither promises nor our remaining types makes a
  clean no-op instead of the old garbage. The old layout published a bogus volume-relative `file://` URL plus a path
  string, and Finder materialized the string into `photos:sunset.jpg.textClipping`.
- **Terminals get no text from a virtual pane.** A volume-relative path is meaningless outside Cmdr, so virtual sessions
  intentionally publish no `public.utf8-plain-text` — dropping a phone file into Warp inserts nothing. Local-pane drags
  keep the terminal-text affordance.
- **In-app drops are unaffected** — they use the recorded self-drag identity above, NOT the pasteboard paths, and the
  promise-only layout still fires wry's drop event (empty path vec, no panic). So a virtual self-drag still works. The
  promise is fulfilled only when an EXTERNAL consumer (Finder) asks, so it never touches in-app behavior.
- **Completion feedback.** Finder shows nothing while a promise downloads, so Cmdr surfaces a signs-of-life in-progress
  toast when fulfillment starts and a completion (or failure) toast when the session drains — one toast per drag
  SESSION, not per file. See the native_drag doc § "Completion toasts".

Drop preparation runs through the shared transfer entry seam (`pane/transfer-entry.ts`), the SAME path F5/F6 and
clipboard paste use, so all three entry points prepare a transfer identically. On drop,
`pane/drag-drop-controller.svelte.ts::handleFileDrop`:

- **Runs the shared destination guard** (`checkTransferDestinationGuard`) FIRST. Dropping onto a read-only volume shows
  the same "Read-only device" alert F5 shows (not a copy dialog the backend would later reject); dropping onto a
  search-results pane shows the not-a-folder toast. The guard short-circuits before any stat / volume-resolution work.
- **Resolves the REAL source volume** via `resolveSourceVolumeId` (frontend `findVolumeIdForPath` longest-prefix →
  backend `resolve_path_volume` for the common parent → honest-unknown `root` default) and passes it to
  `buildTransferPropsFromDroppedPaths`. This is what feeds the dialog's byte scan (`startScanPreview`'s `sourceVolumeId`
  arg), so an MTP→local / local→MTP drop stats the right volume and the counters fill. The old hardcoded
  `sourceVolumeId = destVolumeId` placeholder stat'd the source paths as the wrong shape and reported 0 bytes / 0 files.
  `resolveSourceVolumeId` NEVER ships a knowingly-wrong id — when sources span volumes or resolution fails, it returns
  `root` (the honest unknown, today's degraded-but-correct behavior).
- **Fetches each dropped path's top-level kind** (file vs. folder) in one batched `stat_paths_kinds` IPC before opening
  the confirmation dialog, so both the dialog and the completion toast report the real "N files and M folders" split.
  The stat runs under the backend read timeout (2 s) and falls back to all-unknown on a hung mount, so it never blocks
  the drop. The split is all-or-nothing: if ANY path's kind is unknown (a virtual MTP/SMB path that landed on the
  pasteboard, a vanished entry, a stat timeout, or a length mismatch), `buildTransferPropsFromDroppedPaths` reverts the
  whole batch to the legacy approximate shape (`fileCount = count`, `folderCount = 0`), which makes the toast composer
  fall back to flattened file-count wording. Honest beats half-right — a partial split would misreport.

### Drag image detection (macOS-specific hack)

**Problem**: Tauri's `DragDropEvent` doesn't include drag image size. Need size to decide whether to suppress Cmdr's
overlay (large source preview) or show it (tiny/no preview).

**Solution**: Method swizzling on `WryWebView`'s `draggingEntered:`. Reads `NSDraggingItem.draggingFrame()` via
`enumerateDraggingItems`, emits `drag-image-size` event. Feature request to wry:
[wry#1669](https://github.com/tauri-apps/wry/issues/1669). If accepted, swizzle can be removed.

Key files:

- `drag_image_detection.rs`: Swizzle install + `draggingEntered:`/`draggingUpdated:`/`draggingExited:` overrides
- `drag_image_swap.rs`: Image swapping logic for self-drags (transparent inside window, rich outside)

## Key decisions

- **Decision**: Always show confirmation dialog on drop
  - **Why**: Drag-and-drop is imprecise. The dialog is the safety net regardless of which operation is preselected.
- **Decision**: Same-pane pane-level drops are no-ops
  - **Why**: Dropping onto a subfolder within the same pane is valid. The controller still keeps drag auto-scroll alive
    for same-pane pane-level no-ops, because scrolling is how the user reveals that valid subfolder target.
- **Decision**: Native drag auto-scroll is controller-driven and list-executed
  - **Why**: The controller owns drag lifecycle (`enter` / `over` / `drop` / `leave`) and can stop the
    `requestAnimationFrame` loop on every terminal path. The list owns layout-specific scroll state. FullList scrolls
    vertically when the pointer enters its top/bottom edge bands; BriefList scrolls horizontally when the pointer enters
    its left/right edge bands. After any scroll frame moves content, the controller re-runs hit testing at the last drag
    position so a stationary cursor can target newly revealed folder rows. Drop uses the final hit test's folder path,
    not stale highlight state.
- **Decision**: Block drops onto the source itself or into a descendant
  - **Why**: Dragging `/a/b` onto `/a/b` or into `/a/b/c` can't produce a sensible result. Invalid targets don't
    highlight and show "Can't drop here" in the overlay, matching the pre-existing same-pane no-op behavior. The check
    applies to both folder-row and pane-level targets, and covers external drags too.
- **Decision**: The ".." row is a regular folder drop target pointing to the parent path
  - **Why**: Gives users a quick "drop into parent" gesture. The source-descendant check exempts it naturally (the
    ancestor isn't a descendant of its children).
- **Decision**: Rich PNG drag image for external visibility, transparent 1x1 inside window
  - **Why**: Self-drags swap images mid-drag via `setDraggingFrame:contents:` (entered → transparent, exited → rich).
    DOM overlay provides feedback inside.
- **Decision**: Custom `native_drag/` module instead of the upstream `drag` crate
  - **Why**: The upstream crate writes only `public.file-url` per item and uses a single-op mask (Move OR Copy). That
    failed three ways: (1) terminals like Warp listen for `public.utf8-plain-text`, not file URLs, so drops were
    silently dropped; (2) wry's `collect_paths` reads `NSFilenamesPboardType` and panics if the auto-derivation fails;
    see [wry#1723](https://github.com/tauri-apps/wry/pull/1723) for the upstream fix; (3) terminals only accept
    `NSDragOperationCopy`, so a Move-only source mask makes them reject the drop entirely. For a LOCAL session our
    version advertises file-URL + shell-escaped text + legacy filenames per the layout above, and always publishes a
    permissive op mask. Finder/IntelliJ behavior is unchanged (they read file URLs); terminals get working text drops;
    macOS modifier keys arbitrate the operation natively. For a VIRTUAL session each item carries an
    `NSFilePromiseProvider` and no legacy types (see the locality-aware layout above), so an external drop downloads the
    real bytes instead of materializing garbage.
- **Decision**: Modifier keys tracked via NSEvent.modifierFlags
  - **Why**: Tauri doesn't expose modifier state in DragDropEvent. Emits `drag-modifiers` event only when state changes.
- **Decision**: Drop operation follows Finder's volume-aware default plus Alt/Cmd/Shift modifiers
  - **Default**: same volume → Move, cross-volume → Copy (matches Finder's behavior on a stock macOS install).
  - **Alt (Option)** held → force Copy. Beats Cmd/Shift if both are held: the user is asking for Copy.
  - **Cmd** held → force Move. Matches Finder's force-move modifier.
  - **Shift** held → force Move. Windows convention; included as a friendly accelerator for cross-platform users.
  - **Why**: Earlier we kept Copy-as-default for safety, but it created a confusing inconsistency: dragging out of Cmdr
    to the Desktop becomes Move (macOS arbitrates the outgoing operation from the source mask + modifiers), so the same
    gesture meant different things inside vs. outside the app. Matching Finder removes that surprise. The confirmation
    dialog still catches mistakes, so we don't lose the safety net.
  - The same `pickDropOperation` runs for both the live overlay (`handleDragOver`) and the actual drop (`handleDrop`),
    so the two can't diverge.
- **Decision**: Internal-drop modifier semantics align with the outgoing arbitration macOS does for us
  - **Why**: For drags out to other apps, AppKit arbitrates the operation from the source mask plus modifiers (Alt →
    Copy, Cmd → Move, Ctrl-Alt → Link). We own the choice for internal drops, so we replicate the same Alt/Cmd
    convention. Shift is an extra Move accelerator for Windows-trained users; Link isn't supported.
- **Decision**: Viewport position correction only in dev mode
  - **Why**: DevTools docked mode shrinks viewport but Tauri reports window-relative positions. Offset computed via
    `outerSize()` vs `innerHeight`. Zero overhead in prod.
- **Decision**: Self-drag op override: swizzle returns our resolved `NSDragOperation`, not wry's
  - **Why**: Wry's stock `draggingEntered:`/`draggingUpdated:` returns `NSDragOperation::Copy` unconditionally for file
    pasteboards. Without an override, macOS would always draw the green "+" copy badge inside our window even when the
    user is performing a Move. The swizzle in `drag_image_detection.rs` forwards to wry's implementation (so Tauri's
    `onDragDropEvent` keeps firing), then substitutes the return value with our resolved op when `SELF_DRAG_ACTIVE` is
    true. The frontend pushes the resolved op via `setSelfDragResolvedOperation` from both `handleDragOver` (target
    hover changes) and the `drag-modifiers` event handler (modifier-only changes), deduped to op transitions only so IPC
    traffic is minimal. External drag-in is unaffected (`SELF_DRAG_ACTIVE` is false then, so wry's default applies).

## Gotchas

- **Gotcha**: `startDrag()` resolves before macOS delivers drag events
  - **Why**: Self-drag state (rich image path, active flag) must NOT be cleared from async JS after `startDrag`. Only
    cleared on drop/leave via `endSelfDragSession()`. Temp PNG files survive entire drag session (cleanup deferred to
    `pendingImageCleanup`).
- **Gotcha**: Swizzle must catch panics across FFI boundary
  - **Why**: All native calls wrapped in `catch_unwind` + `warn_once` to prevent crashes mid-drag. Gracefully degrades
    if wry renames `WryWebView` or Apple removes deprecated APIs.
- **Gotcha**: `setDraggingFrame:contents:` modifications persist globally
  - **Why**: Transparent swap in `draggingEntered:` would remain visible outside without swap-back in `draggingExited:`.
    Session-level image (from `startDrag`) is separate.
- **Gotcha**: Re-entry detection uses fingerprint (count + first 5 paths)
  - **Why**: O(1) check on drag enter, avoids iterating 50k+ paths. Restores `isDraggingFromSelf` flag if match.
- **Gotcha**: Icon loading is async
  - **Why**: Canvas renderer preloads all icons in parallel. Falls back to geometric shapes (filled rect = file, open
    rect = folder) on cache miss.
- **Gotcha**: Middle-truncation preserves extensions
  - **Why**: `"very-long-filename.txt"` → `"very-lon…me.txt"`. Splits basename, keeps extension intact.
- **Gotcha**: External drags with large images suppress overlay
  - **Why**: If source preview is > 32x32, hide Cmdr's overlay. Self-drags always show overlay (OS image is transparent
    inside).
- **Gotcha**: Drag image shows 12 names max, overlay shows 20 max
  - **Why**: Different limits: canvas is fixed-size retina-aware, DOM is flexible. Truncation: first 8/10 + "and N
    more".
- **Gotcha**: `public.file-url` must be set with `setString:` not `setPropertyList:`
  - **Why**: Setting it via the property-list path (e.g., from `NSURL.pasteboardPropertyListForType:`) produces a value
    AppKit can't parse back into a URL. It logs "An invalid URL was found on the pasteboard" and breaks downstream
    derivations like `NSFilenamesPboardType`. Use `[item setString: url.absoluteString forType: "public.file-url"]`.
- **Gotcha**: Stock wry panics on self-drag re-entry without `NSFilenamesPboardType`
  - **Why**: `wry-0.54.x::wkwebview::drag_drop::collect_paths` reads only the legacy `NSFilenamesPboardType` and
    `unwrap()`s. AppKit advertises the type as "available" via auto-derivation but fails to produce the property list,
    hitting the unwrap. We always publish `NSFilenamesPboardType` explicitly on the first dragging item.
    [wry#1723](https://github.com/tauri-apps/wry/pull/1723) fixes this upstream by switching `collect_paths` to
    `readObjectsForClasses:[NSURL]options:`. Once that PR lands and the fix ships in a wry release we consume, drop our
    `NSFilenamesPboardType` publishing.
- **Gotcha**: Source op mask must include the destination's required operation
  - **Why**: The destination's `draggingEntered:` is constrained to the source's mask. Terminals only accept
    `NSDragOperationCopy`; if the source publishes Move-only, the drop is rejected and the drag animates back. Publish a
    permissive mask (`Copy | Link | Generic | Move`) and let macOS modifier keys arbitrate.
- **Gotcha**: The green "+" copy badge and the red multi-item count circle are macOS-rendered, not ours
  - **Why**: AppKit's dragging service composites both adornments on top of whatever drag image we hand it. There's no
    public API to disable, restyle, or recolor them: they're system UI. The "+" tracks the resolved `NSDragOperation`
    (`Copy` → green +, `Link` → curly arrow, `Move`/`Generic` → no badge); the count circle appears whenever there are
    > 1 `NSDraggingItem`s on the pasteboard. Even when the OS image is swapped to transparent for self-drags, the badges
    > still draw because they're separate sprites near the cursor, not painted onto the image surface. Don't try to
    > replace or skin them. Invest custom branding into `DragOverlay.svelte` instead, which is fully under our control.
- **Gotcha**: For cross-volume self-drags, the "+" badge may appear ~1–2 frames late
  - **Why**: Both `performSingleFileDrag` and `performSelectionDrag` seed the swizzle with `'move'` via
    `setSelfDragResolvedOperation` right before `startDrag`, so the same-volume case (the default, most common) shows no
    "+" from frame one. For cross-volume drags the resolved op is `'copy'`, but JS only learns the target volume after
    the first `handleDragOver`, so the badge flips from "no +" to "+" on the next `draggingUpdated:`, a slight "+"
    appearing late, ~5–30ms. Picked this direction over the reverse because a badge appearing later feels intentional,
    while a badge appearing-then-disappearing reads as a glitch.

## Platform support

macOS only. Backend commands and swizzle gated `#[cfg(target_os = "macos")]`. Drop receiving uses cross-platform Tauri
API but untested on other platforms. `drag-image-size` event never fires on non-macOS (frontend defaults to showing
overlay).
