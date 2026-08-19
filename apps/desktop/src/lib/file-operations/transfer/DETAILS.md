# Transfer details

Pull-tier docs for `lib/file-operations/transfer/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in `CLAUDE.md`.

## The stalled-transfer notice

`transfer-stall.ts` holds one decision: how long to wait before speaking (`STALL_NOTICE_SECONDS`, 10 s). Everything else
comes from the backend's `TransferActivity`, derived from the live in-flight probe (see
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "The stall signal").

**Why the threshold differs from the log's.** The log watchdog waits 20 s because a log line wants to stay rare across a
long transfer. Ten seconds of a frozen bar is already long enough that a person wonders whether the app has died, and a
countdown is a lie the moment it stops being true. Both read the same `stillForSeconds`, so the two can't contradict
each other.

**What stays silent.** `paused` and `you` (a conflict prompt is open) are the transfer behaving correctly, and the
dialog already says so in its title. An operation with no activity at all — local copy, delete, trash, which keep no
in-flight table — also stays silent rather than guessing.

**Where it sits, and what it's made of.** A warning-toned `SectionCard` at the FOOT of the dialog body, below the
current-file line and directly above the button row, at full content width with no inset of its own. It's the reason a
person reaches for Cancel, so it belongs beside the button they'd reach for rather than wedged into the readout, and it
gets the card's `--spacing-lg` padding so a warning doesn't read as a cramped aside. The tone comes from the house
primitive (`$lib/ui/SectionCard.svelte`), the same one `TransferDialog`'s conflict block uses, so the two warning
surfaces in the transfer flow can't drift apart and both themes are handled in one place. Per the tone contract the fill
and border carry the warning and the text keeps its normal color; the hourglass icon is the one colored mark, in
`--color-warning-text` (the brand `--color-warning` only clocks ~3.3:1 on the tint). The notice renders outside the
branch that owns the progress bars, so `TransferProgressDialog`'s `showStall` re-states the two phases that branch
excludes: a scan writes nothing to be stalled about, and a view with no phase yet knows too little to accuse anything.
Pinned by `TransferProgressDialog.stall.test.ts` (placement, tone, silence while moving, and an axe pass over the
stalled state, which the tier-3 suite can't reach because it only renders the just-mounted dialog).

**The in-flight line is conditional, not permanent.** It renders only inside the stall notice, and only when
`inFlight > 0`. During a healthy transfer the file counter plus a speed and an ETA is enough, and "5 files in flight"
would be noise on every copy. It earns its place exactly when it explains something: a stalled counter reading lower
than what the person can see at the destination, which is the confusion the 2026-07-31 incident produced.

## File map

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. What the pieces DO is in the sections below: the two dialogs and both state factories in § "How transfer flows",
the compress components (level slider, estimate line, name helper, dest-exists check) in § "Compress mode", the password
prompt in § "Archive-password prompt", the `..` helpers in § "Index conversion for `..` entry", and pause / queue /
`backgrounded` in § "Pause, Queue, and auto-queue". Only the layout facts that none of those carry live here:

- **In `transfer-progress-state.svelte.ts`, `backgrounded` and `destroyed` are plain `let`s, NOT `$state`.** They're
  read on teardown paths that run during synchronous reactive-scope disposal, where a `$state` rune read returns a STALE
  value: that is how a just-queued transfer once got cancelled, killing the transfer and opening the queue window empty.
  Full why in the module header; don't "modernize" them into runes.
- **`DirectionIndicator.svelte` is the progress dialog's alone** (the confirm dialog shows its `From` card instead). Its
  optional `sourceLabel` / `destinationLabel` props override the path-basename label so a volume root renders the volume
  display name, not a raw machine id (an MTP storage id like `65538`). `direction` is optional too: it points the arrow
  at a PANE, which an adopted operation has no way to know (the registry snapshot names paths), so without it the
  indicator reads source → destination — the same "from → to" the queue row the user just came from shows. ❌ Never gate
  the whole indicator on `direction`: a dialog that named neither end of the transfer is worse than one that names both
  without pointing at a pane.
- **`conflict-policy.ts` is the ONE map from an MCP `onConflict` name to a conflict policy**, read by both programmatic
  entries: `TransferDialog.svelte` (a `copy` / `move` with `autoConfirm`, which confirms itself on mount) and
  `pane/dialog-state.svelte.ts` (`dialog confirm` on an already-open one). They each used to carry a private copy, and
  the copies had drifted — one spelled the conditional policies `overwrite_all_smaller`, the other
  `overwrite_smaller_all` — with neither spelling reachable through any tool, so nothing surfaced it. The drift matters
  because the fallback is silent: a name the map doesn't know becomes `skip`, so a caller asking to be asked per file
  would instead watch every clash get skipped. The backend validates the name against one list
  (`mcp/executor/mod.rs::CONFLICT_POLICIES`), and both callers now log a name this map has never heard of.
- **`ScanPhaseBody.svelte` is shared by the progress dialog and the queue row** (`comfortable` and `compact` densities),
  so a change to it lands on both surfaces.
- **`transfer-complete-toast.ts::composeTransferCompleteToast` splits TOP-LEVEL items by type only** ("Moved 1 file and
  3 folders"), never interior counts. It omits zero parts, and the skip suffix is file-only because folders always
  merge. When a top-level kind probe comes back partial it falls back to flattened file-count wording. F5/F6 feed it the
  split from real selection stats; drag-and-drop and clipboard paste feed it from a batched `stat_paths_kinds` /
  `read_clipboard_files` probe.

## How transfer flows

1. **TransferDialog** (destination picker + dry-run scan)
   - Pre-fills destination from the opposite pane.
   - The segmented Copy/Move toggle is always shown so the user can flip the operation regardless of how the dialog was
     triggered (F5/F6, command palette, drag-and-drop).
   - Validates path structure via `validateDirectoryPath()` from `$lib/utils/filename-validation` (empty, absolute, null
     bytes, length limits), then checks logical constraints (subfolder, same location).
   - Optional dry-run scan to detect conflicts upfront. Shows sampled conflicts (max 200) with streaming progress.
   - User makes conflict decisions before operation starts, inside a `warning`-toned `SectionCard`: the count and the
     question it raises ("3 files already exist. What do you want to do with them?") in normal text color, over five
     radios laid out `columns={3}` so they fill the card's width as 3 + 2 rather than wrapping wherever the labels
     happen to run out. The options are "Skip all", "Overwrite all", "Overwrite all smaller", "Overwrite all older",
     "Ask for each". When `totalConflictCount === 1`, the radio labels drop "all" ("Skip", "Overwrite", "Overwrite if
     smaller", "Overwrite if older") and "Ask for each" becomes "Ask later" since a single conflict can't be asked "for
     each". The conditional policies map to the typed `ConflictResolution` variants `overwrite_smaller` /
     `overwrite_older`. See the BE doc § "Key patterns and gotchas (shared)" for the strict-comparison / fail-closed
     contract.
   - **Folders always merge; the upfront check classifies collisions.** The conflict check (`conflicts.check()`, from
     `transfer-conflict-check.svelte.ts`) runs on mount **in parallel with the scan preview** (it's one cheap dest
     listing, not the recursive byte scan — `conflictCheckPromise` is assigned synchronously in `onMount` BEFORE the
     auto-confirm branch so the MCP `Skip all` fast path dispatches with `conflictNames` populated). "Cheap" is
     relative: on a big remote directory that one listing still runs for minutes, which is why the confirm doesn't wait
     for it (§ "The confirm dispatches without waiting for the conflict check"). Each collision is classified by the
     backend-resolved `sourceIsDirectory` / `destIsDirectory` flags (the BE resolves real per-item types + sizes from
     the source volume via one batched stat when the check passes `sourceVolumeId` + `sourcePaths`):
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
   - Dispatches the operation on mount, whether or not `TransferDialog`'s preview has finished walking. The operation
     claims that preview in the backend and its own task waits for it, so the dialog is a view over a named operation
     from the first frame. While `phase === 'scanning'` it renders the scan-phase body, hides Pause (the backend
     declines a pause in a scan-wait), and disables Rollback (nothing written yet); Background stays, which is the whole
     point.
   - Routes to a backend command through `transfer-dispatch.ts`, then binds the session for the id it gets back.
   - Subscribes to nothing. The window's fan-out holds the seven streams and buffers whatever arrives for an id no
     session has claimed yet, which covers the gap between the start command answering and the binder acquiring. § "The
     dialog is a view".
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
   - **Scanning-phase UI** (`phase === 'scanning'`, the one path there is now): rendered via `ScanPhaseBody`. Shows
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
  progress dialog with the same props but `previewId: null` — the first dispatch consumed the scan preview, so the retry
  re-scans the archive index (fast; scanning reads the index without decrypting). ⚠️ Clearing the id is now load-bearing
  rather than tidy: the retry is a NEW operation, and the backend refuses a second claim on one preview, so a
  carried-over id would silently downgrade to a full re-walk. A wrong password makes the backend raise
  `archive_needs_password` again with `wrongAttempt: true`, so the interception fires a second time and the dialog
  re-prompts (its distinct copy, empty field via a fresh mount).
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
`crates/cmdr-archive/DETAILS.md` § "Password-protected archives".

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
  refusal (not-a-folder toast, gated `!canWrite` scoped to the `search-results` kind so the wording stays correct) then
  read-only alert (off `VolumeInfo.mountIsReadOnly`). Returns `{ ok: true }` or a `{ ok: false, alert | toast }` the
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

**A transfer into the folder the sources already live in is asymmetric between copy and move**, and both halves live on
this side. A COPY there duplicates each item under a free ` (N)` name, so
`transfer-dialog-logic.ts::getPathValidationError` accepts it (the subfolder rejection above it stays: copying a folder
into its own subtree recurses until the disk fills). A MOVE there is already done, so the validator still rejects it,
and `clipboard-operations.ts::pasteWouldMoveNothing` short-circuits a cut-paste whose sources are ALL already in the
destination to nothing at all: no dialog, no transfer, no "Moved 0 files", and the clipboard survives so the next paste
still works. A PARTIAL set dispatches normally and the backend drops the ones already there. That frontend check is
lexical because the frontend holds paths and nothing else; the backend settles identity properly
(`src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in place)").

