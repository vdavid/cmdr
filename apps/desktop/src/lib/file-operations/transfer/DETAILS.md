# Transfer details

Pull-tier docs for `lib/file-operations/transfer/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in [CLAUDE.md](CLAUDE.md).

## File map

- **`TransferDialog.svelte`**: Destination picker, segmented Copy/Move toggle, pre-flight dry-run scan, upfront
  conflict-policy radios. Thin shell over the two `*.svelte.ts` factories below + `transfer-dialog-logic.ts`; owns the
  markup, the volume selector / path-input state, and the confirm/cancel wiring
- **`transfer-scan-state.svelte.ts`**: `createTransferScanState(deps)`: deep scan-preview orchestration (Size bar +
  file/dir/byte tallies). Owns the four scan-progress listeners, `start()` / `cancelPreview()` / `freeAndCleanup()` /
  `cleanup()` lifecycle, the awaitable `scanStarted` promise, and the Copy/Move toggle `$effect` that (re)starts or
  cancels the preview around a same-volume move. Deps are getter callbacks (source paths, sort info, source volume id,
  `isSameVolumeMove`, `confirmed`, `destroyed`); state via getters
- **`transfer-conflict-check.svelte.ts`**: `createTransferConflictCheck(deps)`: cheap top-level conflict-check state
  machine (conflict counts, merge-folder count, type-mismatch flag, bulk-skip names). One `check()` runs on mount in
  parallel with the deep scan and stays decoupled from it. Deps are getter callbacks + a logger; state via getters
- **`transfer-dialog-logic.ts`**: Pure helpers lifted from the dialog: `getPathValidationError()` (subfolder /
  already-here checks) and `formatSpaceInfo()` (free-of-total line, byte formatter injected). No reactivity, no IPC
- **`TransferProgressDialog.svelte`**: Execution shell: dual progress bars, progress stages, scan-phase body, the
  direction header, action buttons (cancel/rollback, pause/resume, queue), and the dialog title. Thin over
  `transfer-progress-state.svelte.ts` (the state machine) + `TransferConflictDialog.svelte` (the conflict UI); owns only
  display-derived values (labels, stage chips, `isSameVolumeMove`), the focus-trap `keydown`, and the
  `onMount`/`onDestroy` → `start()`/`destroy()` wiring
- **`transfer-progress-state.svelte.ts`**: `createTransferProgressState(config)`: the execution state machine, headless
  and testable. Owns the six write-event listeners + the `operations-changed` stream, the `operationId`-scoped event
  buffering/replay, the phase machine (scanning → active → flushing, plus rolling_back), the cancel/settle close-out
  (`MIN_DISPLAY_MS` floor, slow-label + last-resort fallback timers), pause/resume, background-to-queue (incl.
  auto-queue behind a busy lane), the conflict prompt, and the scan-wait path (`waitForScanThenStart`). Takes static
  per-operation config + the outcome callbacks (`onComplete`/`onCancelled`/`onError`/`onQueue`); exposes `start()`,
  `destroy()`, the handler methods, and state via getters. `backgrounded` and `destroyed` are plain `let`s (read live
  during disposal), NOT `$state` — see the module header for why
- **`TransferConflictDialog.svelte`**: Self-contained conflict-resolution UI (the comparison grid + the 4×2 button grid
  plus the bottom rollback/cancel row). Props: the `conflictEvent`, operation-type flags (`isCopy`/`isMove`/
  `isSameVolumeMove`), the `isCancelling`/`isResolvingConflict` disable gates, and `onResolve`/`onCancel` callbacks.
  Owns its own size-color helper, the file-over-folder warning, and all conflict CSS
- **`TransferErrorDialog.svelte`**: Modal that renders entirely from the typed `WriteOperationError`, category-colored
  container, optional Retry button
- **`FallbackErrorContent.svelte`**: Renders the FE-derived message (`getUserFriendlyMessage`) for the typed
  `WriteOperationError`
- **`ArchivePasswordDialog.svelte`**: Masked password prompt shown when a copy/move source is inside an encrypted
  archive, in place of the generic error dialog. Props: `archiveName`, `wrongAttempt` (distinct re-prompt copy),
  `onSubmit(password)`, `onCancel`. See § "Archive-password prompt"
- **`ScanPhaseBody.svelte`**: Scan-phase tallies (files/dirs/bytes), throughput readout, current directory, spinner.
  Shared by both scan-phase code paths
- **`DirectionIndicator.svelte`**: Arrow graphic for source → destination (operation-agnostic, reused by
  `DeleteDialog`). Optional `sourceLabel` / `destinationLabel` props override the path-basename label; the transfer
  dialogs pass them so a volume root renders the volume display name, not a raw machine id (an MTP storage id like
  `65538`)
- **`transfer-dialog-utils.ts`**: `generateTitle()`, `deriveTransferLabel()` (volume-root-aware direction-header label),
  `toBackendIndices()` / `toBackendCursorIndex()` ".." offset helpers, `toVolumeRelativePath()`
- **`transfer-error-messages.ts`**: Operation-specific error strings used by `FallbackErrorContent`
- **`transfer-complete-toast.ts`**: Pure `composeTransferCompleteToast({...})`: "Moved 1 file and 3 folders" — splits
  the top-level items by type (never interior counts); omits zero parts; skip suffix is file-only (folders always
  merge); falls back to flattened file-count wording only when the split is unknown (a top-level kind probe came back
  partial). F5/F6 supply the split from real selection stats; drag-and-drop and clipboard paste supply it from a batched
  `stat_paths_kinds` / `read_clipboard_files` kind probe
- **`*.test.ts` / `*.a11y.test.ts`**: Vitest unit tests (utility + component) and a11y assertions

## How transfer flows

1. **TransferDialog** (destination picker + dry-run scan)
   - Pre-fills destination from the opposite pane.
   - The segmented Copy/Move toggle is always shown so the user can flip the operation regardless of how the dialog was
     triggered (F5/F6, command palette, drag-and-drop).
   - Validates path structure via `validateDirectoryPath()` from `$lib/utils/filename-validation` (empty, absolute, null
     bytes, length limits), then checks logical constraints (subfolder, same location).
   - Optional dry-run scan to detect conflicts upfront. Shows sampled conflicts (max 200) with streaming progress.
   - User makes conflict decisions before operation starts via a wrap-friendly flexbox of radios: "Skip all", "Overwrite
     all", "Overwrite all smaller", "Overwrite all older", "Ask for each". When `totalConflictCount === 1`, the radio
     labels drop "all" ("Skip", "Overwrite", "Overwrite if smaller", "Overwrite if older") and "Ask for each" becomes
     "Ask later" since a single conflict can't be asked "for each". The conditional policies map to the typed
     `ConflictResolution` variants `overwrite_smaller` / `overwrite_older`. See the BE doc § "Key patterns and gotchas
     (shared)" for the strict-comparison / fail-closed contract.
   - **Folders always merge; the upfront check classifies collisions.** The conflict check (`conflicts.check()`, from
     `transfer-conflict-check.svelte.ts`) runs on mount **in parallel with the scan preview** (it's one cheap dest
     listing, not the recursive byte scan — `conflictCheckPromise` is assigned synchronously in `onMount` BEFORE the
     auto-confirm branch so the MCP fast path dispatches with `conflictNames` populated). Each collision is classified
     by the backend-resolved `sourceIsDirectory` / `destIsDirectory` flags (the BE resolves real per-item types + sizes
     from the source volume via one batched stat when the check passes `sourceVolumeId` + `sourcePaths`):
     - **dir + dir** → a silent merge, NOT a conflict. Surfaced as an informational line ("N folders will merge with
       existing folders"); never counted in `totalConflictCount`; never forwarded as a bulk-skip name (a merging folder
       must not be skipped wholesale).
     - **file + file / cross-type (file↔folder)** → a real conflict. Counts toward `totalConflictCount` and feeds the
       `preKnownConflicts` bulk-skip list.
     - The file-policy radios show when there's a real conflict OR a folder merge — a merge can surface file clashes
       mid-operation the upfront (top-level-only) check can't see, and the radios pre-answer them.
     - **Cross-type guardrail.** When a real conflict is a type mismatch AND the user selects "Overwrite all", a red
       warning appears (mirrors the per-file dialog's file→folder warning): overwriting replaces items of a different
       type, including folder contents.

2. **TransferProgressDialog** (operation execution)
   - If `scanInProgress`, subscribes to scan preview events (`scan-preview-progress`, `scan-preview-complete`, etc.) to
     continue observing the same scan that `TransferDialog` started. Shows scanning progress UI until scan completes,
     then dispatches the operation (guaranteed cache hit). Handles the race condition where the scan completes between
     dialogs via `checkScanPreviewStatus()`.
   - Calls `copyFiles()` or `moveFiles()` based on `operationType`.
   - Subscribes via `onWriteProgress`, `onWriteComplete`, `onWriteError`, `onWriteCancelled`, `onWriteSettled`,
     `onWriteConflict` wrappers (which internally listen to Tauri events). Uses a `BufferedEvent` discriminated union
     (`{ type: 'progress'; event: WriteProgressEvent }`, etc.) to buffer events until the `operationId` is known.
   - Dual progress bars (size + file count). Speed (both bytes/s and files/s) and ETA come pre-computed from the backend
     (`write_operations/eta.rs`) on every `WriteProgressEvent`; the dialog renders the numbers and applies a tiny
     display low-pass to the ETA to prevent flicker. No FE-side math. See BE § "ETA + throughput".
   - Dynamic stage indicator: "Scanning" → "Copying" (+ "Cleaning up" for cross-FS move).
   - **Flushing phase.** When a `write-progress` event arrives with `phase: 'flushing'`, the dialog title shows
     **"Writing the last piece..."** (exact copy). This is the backend's closing `fdatasync` over the freshly written
     destinations — on slow media (USB sticks, SD cards) it's a real multi-second pause, so the bar must not sit frozen
     at 100% pretending the work is done. The phase maps back to the active stage chip (copying/moving) in
     `getStageStatus`, since it's the tail of the copy, not a separate chip. Shown for both copy and move. Pinned by
     `TransferProgressDialog.flushing.test.ts`. See the BE doc § "Durability" for what the flush actually does.
   - **Scanning-phase UI** (both `waitingForScan` and `phase === 'scanning'` paths): rendered via `ScanPhaseBody`. Shows
     source path, running tallies (`bytesFound / filesFound / dirsFound`), FE-computed throughput from `ScanThroughput`
     (`../scan-throughput.ts`), and a spinner. Current directory (`event.currentDir`) renders above the filename so the
     user sees where in the tree the walker is. Title is reframed per operation: "Verifying before copy…", "Counting
     items to delete…", etc. The backend still emits `expectedFilesTotal` / `expectedBytesTotal` on scan events but the
     FE ignores them — the bar this used to drive was visually indistinguishable from the destructive-phase bar and read
     as "already deleting".
   - Conflict resolution inline (if using `Stop` mode instead of dry-run). The per-file dialog has a 2-column grid: left
     column is the single-file action (`Skip` / `Rename` / `Overwrite`), right column is the apply-to-all variant
     (`Skip all` / `Rename all` / `Overwrite all`). A 4th row holds the two conditional bulk actions
     (`Overwrite all smaller` / `Overwrite all older`), which are always apply-to-all by design (no single-file variant;
     the bulk semantic is the point).
   - Cancel button → rollback transaction (user chooses keep/rollback).
   - **Rollback is DISABLED for same-volume volume moves.** `isSameVolumeMove` is a move where source and destination
     are the SAME non-default volume (one smb2 share / one MTP device). The backend handles these as a server-side
     `volume.rename` rename-merge with NO rollback support — it stops without reversing and reports
     `rolled_back: false`. So both Rollback affordances (the conflict-section footer and the main footer) render the
     Rollback button disabled with the tooltip "Rollback is not available for same-volume moves" (the disabled button is
     wrapped in a span so the tooltip still fires — disabled buttons swallow their own pointer events). Plain Cancel
     stays reachable in both spots; in the conflict footer, where Rollback would otherwise be the only button, a plain
     Cancel renders alongside the disabled Rollback. Local→local same-FS moves keep a live Rollback (real
     `MoveTransaction` rollback), so the default local volume is excluded; cross-volume moves and copies are unaffected.
     Pinned by `TransferProgressDialog.rollback.test.ts`.

3. **TransferErrorDialog** (error display)
   - Renders entirely from the typed `WriteOperationError` (`WriteErrorEvent` carries no prose): title, message, and
     suggestion via `getUserFriendlyMessage` / `FallbackErrorContent`; category + retry classification via
     `getErrorDisplayMeta` (both in `transfer-error-messages.ts`). All words live on the FE.
   - Container colors and icon vary by category: error-bg + CircleAlert (`serious`), warning-bg + TriangleAlert
     (`transient`), neutral secondary-bg + Info (`needs_action`).
   - "Retry" button shows when `category === 'transient'` or the variant's `retryHint` is true.
   - `getErrorDisplayMeta` mirrors the category/retryHint the Rust write-error mapper assigned per variant; keep the two
     in step if a `WriteOperationError` variant is added.

## Archive-password prompt

Copying or moving a source out of an encrypted archive (legacy PKWARE ZipCrypto zip today) needs a password before the
extract can decrypt. The backend raises a typed `WriteOperationError` of type `archive_needs_password` carrying the
source `path` and a `wrongAttempt` flag; the frontend turns that into a prompt-and-retry loop instead of the generic
error dialog.

- **Interception is a branch in `handleTransferError`** (`pane/dialog-state.svelte.ts`), NOT in the transfer dialogs.
  When `error.type === 'archive_needs_password'`, it shows `ArchivePasswordDialog` and returns before the generic-error
  path. It deliberately keeps `transferProgressProps` alive (only unmounting the progress dialog, which is safe because
  the write-error already settled the op) so the same operation can be re-dispatched. The archive lives on the source
  pane's volume (an archive pane keeps its parent drive's `volumeId`), so
  `parentVolumeId = transferProgressProps. sourceVolumeId`; `archivePath` is the errored source `path`
  (`set_archive_password` accepts the archive file OR any inner path). The prompt names the archive via
  `archiveNameFromPath` (the leftmost archive-boundary segment).
- **Submit → store then re-dispatch.** `handleArchivePasswordSubmit` calls `setArchivePassword`, then re-shows the
  progress dialog with the same props but `previewId: null, scanInProgress: false` — the first dispatch consumed the
  scan preview, so the retry re-scans the archive index (fast; scanning reads the index without decrypting). A wrong
  password makes the backend raise `archive_needs_password` again with `wrongAttempt: true`, so the interception fires a
  second time and the dialog re-prompts (its distinct copy, empty field via a fresh mount).
- **Mid-transfer wrong password.** ZipCrypto's open-time check false-accepts ~1/256, caught later at end-of-stream CRC,
  so a `wrongAttempt: true` error can arrive AFTER progress started. The interception is in the running-op error path,
  so this is handled the same as an up-front rejection — no separate pre-flight branch.
- **Cancel settles cleanly.** `handleArchivePasswordCancel` calls `clearArchivePassword` (forget the archive password)
  and runs the same tail a dismissed transfer error does — refresh both panes, drop the source-pane operation snapshot
  and selection, null the props, refocus — so nothing looks stuck. The op already terminated on the backend (the
  write-error settled it), so there's no running op to cancel.
- **AES archives don't reach here.** A WinZip-AES zip or AES 7z returns a typed `Unsupported`, which flows through the
  ordinary unsupported/friendly-error path, not this prompt (a prompt that can't succeed would be dishonest). See the
  polish spec item 2 for the dependency conflict deferring AES.

Backend counterpart (decrypt path, the typed signal, per-archive password storage + LRU lifetime):
[`volume/backends/archive/DETAILS.md`](../../../../src-tauri/src/file_system/volume/backends/archive/DETAILS.md) §
"Password-protected archives".

## Key decisions

### One transfer entry seam for F5/F6, drag-and-drop, and paste

Three entry paths start a transfer, and they all prepare it through `pane/transfer-entry.ts` so they can't drift:

- **F5/F6** (`pane/file-operation-commands.ts::openTransferDialog`) — real volume ids from the listing, listing-stats
  counts, opens `TransferDialog` (destination picker).
- **Drag-and-drop** (`pane/drag-drop-controller.svelte.ts::handleFileDrop`) — absolute dropped paths, opens
  `TransferDialog`. See `file-explorer/drag/CLAUDE.md`.
- **Clipboard paste** (`pane/clipboard-operations.ts::pasteFromClipboard`) — skips `TransferDialog` and goes straight to
  the progress dialog (paste has no destination picker, that's by design), but still runs the same guard.

`transfer-entry.ts` exposes two pure functions every path calls:

- **`checkTransferDestinationGuard(destVolumeId, volumes)`** — the shared destination guard chain. Order: search-results
  refusal (not-a-folder toast, gated `!canPasteInto` scoped to the `search-results` kind so the wording stays correct)
  then read-only alert (off `VolumeInfo.isReadOnly`). Returns `{ ok: true }` or a `{ ok: false, alert | toast }` the
  caller surfaces through its own dialog/toast plumbing. **The copy is the E2E-asserted contract — don't reword it.** An
  unknown destination id (no `VolumeInfo`) is allowed through: we can't prove read-only, and blocking on "unknown" would
  break a transfer to a freshly-mounted volume.
- **`resolveSourceVolumeId(paths, volumes, resolvePathVolume)`** — resolves the REAL source volume for dropped/pasted
  paths so they carry the same accurate `sourceVolumeId` an F5 transfer does. FAVORITES (`category === 'favorite'`) are
  filtered out of the candidate set first: they're picker-only pseudo-volumes the backend can't dispatch against, so a
  path under `~/Desktop` must resolve to its BACKING real volume (`root`), not the non-existent `fav-desktop` (dropping
  a Desktop file used to fail with "Source volume 'fav-desktop' not found"). Then frontend longest-prefix
  (`drag/drop-operation.ts::findVolumeIdForPath`, handles MTP-shaped paths) → backend `resolve_path_volume` for the
  common parent when no registered root matches → `root` (the honest unknown). NEVER returns a knowingly-wrong id: when
  per-path matches disagree (sources span volumes) or resolution fails, it returns `root`, which gives today's
  degraded-but-correct behavior. The drop path feeds the result into `startScanPreview`'s `sourceVolumeId` arg via
  `TransferDialog`, so the byte scan stats the right volume (a cross-volume drop's counters fill instead of reading 0).
  This resolver runs only for EXTERNAL drops and paste; an in-app self-drag bypasses it via the recorded self-drag
  identity (the drop carries the source volume + volume-relative paths directly — see `file-explorer/drag/CLAUDE.md` §
  "Self-drag identity").

The paste path keeps its MTP-specific refusal ("Use F5 to copy files to MTP devices") SEPARATE and BEFORE the shared
guard, because that toast points the user at the F5/F6 flow paste lacks; the shared guard then handles read-only /
search-results destinations uniformly.

### Unified components for Copy + Move

Copy and Move share 95%+ of UI/flow. Differences:

- Labels ("Copy" vs "Move")
- Backend command (`copyFiles()` vs `moveFiles()`)
- Post-completion: move refreshes both panes (source files gone)
- Cross-FS move has an extra "Cleaning up" stage

Parameterizing by `operationType` avoids duplication and guarantees UX consistency.

### Compress mode (the Transfer dialog's third operation)

Compress rides the SAME dialog/progress/state components as copy/move via a third `operationType: 'compress'`; its
"Compress" identity is frontend-only (title, toggle, confirm label, the `file.compress` command). The backend reuses
`WriteOperationType::ArchiveEdit` — see
[`write_operations/DETAILS.md`](../../../../src-tauri/src/file_system/write_operations/DETAILS.md) § "Compress = seed an
empty zip, then copy-into" for the seed mechanism. The user-visible differences from copy/move:

- **The path field is a new FILE, not a destination folder.** It defaults to the other pane's folder plus a suggested
  `<name>.zip` (`initialEditedPath` + `suggestCompressArchiveName`) and stays editable. Suggested name: single source →
  `<basename>.zip`; multiple → `<source-directory-basename>.zip`, falling back to the first selection's basename at a
  volume root. The extension is never stripped, so a `.zip` source becomes `data.zip.zip` (a NEW archive) and a dotted
  folder name is never mangled. `transfer-compress-name.ts` is a pure, unit-tested helper.
- **Dest-exists overwrite, NOT the conflict-policy UI** (decided; the multi-file skip/overwrite/rename policy is about
  files landing INTO a folder, which is meaningless when creating ONE new file). Compress skips
  `transfer-conflict-check` entirely and instead runs `createTransferDestExistsCheck` on the target `.zip`, surfacing a
  yellow "a file with this name is already here — Cmdr will replace it" warning (`targetWillBeOverwritten`); the
  conflict-policy radios never render. The inner-conflict policy passed to the backend is a fixed `overwrite` constant
  (a fresh empty zip has no entries, and two sources in one folder can't share a name).
- **Auto-confirm never silently overwrites (data-safety gate).** For the MCP `compress {autoConfirm}` path,
  `handleConfirm(isAuto=true)` proceeds unattended ONLY when the target doesn't already exist; if it does, it clears
  `confirmed` and leaves the dialog open for the user to decide. The MCP tool's composed ack
  (`GenerationAdvancedOrSoftDialog`) honestly reflects both outcomes — see
  [`src-tauri/src/mcp/executor/ack.rs`](../../../../src-tauri/src/mcp/executor/ack.rs). Don't refactor this gate away.
- **Confirm routes to `compressFiles`**, not `copyBetweenVolumes`
  (`transfer-progress-state.svelte.ts::dispatchCompress`). One command handles local and (later) remote sources; the
  scan preview still runs for the Size bar.
- **A compression-level slider shows in compress mode only** (`CompressLevelControl.svelte`, below the scan tallies). It
  frames the shared `SettingSlider` with "Faster"/"Smaller" end labels and binds to `behavior.archiveCompressionLevel`
  by id, so the dialog and the Settings › Behavior › Archives row are ONE persisted value with no dialog-local state —
  moving either reflects in the other live. `createTransferProgressState` reads the setting once at dispatch and passes
  `compressionLevel` in the op config for compress, copy, AND cross-volume move (one uniform level for every user-driven
  zip write; the backend ignores it for non-archive copies). The level's effect on the archive (added-entries-only,
  clamped 1..=9, `None` = crate default 6) is single-sourced in
  [`write_operations/DETAILS.md`](../../../../src-tauri/src/file_system/write_operations/DETAILS.md) § "Archive edits" →
  the mutation `DETAILS.md`.
- **An explicitly-approximate estimated size shows in compress mode only** (`CompressEstimateLine.svelte`, beside the
  scan tallies). The backend samples it once during the deep scan (local sources only; suppressed for remote) and ships
  per-class level-6 subtotals on `scan-preview-complete`; `transfer-scan-state` exposes them as `estimatedBytes`. The
  line re-scales to the selected level via `compress-estimate-scaling.ts` with no re-scan (it subscribes to the same
  `behavior.archiveCompressionLevel` setting the slider writes), shows a loading affordance while a local scan runs, and
  renders nothing when the estimate is absent. The sampler, budgets, and level curve are single-sourced in
  [`write_operations/DETAILS.md`](../../../../src-tauri/src/file_system/write_operations/DETAILS.md) § "Compressed-size
  estimate".

### Same-FS move optimization

When source and destination are on the same filesystem (checked via `metadata.dev()`), backend uses instant `rename()`.
Frontend handles this by:

- Skipping progress dialog if operation completes before render
- Showing brief success toast instead
- Still doing conflict scan upfront in dry-run mode (just `exists()` checks, ~100 ms for 10k files)

### Same-volume move skips the deep scan preview

`isSameVolumeMove = activeOperationType === 'move' && sourceVolumeId !== DEFAULT_VOLUME_ID && sourceVolumeId === selectedVolumeId`
(derived in `TransferDialog`, no extra prop). For a same-volume move the backend does a server-side rename-merge that
transfers zero bytes, so the deep recursive scan preview — which exists only to feed the Size bar — is pure waste. On a
NAS it used to cost 30–40 s of "Verifying before move…" before a 100 ms rename. So:

The `DEFAULT_VOLUME_ID` exclusion is load-bearing and mirrors the same guard in `TransferProgressDialog`'s
`isSameVolumeMove`: a local→local move (root → root) is NOT a server-side rename. The backend's local move path
**consumes** the preview cache via `config.preview_id`, and the dialog's tallies come from the preview — so cancelling
it for a local→local move both zeroes the dialog counters and forces a backend re-scan. Local→local keeps the deep
preview running.

The scan-preview machinery (the listeners, `start()` / `cancelPreview()`, the toggle `$effect`, the awaitable
`scanStarted` promise) lives in **`transfer-scan-state.svelte.ts`** (`createTransferScanState`), and the conflict-check
machinery in **`transfer-conflict-check.svelte.ts`** (`createTransferConflictCheck`). `TransferDialog` instantiates both
synchronously during init (so the scan factory's internal `$effect` lands in the component's effect-tracking context,
the L3 pattern), passes its reactive inputs as getter callbacks, and reads state back through getters. The dialog keeps
`isSameVolumeMove` as its own `$derived` (it folds in the `DEFAULT_VOLUME_ID` exclusion); the scan factory only reacts
to the boolean.

- `onMount` calls `scan.start()`, which starts the deep preview only when NOT a same-volume move.
- The scan factory's `$effect` keyed on `isSameVolumeMove` handles Copy/Move (or destination-volume) toggles AFTER
  mount: flipping to a same-volume Move **cancels** the in-flight preview (`cancelPreview()` evicts it without touching
  the independent conflict check); flipping away (to Copy, or a cross-volume Move) **(re)starts** it (Copy genuinely
  needs byte totals).
- `handleConfirm` for a same-volume move dispatches IMMEDIATELY with `previewId = null` and `scanInProgress = false`, so
  `TransferProgressDialog` never enters `waitForScanThenStart` — it calls `startOperation()` directly (no scan
  listeners, no gating). It still awaits the cheap conflict check for `conflictNames`.
- The cheap top-level conflict check (decoupled from the deep preview) keeps running independently on mount, so a
  same-volume move still surfaces "N folders will merge" and the file-policy radios. This decoupling is the prerequisite
  that lets us cancel the deep preview without degrading the conflict UX.
- Size bar: `bytesTotal = 0` already hides it (`{#if bytesTotal > 0}`), honest for a rename. The progress dialog reads
  with Files-only progress; the complete toast counts top-level items (a moved folder counts as one item).
- Pinned by `TransferDialog.test.ts` § "same-volume move scan gating" (no scan started for a same-volume move; the
  preview starts for a same-volume copy; toggle both directions cancels/restarts; immediate dispatch with
  `previewId = null` / `scanInProgress = false`).

### `data-scan-state` marker on the tallies element

`TransferDialog`'s `.scan-stats` element carries a `data-scan-state` attribute (`counting` | `done` | `skipped`) derived
from the existing `scanComplete` / `isSameVolumeMove` state — NO new wire event. It's the race-free "counting done"
signal E2E uses: the shared `expectDialogCounters(tauriPage, …)` helper polls it to a terminal state before asserting
the counter line, so an assertion never fires against a partial in-flight tally.

- `done` → the deep scan finished; the tallies are final. `done` wins over `skipped` (a same-volume COPY still scans).
- `skipped` → no deep scan runs (a same-volume move renames server-side, zero bytes), so the tallies legitimately stay
  at 0. The helper only accepts this state when the caller opts in with `allowSkipped`.
- `counting` → a scan is in flight or about to start on mount.

Pinned by `TransferDialog.test.ts` § "data-scan-state marker" (counting → done, the skipped fast path, and the counting
→ skipped toggle).

### Destination path: home shortcut, long-form display, and "will be created" warning

The destination box (`editedPath`) accepts the home shortcut as well as absolute paths: `validateDirectoryPath` passes a
leading `/`, a bare `~`, or `~/…`. `~` is the app's internal stand-in for the home dir; the backend expands it on
execution (the local `copy_files`/`move_files` commands always did, and `copy_between_volumes`/`move_between_volumes`
now expand a leading `~` for a LOCAL destination via `expand_local_dest`).

Two niceties on top:

- **Home shows as its long form.** On mount the dialog resolves `homeDir()` and, when `editedPath` is exactly `~` (the
  destination pane sitting at home root), replaces it with the absolute path (`/Users/me`) — a bare `~` in the box reads
  as a glitch. A `~/sub` path keeps its short form; only the exact-home case expands. Done before the scan and conflict
  check so they run against the absolute path.
- **Yellow "this folder will be created" warning.** A debounced (`createDebounce`, 300 ms) `pathExistsChecked` probe of
  the resolved destination flips `targetMissing`. When the path is structurally valid (no red `pathError`) but the
  folder doesn't exist, the box gets a yellow outline (`.path-input.has-warning`) and a yellow message line
  (`.path-warning`, keys `targetWillBeCreated{Copy,Move}`). The red error always wins — the two never show at once. A
  timeout is inconclusive (hung mount), so it stays quiet rather than over-promising. A monotonic `existsCheckSeq` drops
  a stale probe that lands after a newer keystroke.

Backend counterpart: every transfer path creates a missing destination (and ancestors) before transferring — the local
copy/move paths via `ensure_destination_dir` (`write_operations/validation.rs`), and the cross-volume +
same-volume-rename pipelines via `Volume::create_directory_all` (recursive mkdir on the dest volume, works on local,
SMB, MTP, in-memory). So the warning is honest for EVERY destination type, which is why it's no longer gated to local
destinations (there's no `isLocalDestination` check — `showTargetWarning` keys only off `targetMissing` + no
`pathError`).

### Index conversion for ".." entry

When the directory has a parent entry shown at index 0, frontend indices are offset by +1 from backend:

- Frontend `[0, 1, 2, 3]` with `hasParent=true` → Backend `[-1, 0, 1, 2]` → filtered to `[0, 1, 2]`
- Index 0 with `hasParent=true` is always the ".." entry (backend index `-1`, invalid)
- `toBackendCursorIndex(0, true)` returns `null` to signal no-op

## Gotchas

- **Always use batch IPC for selection lookups.** `get_paths_at_indices` (paths only) and `get_files_at_indices` (full
  `FileEntry` objects) fetch all selected items in a single IPC call. Never loop over `getFileAt` per-index; with 50k
  selected files, per-file IPC takes 5-10 seconds. Batch calls take ~1 ms regardless of count.
- **MTP move is interleaved copy + delete per file.** Moves involving MTP volumes copy and then delete each file
  individually (not copy-all-then-delete-all). Minimizes duplicates on partial failure: if it fails mid-way, only the
  current file exists in both places. The progress UI shows three stages (Scanning → Copying → Removing source). If copy
  succeeds but delete fails, the user keeps files in both places (safer than losing data). Rollback is hidden during the
  delete phase since the copy is already done.
- **Dry-run conflict sampling.** If >200 conflicts, `DryRunResult.conflicts` contains a random sample. Check
  `conflictsSampled: true` and `conflictsTotal` for the exact count.
- **Progress dialog edge case.** Same-FS move completes so fast that the complete event may fire before the dialog
  mounts. Handle by checking operation status on mount and showing toast if already done.
- **Source pane refresh.** Move operations must refresh **both** panes post-completion (source files disappeared). Copy
  only refreshes destination.
- **Rollback / Cancel buttons disable during settle window.** `TransferProgressDialog` holds open for
  `MIN_DISPLAY_MS = 400 ms` after `write-complete` so the user can read the final state. During that window, both Cancel
  and Rollback buttons must be disabled (`disabled={isCancelling || operationSettled}`); a click here hits a backend
  whose operation state was already removed, so it's a no-op but briefly flashes "Rolling back..." giving false
  feedback. `operationSettled` is a `$state(false)` that flips when the operation reaches a terminal state.
- **Cancel close is two-condition: `write-cancelled` + `write-settled`.** When the user clicks Cancel (without
  rollback), `TransferProgressDialog` does NOT close immediately. It keeps the "Cancelling…" label up until both events
  have arrived for this `operationId`, then applies the existing `MIN_DISPLAY_MS` floor and closes via
  `onCancelled(filesProcessed)`. After 200 ms of waiting, the label gains a clarifying tail: "Cancelling… (finishing USB
  transfers)". The BE-side contract — settle fires after a fully-torn-down spawn task, even on panic — lives in the BE
  doc § "Settle contract". Race protection: if `write-settled` arrives before `write-cancelled` (shouldn't happen, but
  is defensive), the dialog buffers it and closes only after `write-cancelled` has been processed. Complete / error
  paths are unchanged: they still close on the existing `MIN_DISPLAY_MS` gate without waiting for settle. Why it
  matters: the original incident was an MTP delete cancel followed by an immediate second F8 — the device was still
  mid-teardown, the second op queued behind the 17 s tail, hit the 30 s op timeout, and wedged the USB session.
