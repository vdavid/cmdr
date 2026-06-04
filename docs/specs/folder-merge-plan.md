# Folder merge for copy/move conflicts + instant same-volume moves

Plan for making **folder-vs-folder clashes always merge — no prompt, no policy, no exceptions** — across the volume
copy/move engines, with deep per-file conflict handling inside merges ("scan-as-you-merge"), and — riding on the same
machinery — dropping the mandatory deep pre-flight scan for same-volume moves, which today turns a 100 ms server-side
rename into a 30–40 s "Verifying before move…" wait.

This document captures the **intention** behind each decision so the implementing agent can adapt details when reality
pushes back, as long as the intentions stay intact.

## Why

Three problems, one mechanism. The full audit behind these findings is summarized below; the code references are the
authority.

1. **Folder merges on volume backends (smb2, MTP, cross-volume) are blunt and under-informed.** Conflict detection
   checks only top-level names (`scan_for_conflicts` in every backend lists the dest dir once — none recurse), and the
   merge engine (`volume_strategy.rs::copy_directory_streaming`) streams every deep child straight in with **no per-file
   conflict check**. So the user confirms "Overwrite" once at the folder level and every same-named file deep in the
   tree gets overwritten with no individual prompt. Dest-only files survive (the merge is real), but a
   clashing-but-newer NAS file can lose to an older local one with only a folder-level OK. The local-FS engine, by
   contrast, resolves conflicts per file — the volume path should match that granularity.
2. **The folder-level conflict prompt is the wrong question.** For dir-vs-dir, there is exactly one answer a user ever
   wants: merge. Skipping a folder wholesale means "my new files mysteriously didn't move"; renaming a folder to
   `photos (1)` splits content the user thinks of as one thing; "Overwrite" on a folder is a lie in today's UI (the
   volume engine's same-type-dir branch in `volume_conflict.rs::apply_volume_conflict_resolution` deliberately skips the
   delete and merges). The local engine already encodes this truth: `move_op.rs::merge_move_directory` auto-merges
   dir-dir **without asking**, and per-file conflict resolution inside handles the part that actually needs a human. So:
   **folders always merge, silently; files keep the full conflict machinery.** One behavior, all engines.
3. **Same-volume moves pay a full recursive pre-flight scan for nothing, and folder collisions error out.**
   `move_within_same_volume_with_progress` runs `scan_volume_sources` (a full subtree walk over the network, ~180
   files/s on a NAS → 30–40 s for a few thousand entries) only to feed a Size bar for an operation that transfers **zero
   bytes** (a server-side rename). And when a top-level folder collides, the resolver hands back the existing dest dir
   and the loop calls `volume.rename(source, dest, force=false)` — which returns `AlreadyExists`, so the move fails with
   an error instead of merging. A recursive rename-merge fixes the collision case; with merges handled, the deep
   pre-scan loses its last excuse to exist on this path.

Design-principles tie-ins: radical transparency (the upfront dialog says which folders will merge; deep clashes surface
as real per-file prompts), no progress bar lies (drop the Size bar for renames instead of showing a fake one), protect
the user's data (per-file policy inside merges; the merge-never-deletes invariant test), responsive UI (instant
same-volume moves).

## The audit baseline (what today's code does)

| Path                                                            | Engine                                                     | Dest-only files on folder clash | Granularity                                                           |
| --------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------- | --------------------------------------------------------------------- |
| Copy, local → local                                             | `copy.rs::copy_single_item` (flattens to per-file)         | preserved                       | per deep file                                                         |
| Copy, any volume pair                                           | `volume_copy.rs` + `copy_directory_streaming`              | preserved (merge)               | top-level only; deep files silently overwritten                       |
| Move, local → local (and any two local-FS-backed volumes)       | `move_op.rs::merge_move_directory`                         | preserved                       | per deep file; dir-dir auto-merges with no prompt                     |
| Move, cross-volume                                              | `volume_move.rs::move_volumes_with_progress` (copy+delete) | preserved (merge)               | top-level only; deep files silently overwritten                       |
| Move, same non-default volume (one smb2 share / one MTP device) | `move_within_same_volume_with_progress` (wholesale rename) | preserved                       | folder Overwrite **errors** (`rename(force=false)` onto existing dir) |