#### Only paste and F5 end a duplicate in the rename editor

A transfer that duplicates ONE item in the folder it already lived in can end by opening the inline rename editor on the
copy, stem selected, so naming it costs one keystroke sequence and Esc keeps the generated ` (N)` name. Which gestures
ask for that is carried by `duplicateFollowUp`, a REQUIRED field on both `TransferDialogPropsData` and
`TransferProgressPropsData`: every gesture dispatches the same backend copy, so a trigger that said nothing would
inherit whatever the last one wanted. The mechanism is `pane/duplicate-rename.ts`; the settled tail that runs it is
`pane/DETAILS.md` § "Naming a duplicate".

Who answers what, and why:

- **Paste** (`clipboard-operations.ts`) and **F5** (`file-operation-commands.ts::openUnifiedTransferDialog`) say
  `openRenameEditor`. They're the gestures where a person has just DIRECTED a copy somewhere, and they're what issue #50
  actually asked for ("all file managers I used so far would ask for a new name **when pasting**").
- **An auto-confirmed F5 is MCP**, and says `nothing`: an agent's copy has no business pulling focus into a text field
  in front of whoever is watching.
- **Drag and drop** (`drag-drop-controller.svelte.ts::DROP_DUPLICATE_FOLLOW_UP`) says `nothing`. A drag ends with the
  mouse, and stealing focus into a text field on mouse-release is the wrong shape.
