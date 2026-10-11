# Transfer details

Pull-tier docs for `lib/file-operations/transfer/`: architecture, flows, and decision rationale. Must-know invariants
and gotchas live in `CLAUDE.md`.

## The stalled-transfer notice

`transfer-stall.ts` holds two presentation decisions: how long to wait before calling a transfer stalled
(`STALL_NOTICE_SECONDS`, 10 s), and what the time line says instead of an ETA while bytes are genuinely on their way
(`waitLineFor`). Everything else comes from the backend's `TransferActivity`, derived from the live in-flight probe (see
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "The stall signal").

**The wait line, before the first byte and while a response arrives.** Two backend readings drive it, and the readout's
time line shows at most one, below a stall and above an ETA:

- `sourceInboundBytesPerSecond` set: "Receiving from the source at 19 kB/s". The backend sends it only while the byte
  counter stands still, so it's the rate of a response that hasn't finished arriving (one SMB compound read, or one 8 MB
  chunk on a slow link). It also HOLDS BACK the stall notice: data still flowing is slow, not stopped, and a warning
  card over a live rate would contradict itself. ❌ Never add it to a bar: it's connection-wide and unverified.
- `openingSource` set and under 10 s still: "Waiting for the source to start sending (4s)". Before the first byte there
  is no countdown to protect, so there's nothing to wait out; the backend heartbeats from the first still second for
  exactly this. Past 10 s with nothing arriving, the stall notice takes over and names the source.

Both stay silent for `paused` and `conflict`. ERR-CNK7M is the case: a 377 kB file sat 20 s on a silent 0% bar. Pinned
by `transfer-stall.test.ts` (`waitLineFor`), `TransferProgressReadout.svelte.test.ts`, and
`TransferProgressDialog.stall.test.ts`.

**Why the threshold differs from the log's.** The log watchdog waits 20 s because a log line wants to stay rare across a
long transfer. Ten seconds of a frozen bar is already long enough that a person wonders whether the app has died, and a
countdown is a lie the moment it stops being true. Both read the same `stillForSeconds`, so the two can't contradict
each other.