Other relevant facts the design builds on:

- Default conflict mode is `ConflictResolution::Stop` (= ask). Nothing auto-overwrites files.
- The local engine ALREADY auto-merges dir-dir without a prompt (both `move_with_rename` and the cross-FS staging path
  branch into `merge_move_directory` before consulting `resolve_conflict`; local copy implicitly merges parent dirs).
  "Folders always merge" is the local semantics promoted to every engine — not a new policy.
- `rename` is called with `force=false` at every transfer-engine call site (`volume_move.rs`, `volume_conflict.rs`
  safe-replace finalize). The MTP duplicate-name and SMB delete-first hazards only exist behind `force=true`, which only
  the F2 single-rename command uses. Don't introduce `force=true` calls in this work.
- `WriteConflictEvent` already carries `source_is_directory` / `destination_is_directory`, and the FE conflict dialog
  already branches on them (the file-over-folder red warning). Cross-type clashes keep using this; dir-dir conflict
  events simply stop existing.
- The async transfer driver (`drive_transfer_serial_async`) already detects **top-level** conflicts itself via its
  `dest_meta_fetcher` (`get_metadata` per top-level dest) — the deep pre-flight scan is NOT what powers top-level
  conflict detection. This is what makes dropping it for same-volume moves safe.

## Scope / non-goals