- **The Duplicate command** says `nothing` when it lands. ⌘D _is_ Finder's Duplicate, and the familiarity that justifies
  the key rests on it asking nothing; an editor would also break stamping out several copies in a row, since after the
  first ⌘D focus sits in an editor and the second does nothing until Esc.

No setting gates this. The same reasoning that rejects a modal "name the copy" prompt rejects a toggle: two gestures
that ask and two that don't already cover both preferences.

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
`WriteOperationType::ArchiveEdit`; the seed mechanism lives in
`apps/desktop/src-tauri/src/file_system/write_operations/archive_edit/DETAILS.md` § "The driver, op by op". The
user-visible differences from copy/move:

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
  `apps/desktop/src-tauri/src/mcp/executor/ack.rs`. Don't refactor this gate away.
- **Confirm routes to `compressFiles`**, not `copyBetweenVolumes` (`transfer-dispatch.ts::dispatchCompress`). One
  command handles local and (later) remote sources; the scan preview still runs for the Size bar.
- **A compression-level slider shows in compress mode only** (`CompressLevelControl.svelte`, below the scan tallies). It
  renders the shared `SettingSlider` with "Faster"/"Smaller" `endLabels` and binds to `behavior.archiveCompressionLevel`
  by id, so the dialog and the Settings › Behavior › Archives row are ONE persisted value with no dialog-local state —
  moving either reflects in the other live. `dispatchTransferOperation` reads the setting once at dispatch and passes
  `compressionLevel` in the op config for compress, copy, AND cross-volume move (one uniform level for every user-driven
  zip write; the backend ignores it for non-archive copies). The level's effect on the archive (added-entries-only,
  clamped 1..=9, `None` = crate default 6) is single-sourced in
  `apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Archive edits" → the mutation `DETAILS.md`.
- **An explicitly-approximate estimated size shows in compress mode only** (`CompressEstimateLine.svelte`, beside the
  scan tallies). The backend samples it once during the deep scan (local sources only; suppressed for remote) and ships
  per-class level-6 subtotals on `scan-preview-complete`; `transfer-scan-state` exposes them as `estimatedBytes`. The
  line re-scales to the selected level via `compress-estimate-scaling.ts` with no re-scan (it subscribes to the same
  `behavior.archiveCompressionLevel` setting the slider writes), shows a loading affordance while a local scan runs, and
  renders nothing when the estimate is absent. The sampler, budgets, and level curve are single-sourced in
  `apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Compressed-size estimate".
- **The Copy / Move / Compress row is the `ui/ToggleGroup` primitive** (`semantics: 'toggles'`, `fullWidth`), wrapped in
  a `.operation-toggle` div that supplies only the side inset. `toggles`, not `tabs`: the row picks a stored value and
  has no tab panels, so AT should hear "toggle button, Compress, pressed" rather than a promised "tab 1 of 3". E2E and
  unit tests select its cells as `.tg-root .tg-item`, and the active one as `.tg-item[data-state='on']` (the `tabs`
  branch marks it with `.is-active` instead).