**What stays silent.** `paused` and `conflict` (a clash is open and unanswered) are the transfer behaving correctly, and
a surface is always saying so: the progress dialog in its title, and for an operation no dialog owns, the main window's
prompt (`../DETAILS.md` § "Conflict prompts for operations with no dialog"). That's what makes the silence honest, so a
clash that could go unprompted would turn `conflict` into a wedge with nothing on screen to explain it. An operation
with no activity at all (local copy, delete, trash, which keep no in-flight table) also stays silent rather than
guessing.

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
than what the person can see at the destination, which is the confusion the 2026-07-31 incident produced. The counter
itself is honest and ❌ must not be "fixed" to close that gap; why, and what the payload can't express, is
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/transfer_driver/DETAILS.md` § "The file counter counts
COMPLETED files".

## File map

`TransferDialog.test.ts` covers preflight and conflict UX; `TransferDialog.targets.test.ts` covers filename targets and
rename-by-move. Both use the IPC doubles and mount helpers in `test-transfer-dialog-harness.ts`, which unmounts every
dialog after each test so pending debounce work cannot reach another test's mocks.

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
- **The progress dialog's Rollback is a controller plus a button.** `transfer-rollback.svelte.ts` owns the one reversal
  decision (blocked or live, what it promises, the confirmation's asked state) and `TransferRollbackControls.svelte`
  renders the button in its three readings. It's split that way because the two halves can't share a subtree: the button
  sits in the dialog's button row, while `RollbackConfirmDialog` stacks over the whole dialog and is also raised from
  the conflict body, which replaces that row. `../DETAILS.md` § "Rollback asks first".
- **`ScanPhaseBody.svelte` is shared by the progress dialog and the queue row** (`comfortable` and `compact` densities),
  so a change to it lands on both surfaces.
- **`transfer-complete-toast.ts::composeTransferCompleteToast` splits TOP-LEVEL items by type only** ("Moved 1 file and
  3 folders"), never interior counts. It omits zero parts, and the skip suffix stays file-worded (the backend counts
  skipped LEAVES). When a top-level kind probe comes back partial it falls back to flattened file-count wording. F5/F6
  feed it the split from real selection stats; drag-and-drop and clipboard paste feed it from a batched
  `stat_paths_kinds` / `read_clipboard_files` probe.
- ❗ **Each selection count is reduced by the skips of ITS OWN kind**, from `WriteCompleteEvent.topLevelSkipped`
  (`{files, folders}`). `filesSkipped` cannot stand in for it: that counts leaves, nested ones included, so subtracting
  it from `fileCount` only held while every skip was a top-level file. Once a folder's child could be skipped, a
  Skip-All over `readme.txt` + `docs/` read "Copied 1 folder, skipped 3 files" — the two skipped children ate the
  top-level file that DID copy — and a folder refused onto a same-named file still read "Copied 1 folder". A folder
  counts as skipped only when NOTHING under it landed; one refused child leaves it copied, because it partly arrived.
  The `?? filesSkipped` fallback is that old heuristic on purpose, for the engines that report no breakdown
  (cross-volume, in-archive), whose skips are all top-level files today. Backend contract:
  `write_operations/types/events.rs::TopLevelSkipped`.
- **A move that left something in the source appends a sentence per kind** ("2 items appeared in Work during the move
  and stay there", "1 item changed during the move and stays in Work"), off `WriteCompleteEvent.appearedDuringMove` —
  typed data (`itemCount`, `changedCount`, `folderName`, `folderCount`), never prose crossing IPC. It means a
  cross-filesystem move found files the copy phase never carried, or originals saved over after their copy, so the
  source sweep left them alone (backend: `write_operations/transfer/DETAILS.md` § "deletes a LEDGER"). The move still
  reads as a success, and the toast stays `success`, not a warning: nothing went wrong, some files simply arrived too
  late to travel. Absent on every other ending, so the historic wordings render byte-identical.

## Single-item destinations

A single-item Copy/Move target always includes the destination leaf, prefilled with the original name. Absolute local
paths name the exact location, including the mount root; an absolute remote path names a location within the selected
volume. Relative paths resolve from the source folder and volume, including nested paths and `..`; `~/` resolves from
the local home directory. `resolveTransferFilename` splits and normalizes the path, and `transfer-target.ts` selects the
effective volume. Empty leaves and final `.` / `..` components cannot confirm. A folder copy names the copied or moved
folder itself. Multiple selections still target a directory, preserving each selected item's name.

**A trailing slash means "into this folder"**: `/Users/me/Documents/` keeps the item's own name inside `Documents`.
Without it the last segment is always the new name, so a pasted folder path would rename the item after that folder: a
folder source silently MERGES into it (dir + dir is a merge, not a conflict), and a Move then removes the original.
**Decision/Why:** we kept "the last segment is the name" (no existence lookup deciding the meaning, which would make the
same text mean two things depending on disk state) and made the slash the explicit "into". When the conflict check finds
the named target is an existing folder under a different name than the source's, the conflict card leads with
`namedFolderHint` pointing at the slash (`transfer-conflict-check.svelte.ts::namedTargetIsFolder`). Same name means an
ordinary merge, so it stays quiet.

A complete target equal to its source shows the existing "already in this location" warning and disables confirm for
both Copy and Move, including Enter and MCP auto-confirm. The comparison uses the full canonical destination, so
matching volume-relative names on different volumes remain valid. A different name in the source folder is valid.
Copy/Move toggles preserve the complete edited target; switching to/from Compress derives a target from the same parent.

**A single-item Move inside its own folder is a rename, and the rename flow runs it.**
`transfer-target.ts::isRenameInPlace` flags it on the confirm (`renameInPlace`): same volume, target parent equal to the
source's folder, not S3. `dialog-state.svelte.ts::renameInSourcePane` then calls the source pane's
`startRename({ initialName, commitTarget })` (`pane/rename-flow.svelte.ts`), which renames that path at once, with no
Enter. **Decision/Why:** the rename engine already answers everything such a move needs, on every volume: F2's conflict
and extension asks, a case-only change on a case-folding filesystem (`notes.txt` to `Notes.txt`, which the move engine's
identity check would drop as "already in place"), and one undo row. Teaching the move engines a case-only special case
instead fixed local drives only, since SMB and MTP have no inode to settle identity. It falls back to the transfer for
an MCP call (it waits for an operation id) and when the source pane has left the folder. S3 keeps the transfer, which is
what its rename runs on anyway.

The confirm carries the effective volume, its volume-relative parent, and `destinationName` separately. The volume
selector and space, existence, and conflict checks follow that destination. A filename conflict probe asks for the new
name but forwards the ORIGINAL source name as a bulk-skip key. Destination edits invalidate old conflict answers
immediately and debounce another check; an old in-flight answer cannot supply skip names to a newer target.

The pane preserves `destinationName` through progress and retry and suppresses the automatic rename editor when the user
already supplied a name. Backend landing and safety:
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "Named destinations".

## How transfer flows

1. **TransferDialog** (destination picker + dry-run scan)
   - Pre-fills destination from the opposite pane, volume-relative via `toVolumeRelativePath`. That derivation is
     load-bearing rather than cosmetic: a prefill that comes out RELATIVE fails the absolute-path check below, and
     `handleConfirm` then refuses to dispatch, so the Copy button does nothing and says nothing. Why a remote volume's
     root needs trimming before the slice: `$lib/path/DETAILS.md` § "Volume membership is a component match".
   - The segmented Copy/Move toggle is always shown so the user can flip the operation regardless of how the dialog was
     triggered (F5/F6, command palette, drag-and-drop).
   - Resolves a complete single-item Copy/Move target as described in § "Single-item destinations", then validates path
     structure via `validateDirectoryPath()` from `$lib/utils/filename-validation` (empty, absolute, null bytes, length
     limits), then checks logical constraints (a folder into its own subfolder). A destination that IS the source's own
     folder is allowed: it duplicates, and the backend resolves that per item
     (`src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in place)").
   - The logical check anchors BOTH sides on their own volumes first (`anchorOnVolume`, the frontend twin of
     `root_anchored`): the box is volume-relative, the sources are the pane's absolute paths. Comparing raw spellings
     never caught a folder copied into itself on a drive under `/Volumes`, and refused cross-volume copies whose paths
     merely repeated. It doesn't fold case (it can't know the volume's case rule); the backend's canonicalizing guard
     catches that. The skip-confirmation path calls the same validator.
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
     the source volume with one `get_metadata` per top-level path, 16 at a time and never a subtree walk, when the check
     passes `sourceVolumeId` + `sourcePaths`):
     - **dir + dir** → a silent merge, NOT a conflict. Surfaced as an informational line ("N folders will merge with
       existing folders"); never counted in `totalConflictCount`; never forwarded as a bulk-skip name (a merging folder
       must not be skipped wholesale).
     - **file + file / cross-type (file↔folder)** → a real conflict. Counts toward `totalConflictCount` and feeds the
       `preKnownConflicts` bulk-skip list.
     - The file-policy radios show when there's a real conflict OR a folder merge — a merge can surface file clashes
       mid-operation the upfront (top-level-only) check can't see, and the radios pre-answer them.
     - **Top-level is where the check stops, deliberately.** Making it recursive means walking the whole destination
       tree before every copy, and the single listing it does today already runs for minutes on a big remote directory.
       Deep clashes are meant to surface mid-operation and get answered there, by the progress dialog or by the main
       window's prompt (`../DETAILS.md` § "Conflict prompts for operations with no dialog"). So a deep clash that nobody
       answers is a missing listener, never a reason to widen this check.
     - **Cross-type guardrail.** When a real conflict is a type mismatch AND the user selects "Overwrite all", a red
       warning appears (mirrors the per-file dialog's file↔folder warning): overwriting replaces items of a different
       type, including folder contents.

2. **TransferProgressDialog** (operation execution)
   - Dispatches the operation on mount, whether or not `TransferDialog`'s preview has finished walking. The operation
     claims that preview in the backend and its own task waits for it, so the dialog is a view over a named operation
     from the first frame. While `phase === 'scanning'` it renders the scan-phase body and disables Rollback (nothing
     written yet); Pause and Background both stay. Pause reaches the WALK — it parks between entries on the operation's
     own gate (`write_operations/scan_bridge.rs` § `ScanPause`) — so a paused scan is titled "Paused" and drops its
     spinner and its rates, because it has genuinely stopped.
   - Routes to a backend command through `transfer-dispatch.ts`, then binds the session for the id it gets back.
   - Subscribes to nothing. The window's fan-out holds the seven streams and buffers whatever arrives for an id no
     session has claimed yet, which covers the gap between the start command answering and the binder acquiring. § "The
     dialog is a view".
   - Dual progress bars (size + file count). Speed (both bytes/s and files/s) and ETA come pre-computed from the backend
     (`write_operations/eta.rs`) on every `WriteProgressEvent`; the dialog renders the numbers and applies a tiny
     display low-pass to the ETA to prevent flicker. No FE-side math. See BE § "ETA + throughput".
   - Dynamic stage indicator: "Scanning" → "Copying" → "Writing the last piece..." (+ "Removing the originals..." for a
     cross-disk move, § below).
   - **Flushing phase.** When a `write-progress` event arrives with `phase: 'flushing'`, the dialog title shows
     **"Writing the last piece..."** (exact copy). This is the backend's closing `fdatasync` over the freshly written
     destinations — on slow media (USB sticks, SD cards) it's a real multi-second pause, so the bar must not sit frozen
     at 100% pretending the work is done. The phase maps back to the active stage chip (copying/moving) in
     `getStageStatus`, since it's the tail of the copy, not a separate chip. Shown for both copy and move. Pinned by
     `TransferProgressDialog.flushing.test.ts`. See the BE doc § "Durability" for what the flush actually does.
   - **Removing the originals** (`phase === 'deleting'` on a MOVE): the closing stage of a move BETWEEN disks, which
     copies first and only then removes the sources. The title reads "Removing the originals..."; the readout counts
     ITEMS over the top-level sources (`progressCountKind`), because `remove_dir_all` takes a whole subtree in one call
     and reports nothing from inside it, and the size bar drops out since no bytes move. ❌ Don't fold this into
     `titleActive`'s `delete` arm: "Deleting..." over a move the user asked for reads as their files being destroyed. A
     move WITHIN one disk never reaches it (a rename leaves no originals), and a real `delete` operation keeps
     "Deleting...", which is what its `deleting` phase actually is. Why the backend emits it at all, and what the dialog
     looked like when it didn't: `apps/desktop/src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "The
     source sweep reports itself". Pinned by `TransferProgressDialog.flushing.test.ts`.
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
   - **Rollback is BLOCKED wherever the backend can't reverse, or can't reverse any more.** Both affordances (the
     conflict-section footer and the main footer) render the button `aria-disabled` with a tooltip that says why, and
     the press is guarded so a blocked click asks nothing. `aria-disabled` rather than `disabled`, in both spots and for
     both reasons: a disabled button leaves the tab order and takes its explanation with it. Plain Cancel stays
     reachable throughout; in the conflict footer, where Rollback would otherwise be the only button, a plain Cancel
     renders alongside it. Two reasons reach the state, and the dialog folds them into one `rollbackBlockedTooltip`:
     - **The strategy can't reverse at all**: the operation's `supportsRollback` is off, plus the props-only
       `isSameVolumeMove` for the frames before the first snapshot lands (a move where source and destination are the
       SAME non-default volume — one smb2 share, one MTP device — which the backend runs as a server-side
       `volume.rename` rename-merge that stops without reversing). Tooltip: "Rollback is not available for same-volume
       moves". Local→local same-FS moves keep a live Rollback (real `MoveTransaction` rollback), so the default local
       volume is excluded from the props rule.
     - **It could, and the moment has passed**: `reversalWindowClosed` (`../reversal-wording.ts`) — a local cross-FS
       move in its `deleting` phase has landed every file and committed, so a Rollback there would only stop the source
       sweep. The tooltip says so and points at the Cancel that still spares the untouched originals; the stacked
       confirmation is withdrawn if the phase arrives while it's up, because its promise has just become false.

     Pinned by `TransferProgressDialog.rollback.test.ts`.

3. **TransferErrorDialog** (error display)
   - Renders entirely from the typed `WriteOperationError` (`WriteErrorEvent` carries no prose): title, message, and
     suggestion via `getUserFriendlyMessage` / `FallbackErrorContent`; category + retry classification via
     `getErrorDisplayMeta` (both in `transfer-error-messages.ts`; the details block in `transfer-error-details.ts`). All
     words live on the FE.
   - Container colors and icon vary by category: error-bg + CircleAlert (`serious`), warning-bg + TriangleAlert
     (`transient`), neutral secondary-bg + Info (`needs_action`).
   - "Retry" button shows when `category === 'transient'` or the variant's `retryHint` is true, AND there's something to
     retry: `DialogManager` passes `onRetry` only when `TransferErrorPropsData.retry` holds the failed operation's birth
     context. Retry settles the failure like Close, then starts that context again through `startBirthOperation` as a
     NEW operation (`retryPropsFrom`: fresh preview, no stale pre-known conflicts, no MCP round-trip, initiated by the
     user). An adopted failure has no birth context, so no Retry. ❌ Don't drop the `onRetry` wiring: without it the
     whole meta table is decoration (it was, until cmdr-reports#17). Pinned by `DialogManager.svelte.test.ts`.
   - `getErrorDisplayMeta` mirrors the category/retryHint the Rust write-error mapper assigned per variant; keep the two
     in step if a `WriteOperationError` variant is added.
   - **A refused trash words itself from its typed `TrashRefusalKind`, and never says "try again".** `trash_refused`
     exists precisely because the flattened `io_error` version said "Try again. If the problem persists, check the
     technical details below" over a permission refusal that retrying cannot change, with the OS's actual reason folded
     away behind a disclosure. Each reason gets its own explanation and its own way through (permanent delete for the
     two that have one). When this Mac is ALSO missing Full Disk Access and the reason is permission-shaped, one extra
     line offers that — additive, never a replacement, and never on a reason Cmdr couldn't classify. The rule and why
     it's narrow: `$lib/onboarding/DETAILS.md` § "What an error message may add about it".

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
- **An agent's unlock stores but never starts.** The MCP `unlock_archive` tool routes to `supplyStoredPassword` instead
  of `handleArchivePasswordSubmit`, and ❌ never re-dispatches: it settles the transfer, and the agent runs `copy` /
  `move` again so extraction goes through the same gate as every other write. `pane/DETAILS.md` § "Only a PERSON's
  submit re-dispatches".
- **Mid-transfer wrong password.** ZipCrypto's open-time check false-accepts ~1/256, caught later at end-of-stream CRC,
  so a `wrongAttempt: true` error can arrive AFTER progress started. The interception is in the running-op error path,
  so this is handled the same as an up-front rejection — no separate pre-flight branch.
- **Cancel settles cleanly.** `handleArchivePasswordCancel` calls `clearArchivePassword` (forget the archive password)
  and runs the same tail a dismissed transfer error does — refresh both panes, drop the source-pane operation snapshot
  and selection, null the props, refocus — so nothing looks stuck. The op already terminated on the backend (the
  write-error settled it), so there's no running op to cancel.
- **AES archives reach here too.** WinZip AES (AE-1/AE-2, what `7z -mem=AES256` and recent WinZip write) decrypts
  through the `zip` crate's `aes-crypto` feature, and a password-protected 7z through `sevenz-rust2`'s `aes256`, so both
  raise this prompt and both can be unlocked by it (`crates/cmdr-archive/src/read/archive_test.rs`,
  `read/multiformat_test.rs`). Only the rejection family — not-an-archive, an unsupported codec, a synthesized tree past
  the node cap — collapses to `Unsupported` and takes the ordinary friendly-error path instead.

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
- **`resolveSourceVolumeId(paths, volumes, resolvePathVolume)`** — resolves the REAL source volume for DROPPED and
  PASTED paths so they carry the same accurate `sourceVolumeId` an F5 transfer does. FAVORITES
  (`category === 'favorite'`) are filtered out of the candidate set first: they're picker-only pseudo-volumes the
  backend can't dispatch against, so a path under `~/Desktop` must resolve to its BACKING real volume (`root`), not the
  non-existent `fav-desktop` (dropping a Desktop file used to fail with "Source volume 'fav-desktop' not found"). Then
  frontend longest-prefix (`drag/drop-operation.ts::findVolumeIdForPath`, handles MTP-shaped paths) → backend
  `resolve_path_volume` for the common parent when no registered root matches → `root` (the honest unknown). NEVER
  returns a knowingly-wrong id: when per-path matches disagree (sources span volumes) or resolution fails, it returns
  `root`, which gives a degraded-but-correct result. The drop path feeds the result into `startScanPreview`'s
  `sourceVolumeId` arg via `TransferDialog`, so the byte scan stats the right volume (a cross-volume drop's counters
  fill instead of reading 0). It runs for EXTERNAL drops only; an in-app self-drag bypasses it via the recorded
  self-drag identity (the drop carries the source volume + volume-relative paths directly — see
  `file-explorer/drag/CLAUDE.md` § "Self-drag identity").

  **What naming the source volume buys the paste path.** The bytes never depended on it: clipboard paths are absolute
  and the backend's both-local branch does `src_root.join(absolute)`, which is the absolute path, and the local move
  engine picks same-fs vs cross-fs from runtime device ids. Everything keyed on the volume ITSELF does depend on it, and
  a flat `root` broke all of it: the source volume has to enter the busy set so Eject stays DISABLED while a paste reads
  off a USB stick, a DMG, or a mounted share (`status_cache.rs::compute_busy_volume_ids` filters root out, and
  `volume/eject/mod.rs::eject` refuses only what's in that set); the operation has to take the source mount's lane so
  two pastes off one device serialize; the operation log records the source; and `TransferProgressDialog`'s direction
  header resolves the source label off that id.

  ❌ Not `pane/snapshot-source-volume.ts::resolveSnapshotSourceVolume` for the paste — that one also answers
  `supportsTrash`, which a paste has no use for, and it deliberately skips the backend round-trip because a snapshot's
  paths came out of ONE volume's index. Clipboard paths carry no such guarantee (they can come from Finder or any other
  app), so the `resolve_path_volume` fallback is exactly what they need.

The paste path keeps its scheme-path refusal ("Use F5 to copy files onto this device.", or the server wording) SEPARATE
and BEFORE the shared guard, because that toast points the user at the copy flow paste lacks; the shared guard then
handles read-only / search-results destinations uniformly. The key it names is read live off `file.copy`, never spelled
`F5` in the catalog: both transfer keys are rebindable.

**Folder-targeted transfers into the source folder are asymmetric between Copy and Move.** Copy duplicates each item
under a free ` (N)` name. Move is already done, so the batch validator rejects it, and
`clipboard-operations.ts::pasteWouldMoveNothing` short-circuits a cut-paste whose sources are all already in the
destination: no dialog or transfer, and the clipboard survives for the next paste. A partial set dispatches normally and
the backend drops identity moves. The frontend check is lexical; the backend settles filesystem identity
(`src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in place)"). The
single-item dialog instead compares complete destinations, as described in § "Single-item destinations".

What the pre-confirm conflict check owes that backend is the SOURCE side of the question:
`transfer-conflict-check.svelte.ts` forwards `sourceVolumeId` and `sourcePaths` to `scanVolumeForConflicts`, and only
with those can the command drop the collisions that name a source itself. Without them the check matches by name alone,
every source of a same-folder copy comes back as its own clash, and the dialog grows a conflict count, the
overwrite/skip/rename radios, and a bulk-skip list naming the files the user asked to duplicate.
`transfer-conflict-check.svelte.test.ts` pins it with a mock that behaves like that backend rather than answering a
canned list.

#### Only paste and F5 end a duplicate in the rename editor

A transfer that duplicates ONE item in the folder it already lived in can end by opening the inline rename editor on the
copy, stem selected, so naming it costs one keystroke sequence and Esc keeps the generated ` (N)` name. Which gestures
ask for that is carried by `duplicateFollowUp`, a REQUIRED field on both `TransferDialogPropsData` and
`TransferProgressPropsData`: every gesture dispatches the same backend copy, so a trigger that said nothing would
inherit whatever the last one wanted. The mechanism is `pane/duplicate-rename.ts`; the settled tail that runs it is
`pane/DETAILS.md` § "Naming a duplicate".

Who answers what, and why:

- **Paste** (`clipboard-operations.ts`) and **batch F5** (`file-operation-commands.ts::openUnifiedTransferDialog`) say
  `openRenameEditor`. They're the gestures where a person has just DIRECTED a copy somewhere, and they're what issue #50
  actually asked for ("all file managers I used so far would ask for a new name **when pasting**").
- **An auto-confirmed F5 is MCP**, and says `nothing`: an agent's copy has no business pulling focus into a text field
  in front of whoever is watching.
- **Drag and drop** (`drag-drop-controller.svelte.ts::DROP_DUPLICATE_FOLLOW_UP`) says `nothing`. A drag ends with the
  mouse, and stealing focus into a text field on mouse-release is the wrong shape.
- **The Duplicate command** (`pane/duplicate-command.ts`) says `nothing`. ⌘D _is_ Finder's Duplicate, and the
  familiarity that justifies the key rests on it asking nothing; an editor would also break stamping out several copies
  in a row, since after the first ⌘D focus sits in an editor and the second does nothing until Esc.

**The modal "name the copy" prompt issue #50 asked for is what this replaces.** A blocking dialog would tax every paste
for the minority of pastes where the generated name isn't fine, and it's the wrong shape in a keyboard-first app. The
two mainstream conventions disagree only about WHEN the name is chosen (Finder names it after, with no prompt; Total
Commander names it before, in the copy dialog's target field), and the split above serves both: the gestures that ask
land in an editor one keystroke sequence long, the ones that don't are already finished. No setting gates it either, for
the same reason: two gestures that ask and two that don't already cover both preferences, so a toggle would be a
preference nobody needs to find.

**Single-item F5/F6 chooses the name in its target field**, so its duplicate follow-up is `nothing`; the target contract
lives in § "Single-item destinations". Batch F5 and Paste retain the inline rename follow-up above.

### Unified components for Copy + Move

Copy and Move share 95%+ of UI/flow. Differences:

- Labels ("Copy" vs "Move")
- Backend command (`copyBetweenVolumes()` vs `moveBetweenVolumes()` / same-volume `moveFiles()`)
- Post-completion: move refreshes both panes (source files gone)
- Cross-FS move has an extra closing stage, § "Removing the originals"

Parameterizing by `operationType` avoids duplication and guarantees UX consistency.

### Compress mode (the Transfer dialog's third operation)

Compress rides the SAME dialog/progress/state components as copy/move via `operationType: 'compress'`. The backend
carries `WriteOperationType::Compress` through registration, progress, terminal events, and bindings while mapping it to
archive-edit journal and analytics semantics. The seed mechanism lives in
`apps/desktop/src-tauri/src/file_system/write_operations/archive_edit/DETAILS.md` § "The driver, op by op". The
user-visible differences from copy/move:

- **The progress title follows the backend phase.** `Compressing` measures uncompressed source bytes; `Transferring`
  measures the completed archive's compressed bytes and starts a new ETA. `FinishingCompression` and `FinishingTransfer`
  have no honest denominator, so the dialog renders a spinner and phase label instead of `TransferProgressReadout`. A
  local compress has no transfer phase. The queue row and corner chip consume the same `archivePhaseLabel` /
  `isIndeterminateProgressPhase` rules, so backgrounding cannot change the claim. A copy or move INTO a zip on a remote
  volume ends with the same two upload phases under its own `copy` / `move` dialog.
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

### Rename mode (F2 on a big S3 folder)

`TransferDialogPropsData.newName` turns the Move dialog into a rename confirmation (`rename/DETAILS.md` § "A rename that
copies"). The path box holds folder + new name, the Copy/Move/Compress toggle gives way to one hint line, and the volume
picker is disabled (`renameByMove` works on one volume). It is ❌ never `isSameVolumeMove`: the deep scan runs so the
dialog shows the counts, and the backend consumes the preview. The top-level conflict check is skipped
(`data-conflict-state="skipped"`; the source would clash with itself), the leaf is validated as a NAME, and the
dest-exists probe asks about the folder. Confirm splits the box (`splitPathLeaf`: a trailing slash is an empty name, ❌
never the folder's name one level up) into `destination` + `newName`, which rides to `transfer-dispatch.ts` and routes
to `renameByMove`. `retryPropsFrom` keeps it.

### Same-FS move optimization

When source and destination are on the same filesystem (checked via `metadata.dev()`), backend uses instant `rename()`.
Frontend handles this by:

- Skipping progress dialog if operation completes before render
- Showing brief success toast instead
- Still doing conflict scan upfront in dry-run mode (just `exists()` checks, ~100 ms for 10k files)

### Same-volume move skips the deep scan preview

`isSameVolumeMove = activeOperationType === 'move' && sourceVolumeId !== DEFAULT_VOLUME_ID && sourceVolumeId === selectedVolumeId && !renamesCanCopy`
(derived in `TransferDialog`, no extra prop; rename mode is never it either). For a same-volume move the backend does a
server-side rename-merge that transfers zero bytes, so the deep recursive scan preview — which exists only to feed the
Size bar — is pure waste. On a NAS it used to cost 30–40 s of "Verifying before move…" before a 100 ms rename. So:

The `DEFAULT_VOLUME_ID` exclusion is load-bearing and mirrors the same guard in `TransferProgressDialog`'s
`isSameVolumeMove`: a local→local move (root → root) is NOT a server-side rename. The backend's local move path
**consumes** the preview cache via `config.preview_id`, and the dialog's tallies come from the preview — so cancelling
it for a local→local move both zeroes the dialog counters and forces a backend re-scan. Local→local keeps the deep
preview running.

**A volume whose renames copy keeps the scan too** (`capabilitiesFor(sourceVolumeId).renamesCanCopy`, S3): its move is a
server-side copy per object, billed, so the dialog scans for the counts and the S3 cost line, and confirm hands the
preview id to `move_within_same_volume`, which waits it out through `await_claimed_preview` exactly as for rename mode.

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

### Conflict-check indicator

The conflict check uses a reserved spinner slot beside the file count. It appears only after the actual check has run
for 100 ms, clears immediately on completion or target changes, and never adds a row or shifts the count. The existing
checking message labels the spinner for assistive technology; the shared spinner honors reduced motion. The debounce
before a check starts does not count toward the delay.

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
execution (the local `move_files` command always did, and `copy_between_volumes`/`move_between_volumes` expand a leading
`~` for a destination volume with a local path via `write_operations/routing.rs::resolve_dest_path`, which the
write-access probe below anchors through too).

Two niceties on top:

- **Home shows as its long form.** On mount the dialog resolves `homeDir()` and, when `editedPath` is exactly `~` (the
  destination pane sitting at home root), replaces it with the absolute path (`/Users/me`) — a bare `~` in the box reads
  as a glitch. A `~/sub` path keeps its short form; only the exact-home case expands. Done before the scan and conflict
  check so they run against the absolute path.
- **Yellow "this folder will be created" warning.** A debounced (`createDebounce`, 300 ms) `destinationExists` probe of
  the resolved destination flips `targetMissing`. When the path is structurally valid (no red `pathError`) but the
  folder doesn't exist, the field takes `TextInput`'s `warning` state (a yellow border and ring) and a yellow message
  line (`.path-warning`, keys `targetWillBeCreated{Copy,Move}`). The red error always wins — the two never show at once.
  A timeout is inconclusive (hung mount), so it stays quiet rather than over-promising. A monotonic `existsCheckSeq`
  drops a stale probe that lands after a newer keystroke. **The probe counts a look-alike** (`destinationExists`, ❌ not
  `pathExistsChecked`): a folder the share holds in another Unicode spelling (`fotók` composed vs decomposed) is where
  the copy merges, so "will be created" would lie, and compress's overwrite warning (below) must show for an archive
  it's about to replace in place. It costs one listing of the parent, only for a non-ASCII name that missed on a
  byte-exact volume; a listing that fails reads as "couldn't tell". Backend:
  `apps/desktop/src-tauri/src/commands/file_system/listing.rs`.
- **Red "nothing can go here" notice.** The same debounced probe also asks `destinationWriteAccess` (the
  `destination_write_access` command: `Volume::write_access_at` on the resolved folder, 2 s, `unknown` for an
  unregistered volume). A definite `unwritable` shows a red line under the box (`#transfer-path-refusal`, keys
  `destinationReadOnly` / `destinationNoPermission` / `destinationNotWritable`, one per reason the backend could tell
  apart) and suppresses the yellow "will be created" warning, which would be a promise the transfer can't keep. The
  structural red error still wins. While it shows, Confirm is disabled and Enter does nothing (`confirmFromUser`), the
  way a path error disables them, with the notice as the reason. Decision/Why: the transfer asks the same question
  before it writes and refuses with the typed `destination_not_writable` error anyway, so an enabled button led only to
  that refusal in an extra dialog. ❗ The gate sits in `confirmFromUser`, ❌ never `handleConfirm`: an MCP confirm and
  the auto-confirm go through `handleConfirm` and meet the backend's typed refusal, where a refused confirm would leave
  their round trip waiting. `unknown` shows nothing and blocks nothing. The phone case this exists for: copying onto a
  Pixel's `/` surfaced only after confirm, as "Not enough space".
- **Yellow "this path repeats the place's folder" warning (#164).** The same debounced probe asks `destinationRootEcho`
  (the `destination_root_echo` command over `cmdr_fs::volume::root_echo`, whose rule and why live in
  `crates/cmdr-fs/DETAILS.md` § "`root_anchored`"). On a place rooted at a server folder, `/srv/data/photos` reads two
  ways, so `#transfer-path-root-echo` names where it goes (`/srv/data/srv/data/photos`) and offers `/photos` behind a
  "Use shorter path" button. ❌ Nothing rewrites the box on its own, and Enter sends what the box says: the doubled
  folder can be real. It shows for a prefilled path too, and outranks "will be created" (usually also true of the
  doubled folder); red errors and the refusal outrank it.

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
- **A look-alike clash says so** (`destinationIsLookAlike`: the destination holds the name in another Unicode spelling,
  `café` composed vs decomposed). A quiet line under the folder (`lookAlikeHint`) explains that the names look the same
  but the server spells them differently and that Overwrite replaces the one that's there. The headline is already the
  stored entry's own spelling, which is the one Overwrite replaces and keeps, so nothing else changes; the incoming
  spelling prints identically and isn't shown. Backend side:
  `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Look-alike names and new-name
  spelling".

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

**A source no volume answers for is refused, with neither Retry nor Confirm.** A phone unplugged under a search-results
pane leaves a non-local volume id nothing registers. `start_scan_preview` refuses it with a typed `ScanPreviewRefusal`
(`SourceNotConnected`) instead of walking `adb://…` on the Mac, and the wrapper hands it back as `{ refusal }`, which
the scan state keeps as `sourceRefusal`. The dialog reads `unavailable`, says the phone or server isn't connected any
more, and disables Confirm (Enter too): retrying or proceeding can only fail until it's back. A confirm that beats the
refusal (MCP auto-confirm) reaches the backend, which answers `source_no_longer_connected` and its own error dialog. The
delete dialog treats the refusal like a walk that stopped. Pinned by `TransferDialog.unavailable.test.ts` § "a source no
volume answers for".

**The conflict check.** `transfer-conflict-check.svelte.ts` carries a `status` of `idle` / `checking` / `answered` /
`unknown` (a bounded `withTimeout` at 35 s over the IPC, just above the backend's own 30 s budget, catches a call that
never returns at all). `unknown` renders its own line, because rendering nothing is what a genuinely clean destination
renders, and the user is about to decide what happens to their files on the strength of it. Because that line is on
screen, a volume that couldn't answer (a rejection or the timeout) logs at warn; only a throw while reading an answer
the volume DID give logs at error, since that one is our defect. At error level, every slow or disconnected volume filed
an error report (ERR-J9BKB, ERR-F7N2B, ERR-YKADZ).

**A wedged SOURCE no longer costs the whole answer.** The 30 s backend budget covers both legs, but the optional
source-stat leg is capped at a third of it, so a source that never answers still leaves the mandatory destination scan
two thirds of the budget. The check then comes back with a real verdict built on the FE's own name-only
`sourceIsDirectory` values, rather than `unknown`. Only a destination that can't be read reaches `unknown` now.

**Why an unknown check is still safe to transfer on.** The pre-flight names feed `pre_known_conflicts`, which the
backend reads under `Skip` alone as a bulk-skip PERF hint (`build_pre_skip_set`); every clash is still detected and
arbitrated at write time. An unknown check contributes no names, so nothing is pre-skipped, and the policy radios never
render for it — leaving the default `stop`, which prompts per clash. ❌ Don't "helpfully" default an unknown check to a
non-prompting policy: that would turn "nobody looked" into a silent overwrite.

## What the error dialog says when a drive was pulled

`device_disconnected` is the one write error whose copy is about WHERE THE USER'S FILES ARE, not about what went wrong,
because it's read by someone who has just pulled a stick mid-transfer and is worried. Three facts in order: which drive
went, how far the transfer got, and what's where now.

The backend supplies both: `error.side` (`DisconnectedSide`: role, volume id, the volume's name, and the counterpart
volume's name) and `WriteErrorEvent.progressAtStop` (files and bytes done, plus a move's `sourcesRemoved` /
`sourcesLeft`). ❗ Every one of those is captured when the transfer STARTS — the volume list has already dropped an
unmounted volume by the time this renders, so ❌ nothing here looks a name up or infers a side from a path.

`progressAtStop` rides on the EVENT, not on the error, so it travels as its own argument: `transfer-progress-state` →
`onError(error, progressAtStop)` → `dialog-state.openTransferError` → `TransferErrorPropsData` → `TransferErrorDialog` →
`FallbackErrorContent` → `getUserFriendlyMessage(error, op, progressAtStop)`. A surface that kept only the error (a
retained failure in the queue, `queue/failure-reason.ts`) passes nothing and gets the sentence that needs no counts,
which is why the sided keys fall back rather than rendering "0 of 0 files".

Four sided sentences (`errors.write.deviceDisconnected.sided.<role>.<copy|move>`) plus the plain per-op ones for a
backend session that dropped with no typed side (MTP, SMB). The move-to-the-drive sentence carries no counts on purpose:
a move that stops keeps every original, so there is no partial state to report. `move_not_confirmed` is its neighbour:
the move's closing flush couldn't prove the copies were on disk, so every original stayed put, and the copy says exactly
that.

## Copy anyway

`insufficient_space` is a question, not a verdict: its `required` is an upper bound (files already at the destination
can make a copy need less, and the backend only looks at them on a local disk), so the copy is worded "may not have
enough space" and the error dialog offers **Copy anyway** beside Close. The click runs
`dialog-state.handleTransferErrorCopyAnyway`, which starts the failed copy's birth context again (`retryPropsFrom`,
fresh preview, same conflict policy) with `spaceShortfall: 'proceed'`, settling the failed one like a close. The field
rides `TransferProgressPropsData` → `TransferProgressDialog` → `TransferDispatchConfig` → `copyBetweenVolumes`, and the
backend then skips its free-space check (`SpaceShortfall` in
`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "The free-space pre-flight"). A destination that
really fills up still stops the copy, as `destination_full`.

Close stays primary, so Enter takes the safe way out. The button appears only for a `copy` with a birth context: an
adopted operation (queue window) and a retained failure have nothing to start again, and the suggestion text never names
the button for that reason. A plain Retry of a refused copy asks about space again; one of a copy already started anyway
keeps `proceed`. An MCP-started copy surfaces the same refusal to the agent, and only the person's click goes ahead.
Pinned by `TransferErrorDialog.typed.test.ts`, `DialogManager.svelte.test.ts`, and
`dialog-state.failure-handover.svelte.test.ts`.

## Gotchas

- **Always use batch IPC for selection lookups.** `get_paths_at_indices` (paths only) and `get_files_at_indices` (full
  `FileEntry` objects) fetch all selected items in a single IPC call. Never loop over `getFileAt` per-index; with 50k
  selected files, per-file IPC takes 5-10 seconds. Batch calls take ~1 ms regardless of count.
- **MTP move is interleaved copy + delete per file.** Moves involving MTP volumes copy and then delete each file
  individually (not copy-all-then-delete-all). Minimizes duplicates on partial failure: if it fails mid-way, only the
  current file exists in both places. The progress UI shows three stages (Scanning → Copying → Removing source). If copy
  succeeds but delete fails, the user keeps files in both places (safer than losing data).
- **Whether Rollback works is the OPERATION's answer, never the volume ids'.** A cross-volume move can't be reversed at
  all (the driver treats `RollingBack` exactly like `Stopped` and reports `rolled_back: false`) and looks, from the
  props, exactly like a perfectly reversible transfer. So both this dialog's footers read `supportsRollback` off the
  registry snapshot, the same flag the queue window reads. The props-only same-volume-move rule stands beside it for the
  frames before the first snapshot lands. See `src-tauri/src/file_system/write_operations/DETAILS.md` § "Rollback
  availability".
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
- **`write-cancelled` carries what the REVERSAL managed, not a boolean.** `event.rollback` is a `CancelRollback`: a
  three-state `outcome` (`notRolledBack` / `rolledBack` / `partiallyRolledBack`), how many items came back, and what the
  reversal left behind grouped by reason with one example name each. A reversal leaves things behind on purpose — it
  refuses to delete a destination something else changed since the transfer wrote it (BE doc:
  `write_operations/transfer/DETAILS.md` § "What a reversal does with that identity"). § "What a cancelled transfer's
  reversal says afterwards" is what turns it into a sentence.
- **Cancel close is two-condition: `write-cancelled` + `write-settled`.** When the user clicks Cancel (without
  rollback), `TransferProgressDialog` does NOT close immediately. It keeps the "Canceling…" label up until both events
  have arrived for this `operationId`, then applies the existing `MIN_DISPLAY_MS` floor and closes via
  `onCancelled(filesProcessed)`. After 200 ms of waiting, the label gains a clarifying tail: "Canceling… (finishing USB
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
- **A `transfer-scan-state` start can be overtaken before it's under way.** An MCP `dialog confirm` unmounts the dialog,
  or the Copy/Move toggle lands on a same-volume move and `cancelPreview()` resets the scan, while the listeners are
  still registering. `startScan` reads its getters before the first await, and each start carries a generation that the
  next start and `cancelPreview()` bump. An overtaken start keeps no listener, starts nothing, and frees a preview whose
  id lands late, so it can't write that id over the reset or over the scan a toggle back to Copy started.
  `../DETAILS.md` § "A dialog's async start can outlive it".

## What a cancelled transfer's reversal says afterwards

`cancel-rollback-toast.ts` reads `event.rollback` and raises the toast the user reads a second after the progress dialog
closes. `readCancelRollback` is pure and returns the already-localized lines; `raiseCancelRollbackToast` is the only
impure half. `CancelRollbackToastContent.svelte` stacks them: a headline, the expectation-setting line, one bullet per
typed reason, then any recovered-original paths and staged scratch.

**Why this exists at all.** The in-flight reversal's bar DRAINS, and every item it walks past advances it, so it always
lands on zero (BE doc: `write_operations/transfer/DETAILS.md`). Zero therefore means "this reversal is finished", not
"everything came off the disk" — and without a summary, a bar hitting zero on a reversal that deliberately left four
files behind is a lie the user has no way to catch. The bar completing and the summary telling the truth are ONE design,
not two features: a bar stranded at 94% reads as a crash, and a user who thinks the app crashed never reads the line
that would have explained things.

**Raised here, not by the parent.** The completion toast is composed up in `pane/dialog-state.svelte.ts` because it
needs birth context (the selection's file/folder split) that only the parent holds. This one needs nothing but the
event, and raising it where the event lands means the started arm and the ADOPTED arm both get it without
`onCancelled`'s signature having to carry a report the parent would only pass through.

**Two deliberate silences.** `notRolledBack` says nothing: everything the transfer wrote is still where it landed, which
is exactly what a plain Cancel asks for and what stopping a reversal before its first item leaves. A clean reversal with
`reversed === 0` says nothing either — the transfer had written nothing to undo, and "Removed 0 items" is noise.

**Both silences break on a field, never on `outcome`.** `stagedLeftovers` breaks them because `outcome` answers for the
LEDGER alone, and Cmdr's own scratch can outlive a perfect reversal. `originalsStillInPlace` breaks the first one for
the same reason and is the sharper case: a cross-filesystem move stopped on its last step (deleting the originals, after
every file has already arrived) has some of the user's originals gone for good and no reversal coming for them, yet its
`outcome` is a truthful `notRolledBack`. Silence there tells a user who pressed Rollback that nothing happened. So
`moveAlreadyLanded` says that Cmdr stopped part-way through removing the originals, that everything it moved is already
at the destination and staying there, and how many originals are still in the folder the move started from — at `info`,
because nothing went wrong, and claiming no undo, because none is coming. The removal clause earns its length: without
it the user has to infer from a bare count that some originals went, which is the one fact they can't check by looking
at the toast. Why the backend spends a field on this instead of an outcome: `write_operations/transfer/DETAILS.md` § "A
stop in the source sweep says where the files are".

`recovered` breaks both silences too. It names each original that an Overwrite displaced and the exact ` (recovered)`
path where Cmdr preserved it when its old name stayed occupied. This is data the user can find, not a reversal skip and
not Cmdr scratch, so it gets its own bullet list, exact full paths, the longer timeout, and `info` level. A recovered
entry also downgrades clean-completion wording and colour: `rolledBack` answers for the ledger, not whether every
displaced original reclaimed its name.

**How a stopped reversal is told apart from a skipping one**, with no extra field on the wire: a full pass that skipped
nothing lands `rolledBack`, so `partiallyRolledBack` with an EMPTY `skips` can only be a reversal the user stopped, and
it gets its own wording. When a stop DOES carry skip groups the partial wording covers it, because every line that
wording prints is true either way and none of them claims the ledger was walked to the end. ❌ Don't "fix" this with a
`stoppedEarly` flag on the event: the wire already answers the question, and the partial wording is written so it can't
overclaim.

**The verb comes off the EVENT's operation type, never a view's config.** Only a same-volume move carries items home;
every other in-flight reversal deletes what the transfer wrote, and a cross-drive move can't be reversed at all
(`notRolledBack`, so it never picks a verb — its own line is `moveAlreadyLanded`). A dialog that ADOPTED a running
operation was handed no birth context, so its config's operation type is inert there — reading it would word a move's
reversal as a delete on exactly the path where nobody could see it coming.

**Level is decided by whether Cmdr CHOSE the leftover.** Drift, an unverifiable snapshot, an occupied spot, and a
non-empty folder are all Cmdr protecting something, which is `info`. `failed` is the drive turning the undo down, which
is `warn` and may be worth retrying. ❌ Never colour a deliberate skip as a warning: the whole point of the copy is that
the user finishes reading it feeling Cmdr did the careful thing.

**The reason lines are two keys per reason, named and counted**, for the reason `$lib/ask-cmdr/`'s rename-undo rail
already found: "name the one file" vs "count them" is a display decision, not a plural category, and a locale with only
`other` (Chinese, Vietnamese) can't express both from one message. `alreadyGone` maps to `null` — the backend counts it
as reversed, so it never arrives as a skip — and it stays in the map so a NEW `SkipReason` is a compile error here
rather than a count silently dropped off the list.

**Cmdr's OWN leftover gets its own line, outside the reason list.** `rollback.stagedLeftovers` is the `.cmdr-tmp-*`
scratch a cancelled transfer's sweep asked the destination to remove and didn't get, because an abandoned write task
still holds the handle (BE doc: `write_operations/transfer/DETAILS.md` § "Naming what a cancel left behind"). Three
rules the readout enforces, and they exist because the defect being fixed was a toast that said the reversal was clean
while gigabytes sat on the user's NAS:

- **A leftover always speaks**, including through both silences above. `notRolledBack` carrying one raises a toast with
  no headline at all, which is how a plain Stop and a cross-volume move's cancel get to report theirs.
- **It never renders as a clean success.** The headline drops to the partial wording (`someDeleted`, never
  `doneDeleting`, which says "the items Cmdr had written" and would claim a completeness the destination just denied)
  and the level goes `warn`, whatever `outcome` says. `outcome` answers for the LEDGER only, so `rolledBack` plus a
  leftover is a real combination rather than a contradiction to paper over.
- **It sits BELOW the bullets, not among them.** The bullets live under "Cmdr skips anything it isn't sure about", which
  is Cmdr protecting the user's files; this is Cmdr's own working file, and borrowing that framing would read as a
  choice nobody made.

The copy says it is safe to delete and that Cmdr clears it on **a later** transfer there. ❌ Never "the next one": the
backstop sweep spares anything under an hour old, so a person who cancels and retries straight away meets their own
leftover. Over-promising there would re-create the exact defect this line was added to remove.

**The confirmation ends where its siblings do.** `fileOperations.rollbackConfirm.body` carries the same "Cmdr skips
anything it isn't sure about, so a few may stay behind" clause as its three `bodyUndo*` siblings, because the recheck
makes "this deletes every file the operation has written so far" an over-promise. The Rollback TOOLTIP carries no such
hedge: its job is telling Rollback apart from Cancel, the confirmation two clicks later carries the detail, and hedging
a seven-word tooltip would blunt the warning it exists to deliver. It does split by operation, though
(`inFlightRollbackTooltipKey`), because "delete every file written so far" over a move's reversal isn't a hedge missing,
it's the wrong verb.

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
otherwise the same one, down to the buttons. Four things differ, each for its own reason:

- **Auto-queue is off.** It is a decision a DISPATCHING view makes (don't stack a second modal over the one already up);
  a view opened precisely to watch this operation would instead bounce it back out of sight, which reads as the button
  doing nothing. The queue row correspondingly doesn't offer Show on a `queued` row.
- **Rollback comes from the registry row.** `rollbackUnavailable` reads the snapshot's `supportsRollback`, which is a
  promise about the OPERATION; an adopted view has no volume ids or direction to reason from. The props-only
  same-volume-move rule stands beside it for the window before the first snapshot lands, and the two phase gates
  (nothing written during a scan, nothing reversible once a move is sweeping its sources) apply to an adopted view
  exactly as they do to a dispatching one — they read the live phase, which is the one thing both kinds of view always
  have.
- **The parent runs no pane tail.** An adopted view has no birth context, and the two-slot arrangement in `dialog-state`
  is what makes the wrong version unreachable: `../../file-explorer/pane/DETAILS.md` § "Birth context".
- **An adopted REVERSAL ends by leaving the registry.** Show is offered on an operation-log undo too, and that operation
  emits progress with no terminal event, so the view closes through `onCancelled` once its session reports
  `leftRegistry`, whether it finished or a Cancel stopped it. Without that, it never closes on its own and every Cancel
  waits out `CANCEL_SETTLE_FALLBACK_MS`. ❌ The reading is gated on `snapshot.reverses`: an ordinary transfer's removal
  can overtake its `write-complete`, and closing on it would report a cancel for a copy that finished. `canHandOff` also
  refuses an operation that left, so closing the modal can't "background" something that's gone.

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
  flag, shows the queue window WITHOUT focus (`openQueueWindow({ focus: false })`, so the keyboard stays in the pane the
  person was working in), shows a quiet `info` toast (group `transfer-queue`), and calls `onQueue(operationId)` so the
  parent (`dialog-state.svelte.ts` → `handleTransferQueue`) unmounts the modal **without cancelling** the op and hands
  its birth context to the background watch (§ "Starting in the background", for the archive-password stop). The op runs
  on, now managed in the queue window. The button reads "Background" with an empty queue and "Queue" otherwise
  (`../queue/queue-backlog.ts`); the action is the same either way.
- **`backgrounded` is a one-way latch, not a guard against anything.** It records that this view has already handed the
  operation over, so a second Queue press, an auto-queue firing behind a manual one, and a close during the handoff are
  all no-ops. It no longer suppresses a teardown cancel, because there is no teardown cancel: every unmount leaves the
  operation running. It stays a plain `let` regardless — see § "File map".
- **Dialog-scoped F2 → Queue.** `handleKeydown` (passed to `ModalDialog` as `onkeydown`) intercepts `F2` and triggers
  `handleQueue`, so F5, Enter, F2 still works; F5, F2 (TC's own muscle memory) is the setup dialog's, § "Starting in the
  background". It is NOT a `command-registry` binding: F2 is globally `file.rename`. The mechanism that scopes it:
  `ModalDialog`'s overlay `handleOverlayKeydown` `stopPropagation`s every keydown before it can reach the global root
  key handler, so while the dialog is open F2 never reaches `file.rename`; and when the dialog unmounts, the handler
  goes with it, so F2 falls through to `file.rename` again. No global binding is ever installed or removed — the
  leak-free property is structural, not bookkeeping. (Pinned by the negative test in
  `TransferProgressDialog.queue.test.ts`.) `preventDefault` stops any default browser action on the key.
- **Auto-queue surfacing.** When a new op starts on a busy lane, the manager admits it as `queued` rather than spawning
  it. A DISPATCHING view watches `session.status` for that and auto-backgrounds: it surfaces the queue window with a
  quiet "N transfers ahead" toast and unmounts, exactly like a manual Queue. ❌ An ADOPTED view (`adoptedOperationId`,
  the queue window's Show) never does, and the effect returns early for one: a dialog opened precisely to watch this
  operation hiding itself again is the button appearing to do nothing. The currently-foregrounded op keeps its modal; we
  never stack a second modal. "N ahead" counts the ops occupying lanes (running or paused) in the main window's
  operations store — the same live rows the Background/Queue label reads — floored at 1. The dialog needs no seeding
  logic of its own: a session that hears nothing on attach asks `list_operations()` itself, which is what catches the
  registration tick that fired before anything in this window was watching.

## Starting in the background (F2 in a setup dialog)

Total Commander's "F2 Queue": in the copy / move / compress dialog (`TransferDialog.svelte`) and the trash dialog
(`../delete/DeleteDialog.svelte`), F2 or the Background / Queue button beside Confirm starts the operation with NO
progress dialog (#381). So F5, F2, F5, F2 queues two copies without a modal in between.

**F2 is Enter plus one flag.** The dialog runs its own confirm (`confirmFromUser` → `handleConfirm`), so it meets the
same guards (a destination that refuses writes, a source that can't be read), waits for `scan.scanStarted` the same way,
and sends the same `TransferConfirmPayload` with `startInBackground: true`. The button, its label rule ("Queue" while
other work is live, "Background" otherwise, `../queue/queue-backlog.ts` with no self id), and its tooltip naming F2 live
in `../StartInBackgroundButton.svelte`; the plain-F2 matcher in `../start-in-background-key.ts`. Dialog-scoped exactly
like the progress dialog's F2: `ModalDialog`'s overlay stops every keydown, so F2 is `file.rename` again once the dialog
closes (negative tests in `TransferDialog.background.test.ts` and `../delete/DeleteDialog.background.svelte.test.ts`).

**Trash only, ❌ never a permanent delete.** The delete dialog offers the button and F2 only while its FINAL
`isPermanent` is false, so the switch, a held Shift, online-only content (up front or found by the walk mid-confirm), an
archive, and a volume with no trash all take them away, and `⇧F2` matches nothing. The confirm re-checks after its
awaits (Shift can go down meanwhile), and the background callback `onConfirmInBackground(previewId)` carries no mode, so
`dialog-state`'s `handleTrashInBackground` can only build a `trash`. Two more locks behind it: `background.start`
refuses a `delete` outright, and a permanent delete sent off with Queue isn't tagged `startInBackground`, so its
password re-dispatch reopens the progress dialog.

**No modal, ever.** `../../file-explorer/pane/dialog-state.svelte.ts` hands the birth props to
`../../file-explorer/pane/background-operations.svelte.ts`, which starts them through
`transfer-dispatch.ts::startTransferOperation` (the same dispatch, MCP reply, and typed error the progress dialog's
birth uses) and never touches the progress slot. Nothing mounts, not for a frame (pinned with a `MutationObserver` in
`dialog-state.transfer-confirm.svelte.test.ts`), and the next F5 opens at once. Then: the source pane's selection drops
(as a Queue press does), a quiet toast says where the job went, the queue window shows without focus, and the pane gets
the keyboard back (`onRefocus`). **Decision/Why:** mounting the progress dialog with a "background on bind" prop was the
smaller change, but it holds the slot until the id lands and can flash a frame.

**What each follow-up a foreground start has becomes** (pinned in `dialog-state.background.svelte.test.ts` and
`background-operations.svelte.test.ts`):

- **Conflict prompts**: no foreground claim and no foreground id, so `../operation-conflict.svelte.ts` owns any clash
  and asks on the main window, as for any backgrounded job. F2 never means "overwrite silently".
- **Late dialogs never stack.** A background job's archive-password prompt and a refused start's error dialog wait in
  `../../file-explorer/pane/when-dialogs-clear.svelte.ts` until nothing else is on screen (no dialog, no progress slot
  held), then show, one per free moment. The person may be typing in a new setup dialog by then: a stacked prompt would
  steal their keys, and its close would refocus the pane under the dialog still up. A held password prompt says so in a
  quiet toast, so the wait is never silent.
- **A refused start** (the backend says no before anything runs): the error dialog, with no failure to claim. Its Retry
  (and "Copy anyway") starts in the background again, because `startInBackground` rides on the retry props.
- **Archive password**: the backend doesn't retain `archive_needs_password` as a failure, so with no dialog nobody would
  ask. The background module holds the session until the outcome lands and hands that one stop back; `dialog-state`
  borrows the birth slot for the prompt (once the window is free, above), and a person's submit re-dispatches in the
  background again (fresh scan); a cancel settles it without clearing the source selection a second time, since the one
  there now is the person's. A job sent to the background from the progress dialog gets the same watch, which closed the
  same gap there. While Show has the job in the progress dialog, that view stays quiet about the stop
  (`background.watches(id)` filters its error dialog), and the prompt follows once that view has closed.
- **Errors after the start**: the retained failure, the failure toast
  (`$lib/status-corner/operation-failure-watch.svelte.ts`, which speaks for any failure no foreground slot claims, so a
  background start is covered without a second toast), and the corner chip. The two `write-error`s the backend doesn't
  retain are a cancel (the person did it) and the password stop (above).
- **A trash's completion toast, Undo and "Go to trash" included**: raised when a background trash completes, through the
  same `announceCompletion` the progress dialog uses (`onCompleted` from the background module). Trash is offered here
  because it's the reversible delete, and that toast is how it's reversed.
- **Copy / move / compress success, pane refresh, cancel's selection restore, duplicate rename editor**: deliberately
  absent, matching a job sent to the background from the progress dialog (the file watcher updates the panes).
- **Cancel and rollback**: from the queue window's row, like any background job. **Quit gate**: backend-owned, it sees
  the registry, so nothing to do here. **Foreground claim**: none, on purpose (above).

**MCP.** `dialog confirm` with `background: true` presses the same button: both confirmers take `{ startInBackground }`
(`../../file-explorer/pane/dialog-props.ts::ConfirmOptions`), and the delete one answers `refusedPermanentDelete`
instead of pressing, which reaches the agent as `data.refusal`. Flow and contract:
`apps/desktop/src-tauri/src/mcp/DETAILS.md` (the dialogs entry).