- **No folder-level conflict prompt, ever.** Dir-vs-dir is not a conflict; it's a merge. No "Merge folders" buttons, no
  folder policy radios, no new `ConflictResolution` variant, no second apply-to-all bucket. (This supersedes the earlier
  draft of this plan that proposed explicit Merge/Merge-all buttons — the simplification is deliberate; see Why #2.)
- **No "Replace folder" or "Rename folder" options.** Replace-folder is the one genuinely destructive folder action
  (recursive delete of dest) and nobody asked for it; rename-folder (`photos (1)`) splits content. A user who wants
  either can rename/delete explicitly first. Revisit on user demand.
- **Local-FS engines (`copy.rs`, `move_op.rs`) keep their semantics.** They already do exactly the target behavior
  (auto-merge dirs, per-file conflict resolution inside). v1 changes nothing there.
- **No upfront recursive conflict pre-scan.** Deep clashes are discovered and resolved as the merge walks — that's the
  scan-as-you-merge design. The upfront dialog stays top-level and fast.
- **No new progress sub-state.** No "Merging directories…" label; merges run under the existing Copying/Moving phase.
  The existing current-file line already shows where the walker is.
- **Delete/trash flows untouched.**
- **`force=true` semantics untouched** (F2 rename only).

## Confirmed behavior (the contract)

### Dir-vs-dir: always merge, never ask

- Every engine, every operation (copy and move), every backend: a source folder landing on an existing same-named dest
  folder **merges into it**. No prompt, no policy lookup for the folder itself. The configured/asked **file** policy
  governs every clash _inside_ the merge.
- This makes `conflict_resolution` purely a **file** policy. "Skip all" now means "merge folders, skip clashing files"
  on the volume engines — which fixes the documented gotcha ("Skip-All on volume copy/move with a top-level dir conflict
  still skips the entire dir subtree") and matches what the local engine already does. This is a deliberate behavior
  change for the volume paths; note it in the M5 doc updates.
- Cross-type clashes (file vs same-named folder, either direction) are NOT merges — they keep today's conflict machinery
  untouched, including the red file-over-folder warning and explicit Overwrite semantics.

### Transparency instead of a prompt

- The upfront `TransferDialog` shows an **informational line** when the (cheap, top-level) conflict check finds dir-dir
  collisions: "2 folders will merge with existing folders." — not a question, not a count toward the conflict radios.
  Intent: the user who _didn't_ expect a same-named folder at the dest gets a visible cue before confirming, without
  everyone else paying a prompt. (This is the accepted trade for removing the folder prompt: an accidental move onto a
  same-named folder interleaves contents silently if no files clash; the info line plus cancel-ability is the safety
  valve, same as the local engine today.)
- The file-policy radios (Skip all / Overwrite all / smaller / older / Ask for each) appear when there are file
  conflicts **or** folder merges — a folder merge can surface file clashes mid-operation that the upfront check can't
  see (no recursive pre-scan, by design), and the user's file policy is how they're handled.

### Merge semantics (the invariant)

> **A merge never deletes or overwrites a destination file that the source doesn't shadow** — under every file policy,
> on every backend, including cancel and rollback mid-merge.

"Shadow" = a source file at the same relative path whose file-level resolution came out as Overwrite. This is the
property test that anchors the whole feature (see Testing).

Concretely, inside a merged folder, per child:

- **dir + dir** → recurse (merge). Always, no lookup.
- **file + file** → the file policy: latched bulk choice, conditional reduce (smaller/older), or Stop → emit
  `write-conflict` and wait, exactly like top-level file clashes today.
- **file + dir / dir + file (type mismatch)** → existing cross-type handling (`apply_volume_conflict_resolution`), which
  under ask mode shows the existing red-warning dialog.
- **child exists only in source** → copied/moved in (this is the merge's whole point).
- **child exists only in dest** → never touched.

### Same-volume move

- **No deep pre-flight scan.** Top-level conflict detection only (the driver's existing per-item `get_metadata`). A
  non-conflicting same-volume move of any size folder completes in ~one rename round-trip.
- **Progress shape**: `files_total` = number of top-level selected items; each top-level item counts 1 when its rename
  (or merge) completes. `bytes_total = 0`, which the FE already interprets by hiding the Size bar — honest, because a
  rename transfers no bytes. No fake denominators. Inside a folder merge the per-item unit doesn't subdivide; the
  current-file line carries the detail.
- **Dest-inside-source guard (new on this path).** Today's flat per-item rename can't recurse, but the rename-merge walk
  can: moving `/A` into `/A/sub` would re-discover and re-rename content it just moved. Add the same prefix check
  `copy_volumes_with_progress` already has (reject `dest == source || dest.starts_with(source)` for a dir source on the
  same volume, path-prefix based — no canonicalize, these aren't local paths), returning
  `WriteOperationError::DestinationInsideSource`. Test: `move /A into /A/sub` rejected.
- **Dir-dir collision** → recursive **rename-merge**, entered directly (no resolver round-trip for the folder): walk the
  source folder level by level; ensure the dest subdir exists (create, or merge into existing);
  `rename(child_src, child_dest, force=false)` per child (still server-side, still fast); file clashes follow the file
  policy (file Overwrite keeps the same-volume delete-first shape that exists today for top-level file overwrites);
  after a level completes, delete the source dir **only if empty** (skipped children leave it in place, mirroring
  `move_op.rs::delete_dir_preserving_skipped`'s intent). This replaces today's `AlreadyExists`-error behavior by
  construction.
- **Cancel mid-merge** keeps already-renamed children at the destination (the existing "Cancel keeps completed work"
  contract). **Rollback**: match whatever the same-volume move path supports today — verify during M4; if the current
  path has no rollback support, the merge path doesn't add it (don't grow scope), but record the (src, dst) pairs in the
  natural place so adding reverse-rename rollback later is mechanical.

### Compatibility (programmatic callers, MCP)

- Any `ConflictResolution` value arriving alongside a dir-dir clash (old callers, MCP `move`/`copy` tools, scripts) no
  longer influences the folder itself: the folder merges, and the value applies to file clashes inside. For `Overwrite`
  this is today's behavior with a name that finally matches; for `Skip`/`Rename` it's the documented behavior change
  above (folder merges instead of being skipped/renamed wholesale). This is a real semantic change for automation — a
  script that relied on `rename_all` landing a separate `photos (1)` folder now gets a merge. We accept it (no known
  flow depends on folder-level skip/rename; the MCP tools are pre-1.0), but the **MCP `onConflict` descriptions in
  `mcp/tools.rs` must be updated** to say the value governs files only and folders always merge — the documented meaning
  must match the behavior the moment it changes, not in a follow-up.
- No enum changes; `resolve_write_conflict` accepts exactly today's values. Dir-dir `write-conflict` events stop being
  emitted (the FE dialog never sees one).

## Architecture decisions

**Decision: dir-vs-dir is not a conflict — it short-circuits to merge before any policy/prompt dispatch.** Why: see Why
#2. Mechanically, `resolve_volume_conflict`'s dir-dir path stops consulting `conflict_resolution` and stops emitting
`write-conflict`; it returns the merge outcome unconditionally. The same-volume move loop branches into rename-merge on
dir-dir before the resolver. This removes (relative to the earlier draft): a new enum variant and its match-arm fan-out,
the third `ApplyToAll` bucket, the folder radio group, and the dialog button swap. What remains of conflict resolution
is purely file-scoped — one policy, one latch model, today's IPC surface.

**Decision: scan-as-you-merge — deep conflicts are detected by listing the dest side of each merged level once, inline,
during the walk.** Why: the merge recursion must list the _source_ level anyway; one extra `list_directory` of the
_dest_ level (only for levels that pre-existed — a freshly created subdir can't clash) gives complete clash detection
for that level via a name map. Cost scales with merge depth actually traversed, is zero for non-conflicting operations,
and avoids both the upfront recursive pre-scan (slow, pays before the user even confirms) and a pause-the-op scan phase
(complexity, a new state). Per-child `get_metadata` probes are banned here — one listing per level, then in-memory
lookups (the same batching philosophy as `scan_for_copy_batch`).

**Decision: same-volume move drops `scan_volume_sources` entirely; totals come from the top-level item count.** Why: the
scan's three outputs (Size-bar total, per-source `is_directory` hints, conflict size hints) are all replaceable on this
path: the Size bar should be absent for renames (zero bytes — showing it is the lie), the driver already stats each
top-level dest, and a batched stat of the top-level _source_ items supplies `is_directory` + size hints at one pipelined
round-trip instead of a subtree walk. The FE must stop _waiting_ for the deep scan preview too (the 30–40 s in the field
was the FE blocking on `scan-preview-complete` before dispatch): for same-volume moves, dispatch immediately and cancel
the preview. Watch the Copy/Move segmented toggle — flipping to Copy must (re)start the preview because copy genuinely
needs byte totals.

**Decision: `WriteConflictEvent.source_size` becomes `Option<u64>`.** Why: cross-type clashes (the only conflict shape
left that can carry a folder source) can now surface on the same-volume fast path, where no pre-flight scan ran, so a
folder source's size is genuinely unknown. The FE already renders `(unknown)` for `destination_size: None`; extend the
same treatment to the source side. Populate it opportunistically when a cached preview or the drive index knows the
size. Note the fan-out: every emit site wraps in `Some(...)` (`conflict.rs::build_conflict_event` — note
`write_operations/helpers.rs` was split into `conflict.rs` / `validation.rs` / `overwrite.rs` / `durability.rs` /
`cancellable.rs`; the conflict machinery incl. `ApplyToAll`, `resolve_conflict`, and `apply_resolution` lives in
`conflict.rs` now — plus `resolve_volume_conflict`'s Stop branch in `transfer/volume_conflict.rs`, which was not split),
and `size_difference` must treat `source_size: None` the same way it already treats `destination_size: None` (collapse
to `None`).

## Milestones

Sequential by default. M2 (FE) and M3 (BE engine) can proceed in parallel after M1 if two agents are available
(near-zero file overlap — but sequential is fine and preferred when in doubt). M4 depends on M1 + M3. M5 last.

### M1 — Types and plumbing (no behavior change)

Intent: small now — the simplification removed the enum/latch work. What's left is the data the FE and the fast path
need.

- `ScanConflict` gains `source_is_directory` + `dest_is_directory` (all four backends populate from data they already
  have in hand — the dest listing entry and the caller-supplied source info; extend `SourceItemInfo` with
  `is_directory`). This is what lets the FE classify dir-dir collisions as "will merge" info instead of conflicts.
- `WriteConflictEvent.source_size: Option<u64>` (see the architecture decision for the emit-site fan-out and the
  `size_difference` None-collapse).
- `cd apps/desktop && pnpm bindings:regen`; FE type fallout (render `(unknown)` for null source size).
- Tests: serde round-trips, backend `scan_for_conflicts` flag population. Run `./scripts/check.sh` before commit.

### M2 — Upfront dialog UX (FE + the thin command layer)

Intent: transparency without a prompt, and the decoupling M4 depends on.

- **Decouple the top-level conflict check from scan-preview completion.** Today `checkConflicts()` (the cheap
  `scanVolumeForConflicts` call — one dest listing) only fires from `onScanPreviewComplete` / the status-check branch in
  `TransferDialog.svelte`. That coupling is accidental: the conflict check doesn't need the recursive byte scan. Run it
  on mount, in parallel with the preview. Intent: (a) conflict info appears as soon as the one cheap listing returns,
  instead of after a potentially-minutes-long deep scan; (b) M4 can cancel the deep preview for same-volume moves
  **without** losing `preKnownConflicts` — without this decoupling, the conflict UX would silently degrade on exactly
  the path this feature targets. **Ordering matters for auto-confirm (MCP)**: assign
  `conflictCheckPromise = checkConflicts()` synchronously in `onMount`, BEFORE the auto-confirm branch — mirroring how
  `scanStarted = startScan()` is already assigned before `if (autoConfirm)`. Otherwise the `if (conflictCheckPromise)`
  await guard in `handleConfirm` sees `undefined` on the fast path and dispatches with empty `conflictNames`. Tests: (a)
  conflict info renders while the preview is still running; (b) auto-confirm dispatches with `conflictNames` populated.
- **Classify dir-dir collisions as merge info, not conflicts**: the "N folders will merge with existing folders" line
  (copy per style-guide: sentence case, active voice). The filter is explicit: merge info =
  `source_is_directory && dest_is_directory`; a type mismatch (`source_is_directory != dest_is_directory`) stays in
  `totalConflictCount` and keeps the red-warning path. Dir-dir entries don't count toward `totalConflictCount` for radio
  labeling, but their _presence_ (like file conflicts) shows the file-policy radios — a merge can surface file clashes
  mid-op, and the radios are how the user pre-answers them.
- **`pre_known_conflicts` / bulk-skip**: dir-dir names must NOT enter the file bulk-skip set ("Skip all" must not skip
  folders wholesale anymore). `build_pre_skip_set` is already file-only via `known_directory_paths` — keep it that way
  and pin it with a test now that the semantics matter more.
- Cross-type guardrail (upfront): if pre-known conflicts include a type mismatch and the user selects "Overwrite all",
  show the existing red-warning copy adapted: overwriting will replace items of a different type, including folder
  contents. Intent: the ask-mode dialog already warns; the bulk path must not be quieter than the per-file path.
- The per-file conflict dialog in `TransferProgressDialog.svelte` needs **no button changes** (dir-dir events stop
  arriving; file and cross-type shapes are unchanged). Remove nothing; just verify with a test that dir-dir never
  renders.
- Tests: Vitest component tests for the info line, radio visibility rule, payload wiring, and the a11y pass
  (`*.a11y.test.ts` siblings).

### M3 — Volume merge engine: per-file conflict resolution inside merges

Intent: the volume path reaches the local engine's granularity. This is the heart of the safety fix; take the test bar
seriously (this module's history: see "Testing bar" in `write_operations/CLAUDE.md`).

- Thread context into `copy_directory_streaming` via a **`MergeCtx` struct** (event sink, op id, config,
  `WriteOperationState`, latch cell, source hints) — the function already carries eight parameters; don't widen the
  signature further. Note the call-site fan-out: one function, ~four call paths (`copy_single_path` top-level dirs, its
  own recursion, and through both the serial and concurrent `volume_copy` paths plus cross-volume `volume_move`) — all
  compile-break together, which is the point.
- Per merged level: the existing `create_directory` → `AlreadyExists` branch (currently a silent no-op) becomes the
  trigger — **a pre-existing level lists its dest side once**, builds a `name → FileEntry` map, and dispatches each
  source child with a hit through the conflict resolver (which already handles Stop-wait, latches, conditional
  reduction, type mismatches); **dir-dir children recurse unconditionally** (no resolver call for the folder itself);
  honor Skip without touching dest. Freshly created levels (`Ok(())`) skip the dest listing — nothing to clash with.
  **Verify, don't assume, that every backend's `create_directory` returns `AlreadyExists` for an existing same-name
  dir** — MTP especially: the protocol allows same-name sibling objects, and if MTP silently created a duplicate
  `photos` instead of erroring, the merge would target the wrong dir. If it doesn't hold, pre-check existence on MTP
  before `create_directory` (one listing the merge level pays anyway).
- `resolve_volume_conflict`'s dir-dir branch becomes the unconditional-merge short-circuit (no policy lookup, no
  `write-conflict` emit). The driver stays type-agnostic as today (its `ConflictDecisionInput` ships
  `source_is_directory_hint: None`; the per-op resolver closures recover hints from `source_hints`).
- **Concurrency: deep-conflict resolution must be serialized explicitly.** The recursive merge runs inside
  `copy_single_path`, which the concurrent `FuturesUnordered` path in `volume_copy.rs` executes in parallel (3+ sources)
  — but `WriteOperationState` has exactly **one** `conflict_resolution_tx` slot, and the latch is one cell. Two parallel
  merges both hitting a Stop-mode deep file clash would clobber each other's oneshot sender. Decision: a per-operation
  `tokio::sync::Mutex` — **living on `WriteOperationState` next to `conflict_resolution_tx`** (they're the same concern:
  one human, one oneshot slot; this also means `MergeCtx` doesn't need a new field, `state` already rides along) —
  guards the whole Stop-mode dispatch. Sequence: acquire → **check `is_cancelled(&state.intent)`; if cancelled, release
  and return `Cancelled`** → re-check the latch → if still unresolved, emit `write-conflict` and wait → store latch →
  release. The cancellation check is load-bearing: on cancel, the sender-drop unblocks only the ONE task awaiting the
  oneshot; a task parked on the mutex would otherwise acquire it next and emit a fresh `write-conflict` that no one will
  ever answer (the dialog is tearing down) — a hang. The latch double-check means a "…all" answer from the first prompt
  silently resolves the queued ones. The guard is released on every exit path (scope-drop at the end of the resolve
  step) and is NEVER held across the subsequent file write — serialize the human, not the I/O. **The concurrent spawn
  loop's top-level `resolve_volume_conflict` dispatch acquires the SAME mutex** — the loop is serial with itself but
  runs concurrently with already-spawned tasks' deep merges, so without this a top-level prompt and an in-flight deep
  prompt still race the one oneshot slot. Known acceptable residual: the mutex doesn't retroactively resolve a prompt
  that was already emitted before another task latched an "…all" — a rare extra prompt, same as the serial path
  tolerates, never a data risk. Pin with: a concurrent-merge-with-two-deep-clashes test, a top-level-vs-deep race test,
  and a cancel-while-queued test (task A awaiting the prompt, task B parked on the mutex, cancel → both return
  `Cancelled`, no hang; the existing `cancel_mid_merge_stream_concurrent_*` tests prove this path is live).
- Cancel/rollback: the `CreatedPaths` ledger semantics are already merge-safe (records only newly created files/dirs;
  empty-only dir pruning) — deep merge children flow through the same ledger. Do not record pre-existing dest dirs.
- Both volume copy (`volume_copy.rs`, serial AND concurrent paths) and cross-volume move
  (`volume_move.rs::move_volumes_with_progress`) pick this up via the shared strategy function — confirm all three,
  don't assume.
- Tests (InMemoryVolume + CollectorEventSink unless noted):
  - **The invariant property test**: enumerate file-policy combos (including ask-mode responses) over a fixture tree
    with dest-only files, source-only files, clashing files, nested clashes, and a type mismatch; assert dest-only files
    byte-identical afterward, every time. Include cancel-mid-merge (flip intent partway via the public path) and
    rollback-mid-merge variants.
  - Dir-dir top-level AND deep both merge with zero `write-conflict` emits for the folders themselves, under every file
    policy including Stop.
  - "Skip all" merges folders and skips only clashing files (the old skip-whole-subtree behavior is pinned as _gone_).
    This resolves the documented gotcha in `transfer/CLAUDE.md` ("Skip-All on volume copy/move with a top-level dir
    conflict still skips the entire dir subtree") — the SMB Docker integration test below must assert the Skip-all shape
    specifically, since the gotcha explicitly flags the missing volume-side pin (the existing Playwright pin covers only
    the local-FS path).
  - Stop-mode deep file clash emits `write-conflict` with correct paths/flags and resumes on response.
  - One SMB Docker integration test (real Samba, `smb2::testing` helpers): merge with deep clash + skip, assert
    dest-only survival. Per `docs/testing.md` decision table this is the integration tier; keep it to one or two
    scenarios, the matrix lives in unit tests.
  - `cargo mutants --file src/file_system/write_operations/transfer/volume_strategy.rs` (and `volume_conflict.rs` if
    touched substantially) — this module's bar is ~85-90%, don't regress.

### M4 — Same-volume move: rename-merge + drop the deep pre-flight (the perf fix)

Intent: a same-volume move is a rename and should feel like one. Conflict handling rides M3's resolver for files; the
walk uses renames instead of byte streams; folders merge by construction.

- BE (`volume_move.rs::move_within_same_volume_with_progress`):
  - Replace the `scan_volume_sources` call with top-level hints: consume a cached preview if one happens to exist
    (free), else stat the source items — **via `scan_for_copy_batch` (or the SMB pipelined-stat path), not a serial
    per-item `get_metadata` loop**, so the pre-dispatch cost is one pipelined batch, O(top-level items), never
    O(subtree). (200 selected items at 60 ms RTT serial would be 12 s — the batch path is the difference between "fast"
    and "instant".) `files_total = N top-level items`, `bytes_total = 0`. The per-source metadata pass must also
    populate **`known_directory_paths`** — `build_pre_skip_set` depends on it to keep bulk-skip file-only, and the
    per-source `is_directory`/size hints feed the conflict resolver as before.
  - **Dir-dir collision branches directly into the new recursive rename-merge helper** (contract above) — before any
    resolver dispatch; the resolver is for files and cross-type only. The transfer closure's existing two-way branch on
    `rc.replace_after_write` (file safe-replace collapse vs plain rename) stays for those shapes.
  - **Downloads-watcher hook per child rename**: call `note_pending_for_local_volume` on BOTH halves of every deep child
    rename, mirroring the existing top-level shape ("Renames register both halves" — `write_operations/ CLAUDE.md`).
    Without it, a same-volume move into `~/Downloads` toasts "Downloaded …" once per deep child.
  - **Name collisions the exact-match map misses (case-insensitive backends)**: the dest-level `name → entry` map is
    exact-match, but SMB servers and APFS are typically case-insensitive — `Foo.txt` vs `foo.txt` collides at the
    backend without a map hit. Safety net: treat an unexpected `AlreadyExists` from a child `rename` as a
    **late-detected conflict** and route it through the resolver (the backend is the authority on collisions), never as
    a hard error. **Scope the net to children that had NO map hit**: track decisions already made via the map in a
    per-level `name → resolution` map (NOT a bare name set — the late path must branch on the stored decision: Overwrite
    → finalize the case-folded replace, delete-then-rename, same shape as the exact-name path; any other stored decision
    reaching a colliding rename is unexpected → route through the resolver as a fresh conflict). NEVER re-prompt a child
    whose stored decision explains the collision. A case-folded **dir**-dir collision (map missed it, rename returns
    `AlreadyExists`, dest entry is a directory) enters the rename-merge recursion like any other dir-dir. Note this
    per-level map is orthogonal to the op-wide `ApplyToAll` latch: the latch is "how to answer future clashes," the map
    is "which children of THIS level were already handled." Tests: a case-folded collision prompts exactly once; a child
    resolved Overwrite that then collides on case-fold does not prompt twice. Same shape protects against TOCTOU (a file
    appearing between the listing and the rename).
  - **Symlinks**: renamed as opaque entries (one `rename` per symlink), never descended. Matches the module-wide "never
    dereference" rule.
  - Source-dir cleanup: delete when emptied; leave when children were skipped. Assert both in tests.
  - Keep the settle guard / event ordering exactly as is (see "Settle contract").
- FE:
  - For same-volume moves, don't gate dispatch on the deep scan preview: dispatch immediately, cancel the preview. The
    top-level conflict check keeps running independently (decoupled in M2 — that decoupling is a prerequisite here).
    Audit `waitForScanThenStart` and the `scanInProgress` handoff so no listener leaks or double-dispatch (the `started`
    flag pattern is already there — extend, don't fork).
  - **The Copy/Move toggle gating is NEW reactive logic, not an adjustment.** Today `startScan()` runs once in `onMount`
    and the toggle doesn't touch the scan at all. Introduce an `$effect` keyed on `activeOperationType` + same-volume
    status: flip to Move on same volume → cancel the recursive preview; flip to Copy → start (or resume) it, because
    copy genuinely needs byte totals. Pin with a component test toggling both directions.
  - Size bar: nothing to do — `bytesTotal = 0` already hides it. Verify the dialog reads sensibly with Files-only
    progress, that the instant-completion edge path (complete-before-mount toast) still works, and that the
    complete-toast wording reads sensibly when one "item" was a folder containing thousands of files (item-level counts
    are honest; just check the copy).
- Tests:
  - Unit: rename-merge matrix on InMemoryVolume (merge with zero folder prompts, skip-child leaves source dir, file
    policy inside merge, cancel-mid-merge keeps moved children, invariant test reused with a rename-merge flavor).
  - Regression (the perf contract): same-volume move dispatch must not run a recursive scan — assert no subtree walk AND
    stat-call count O(top-level items) (for example, a sentinel volume whose `list_directory` / `get_metadata` count
    calls).
  - SMB Docker integration: same-share folder move with collision → merges with no prompt for the folder;
    non-conflicting big-folder move completes without listing the subtree.
  - Playwright E2E: one spec — same-volume move with a folder collision auto-merges, file clash inside prompts,
    dest-only files survive. (See `e2e-playwright/CLAUDE.md` for single-spec iteration.)

### M5 — Docs, polish, full verification

Intent: leave the codebase explaining itself; this feature changes documented contracts in at least four CLAUDE.mds.

- Update colocated docs: `write_operations/CLAUDE.md` (conflict model: folders always merge, file-only policy, the
  invariant, the new conflict-dispatch mutex), `transfer/CLAUDE.md` BE (merge engine; same-volume fast path replaces the
  "Volume move runs the same preflight scan as volume copy" decision — rewrite that decision's Why, don't leave a stale
  one; delete the now-fixed "Skip-All skips the entire dir subtree" gotcha) and FE (dialog contract: merge info line,
  scan-skip flow), `volume/CLAUDE.md` (`scan_for_conflicts` field additions, `SourceItemInfo` change). Follow
  `.claude/rules/docs-maintenance.md`: current behavior only, drop superseded narration.
- Call out the behavior change ("Skip"/"Rename"/"Overwrite" no longer apply to folders; folders always merge) in the
  user-facing changelog, and update the **MCP `onConflict` descriptions in `mcp/tools.rs`** to match (the value governs
  files only).
- Sweep UI copy against `docs/style-guide.md` (sentence case, active voice).
- Final checks: `./scripts/check.sh` then `./scripts/check.sh --include-slow` (E2E suites), plus `pnpm bindings:regen`
  freshness and `--check oxfmt` (always).

## Testing strategy summary

- **Unit (Rust)** — the bulk: file-policy matrices, the merge invariant property test, zero-folder-prompt pins,
  rename-merge semantics. InMemoryVolume + CollectorEventSink; drive cancellation through the public path (`state.rs`
  testing bar).
- **Integration** — two or three SMB Docker scenarios (real server semantics for rename-onto-existing and merge), per
  `docs/testing.md`'s "one integration pin per behavior class, matrix in units".
- **FE unit (Vitest)** — merge info line, radio visibility, payload wiring, a11y.
- **E2E (Playwright)** — one merge-flow spec (same-volume move) and, if cheap, one cross-volume merge spec. Don't matrix
  in E2E.
- **Mutation testing** — `cargo mutants` on the touched conflict/strategy/move files.
- Read `docs/testing.md` before writing any of these (decision table + anti-patterns are binding).

## Open decisions (flagged, with proposed defaults)

1. **Rollback for same-volume rename-merge**: match existing same-volume move behavior (verify in M4); don't add new
   rollback machinery in this feature.
2. **Merge info line placement/wording** in `TransferDialog`: proposed as a neutral info row near the conflict summary;
   exact copy and styling at implementation time per style-guide.

## Execution notes

- Milestone-per-commit (or finer); run `./scripts/check.sh --fast` on the natural rhythm, full suite before each commit,
  `--include-slow` before declaring M4/M5 done.
- M2 (FE) and M3 (BE engine) are parallelizable after M1 with near-zero file overlap, but sequential is fine.
- No pushes, no PRs — direct commits on `main` per repo convention, and never push without explicit approval.