- **Both compress-only blocks live in one `.compress-extras` wrapper** that stacks them on the dialog body's rhythm and
  gives the mode switch a single element to `transition:slide`. The dialog passes `growDownward` to `ModalDialog` so the
  extra height extends downward instead of re-centering the whole dialog mid-switch (`lib/ui/DETAILS.md` § ModalDialog).
  The slide duration is 0 under `prefers-reduced-motion` and 0 before the first paint, so opening straight into Compress
  doesn't animate.

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
- `handleConfirm` for a same-volume move dispatches IMMEDIATELY with `previewId = null`, which the backend reads as
  "this operation has no preview" rather than as a miss, so nothing waits and nothing re-walks. Like every other path,
  it waits for the conflict check only under the `skip` policy (see below).
- The cheap top-level conflict check (decoupled from the deep preview) keeps running independently on mount, so a
  same-volume move still surfaces "N folders will merge" and the file-policy radios. This decoupling is the prerequisite
  that lets us cancel the deep preview without degrading the conflict UX.
- Size bar: `bytesTotal = 0` already hides it (`{#if bytesTotal > 0}`), honest for a rename. The progress dialog reads
  with Files-only progress; the complete toast counts top-level items (a moved folder counts as one item).
- Pinned by `TransferDialog.test.ts` § "same-volume move scan gating" (no scan started for a same-volume move; the
  preview starts for a same-volume copy; toggle both directions cancels/restarts; immediate dispatch with
  `previewId = null`).

### The confirm dispatches without waiting for the conflict check

`handleConfirm` awaits `conflictCheckPromise` **only when `conflictPolicy === 'skip'`**. Every other policy dispatches
as soon as the preview id is in hand, even with the check still running.

**Why it's safe.** The upfront conflict list is not a correctness input, it's a bulk-skip perf optimization:
`build_pre_skip_set` (`src-tauri/src/file_system/write_operations/transfer/transfer_driver/mod.rs`) returns an empty set
unless `config_resolution == Skip`, and the copy pipeline has a second independent `Skip` gate (`transfer/copy/mod.rs`).
`VolumeCopyConfig::pre_known_conflicts` says so in its own doc comment: "Ignored for other resolution modes (Stop still
prompts; Overwrite still proceeds normally)." Under the default `stop` the backend prompts per clash at runtime with
apply-to-all latching (`write_operations/conflict.rs`), and a backgrounded operation's conflict still reaches the user
through `../operation-conflict.svelte.ts`. So dispatching with `conflicts: []` costs pre-flight _information_, never
safety.

**Why `skip` is the exception, and why no human waits for it.** Under `Skip all` the names let the backend drop the
clashing sources upfront instead of discovering each one serially through per-file `get_metadata` stats, so the progress
bar reflects them immediately. A human can't select `skip` while the check is running — the policy radios live in the
`{:else if totalConflictCount > 0 || mergeFolderCount > 0}` branch, unreachable while `isCheckingConflicts` — so that
await belongs to the MCP auto-confirm path (`autoConfirmOnConflict: 'skip_all'`), where nobody is watching a button.

**What still gets awaited on every path:** `scan.scanStarted`. Dropping it is a three-part failure, not a missing id.
The operation would dispatch with nothing to claim, fall into the backend's miss case, and re-walk the tree CONCURRENTLY
with the preview this dialog already started — the exact contention the backend's wait exists to prevent, and worst on
MTP and SMB. The orphaned preview would also have no owner and nothing to cancel it, because the `confirmed` guard keeps
`handleCancel` away from `freeAndCleanup()`, so its result would sit until a TTL sweep. `DeleteDialog` awaits its own
`scanStarted` for the same three reasons. The IPC only mints a UUID, registers the preview, and spawns the walk on a
background thread (`write_operations/scan_preview.rs`), so it returns promptly even against a wedged share; it is NOT
the recursive walk.

**The honest pending state.** `confirmPending` (a `$state`, unlike the plain `confirmed`) disables BOTH footer buttons
and renders a decorative `<Spinner size="sm" />` next to the unchanged `confirmLabel` for however long a path does
await. The spinner carries no `label`, so it's `aria-hidden` and the button's accessible name stays exactly the label —
deliberately, so the pending state costs no new catalog key, no nine-locale translation, and no a11y assertion.

**`handleCancel` returns early when `confirmed`.** Disabling the footer's Cancel is cosmetic, not the protection: the
`×` in the dialog chrome and the Escape key both reach `ModalDialog`'s `onclose` (= `handleCancel`) whatever the footer
looks like. Without the guard, closing during an in-flight confirm runs `scan.freeAndCleanup()` and cancels the preview
out from under the pending `onConfirm` — the progress dialog then opens onto a dead preview. The test drives the `×` for
exactly that reason; asserting through the disabled Cancel would cover nothing.

Pinned by `TransferDialog.test.ts` § "confirm without waiting for the conflict check": a pending check doesn't block a
`stop`-policy confirm or a same-volume move, `skip` still waits and still forwards the names, the button disables and
shows a spinner while genuinely pending, and Cancel during a pending confirm frees nothing.

### `data-scan-state` marker on the tallies element