- **Scan preview reuse.** `TransferDialog` starts a scan preview on mount. If the user confirms before the scan
  finishes, the scan keeps running (`TransferDialog` sets `confirmed = true` and skips cancellation in `onDestroy`).
  `TransferProgressDialog` picks up listening to the same scan events via the `scanInProgress` prop.
  `waitForScanThenStart` subscribes to the scan events first, then awaits `checkScanPreviewStatus()`. Both the
  `scan-preview-complete` listener AND the status check can signal "ready to start", especially for fast scans that
  complete during the status-check `await`. Both paths converge on a local `kickOff()` helper guarded by a `started`
  flag, so `startOperation()` dispatches exactly once. The scan-error and scan-cancelled listeners also flip
  `started = true` as a terminal signal, so a late `scan-preview-complete` event can't dispatch an operation after we've
  errored or cancelled.

## Pause, Queue, and auto-queue (progress dialog)

`TransferProgressDialog` exposes three operation-manager controls during the active copy/move/delete phases, alongside
the existing Cancel/Rollback. They show only while `canPauseOrQueue` is true (op started, not scanning/cancelling/
rolling-back/settled, no conflict prompt up).

- **Lifecycle status comes from `operations-changed`, not `write-progress`.** The dialog subscribes to the manager's
  thin `operations-changed` snapshot and tracks `opStatus` for its own `operationId`. A paused op still reports
  `is_running: true` from the write-op-state map, so the bar-is-moving truth is the snapshot status (`running` vs
  `paused` vs `queued`), never `write-progress`. This mirrors the queue window's rule (see
  [`../queue/CLAUDE.md`](../queue/CLAUDE.md)). The Pause↔Resume label/icon and the "Paused" title both follow
  `opStatus`, so the UI flips only once the backend actually parked — never optimistically.