`TransferDialog`'s `.scan-stats` element carries a `data-scan-state` attribute (`counting` | `done` | `skipped`) derived
from the existing `scanComplete` / `isSameVolumeMove` state — NO new wire event. It's the race-free "counting done"
signal E2E uses: the shared `expectDialogCounters(tauriPage, …)` helper polls it to a terminal state before asserting
the counter line, so an assertion never fires against a partial in-flight tally.

- `done` → the deep scan finished; the tallies are final. `done` wins over `skipped` (a same-volume COPY still scans).
- `skipped` → no deep scan runs (a same-volume move renames server-side, zero bytes), so the tallies legitimately stay
  at 0. The helper only accepts this state when the caller opts in with `allowSkipped`.
- `counting` → a scan is in flight or about to start on mount.
- `unavailable` → the scan stopped without an answer. The tallies stay on screen as a FLOOR, with the notice below
  saying so; see "When the dialog can't find out".

Pinned by `TransferDialog.test.ts` § "data-scan-state marker" (counting → done, the skipped fast path, and the counting
→ skipped toggle), and `TransferDialog.unavailable.test.ts` for `unavailable`.

### `data-conflict-state` marker on the dialog body

The sibling marker for the OTHER async settle in this dialog: the top-level conflict check
(`transfer-conflict-check.svelte.ts`). `.dialog-body` carries `data-conflict-state` (`checking` | `done` | `skipped` |
`unknown`), derived from that factory's status — again no new wire event.

- `done` → the check RAN, and the conflicts section below it is final.
- `unknown` → the check couldn't run, so nothing is known about the destination. ⚠️ A failure used to land in `done`
  with an empty conflict list, which is byte-identical to a clean destination; see "When the dialog can't find out".
- `skipped` → compress makes ONE new file, so the multi-file check never runs; the dest-exists affordance answers
  instead.
- `checking` → the dest listing is in flight, or about to start on mount.

**It lives on the BODY, not on the conflicts section, because there IS no conflicts section when the check comes back
clean.** That asymmetry is the whole point: `waitForConflictPolicy` can only ever observe the conflict outcome, so a
test that legitimately accepts both (`conflict-edge-cases.spec.ts` › `directory-over-file`) has nothing to poll and
reaches for a fixed `sleep`. `waitForConflictCheck` in `conflict-helpers.ts` polls this marker instead.

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
  folder doesn't exist, the field takes `TextInput`'s `warning` state (a yellow border and ring) and a yellow message
  line (`.path-warning`, keys `targetWillBeCreated{Copy,Move}`). The red error always wins — the two never show at once.
  A timeout is inconclusive (hung mount), so it stays quiet rather than over-promising. A monotonic `existsCheckSeq`
  drops a stale probe that lands after a newer keystroke.

Backend counterpart: every transfer path creates a missing destination (and ancestors) before transferring — the local
copy/move paths via `ensure_destination_dir` (`write_operations/validation.rs`), and the cross-volume +
same-volume-rename pipelines via `Volume::create_directory_all` (recursive mkdir on the dest volume, works on local,
SMB, MTP, in-memory). So the warning is honest for EVERY destination type, which is why it's no longer gated to local
destinations (there's no `isLocalDestination` check — `showTargetWarning` keys only off `targetMissing` + no
`pathError`).

### The clash prompt names the folder, not just the file

`TransferConflictDialog` leads with the destination's basename, and under it, quietly, the folder that file sits in
(`containingFolder`, `transfer-dialog-utils.ts`). A bare name answers "what" and not "which", and a copy of a deep tree
raises one prompt per clash: a QA pass over 1,600 folders that each held an `f001` got 1,600 questions that read
identically.

- **The folder, not the whole path.** The name stays the headline, because it is what the buttons act on. A dialog
  leading with a deep absolute path buries that.
- **Mid-truncated toward its tail** (`useShortenMiddle`, `preferBreakAt: '/'`, `startRatio: 0.3`), because the deepest
  segments are the ones that differ, and the house tooltip carries the whole path on hover. Same treatment and same
  parameters as the scan phase's "From:" line and the search results' current path, so paths in tight space behave one
  way across the app.
- **The DESTINATION's folder**, which is where the file that would be overwritten lives and what every button acts on.
- **`null` rather than a guess** for a path that can't yield a parent (relative, `~`-rooted). Backend paths are absolute
  or virtual-volume URLs, so that is a bug elsewhere, and the prompt shows the name alone rather than inventing a
  folder.

### Index conversion for ".." entry

When the directory has a parent entry shown at index 0, frontend indices are offset by +1 from backend:

- Frontend `[0, 1, 2, 3]` with `hasParent=true` → Backend `[-1, 0, 1, 2]` → filtered to `[0, 1, 2]`
- Index 0 with `hasParent=true` is always the ".." entry (backend index `-1`, invalid)
- `toBackendCursorIndex(0, true)` returns `null` to signal no-op

## When the dialog can't find out

Both pre-confirm questions reach a volume that can stop answering, and each has a settled shape for "I couldn't".

**The size scan.** `transfer-scan-state.svelte.ts` records `scanFailure = { timedOut }` from `scan-preview-error`
(`timedOut` is the backend watchdog's typed flag, ❌ never read off the message). The dialog keeps the tallies — they're
a real floor, and blanking them would claim the source is empty — adds a warning-toned line saying either that the
source isn't responding or that the measurement couldn't finish, and offers **Try again**, which frees the dead preview
and walks again (`scan.retry()` = `cancelPreview()` + `startScan()`, so the retry can't adopt the old scan's events).

**Retry, and NOT "proceed without a scan", is the affordance offered.** Proceeding is already possible: the confirm
button stays live throughout, because the preview only feeds this Size line and a cache the operation can rebuild
itself. A second button for something the primary button already does would be noise. What the user can't do without
help is ask again after plugging the network back in.

**The conflict check.** `transfer-conflict-check.svelte.ts` carries a `status` of `idle` / `checking` / `answered` /
`unknown` (a bounded `withTimeout` at 35 s over the IPC, just above the backend's own 30 s budget, catches a call that
never returns at all). `unknown` renders its own line, because rendering nothing is what a genuinely clean destination
renders, and the user is about to decide what happens to their files on the strength of it.

**Why an unknown check is still safe to transfer on.** The pre-flight names feed `pre_known_conflicts`, which the
backend reads under `Skip` alone as a bulk-skip PERF hint (`build_pre_skip_set`); every clash is still detected and
arbitrated at write time. An unknown check contributes no names, so nothing is pre-skipped, and the policy radios never
render for it — leaving the default `stop`, which prompts per clash. ❌ Don't "helpfully" default an unknown check to a
non-prompting policy: that would turn "nobody looked" into a silent overwrite.

## Gotchas

- **Always use batch IPC for selection lookups.** `get_paths_at_indices` (paths only) and `get_files_at_indices` (full
  `FileEntry` objects) fetch all selected items in a single IPC call. Never loop over `getFileAt` per-index; with 50k
  selected files, per-file IPC takes 5-10 seconds. Batch calls take ~1 ms regardless of count.
- **MTP move is interleaved copy + delete per file.** Moves involving MTP volumes copy and then delete each file
  individually (not copy-all-then-delete-all). Minimizes duplicates on partial failure: if it fails mid-way, only the
  current file exists in both places. The progress UI shows three stages (Scanning → Copying → Removing source). If copy
  succeeds but delete fails, the user keeps files in both places (safer than losing data).
- **A cross-volume move can't be rolled back at all**, and this dialog doesn't know it yet: it disables Rollback only
  for a SAME-volume move, so on a cross-volume one it still offers a button whose click only cancels (the driver treats
  `RollingBack` exactly like `Stopped` and reports `rolled_back: false`). The backend now publishes the real verdict as
  `supportsRollback` on the operation snapshot, which the operation queue window reads; pointing this dialog at the same
  flag is the open fix. See `src-tauri/src/file_system/write_operations/DETAILS.md` § "Rollback availability".
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
  feedback. `operationSettled` reads the session's `settled`, which flips the moment a terminal event lands.
- **Cancel close is two-condition: `write-cancelled` + `write-settled`.** When the user clicks Cancel (without
  rollback), `TransferProgressDialog` does NOT close immediately. It keeps the "Cancelling…" label up until both events
  have arrived for this `operationId`, then applies the existing `MIN_DISPLAY_MS` floor and closes via
  `onCancelled(filesProcessed)`. After 200 ms of waiting, the label gains a clarifying tail: "Cancelling… (finishing USB
  transfers)". The BE-side contract — settle fires after a fully-torn-down spawn task, even on panic — lives in the BE
  doc § "Settle contract". Race protection comes free from reading state rather than events: the view closes when the
  session reports BOTH an outcome of `cancelled` and `settleEventReceived`, whichever order they land in. Complete /
  error paths are unchanged: they still close on the existing `MIN_DISPLAY_MS` gate without waiting for settle. The wait
  is never the only exit: `progress.dismiss()` backs a Close button that leaves at once, and the last-resort
  `CANCEL_SETTLE_FALLBACK_MS` (20 s) sits above the backend's 15 s `CANCEL_DRAIN_DEADLINE`, so the automatic path can't
  report `0 files` before the real count lands. Why it matters: the original incident was an MTP delete cancel followed
  by an immediate second F8 — the device was still mid-teardown, the second op queued behind the 17 s tail, hit the 30 s
  op timeout, and wedged the USB session.
- **Scan preview reuse, and who waits for it.** `TransferDialog` starts a scan preview on mount. If the user confirms
  before the scan finishes, the scan keeps running (`TransferDialog` sets `confirmed = true` and skips cancellation in
  `onDestroy`), and `TransferProgressDialog` dispatches the operation IMMEDIATELY. The wait lives in the backend: the
  operation claims that `previewId` at registration and its own task parks on it before writing anything
  (`apps/desktop/src-tauri/src/file_system/write_operations/scan_bridge.rs`). That is what gives a still-scanning
  transfer an `operationId`, a queue row, Background, and a place in the quit gate from the first frame. The dialog
  renders the scan phase from ordinary `write-progress` in `phase: 'scanning'`, which the backend forwards from the
  claimed preview under the operation's id, so one branch feeds both the preview and the backend's own foolproof
  re-scan. ❌ The dialog must never cancel the preview on teardown: the operation owns it, and a viewer detaching is not
  a cancel. The scan-error and scan-cancelled listeners also flip `started = true` as a terminal signal, so a late
  `scan-preview-complete` event can't dispatch an operation after we've errored or cancelled.