- **Pause/Resume** calls `pauseOperation` / `resumeOperation` (no rollback semantics; the op keeps its lane slot while
  paused). `pauseInFlight` guards against a double-click racing the IPC.
- **Queue (send to background)** is FRONTEND-ONLY state, no backend command. `handleQueue` sets the local `backgrounded`
  flag, opens the queue window (`openQueueWindow`), shows a quiet `info` toast (group `transfer-queue`), and calls the
  `onQueue` prop so the parent (`dialog-state.svelte.ts` → `handleTransferQueue`) unmounts the modal **without
  cancelling** the op. The op runs on, now managed in the queue window.
- **`backgrounded` suppresses the onDestroy safety-net cancel.** Normally `onDestroy` cancels a non-settled op (hot-
  reload / window close must not leak silent background work). Backgrounding is the deliberate exception: the user chose
  to keep it running, so the cancel is gated on `!backgrounded`. This is the one path where the modal unmounts and the
  op survives.
- **Dialog-scoped F2 → Queue.** `handleKeydown` (passed to `ModalDialog` as `onkeydown`) intercepts `F2` and triggers
  `handleQueue`, mirroring Total Commander's copy-dialog-local F2. It is NOT a `command-registry` binding: F2 is
  globally `file.rename`. The mechanism that scopes it: `ModalDialog`'s overlay `handleOverlayKeydown`
  `stopPropagation`s every keydown before it can reach the global root key handler, so while the dialog is open F2 never
  reaches `file.rename`; and when the dialog unmounts, the handler goes with it, so F2 falls through to `file.rename`
  again. No global binding is ever installed or removed — the leak-free property is structural, not bookkeeping. (Pinned
  by the negative test in `TransferProgressDialog.queue.test.ts`.) `preventDefault` stops any default browser action on
  the key.
- **Auto-queue surfacing.** When a new op starts on a busy lane, the manager admits it as `queued` rather than spawning
  it. The dialog detects this from the snapshot (a one-shot `list_operations` seed after `operationId` arrives catches
  the registration tick that may have fired before we knew our id; live ticks keep it current thereafter) and
  auto-backgrounds: it surfaces the queue window with a quiet "N transfers ahead" toast and unmounts, exactly like a
  manual Queue. The currently-foregrounded op keeps its modal; we never stack a second modal. "N ahead" counts the ops
  occupying lanes (running or paused), floored at 1.