## The dialog is a view

`createTransferProgressState` is two things with one lifetime between them, and the split is the point.

**Birth** runs once. It claims the foreground slot, calls `dispatchTransferOperation`, answers the MCP round-trip, and
ends when an `operationId` exists. **The view** is everything after: it binds the session for that id
(`bindOperationSession`) and renders it, commands through it, and owns only what belongs to a piece of UI.

What the view owns, and why each one is genuinely view-scoped:

- `MIN_DISPLAY_MS`, the anti-flicker floor, measured from when THIS VIEW appeared rather than when the operation
  started. The floor exists because something appeared and vanished too fast to read, which is a fact about the thing on
  screen. The two clocks coincide for a dialog that started its own transfer, and they diverge for one that adopts a
  transfer already in flight: the operation's clock would say "twenty minutes, no flash possible" about a dialog that
  had been up for 50 ms. The view's clock is the honest one.
- `dismiss()`, the settle-slow label, and the last-resort close timer: all about how long a person is made to watch.
- `backgrounded` and the Queue handoff: the decision that THIS dialog should stop showing a queued operation. Another
  view of the same operation sees the same status and does nothing. (The auto-queue path is that split in miniature: the
  session observes `status === 'queued'`, the view decides to detach.)

What the session owns: phase, counts, `currentFile`, rates, the smoothed ETA, the scan readout, `activity`, the clash,
the outcome, the lifecycle status, and all five commands with their in-flight guards. ❌ The view keeps no second copy
of any of it, and no listener of its own — the window's fan-out is the only subscriber, and its unclaimed-id buffer is
what covers the gap between the start command answering and the binder acquiring on the next effect flush.

**Birth is skippable.** `adoptOperationId` names an operation that is already running, and `start()` binds its session
instead of dispatching: the queue's Show button, and the reason the two halves are worth separating at all. The view is
otherwise the same one, down to the buttons. Three things differ, each for its own reason:

- **Auto-queue is off.** It is a decision a DISPATCHING view makes (don't stack a second modal over the one already up);
  a view opened precisely to watch this operation would instead bounce it back out of sight, which reads as the button
  doing nothing. The queue row correspondingly doesn't offer Show on a `queued` row.
- **Rollback comes from the registry row.** `rollbackUnavailable` reads the snapshot's `supportsRollback`, which is a
  promise about the OPERATION; an adopted view has no volume ids or direction to reason from. The props-only
  same-volume-move rule stands beside it for the window before the first snapshot lands, and the phase gate (nothing
  written during a scan) is unchanged.
- **The parent runs no pane tail.** An adopted view has no birth context, and the two-slot arrangement in `dialog-state`
  is what makes the wrong version unreachable: `../../file-explorer/pane/DETAILS.md` § "Birth context".

**An adopted view never shows `OPENING_PHASE`, and that is a decision.** `scanning` is what a DISPATCHING view opens on,
because a confirmed transfer is about to count; an adopted operation could be anywhere, and titling a 21%-written copy
"Verifying before copy…" over an empty scan readout is what shipped for about an hour before the real-app run caught it.
So an adopted view reports `phase: null` until the operation speaks, and the dialog renders its title, its paths, and
its buttons with no bars at all.

That is normally the same frame: the window's fan-out keeps the newest tick of every live operation and hands it to a
session attaching late (`../operation-session/DETAILS.md` § "Where a live operation had got to"). The empty state is
what is left over — a window that has heard nothing at all, with the operation paused so no tick is coming, which after
a reload is a real path. It shows "Paused", offers Resume, and fills in the moment the operation says where it is.

**Teardown stops nothing.** A close is a detach: `ModalDialog`'s `onclose` goes to `detach()`, which hands a
still-running operation to the queue window (exactly as the Queue button does) and otherwise just stops watching. An
unmount does neither — the operation lives in the backend registry, and the corner chip and the queue window keep
showing it. The one teardown-adjacent flag that still means "stop" is `cancelRequestedBeforeId`: an explicit Cancel
pressed while the start command was in flight, which birth honours through `cancel_operation` (the MANAGER-level cancel,
because an operation admitted behind a busy lane has no write op to cancel yet).

Two shapes of that rule are worth stating, because each is one condition standing between a keystroke and the wrong
answer:

- **While a clash is showing there is no `onclose` at all**, so no × and no Escape. Backgrounding a parked operation
  would leave it waiting on a question nobody is asking (the conflict host discards a clash the foreground owned), and
  dismissing would tell the pane "cancelled" about an operation that is still parked. The conflict body carries Skip,
  Rename, Overwrite, Cancel, and Rollback, which is every honest way out. Same rule the main window's conflict prompt
  follows (`../DETAILS.md` § "Conflict prompts").
- **`dismiss()` refuses to speak for an operation that ended some other way.** It reports `onCancelled`, and the pane
  runs a different tail for a cancel than for a completion, so it fires only while a cancel is what is happening.
- **A detach with no session leaves the operation alone.** The binder acquires on the first effect flush after the id
  lands, and in that sub-frame sliver nothing knows whether the operation is still running — so `detach()` logs and
  returns rather than falling through to `dismiss()`, which would report `onCancelled(0)` and run the pane tail over a
  transfer that is still copying. Same refusal `handleCancel` makes in the same window, and for the same reason: with no
  session there is nothing to ask and nothing to say.

**A `gone` outcome closes the dialog too.** The session resolves `gone` when it has heard nothing about the operation
and `list_operations()` doesn't have it either. For a dialog that just dispatched, that means the operation ended inside
the sliver between the start command answering and the session claiming its id — and the honest reading is "it's over,
we don't know how", so the view closes through `onCancelled(0)` rather than sitting empty. The buffered terminal event
normally wins that race, which is why this is a corner rather than a path.

## Pause, Queue, and auto-queue (progress dialog)

`TransferProgressDialog` exposes three operation-manager controls during the active copy/move/delete phases, alongside
the existing Cancel/Rollback. They show only while `canPauseOrQueue` is true (session bound, not cancelling/rolling-
back/settled, no conflict prompt up).

- **Lifecycle status comes from `operations-changed`, not `write-progress`.** The dialog reads `session.status`, which
  the session takes from the manager's thin snapshot. The bar-is-moving truth is that snapshot status (`running` vs
  `paused` vs `queued`), never `write-progress`: a parked op emits no further ticks, so its last one describes a
  transfer that has stopped. This mirrors the queue window's rule (see `../queue/CLAUDE.md`). The Pause↔Resume
  label/icon and the "Paused" title both follow it, so the UI flips only once the backend actually parked — never
  optimistically.
- **Pause/Resume** is `session.togglePause()`, which steers by that same status (no rollback semantics; the op keeps its
  lane slot while paused). The session's `pauseInFlight` guards against a double-click racing the IPC, and because the
  guard lives on the shared session, a queue row watching the same operation sees the press too.
- **Queue (send to background)** is FRONTEND-ONLY state, no backend command. `handleQueue` sets the local `backgrounded`
  flag, opens the queue window (`openQueueWindow`), shows a quiet `info` toast (group `transfer-queue`), and calls the
  `onQueue` prop so the parent (`dialog-state.svelte.ts` → `handleTransferQueue`) unmounts the modal **without
  cancelling** the op. The op runs on, now managed in the queue window. The button reads "Background" with an empty
  queue and "Queue" otherwise (`../queue/queue-backlog.ts`); the action is the same either way.
- **`backgrounded` is a one-way latch, not a guard against anything.** It records that this view has already handed the
  operation over, so a second Queue press, an auto-queue firing behind a manual one, and a close during the handoff are
  all no-ops. It no longer suppresses a teardown cancel, because there is no teardown cancel: every unmount leaves the
  operation running. It stays a plain `let` regardless — see § "File map".
- **Dialog-scoped F2 → Queue.** `handleKeydown` (passed to `ModalDialog` as `onkeydown`) intercepts `F2` and triggers
  `handleQueue`, mirroring Total Commander's copy-dialog-local F2. It is NOT a `command-registry` binding: F2 is
  globally `file.rename`. The mechanism that scopes it: `ModalDialog`'s overlay `handleOverlayKeydown`
  `stopPropagation`s every keydown before it can reach the global root key handler, so while the dialog is open F2 never
  reaches `file.rename`; and when the dialog unmounts, the handler goes with it, so F2 falls through to `file.rename`
  again. No global binding is ever installed or removed — the leak-free property is structural, not bookkeeping. (Pinned
  by the negative test in `TransferProgressDialog.queue.test.ts`.) `preventDefault` stops any default browser action on
  the key.
- **Auto-queue surfacing.** When a new op starts on a busy lane, the manager admits it as `queued` rather than spawning
  it. A DISPATCHING view watches `session.status` for that and auto-backgrounds: it surfaces the queue window with a
  quiet "N transfers ahead" toast and unmounts, exactly like a manual Queue. ❌ An ADOPTED view (`adoptedOperationId`,
  the queue window's Show) never does, and the effect returns early for one: a dialog opened precisely to watch this
  operation hiding itself again is the button appearing to do nothing. The currently-foregrounded op keeps its modal; we
  never stack a second modal. "N ahead" counts the ops occupying lanes (running or paused) in the main window's
  operations store — the same live rows the Background/Queue label reads — floored at 1. The dialog needs no seeding
  logic of its own: a session that hears nothing on attach asks `list_operations()` itself, which is what catches the
  registration tick that fired before anything in this window was watching.
