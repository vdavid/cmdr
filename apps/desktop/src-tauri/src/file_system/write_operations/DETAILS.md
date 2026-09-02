# Write operations details

Pull-tier docs for `src-tauri/src/file_system/write_operations/`: architecture, flows, and decision rationale.
Must-know invariants and gotchas live in `CLAUDE.md`.

Frontend counterpart: `apps/desktop/src/lib/file-operations/CLAUDE.md`
(umbrella) plus colocated child docs for `apps/desktop/src/lib/file-operations/transfer/CLAUDE.md`,
`apps/desktop/src/lib/file-operations/delete/CLAUDE.md`,
`apps/desktop/src/lib/file-operations/mkdir/CLAUDE.md`, and
`apps/desktop/src/lib/file-operations/mkfile/CLAUDE.md`.

Subdirs:
- `transfer/CLAUDE.md` — copy + move (local FS, cross-volume, MTP, SMB), conflict resolution, the shared transfer driver, platform-specific copy backends.
- `delete/CLAUDE.md` — delete walker (local + volume-aware), trash, the oracle-aware delete fast path.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites, network mounts, cross-filesystem moves, and name/path length limits.

Ask Cmdr bulk renames use the same managed-operation lifecycle. `LocalPosixVolume` makes every non-forced local rename
atomic-no-overwrite, including attached disks and cloud folders registered under non-root volume IDs: macOS uses
`renamex_np(RENAME_EXCL)` and Linux uses `renameat2(RENAME_NOREPLACE)`. Root bulk-renames share that primitive directly.
The dialog's target-exists preflight improves review UX, but only the kernel operation closes the check-to-rename race.

Pre-flight scans reuse cached listings when the source volume reports `WatchCoverage::EveryWriter`, avoiding redundant `list_directory` calls. The freshness contract and per-backend debounce windows are documented in `../volume/CLAUDE.md` and `../listing/caching.rs::try_get_authoritative_listing`.

## Files (top level)

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The spine: `CLAUDE.md` § Module map.
The full top-level inventory is here:

- `mod.rs` (public API + the `start_write_operation` lifecycle), `manager.rs` (registry + lane admission), `state.rs`
  (`WriteOperationRegistry`, `WriteOperationState`, the settle guard, plus `state/controls.rs`:
  the by-id cancel / abort / pause / resume / conflict-answer entry points, re-exported through `state`),
  `status_cache.rs` (the status cache, the busy-volumes set it derives, the external drag-out seam, and
  `list_active_operations` / `get_operation_status`), `operation_intent.rs` (`OperationIntent`, `PauseGate`), `human_wait.rs` (how long a person has kept the operation waiting),
  `archive_edit/` (the zip-edit driver).
- The in-flight ledgers and what reverses them: `ledger.rs` (`CopyTransaction` and `WrittenFile`, the vocabulary of what
  an operation currently has at the destination, plus the `Drop` panic net) and `reversal.rs` (the policy over it: the
  recheck before each destructive act, and `ReversalTally`). `in_flight_temps.rs` registers every `.cmdr-` temp so a
  startup sweep can find one an abandoned run left behind; see § "Testing the in-flight temp ledger" for the two rules
  its process-wide singleton imposes on tests.
- Scan and preview: `scan.rs`, `scan_preview.rs`, `scan_cache.rs`, `scan_bridge.rs` (the scan-progress seam the drivers
  feed), `scan_watchdog.rs` (the inactivity bound on a preview), `compress_estimate.rs`. Conflicts and overwrite:
  `conflict.rs` (policy), `unique_name.rs` (the ` (N)` namer), `conflict_slot.rs` (the one-answer-wins slot behind
  `resolve_write_conflict`), `overwrite.rs`. Cancellation and durability: `cancellable.rs`, `rollback.rs`, `durability.rs`.
  `rollback.rs` wears two hats: the history dialog's reversal, and the executor the operation-log engine injects.
- Vocabulary and edges: `types.rs` (+ `types/events.rs`, every `#[tauri_specta(event_name)]` payload, re-exported
  through `types`), `event_sinks.rs`, `error_classification.rs`, `mutation_error.rs` (the typed refusal an instant
  mutation returns), `validation.rs`, `analytics.rs`, `eta.rs`. Journaling: `journal.rs`, `journal_search.rs`. Remote
  archive I/O: `archive_remote_edit.rs`, `scratch_dir.rs`. Entry points: `create/` + `create.rs`, `rename/` +
  `rename.rs`, `paste_clipboard.rs`, `routing.rs` (the one routing every cross-volume transfer takes:
  `start_volume_{copy,move,compress}`). `source_binding.rs` is the optional set of sources an op may touch. Fixtures:
  `test_support.rs`, plus `network_transfer_test_support.rs` and `network_gated_source_test_support.rs` (the
  backend-blind transfer scenarios the WebDAV and SFTP Docker suites both drive, and the chunk-gated source one of them
  needs; see § "The network transfer suites").

What the mechanisms DO is in the sections below: the registry, lanes, and `run_instant` in § "Operation manager";
the zip-edit driver in § "Archive edits"; cancellation, pause, Stop-mode conflicts, safe overwrite, scan-preview caching,
and the compressed-size estimate in § "Key patterns and gotchas"; durability and the two byte totals in § "Key
decisions"; the estimator in § "ETA + throughput"; `WriteSettledGuard` in § "Settle contract"; journaling in
`../../operation_log/DETAILS.md` § Capture. Only the layout facts that none of those carry live here:

- **Two re-export facades are deliberate, not collapsible** (§ "Key decisions" has the why): `mod.rs` re-exports
  `transfer::*` + `delete::*` so callers keep their `crate::file_system::write_operations::<symbol>` paths, and
  `state.rs` re-exports the `operation_intent` + `scan_cache` + `status_cache` names. `types.rs` has none: it is the
  vocabulary floor (§ "Why `types` imports nothing"), so a sink, a classifier, or a lifecycle name is imported from the
  module that DEFINES it. `OperationEventSink` and `TauriEventSink` are re-exported at the `write_operations` module
  root (and up through `file_system`) for the IPC edge.
- **Event structs and their builders live apart on purpose**: the struct definitions in `types/events.rs`, the
  `WriteProgressEvent` (`new` / `with_scan_meta`) and `WriteErrorEvent` (`new`) impls in `event_sinks.rs` beside the
  sinks that emit them.
- **What splits `types.rs` from `types/events.rs` is one rule**: a struct carrying `#[tauri_specta(event_name = ...)]`
  goes in `events.rs`, and so does an enum whose only carrier is one of them (`SourceItemOutcome`,
  `CancelRollbackOutcome`). A name two homes speak stays in `types.rs`, which is why `TransferActivity` sits there:
  `WriteProgressEvent` carries it AND so does `OperationStatus`, a snapshot nobody emits.
- **`analytics.rs` is `pub(super)` and reached ONLY from `TauriEventSink::emit_complete`.** Every property is
  categorical (op kind, a count bucket, a bool): no names, no paths ever. Copy/Move → `file_transfer_completed`,
  Delete/Trash → `delete_used`.
- **`error_classification.rs` classifies from `errno` / `ErrorKind` only, never the message**.
- **`validation.rs`'s `ensure_destination_dir` runs AFTER `validate_destination_not_inside_source`**, so creating a
  missing destination (and its ancestors) can never materialize a folder inside a source. The volume-aware pipelines
  mirror both the behavior and the order with `Volume::create_directory_all(dest)`; see `../volume/DETAILS.md`
  § "Recursive destination create".
- **`unique_name.rs::numbered_name(stem, ext, counter)` is the ONE ` (N)` formatter** (`counter 0` = bare, `1..` = ` (N)`).
  `find_unique_name`, `next_available_name`, `paste_clipboard.rs`, and the volume namer
  (`transfer/volume/naming.rs::find_unique_volume_name`) all go through it, so the numbering paths can't drift.
  `archive_edit/conflicts.rs::find_unique_inner` is deliberately outside this: it numbers slash-joined inner-path
  strings against an `ArchiveIndex`, and its doc comment says so.
- **Decision: the suffix is ` (N)`, everywhere, including a duplicate.** macOS Finder would name a duplicate
  `photo copy.jpg` with a per-language word for "copy", and `docs/design-principles.md` prefers platform-native over
  generic, so the pull toward it is real. It loses on two counts. One scheme for what is structurally one operation:
  numbering the duplicate path differently from the conflict path would have Cmdr generating `photo copy.jpg` on a
  duplicate and `photo (1).jpg` on a clash, which is worse than either scheme alone, and switching BOTH means
  re-reviewing the suffix in every shipped locale. And ` (N)` is language-neutral, where a translated word would owe
  `split_sequence` every one of those words before it could continue a series. Changing the scheme later is a one-place
  edit here plus its tests, which is the hedge that makes the decision cheap to revisit rather than a reason to revisit
  it.
- **`unique_name.rs::split_sequence(stem) -> (base, next_counter)` is the ONE sequence rule.** It reads a trailing ` (N)`
  off a stem so a search continues the series instead of nesting: duplicating `photo (1).jpg` gives `photo (2).jpg`,
  never `photo (1) (1).jpg`. What counts as a sequence is narrow on purpose, because everything else is somebody's
  filename: the separating space is required and the digits must be ASCII (`Report (final).pdf`, `photo(1).jpg`, and
  `photo (+1).jpg` are plain text); zero padding isn't preserved (`photo (007)` continues at `(8)`); and a number with
  no `u32` successor is plain text too, which keeps the returned counter always advanceable.
- **`unique_name.rs::NameCandidates` is the whole of what the ` (N)` searches share**: the parent, the base to number from,
  and the counter to try next, walked with `current()` / `advance()`. Built per item KIND, and the kind is not cosmetic:
  `for_file` keeps the extension at the end (`photo.jpg` → `photo (1).jpg`), while `for_directory` numbers the whole name,
  because a directory has none (`my.dir` → `my.dir (1)` rather than `my (1).dir`; likewise `backup.2024` and `v1.2.3`).
  `create_unique_dir` and the volume namer's `is_directory` branch pick the second, everything else the first.
  Every search walks the same candidates and differs only in how it TESTS one. `find_unique_name` RESERVES its pick with
  an `O_CREAT|O_EXCL` placeholder and must keep advancing when it loses that race; `next_available_name` only probes
  (`path_exists_or_is_symlink`, so a dangling symlink counts as taken) and creates nothing;
  `transfer/volume/naming.rs::find_unique_volume_name` reserves only when the destination volume is local-FS-backed
  AND the item is a FILE (`local_path().filter(|_| !is_directory)`), and probes otherwise — a directory takes the probe
  on every backend, for a reason `transfer/volume/DETAILS.md` states as load-bearing (a placeholder FILE where the copy
  is about to create a directory makes the merge walker merge into it, and leaves rollback nothing to remove).
  That difference is exactly why there's no shared search loop to layer them on as "search, then reserve". `attempts()`
  (not the counter's absolute value) is what a search bounds its own effort by, since a name ending in a high ` (N)`
  starts the sequence there.
- **Reach for the non-reserving `next_available_name` when a file placeholder would be in the way**: a directory claims
  its name with `create_dir` instead, and an ordinary non-overwrite write would otherwise find `find_unique_name`'s own
  placeholder sitting at the destination and raise a conflict against it.
- **`unique_name.rs::ClaimedNames` is what a probe can't have: a record of the names this operation already handed out**,
  before their bytes land. Every non-reserving picker (`next_available_name`, `create_unique_dir`, and both branches of
  the volume namer) consults it and records its own pick in one step, so two sources of one ` (N)` family duplicated
  together (`photo.jpg` and `photo (1).jpg`, which continues its series at exactly the name the first one took) can't
  both arrive at `photo (2).jpg` and turn two requested copies into one. It lives on `state::WriteOperationState`, so
  the ledger's lifetime is the operation's and both engines read the same one; interior-mutable because the volume
  engine's concurrent driver resolves several top-level sources at once.
- **`create.rs` co-locates the synthetic listing-cache diff** (`should_emit_synthetic_diff` /
  `emit_synthetic_entry_diff`, both `pub(super)`) that lands a brand-new entry in the pane on local-FS-backed volumes.
  `paste_clipboard.rs` reuses both so a pasted file cursor-lands exactly like mkfile.
- **Scan-phase `expected_files_total` / `expected_bytes_total` come from
  `crate::indexing::read::expected_totals::expected_totals_for_sources`** and are `None` when the index doesn't cover
  every source; the FE then falls back to a tally-only display instead of a progress bar. `scan.rs`'s walker derives
  `current_dir` from `path.parent()`, which is what lets the UI show "in directory: …" beside the filename.
- **`rename/bulk.rs` is Ask Cmdr's reviewed batch-rename driver**, and it does NOT use `run_instant`: `start_bulk_rename`
  takes only backend-owned rows accepted by preflight and runs through `spawn_managed` as one lane-queued operation. Its
  dependency planner renames independent rows directly, peels acyclic chains from their free destination, uses one
  same-directory temporary per cycle, and retains one temporary for a case-only rename on a case-insensitive filesystem.
  Local and remote drivers share the plan, so remote rename-as-copy backends don't duplicate every transfer.
  Cancellation happens between components: a started cycle finishes or reverses before the driver observes cancellation
  again. It journals one header and one final outcome per row. The Ask Cmdr command is the only caller, and it never
  receives paths or names from the frontend.
- **`paste_clipboard.rs::write_payload_to_dir` runs under a 30 s write timeout** (`commands/clipboard.rs`), a longer
  tier than the 5 s empty-mkfile write, because the payload can be a large image landing on a slow network volume. It
  takes an already-read `ClipboardPayload` + a `&Path`, decoupled from NSPasteboard and the IPC edge, so it's
  `TempDir`-testable; the retry loop writes via `Volume::create_file` (O_EXCL) and bumps the counter on the TYPED
  `VolumeError::AlreadyExists`, so there's no pre-scan-then-write TOCTOU and it works on any writable volume.
  **Partial-file-on-timeout edge (accepted):** past 30 s the write future is dropped and a partial `pasted.<ext>` may
  remain (the user sees a timeout and can retry or delete). It's bounded and rare (local writes never approach 30 s, and
  on a local FS `create_file`'s `spawn_blocking` isn't even cancellable, so the file actually completes), and only
  affects slow network volumes. If it ever matters, route paste-as-file through the managed transfer engine for
  cancellation + no-partial guarantees. Pasteboard read + flavor precedence:
  `apps/desktop/src-tauri/src/clipboard/DETAILS.md` § Paste clipboard content as a file.

## Why `types` imports nothing

`types.rs` is the vocabulary floor: every other module here speaks its enums, event structs, error types, and
configuration, and it speaks nobody else's. Nothing in it may `use` a sibling (the `CLAUDE.md` rule), and that covers
`types/events.rs` too: a child of the floor is still the floor, so it imports from `super` and from outside
`write_operations`, never sideways.

**Why the rule is absolute.** The dependency graph under `write_operations` is a fan-in: 30-odd modules point at
`types`, and `types` is the sink. A single upward import from the sink closes a circle through everything that fans in
behind it — not a local mistake, a subsystem-wide one. Three such lines (`event_sinks::OperationEventSink`,
`error_classification::IoResultExt`, and `manager::LifecycleStatus`) once welded eleven modules — `analytics`,
`conflict_slot`, `error_classification`, `eta`, `event_sinks`, `manager`, `state`, `status_cache`, `types`,
`unique_name`, `validation` — into one strongly-connected component where nothing could be read, tested, or moved
alone, and it was the app crate's largest tangle. Three lines bought all of that, which is why the rule allows no
exception. Cutting them took the crate from 121 modules in some cycle to 110 and its largest raw component from 11 to
10, changing nothing else in the graph (cargo-modules 0.27.0, `pnpm check module-cycles`, 2026-08-23).

**Where the three names live instead.** `OperationEventSink` comes from `event_sinks` and `IoResultExt` from
`error_classification` — the modules that define them, which is where a reader looks anyway (and a `types::` alias for
either is a `use` of a sibling, so the floor rule already forbids it). `LifecycleStatus` sits in `types.rs` beside
`WriteOperationType` and
`WriteOperationPhase`, because it IS vocabulary: `types::OperationStatus` carries it and it crosses the wire to the
frontend as a serde/specta enum. `manager` imports it like everyone else.

**How it stays cut.** `module-cycles` re-measures this home on every slow run, and its allowlist for
`cmdr::file_system::write_operations` is ratcheted to the remaining tangle. A new upward import from `types` fails the
check with the whole eleven back. `scripts/check/checks/DETAILS.md` § "Rust module cycles" documents how to read its
output, including the ways `cargo-modules` misattributes an edge.

**What's left, and why it stays.** One 2-tangle remains at this home: `archive_edit::engine` ↔ `archive_remote_edit`.
It is the `From`-impl shape trap 4 in `scripts/check/checks/DETAILS.md` describes. `archive_remote_edit.rs` imports
nothing from `archive_edit` — only `engine.rs` knows both types — but it holds BOTH conversions between the twin error
enums (`PlanError` and `RemoteEditError`), and `cargo-modules` files each impl under the module defining the type it
PRODUCES, so one prints as an edge in each direction. Both conversions are live:
`pull_apply_upload_swap` takes `E: Into<RemoteEditError>` and hands back a `RemoteEditError` the engine turns into a
`PlanError` (removing either one fails to compile, verified 2026-08-23). Moving one impl next to the type it produces
would turn a reported cycle into a real one; merging the twin enums is a behavior question for the archive engine, not
a graph cleanup. So it stays, and the allowlist keeps it at 2.

## Architecture / data flow

```
Frontend
  → WriteOperationState created (AtomicU8 intent, oneshot channel for Stop conflicts)
  → stored in WRITE_OPERATION_STATE + OPERATION_STATUS_CACHE
  → operationId returned to frontend immediately (dialog opens, cancel is possible)
  → tokio::spawn (async wrapper)
      → tokio::task::spawn_blocking (local I/O) or direct async (volume ops)
          → validate (sources exist, dest writable, dest not inside source)
          → scan phase: walk_dir_recursive, emit scan-progress events
              (delete on a volume also: `take_cached_scan_result(preview_id)` first;
               on hit, build the entry list from `per_path` — top-level files come
               straight from the cache, top-level dirs recurse via the oracle-aware
               walker; on miss, fall through to `scan_volume_recursive`)
          → disk space check (statvfs)
          → execute phase: per-file copy/delete
              → throttled write-progress events (200ms default)
          → success (copy/move): flush_created_destinations() → emit write-progress (phase: flushing) → fdatasync dests → CopyTransaction::commit(), emit write-complete
          → success (delete/trash): emit write-complete (no sync)
          → cancel (Stopped): CopyTransaction::commit(), emit write-cancelled (rollback: notRolledBack)
          → cancel (RollingBack): rollback_with_progress() → emit write-progress (phase: rolling_back) → emit write-cancelled (rollback: what the reversal managed)
          → error: reverse_copy_transaction() (rechecks each entry, leaves a drifted one standing), emit write-error
      → safety net: start_write_operation emits write-error for unhandled handler errors
  → state removed from both caches
```

## ETA + throughput

Rates and ETA are computed in the backend (`eta.rs`) and shipped on every `WriteProgressEvent` as `bytes_per_second`, `files_per_second`, and `eta_seconds`. The frontend renders these directly, with no client-side math or sample buffer.

**Why backend, not frontend:** one place to test, one set of fields exposed on the wire, identical behavior across copy/move/delete/MTP/SMB/local. Putting the math in Svelte couples the estimator to dialog lifecycle and makes any future client (CLI, menu bar app) reinvent it.

**Why two axes, not one:** the bug we hit in May 2026 was a delete of 5.4 GB / 174k files where the size bar saturated in the first second (a few large files) and the byte-based ETA collapsed to ~0 s while 165k small files were still streaming through. The estimator now tracks bytes/sec and files/sec independently and reports `eta = max(ETA_bytes, ETA_files)`. The operation can't finish before either axis is done, so the larger one is reality. When one axis has zero remaining work, its ETA is `0` and the other axis dominates naturally, with no branching needed.

**EWMA, not blended overall:** `α = 1 - exp(-Δt / τ)` with `τ = 3 s` (see `EWMA_TAU_SECS`). Pure exponential decay, no "overall average" anchor. If the network drops mid-operation, the EWMA converges to the new rate within a few τ instead of being pulled back toward historical numbers. Time-weighted means the response is the same whether progress events arrive every 50 ms or every 500 ms.

**Warm-up:** the estimator returns `None` for ETA until it has ≥ 2 samples in the current phase AND ≥ 800 ms of WORKING time (`MIN_SAMPLES_FOR_ETA`, `MIN_ELAPSED_FOR_ETA`). This kills the early "200 ms in, rate = 50 MB/s → ETA = 0 s" footgun. Rates are populated as soon as we have the first delta; only the ETA is gated.

**The rate window measures WORKING time, never wall time.** Each `EtaSample` carries the operation's cumulative human-wait reading (`human_wait.rs`), and `update()` subtracts whatever accrued since the last sample from the elapsed wall time. An interval that was entirely somebody's — a five-minute conflict prompt, a pause — re-anchors the counters and leaves the rates exactly where they were, so the ETA on screen survives the wait instead of jumping. Without it the first sample after the answer divides one file's bytes by five minutes: a QA pass watched a healthy copy report `0.4 files/s · 409h 39m left` that way, with the queue row for the same operation saying `55m 33s left`.

- **Only a wait on a PERSON counts**, and there are exactly two: the user's pause (`PauseGate::pause` / `resume`) and an unanswered clash (`ConflictSlot`, which mirrors "a prompt is on screen" onto the clock from inside its own transitions, so no path can arm one and forget). A slow SMB read, a busy device, a retry backoff are all the TRANSFER being slow, and the ETA has to say so — `eta_tests.rs`'s `a_device_wait_still_moves_the_eta` is the guard.
- **The two sources are a UNION, not a sum.** The main window pauses an operation before prompting about its clash, so the intervals overlap; adding them would subtract the same seconds twice and leave real working time excluded afterwards.
- **The frontend hides the SPEED during those waits and keeps the ETA** (`operation-session/DETAILS.md` § "Read surface"). The two are one design: the speed is a claim about right now, which is nothing; the ETA is a claim about the work left, which this exclusion keeps true.

**Skipped work is subtracted before the sample.** `enrich_progress` feeds the estimator `bytes_done` / `bytes_total` / `files_done` / `files_total` NET of `state.skipped_totals()`, while the event it enriches keeps the gross numbers the bars render. A skipped file is done and both bars have to reach their totals, but nothing moved for it, so charging it to the EWMA reads as an instantaneous burst — and a merge declining thousands of children back to back would make that burst the whole reported speed. Remaining work is identical either way (`done` and `total` lose the same term), so this changes the rate and never the ETA's numerator. Every skip site calls `state.note_skipped(files, bytes)`; the list and the throttling that goes with it: `transfer/DETAILS.md` § "Skipped work moves the bars, and stays out of the rate".

**Phase transitions reset:** `update()` reseeds on every `phase` change. Without this, the counters' reset (scanning → copying both restart from 0) would feed a negative delta into the EWMA. Rollback is treated as a forward phase toward target `(0, 0)`: the estimator subtracts the new counters from the previous ones and ETA = current value / decay rate.

**Wiring:** every `write-progress` emit site calls `state.emit_progress_via_sink(events, event)`. Production wraps a Tauri AppHandle in `TauriEventSink`; tests use `CollectorEventSink`. `emit_progress_via_sink` calls `enrich_progress` internally, so no caller has to remember. The `bytes_per_second: None, files_per_second: None, eta_seconds: None` placeholders in the struct literals get overwritten before the event reaches the FE.

**Frontend display:** every surface reads the three numbers off the operation's SESSION, which owns the one ETA smoother that operation has in that window (a 25% gap-closure per tick, re-warmed on a phase change) and decides when there is an honest number to print. Contract and rationale: `apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` § "Read surface".

## Parking on a person

An operation waiting on a person emits nothing: it is holding still on purpose, between files, with no chunk callbacks to drive a progress event. So the newest tick every window holds was measured while it was moving, and it keeps a speed on screen over a copy that has stopped. Two pieces close that, and both live on `WriteOperationState`:

- **The wait is classified for every operation, not just the ones with an in-flight table.** `WriteOperationState::activity` asks the transfer probe first (`activity_for`); an operation with no probe — a local copy, a delete, a trash — answers from its own pause gate and conflict slot (`decision_wait`), in the same order `transfer_probe::wait_reason` uses, and reports no in-flight count and no stillness that isn't the answerer's. A local copy therefore says `waiting_on: Conflict` exactly as a volume copy does, which is what lets the frontend decide once, in one place, that a parked operation has no honest speed (`apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` § "Read surface"). § "Naming a wait" for who reads it.
- **Both edges of the wait re-send the last tick.** `announce_human_wait(sink)` re-emits `last_progress` (§ below), counters untouched because nothing moved, and `enrich_progress` re-classifies it on the way out. Called from both Stop-mode dispatchers (`conflict.rs`, `transfer/volume/conflict.rs`) right after `arm` and right after the answer lands. ❌ Arm first, then announce, then prompt: a surface can answer synchronously from inside `emit_conflict`, and an announcement after the emit would describe a wait that is already over.
- **A pause needs no announcement**, because a pause changes the registry snapshot's `LifecycleStatus` and `operations-changed` carries that to every window on its own. A clash leaves the operation `Running`, which is exactly why it needs a voice.
- **`last_progress` is the one copy of "the newest tick", and it is stored WITHOUT its activity.** The transfer probe's stall heartbeat reads the same field (filtering to the phases where a stall means anything) rather than keeping a second one. Activity is stripped on the way in because a re-send exists precisely because what the operation is doing has changed: a stored `waiting_on: Conflict` replayed after the answer would hide the speed for the rest of the transfer.

❌ A new way to park an operation on a person must open the human-wait clock (§ "ETA + throughput") AND announce itself. Miss the first and the estimate collapses on resume; miss the second and every surface lies about the speed until the person answers. Pinned by `conflict_stop_tests.rs`.

## Naming a wait

`TransferWaitReason` is the answer to "this operation says `running` and nothing is moving — why?". Four shapes read identically without it: a slow copy, a wedged mount, a transfer parked on a conflict prompt, and an operation queued behind a busy lane.

**Decision: each variant names the QUESTION, never who may answer it.** `Conflict`, not `You`. Who can answer is a property of which surfaces exist, and it moves underneath the enum every time one ships: `resolve_conflict` made the same park agent-answerable without the operation changing at all, and a name written from the dialog's point of view was wrong the day that landed. The rule lives on the enum itself (`types.rs`), where somebody adding a variant will read it.

**One classifier, two readers.** `WriteOperationState::activity(operation_id)` is the only place a wait is decided. `enrich_progress` puts it on every `write-progress` event; `status_cache::get_operation_status` puts it on every `OperationStatus`, which is what `cmdr://state`'s `operations:` rows render as `waitingOn` / `stillForSeconds` / `inFlight` (`mcp/DETAILS.md` § Resources). A poller and a subscriber therefore can't disagree: a snapshot saying `moving` while the event stream said `destination` sends an agent down the wrong branch.

- **Classified at READ time, never cached.** The status cache echoes the last progress tick's counters; the wait is recomputed on every read, because a wait that is over is worth nothing and a stale one is worse than none. This is also why `get_operation_status` reads it OUTSIDE the cache guard, next to `lifecycle`: the probe registry and the state map are other locks, and that function keeps them all un-nested.
- **`None` means "can't tell", never "it's moving".** An operation that settled (the cache row outlives the state entry) and a backend that keeps no in-flight table with nobody parked on a decision both answer `None`, and every surface renders that as silence rather than a stand-in a poller would act on. Pinned by `status_cache_tests.rs::a_settled_operation_reports_no_activity`, `state_tests.rs::an_operation_with_nothing_to_say_reports_no_activity`, and `mcp/tests/resource_operations_tests.rs::a_backend_that_cannot_classify_its_wait_says_nothing_rather_than_moving`.
- **`enrich_progress` leaves an activity a caller already set alone.** The stall watchdog emits from the probe it is stepping (`transfer_probe::emit_heartbeat`), and its copy is the one that just decided the transfer is wedged; a second lookup there would cost the re-emitted event the very activity it exists to carry. Pinned by `state_tests.rs::enrich_progress_keeps_an_activity_the_caller_already_decided`.

## Key patterns and gotchas (shared)

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**`OperationIntent` state machine.** Replaces the old `cancelled: AtomicBool` + `skip_rollback: AtomicBool` pair with a single `AtomicU8`-backed enum: `Running → RollingBack` (user clicks Rollback), `Running → Stopped` (user clicks Cancel or teardown), `RollingBack → Stopped` (user cancels the rollback). `Stopped` is terminal. The `is_cancelled()` helper returns true for both `RollingBack` and `Stopped`, so the 40+ cancellation check sites just call `is_cancelled(&state.intent)`.

**Cancel vs Rollback: distinct behaviors:**
- **Cancel (`Stopped`)**: Stop immediately. Keep all fully-copied files. Delete only the last *partial* file (a half-written file is corrupted data, not useful to keep). Reports `rollback.outcome: notRolledBack`.
- **Rollback (`RollingBack`)**: Stop copying, then reverse what this operation wrote, newest first, with progress events (`phase: RollingBack`). Each entry is rechecked before it goes, and one something else changed since is left alone and reported (`transfer/DETAILS.md` § "What a reversal does with that identity"). The progress bars drain, and reach zero whatever the reversal managed. The user can cancel the rollback (→ `Stopped`), which keeps whatever hasn't been reversed yet. Reports `rolledBack` or `partiallyRolledBack`.
- Both are triggered from the same `cancel_write_operation` IPC call, distinguished by the `rollback` parameter.

**Two-layer cancellation.** `AtomicU8` (`OperationIntent`) for fast in-loop checks in local file operations. Volume operations (MTP, SMB) use the same `AtomicU8` checks but run on the async executor (no `spawn_blocking`). `run_cancellable` wraps blocking local operations (for example, network-mount copies that may block indefinitely) in a separate thread, polling the flag every 100 ms via `mpsc::channel`.

**Stop-mode conflict resolution.** Creates a per-conflict `tokio::sync::oneshot` channel, **arms `state.conflict_slot` with the sender BEFORE emitting the `write-conflict` event**, then blocks on the receiver (`blocking_recv()` inside `spawn_blocking`; the volume path `await`s instead). Arm-before-emit is load-bearing: a responder can only answer a conflict it has observed, so if the event reached `resolve_write_conflict` (or a test responder sink) before the slot was armed, the answer would land on nothing and the recv would hang. Both the local-FS branch (`conflict.rs`) and the volume branch (`transfer/volume/conflict.rs`) order it this way. Arming also MINTS the clash's `ConflictId`, which is why every emit site arms before it builds the event: the id rides out on it. Frontend calls `resolve_write_conflict(operation_id, conflict_id, resolution, apply_to_all)`, which answers through the slot. `cancel_write_operation` calls `conflict_slot.abandon()`, dropping the sender so the receiver returns `Err` (interpreted as cancellation). No polling, no safety timeout, immediate unblock on cancel. Pinned by `conflict_stop_tests.rs` (local) and the `ConflictResponderSink` suites (volume).

**Answering a conflict is arbitrated, the answer NAMES the conflict it is for, and the arbitration is REPORTED.** `write-conflict` broadcasts to every webview, so several surfaces can render one prompt and each of them can be answered (the progress dialog and the main window's `operation-conflict.svelte.ts` host today). `conflict_slot.rs` is a three-state machine — `Idle` / `Awaiting { id, sender, prompt }` / `Answered { id }` — under one `std::sync::Mutex`, and `answer(conflict_id, response)` performs the whole transition inside it, so exactly one answer reaches the parked operation.

**The armed state holds the QUESTION, and `arm` builds it.** `arm(tx, |id| event)` mints the id, calls the builder with it, stores the event, and hands it back for the caller to emit, so the id on the wire, the id the slot is armed with, and the id `answer` requires back are one value by construction. That question is then readable through `pending()` / `write_operations::pending_write_conflict(op_id)` for anyone who arrives after the broadcast: `write-conflict` only ever reaches whoever was listening when it went out, so `cmdr://state`'s `pendingConflict:` block (the surface an AGENT answers from) has no other way to know what is being asked. ❌ Don't add a second way to arm that skips the question: an operation parked on something nobody can read back is answerable only by a surface that happened to be listening.

**The id is what makes an answer belong to a question.** An operation raises its clashes one at a time, but an answer's round trip (broadcast out, a person, IPC back) can outlast the clash it belongs to: the operation takes an answer, carries on, hits the next clash, and parks on it, all before the answering surface's IPC call has returned. Without an identity that late answer is indistinguishable from an answer to the clash now on screen, and it silently decides a question nobody was shown. `ConflictSlot::arm` mints a `ConflictId` (a per-operation counter, minted under the same lock as the state transition so an id can never belong to two questions), the `write-conflict` event carries it, and `resolve_write_conflict` requires it back.

What each caller gets back is the `ConflictResolutionOutcome` that state produced: `Resolved` (this answer is the one the operation carried on with), `AlreadyResolved` (someone answered THIS SAME clash first; nothing changed), `StaleAnswer` (this answer names a clash the operation has left behind — it reached nothing, and whatever is parked now is untouched), `NoPendingConflict` (the operation is live but isn't asking — never raised a conflict, a cancel abandoned the pending one, or the waiting task stopped listening), or `UnknownOperation` (nothing is registered under that id). It crosses IPC (`bindings.ts`) because a surface that can't tell it lost leaves its prompt up over an answer that did nothing, which is the state the user reads as "I clicked and nothing happened". ❌ Don't collapse `AlreadyResolved`, `StaleAnswer`, and `NoPendingConflict` into each other: "someone beat you to it", "your question is over and there's a new one", and "this operation isn't asking anything" mean different things to a caller, and the FE logs them apart. ❌ Never let a stale answer report `Resolved`: that is the confidently-wrong shape the id exists to remove. A mismatched answer leaves the parked sender in place, untouched — the person the live clash is on screen for hasn't clicked yet.

Every transition also mirrors "a prompt is on screen" onto the operation's human-wait clock (§ "ETA + throughput"), derived from the state it just wrote rather than remembered alongside it.

**And the operation announces the clash as OVER, naming it** (`write-conflict-resolved`, emitted through the sink from the same three Stop-mode dispatchers, right where the answer lands and only on `Ok`). The verdict above reaches exactly one surface: the one whose own IPC call returned. Every other surface showing the same broadcast prompt — the queue window's copy, the main window's host, anything at all when an AGENT answered over MCP (`resolve_conflict`) — would keep asking a question with no answer left to give, and being a modal, block every new operation behind it. The FE drops only the clash the event NAMES (`operation-conflict.svelte.ts`, `operation-session.svelte.ts`), because the operation raises its next clash the moment it takes an answer, so the retraction for the old one routinely lands with the new one already on screen. A cancel emits nothing: it leaves no answer, and the prompts go because the operation itself does.

The state machine exists so the take, the id, and the "was it answered?" bookkeeping are ONE transition; a `bool` (or a counter) beside an `Option<Sender>` can desync from it. Pinned by `conflict_slot.rs::tests` (per-transition, including the retired-clash refusal) and the `resolve_write_conflict` block in `state_tests.rs` (through the public path). The frontend half of the same contract — a session clears only the clash it answered, so one that arrived mid-answer survives — is `apps/desktop/src/lib/file-operations/operation-session/DETAILS.md` § "Answering a clash is a delegation".

**Conflict-dispatch mutex (folder merges).** `WriteOperationState::conflict_dispatch_lock` (a `tokio::sync::Mutex`, next to `conflict_slot`) serializes the whole Stop-mode dispatch for an operation: there is exactly one human and one oneshot slot, so two tasks both hitting a Stop-mode clash at once — the concurrent volume-copy spawn loop, or two parallel deep directory merges — must queue rather than race to emit a `write-conflict` and clobber each other's sender. The dispatch sequence under the lock: check `is_cancelled` (bail with `Cancelled` so a queued task can't emit a prompt no one will answer after the dialog tears down — a hang), re-check the apply-to-all latch (a prior "…all" answer collapses the queued prompt), emit + await, store the latch, release. Released on every exit, NEVER held across the subsequent file write. Volume-side only today (the local-FS engine's per-file conflicts surface serially inside one `spawn_blocking`).

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` → `Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins; subsequent calls with different intent are no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to `Stopped` (teardown should never silently roll back without visual feedback).

**Scan preview state.** `start_scan_preview` registers one `PREVIEWS` entry in `scan_cache.rs` and spawns a walk. The entry is either in flight or settled, and a settled one carries WHY: complete (with its `CachedScanResult`), errored (with its message), or cancelled. `copy_files_start` / `delete_files_start` consume a completed result via `preview_id` in `WriteOperationConfig`, skipping a redundant scan. An entry is freed by three paths: (1) `take_cached_scan_result(preview_id, sources)` at op start (the consume path), (2) `cancel_scan_preview(preview_id)` on dialog teardown — it sets the in-flight cancel flag AND drops the entry, so a dialog dismissed after the scan completed doesn't leak the result — and (3) a TTL safety net: `settle_preview` first evicts settled, UNCLAIMED entries older than `SCAN_RESULT_TTL` (5 min). The TTL is a backstop for a caller that forgets both (1) and (2); the pure `expired_scan_result_ids` helper is unit-tested. A `CachedScanResult` can hold tens of thousands of `FileInfo`, so none of these paths is optional.

**The cache is bound to its request, and says so when it's incoherent.** A `preview_id` proves the frontend once asked for a scan. It proves nothing about WHICH scan, and three of the six consumers act on the cached file list without ever re-reading their own `sources` again, so an id pointing at a preview of a different selection makes each of them fail differently: the LOCAL delete walker (`delete/walker.rs`) deletes the previewed tree instead of the requested one, with no rollback and no progress line naming it; the LOCAL copy (`transfer/copy/mod.rs`) writes the previewed tree to the destination while its bulk-skip set still reads the requested one; the LOCAL move (`transfer/move_op/cross_fs.rs`) stages the previewed tree and then fails in Phase 3 looking for the requested name in staging, a half-staged move. The VOLUME delete and both `transfer/volume/preflight.rs` sites were already source-bound (they iterate `sources` and fall through per-source on a miss), so they degrade to a rescan rather than acting wrong.

Two mechanisms close it at the choke point, and both live in `scan_cache.rs`:

- `CachedScanResult::sources` records what the preview was asked to walk, and `take_cached_scan_result(preview_id, requested_sources)` compares it SET-wise against the operation's own list. A mismatch is a cache miss: the entry is dropped, a warn names both lists, and the caller takes the fresh-scan fallback it already had. The comparison normalizes nothing on purpose. A path that differs only by a trailing separator is an IPC-edge bug; a lenient comparison here would just be another belief.
- `insert_scan_result` carries a coherence canary: a completed walk with `file_count > 0` and an empty `per_path` warns and trips a `debug_assert!`. That's the shape that let a LOCAL preview hand the copy drivers an empty `source_hints` map, which they read as a confident `is_directory: false`. It's one-directional (a volume batch legitimately caches empty `files` with a populated `per_path`), and it's a `debug_assert!`, so a release build still admits the entry and the drivers still have to survive it (`transfer/volume/copy_source_hint_tests.rs` is that defense's proof, seeding past the canary via `seed_incoherent_scan_result_for_test`).

`SCAN_PREVIEW_RESULTS` is private to `scan_cache.rs` so neither can be walked around: `insert_scan_result`, `take_cached_scan_result`, `cached_scan_totals`, and `release_scan_result` are the whole surface, and `state.rs` re-exports the types but not the map. A `pub(super)` static is a choke point in name only. Pinned by `scan_cache_tests.rs` (both mechanisms), `delete/preview_binding_tests.rs` (the destructive one, plus a volume-side regression fence), and one binding test each in `transfer/copy/copy_tests.rs` and `transfer/move_op/move_op_tests.rs`.

**A completed preview always carries `per_path`, whichever walk produced it.** Both `run_scan_preview` (the LOCAL `std::fs` walk, via `scan.rs::walk_sources_with_per_path`) and `run_volume_scan_preview` (SMB / MTP) cache one `CopyScanResult` per top-level source: its type, file/dir counts, and byte totals. That map is the ONLY thing that tells a cross-volume copy whether each selected item is a file or a folder without paying a stat probe per source, and it's what a volume delete's fast path reads for top-level file sizes. Downstream code treats a source that isn't in the map as **unknown** and probes (`transfer/volume/DETAILS.md` § "A missing source hint means unknown"); ❌ don't add a preview path that completes without filling it — an empty map costs one round trip per source on SMB/MTP, and any consumer that guesses instead of probing is a data-safety bug waiting to happen. Per-source accounting is a before/after snapshot of the shared walk counters, so hardlink dedup still spans all sources (a later source's `dedup_bytes` share can read low; informational only). Pinned by `scan.rs::tests::walk_reports_type_and_totals_per_top_level_source`.

**Decision**: `scan_sources_internal` does NOT collect `per_path`, and `ScanResult` / `CachedScanResult` stay two structs.
**Why**: `scan_sources_internal`'s result goes straight back to its caller and never enters the preview cache, so its empty `per_path` can't be read by a consumer that would mistake it for "these sources are files". Filling it would buy a symmetry that only exists on paper, at the cost of a per-source counter bracket on the local copy/move/delete hot path. The uniformity worry underneath is real, though: an empty `Vec` is doing the job of "not collected", which is the same anti-pattern as `SourceHint::default()` one level up. **The elegant fix, when someone wants it, is a named enum** (`PerSource::NotCollected` / `PerSource::Collected(...)`), so "empty because there were no sources" and "empty because this walk doesn't collect them" stop being the same value. That has real reach (six consumers plus two crates' worth of `BatchScanResult` plumbing) and deserves its own decision rather than riding along on a safety pass. Until then the contract lives on `scan_sources_internal`'s doc comment: if that result ever starts crossing the cache, it has to collect `per_path` first.

**Compressed-size estimate (Compress dialog).** When `start_scan_preview` runs with `sample_for_estimate` set (Compress mode only), the LOCAL walk feeds a cheap deflate-sampling estimator (`compress_estimate::CompressEstimator`) that predicts the zip's output size, shown live-ish in the dialog beside the scanned byte total. Mechanics and invariants:

- **Local-FS walk only; remote is suppressed.** The per-file `WalkContext::on_file` hook fires only from `walk_dir_recursive` / `walk_cached_entries` (the `run_scan_preview` path). `run_volume_scan_preview` (SMB/MTP) never samples and never guesses — the estimate is simply `None`. Sampling a remote source would do real network reads and defeat the oracle's zero-I/O short-circuit, and an extension-only guess is unbounded (a single mistyped file → 8×-wrong), so an absent estimate is the honest choice. **Don't add sampling to the volume/oracle path.**
- **Off the walk thread.** The hook is a push on a deliberately UNBOUNDED channel (a `sync_channel` would block the walk when the sampler falls behind, e.g. an oracle-cached fast walk of a huge tree — the transient queue of `(PathBuf, u64)` is the accepted cost, and post-budget the worker drains via cheap lookups; don't "fix" this into a bounded channel). The worker thread deflates a 32 KiB head window per file at reference level 6 under an 8 MiB total byte budget, so the sampling CPU never lands on the walk's critical path and worst-case added time is ~105 ms regardless of tree size. Media-heavy trees cost near zero (an incompressible-extension table shortcuts the read). Files under 4 KiB, budget-exhausted files, and unreadable files take a running-average ratio. The worker cancels with the scan (shared `cancelled` flag) and is joined before the complete event; a sampling panic degrades to `None` and never fails the scan.
- **Per-class subtotals, scaled on the FE.** The estimate ships as three `CompressedSizeEstimate` subtotals of estimated **level-6** bytes, bucketed by each file's sampled compressibility class. The frontend re-scales to the user's selected deflate level via a baked per-class curve (`compress-estimate-scaling.ts`) with no re-scan, so moving the level slider updates the shown number arithmetically. Level 6 is the reference (shown value = sum of subtotals). It rides `scan-preview-complete` (and the `get_scan_preview_totals` recovery path) only; while scanning the dialog shows a loading affordance.
- Parameters (window, budget, tiny threshold, extension table, level curve) and their measured accuracy/cost: `docs/notes/compress-size-estimate-spike.md`.

**Progress throttled to 200 ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>` of canonicalized paths.

**Every local write lands via temp + rename** (`overwrite::stage_and_land_file`). Steps: write the bytes to `dest.cmdr-tmp-<uuid>` (a sibling, so the landing rename is same-directory and therefore atomic); if an existing entry is being replaced, rename dest → `dest.cmdr-temp-<uuid>` (the aside); rename temp → dest; delete the aside. The original is intact until the landing rename completes, and the destination name never holds a partial at any observable moment. The same pattern covers file→folder overwrites (existing dest folder renamed aside, then the source file lands at the original path) and folder→file overwrites (via `safe_overwrite_dir`: existing file renamed aside, the folder materialized in place by the caller's closure, then the aside deleted; on materialize error or cancel, the aside is rolled back).

The four copy mechanisms (`copy_strategy::LocalCopyStrategy`) each hand `stage_and_land_file` a closure that puts bytes at a path, so the landing is written once rather than once per branch. ❌ Don't add a mechanism that writes straight to `dest`. Details and the no-clobber rule: `transfer/DETAILS.md` § "Local copies stage".

**Conditional conflict policies (`OverwriteSmaller` / `OverwriteOlder`)** reduce per-file. The user picks "Overwrite all smaller" / "Overwrite all older" either upfront (TransferDialog radios) or via the per-file conflict dialog's apply-to-all buttons. Each conflict re-evaluates against its own source/dest metadata: `OverwriteSmaller` overwrites only when `dst.len() < src.len()`, `OverwriteOlder` overwrites only when `dst.modified() < src.modified()`. Equal sizes / equal mtimes / unknown metadata all reduce to `Skip` — strict comparison so a borderline file is never silently overwritten. Implemented by `conflict::reduce_conditional_resolution` (sync, local FS) and `transfer/volume/conflict.rs::reduce_volume_conditional_resolution` (async, volume backends). Both log a `target: "conflict_resolution"` info line on every Skip with the reason (not-strictly-smaller, not-strictly-older, missing metadata), so users running an MTP/SMB copy who picked one of these can see in the operation log why their conflicts got skipped instead of being puzzled by silence. **The apply-to-all storage saves the *original* conditional variant**, not the reduced one — subsequent conflicts re-run the comparison against their own files.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open the progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's `Result`: `Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))` emits `write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the frontend's `handleError` removes all listeners on first receipt.

**A volume-aware op never turns its own `Cancelled` into a `write-error`.** The inner handler already emitted `write-cancelled` on that path, so the outer arm passes `WriteOperationError::Cancelled` through silently (`transfer/volume/copy.rs`, `mod.rs`'s safety net, and the archive-edit driver all order it this way). Re-emitting would show the user a failure for something they asked for, and the retained-failure list would have to filter it back out (§ "Retained failures" excludes `Cancelled` by typed variant for exactly this reason).

**`cancel_all_write_operations` is the quit teardown's first move, and its ONLY caller is the quit gate.** ❌ A window going away is not a reason to stop work: an operation outlives the view watching it, so nothing in the frontend may reach for this. `crate::quit::tear_down_and_exit` calls it with keep-partials semantics, waits up to 1.5 s for the operations to answer, and only then escalates. `crate::quit::DETAILS.md` § "The teardown's order".

**`abort_all_write_operations` is the second tier, and is ❌ NOT this.** It fires `WriteOperationState::backend_abort` on top of a cooperative cancel, which makes the cross-volume streaming path stop WAITING for a backend rather than asking it to stop — buying a bounded wind-down at the cost of the backend's own partial cleanup. That is a trade only a deadline holder may make, so the caller is the quit gate and nothing else; a teardown that can still afford to wait stays on `cancel_all_write_operations`. Mechanism, cost, and the invariant it must not break: `transfer/DETAILS.md` § "Two tiers of cancel". The per-operation `abort_write_operation` is `#[cfg(test)]`: a deadline always aborts everything, so production has no use for the narrower form.

**A local blocking read or write cannot be given a timeout, which is why there is no tier 2 for the local path.** The chunked engine checks the cancel before each 1 MiB read (`transfer/chunked_copy.rs`), which is near-instant on a responsive filesystem and unbounded on a hung one: the `read` and `write_all` themselves are plain blocking calls, and nothing can put a clock on them. `O_NONBLOCK` is specified not to apply to regular files, a hung SMB or NFS client blocks down in the VFS whatever the descriptor says, `pthread_kill`-to-`EINTR` is defeated by macOS restarting most filesystem syscalls, and unwinding a thread mid-`write_all` is not something we can make safe. (Reasoned from POSIX and documented macOS syscall-restart behavior, not measured, 2026-08-20.) ❌ So don't add a "timeout" here. The only available move is to stop WAITING for the thread, which is what the quit deadline does, and what `commands/util.rs::blocking_with_timeout` already does elsewhere: the blocking thread runs on, the caller gives up. Abandoning it costs nothing because every local write stages, so a wedged worker is only ever filling a `.cmdr-tmp-*` nobody will rename.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

## Who speaks `MutationError`

Four command families answer with it, so every instant-mutation surface matches ONE exhaustive union: `rename_file`,
`check_rename_permission`, and `move_to_trash` (`commands/rename.rs`); `create_directory` / `create_file`
(`commands/file_system/write_ops.rs`); `check_rename_validity`, whose own answer is a typed `RenameValidityResult` so
the only `Err` it can produce is the deadline or a panicked task; and `paste_clipboard_as_file`, which is a managed
`CreateFile` op under the hood and refuses the way one does (`paste_clipboard.rs`). A volume's own refusal rides through
`MutationError::Volume` carrying the whole `VolumeError`. Full rules: `docs/guides/error-handling.md`.

## Cmdr-own-write hook (downloads watcher)

Every write-op driver MUST register its destination with the downloads watcher's ignore set BEFORE issuing the syscall. This is what makes the watcher silently suppress events Cmdr itself caused, so the user doesn't see a "Downloaded foo.bin" toast when they just used Cmdr to copy 100 files into `~/Downloads`.

**Contract:** call `crate::downloads::note_pending_write_for_cmdr(&dest_path)` immediately before the write syscall (or the volume-trait equivalent: `Volume::write_from_stream`, `Volume::create_file`, `Volume::create_directory`, `Volume::rename`, `Volume::delete`).

**Locked-in scoping:** the prefix check lives INSIDE the helper (and the underlying `IgnoreSet::note_pending`). Call sites invoke unconditionally; paths outside the resolved Downloads root silently no-op. **Don't add `if path.starts_with(downloads_dir)` guards at call sites**: centralizing the scope in the helper keeps it from drifting across call sites (the downloads watcher's ignore-set design lives in the `downloads` module docs).

**No-op when the watcher is dormant.** If the FDA gate is closed (or `refresh_runtime` hasn't been called yet), the watcher isn't installed and the helper is a cheap no-op (single mutex `lock + is_none`). Production write ops fire freely; the cost is one atomic-bool read per write.

**Renames register both halves.** A rename moves a file out of one location into another. The Cmdr-own-write contract requires registering both the source path (so a rename-OUT-of-Downloads is also suppressed via the watcher's rename-from-ignored-source branch) and the destination path (so the rename-arrival event is suppressed). See `commands/rename.rs::rename_file` and `transfer/move_op/` for the pattern.

**Cross-volume writes that land on a local FS** (MTP→Local, SMB→Local) hook via the local helper inside `transfer/volume/strategy.rs::note_pending_for_local_dest` and `transfer/volume/move.rs::note_pending_for_local_volume`. They check `dest_volume.local_path()` first and skip when the destination isn't a local-FS-backed volume (MTP/SMB/InMemory) — those paths can't trigger the watcher anyway.

Example placement:

```rust
// In `copy_single_item` (transfer/copy.rs), just before `copy_file_with_strategy`:
crate::downloads::note_pending_write_for_cmdr(&actual_dest);
let bytes = copy_file_with_strategy(source, &actual_dest, ..)?;
```

See also: `apps/desktop/src-tauri/src/downloads/CLAUDE.md` for the watcher architecture, ignore-set internals, and the FDA-gated lifecycle. End-to-end safety net for the contract lives in `downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`.

## Events emitted

- **`write-progress`**: Every ~200 ms during copy/move/delete/trash
- **`write-conflict`**: Stop mode hit a conflicting destination file
- **`write-complete`**: Operation finished successfully
- **`write-cancelled`**: Operation cancelled. Carries `rollback: CancelRollback` — the three-state outcome (`notRolledBack` / `rolledBack` / `partiallyRolledBack`), how many items the reversal undid, and what it left behind grouped by `SkipReason`.
- **`write-error`**: Operation could not complete. Carries only `error: WriteOperationError` (typed, word-free); no rendered prose crosses IPC. The FE renders the title/explanation/suggestion + category from this typed error via `transfer-error-messages.ts` in `TransferErrorDialog` and applies category-based colors.
- **`write-settled`**: Emitted once per op after the spawned background task fully returns. See [Settle contract](#settle-contract).
- **`volumes-busy-changed`**: The set of volume IDs with an in-flight op changed (an op started or finished). Payload is `string[]`. See [Busy-volumes set](#busy-volumes-set).
- **`operations-changed`**: The operation registry's membership or lifecycle status changed. Thin snapshot (`{ operations: OperationSnapshot[] }`), NOT 200 ms progress. See [Operation manager](#operation-manager).
- **`write-source-item-done`**: All files for a top-level source item processed (for gradual deselection)
- **`dry-run-complete`**: `config.dry_run == true` (returns `DryRunResult`)
- **`scan-preview-progress`**: During `start_scan_preview`
- **`scan-preview-complete`**: Preview scan finished
- **`scan-preview-error`**: Preview scan failed
- **`scan-preview-cancelled`**: Preview scan cancelled

**MCP consumer note**: the MCP server records each op's terminal outcome into a bounded ring (`mcp::terminal_ops`) at the `TauriEventSink` emit sites for `write-complete` / `write-cancelled` / `write-error`, so its `await operation_complete` can report a settled op's status. It has to tap these terminal events specifically: `operations-changed` fires AFTER removal-on-terminal, so a completed or cancelled op leaves the snapshot entirely. A FAILED op is the one exception — it stays on the snapshot as a retained row (see [Retained failures](#retained-failures)) — but the ring still owns `Completed` / `Cancelled`. See `mcp/DETAILS.md` § State stores.

## Operation manager

The full model and the why behind each decision are captured in this section. Design history is in git (former `docs/specs/2026-06-21-transfer-queue-pause-plan.md`).

`manager.rs` is the single coordinator every write op flows through. It exists because there were FIVE independent spawn paths (`start_write_operation` for local copy/move/trash + local delete; the volume-delete branch in `delete_files_start`; `copy_between_volumes`; `move_between_volumes`; `move_within_same_volume`), each hand-rolling its own `tokio::spawn` + state-insert + status-register + `WriteSettledGuard`, and an op always spawned immediately. The manager unifies them behind `spawn_managed(descriptor, state, deferred)` and adds a registry with real lifecycle states plus **lane-based admission** that can serialize ops which would thrash a shared device.

### Lanes and `Volume::lane_key()`

Each op touches the [`LaneKey`](../volume/CLAUDE.md)s of its source and destination volumes (same-volume ops touch one). Lane keys come from `Volume::lane_key()` (in `volume/mod.rs`), NEVER from parsing a `volume_id` string. Per backend: `LocalPosixVolume` → the volume root (the trait default; each local mount is its own `LocalPosixVolume`, so the root IS the mount root); `MtpVolume` → `device_id` (one USB pipe per device, so every storage on a device shares its lane); `SmbVolume` → its `volume_id` (already `smb_volume_id(server, port, share)` — server+share granularity); `InMemoryVolume` → a `with_lane_key(key)` builder, defaulting to root so the ~169 existing `new(...)` sites are untouched and tests opt into same-lane vs different-lane.

The both-local branches of `copy_between_volumes` / `move_between_volumes` compute the two lane keys from the live volume handles and pass them into `copy_files_start` / `move_files_start` as `Some(lanes)`. The plain local commands pass `None`, so the entry point derives a lane from `volume_ids` (`local_lanes`): empty → the `root` lane, else one lane per id. This is a faithful proxy for `lane_key()` on the path where no `Volume` handle is threaded through — it uses each id as an opaque whole, with no substring parsing.

### Admission — global FIFO, atomic multi-lane reservation

The manager keeps one ordered queue (`order`) plus a `lane_use` table (lane → in-use count; budget 1 per lane in v1, a lane is free iff its count is 0; a `HashMap` not a set so v2 budgets > 1 reshape nothing). An admission pass walks pending ops oldest-first and admits the first whose EVERY lane is free, reserving all its slots atomically, flipping it to Running, registering its volumes busy, and spawning its deferred start. It loops so one pass can admit several disjoint-lane ops. A two-lane op can't starve behind churn on a single lane — there are no per-lane queues, so the multi-lane op is always considered at its FIFO position against the whole lane table.

### Deferred start, not "spawn then block on a semaphore"

A queued op holds only DATA describing how to begin: a boxed `FnOnce() -> Pin<Box<dyn Future + Send>>` (`DeferredStart`). The manager spawns it only on admission. Blocking a spawned op on a lane semaphore would pin a `spawn_blocking` pool thread idle per queued op — a leak that can deadlock the finite pool under many queued ops. Each deferred future owns the op end-to-end (the `WriteSettledGuard`, the actual transfer/delete, the terminal-event emit) and ends by calling `manager().on_settled(id)`.

### Dequeue on settle — explicit, NOT in `Drop`

`on_settled(id)` (the happy path) frees the op's lane slots, removes it from the registry, cleans `WRITE_OPERATION_STATE` + the status cache, and runs an admission pass (which may spawn the next op). It's sequenced after the terminal event, exactly where the old per-site cache cleanup ran. The `ManagedTaskGuard` is the panic safety net: held by each spawned task, its `Drop` frees lanes + cleans caches but NEVER spawns (no admission pass). Spawning during the previous op's unwind would re-enter the manager mid-panic (abort) or deadlock on a lock held up-stack. So a panicking op still releases its lanes, but the next op is admitted only on a healthy settle (the next registration's admission pass, or another op's `on_settled`, picks it up). The happy path calls `task_guard.disarm()` right before `on_settled` so its now-redundant Drop is a no-op. Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

### Observing an admission pass (`admission_passes`)

Admission is otherwise invisible from outside: a pass that walks the queue and admits NOBODY changes no status, emits no event, and touches no lane. The negative assertions ("B must still be Queued after A settles") need exactly that moment, and without a signal for it a test can only guess at a wall-clock span.

`OperationManager::admission_passes` is an `AtomicU64` bumped ONCE at the end of `run_admission_pass`, read by `admission_pass_count()` (test-only). A test reads the count, triggers the settle, and waits for it to grow: an advance means a pass walked the whole FIFO queue and either admitted the waiting op or declined to. Three properties make it work:

- **Bumped LAST, and on every exit.** The signal means "the pass finished", so a new early `return` inside `run_admission_pass` that skips the bump would hang every waiting test. Nothing in production reads the count, so the bump is pure signal, never a decision input.
- **`SeqCst` on the bump and the load,** not `Relaxed`. The waiter reads the count, acts, and waits for it to advance, so the bump has to be ordered against the admission work it claims finished; a `Relaxed` counter buys a rarer, harder flake than the sleep it replaces.
- **Global count, global queue.** A pass triggered by an unrelated concurrent test is equally good evidence, because a pass considers every registered op, not just its trigger's.

`Notify` would be wrong here and `watch` is second-best: the settle path runs the pass BEFORE a test could await, so `notified()` created afterwards is a guaranteed lost wakeup. Prefer a monotonic counter for any future "background work reached a point" signal.

`force_admission_pass()` (test-only) runs a pass inline and returns when it's done, for the paths production never runs one on: `set_paused` admits nobody, so `paused_running_op_does_not_admit_a_queued_same_lane_op` runs its own pass and asserts B is STILL Queued. Admission flips a record to Running before it spawns, so a Queued status after a completed pass also proves the deferred start never ran. The waits themselves use `crate::test_support::wait_until_async` (`docs/testing.md`).

### The admission pass spawns admitted ops on the APP runtime, not the caller's

The admission pass (from `spawn_managed` or `on_settled`) spawns each admitted op's deferred start with **`tauri::async_runtime::spawn`, deliberately NOT `tokio::spawn`**. `tokio::spawn` binds to whatever runtime is current when the pass runs — and the pass can run on a runtime that has nothing to do with the op it's admitting. Admission is global and there is a lock-free window between an op's registration (`spawn_managed` inserts it Queued, drops the lock) and its own admission pass: a CONCURRENT op's `on_settled`, running on a different runtime, can reach the pass first and admit the freshly-registered op. So the runtime that spawns an op is racy, not "the op's own caller". `async_runtime::spawn` pins every op to the one process-global runtime that outlives them all.

In production this is a no-op (there is exactly one long-lived Tauri runtime, so ambient and app runtime are identical). **The guard exists for the process-global-manager + per-runtime-caller topology**, which is the test harness: every `#[tokio::test]` runs on its own runtime, and with a bare `tokio::spawn` an op admitted by a runtime that is then torn down is orphaned — it never runs, never settles, and leaks its lane forever, wedging later ops (the observed nondeterministic `wait_until` timeouts). Pinned by `manager::tests::admitted_op_runs_even_if_the_admitting_runtime_is_dropped` (a throwaway runtime admits an op and is dropped without driving it; the op must still complete). This race is lane-INDEPENDENT — it hit even unique-lane ops — so a hermetic-lanes-only test fix could not close it; the guard is the actual fix.

Independently, the archive-edit tests still give each op its OWN lane (`archive_edit::test_support::unique_lane_id`, and `InMemoryVolume::with_lane_key`), matching the `manager::tests` discipline ("unique operation ids + lane keys"). That's for test ISOLATION and parallel speed — a shared global lane serializes unrelated tests and couples their timing — not for orphan-safety, which the guard now owns. The one behavior that needs a `"root"` id (a root parent settling with `None`) is pinned by `move_out_tests`, whose lanes come from the volume objects, so it passes a `"root"` settle id WITHOUT reserving the `"root"` lane.

### Lifecycle status and `operations-changed`

`LifecycleStatus` (Queued/Running/Paused/Done/Cancelled/Failed) lives on the manager record. The snapshot also carries
`supports_rollback` (see below) and `error` (see [Retained failures](#retained-failures)). Admission and settle set Queued/Running and removal-on-terminal; the pause/resume path sets the `Paused`↔`Running` flip (see [Pause / resume](#pause--resume)). It is distinct from `WriteOperationPhase` (the progress phase) and `OperationIntent` (the cancel/rollback machine). The `operations-changed` typed event carries a THIN snapshot (`Vec<OperationSnapshot>`: id, type, status, source/dest summary), emitted from `spawn_managed` / `on_settled` / `cancel_if_queued` / `set_paused` / the two dismiss commands. It deliberately excludes 200 ms progress — the queue window reads the per-file `write-progress` stream for live bars. `init_operation_event_emitter(app)` wires the emitter at startup (`lib.rs`), mirroring `init_busy_volume_emitter`.

A live RECORD only ever holds `Queued` / `Running` / `Paused`: `Done` and `Cancelled` are declared but never assigned, because `on_settled` deletes the record before anything could set them. `Failed` is the one terminal status that reaches a snapshot, and only through the retained-failure list below, never on a record.

**One lifecycle answer, and the query API carries it too.** `OperationStatus` (`get_operation_status`, the progress/detail query) reports the manager's `LifecycleStatus` in its `lifecycle` field, read through `manager().lifecycle_status(id)`. ❌ Never re-derive a lifecycle from `WRITE_OPERATION_STATE.contains` or any other presence test: `spawn_managed` inserts the state entry BEFORE admission and a paused op keeps it, so presence means "exists and hasn't been torn down" and is true for queued, running, and parked alike. A boolean in that spot reported a parked copy as running, which is what the pause/resume toggle steers by. `lifecycle` is `None` only in the window between an op settling and its status-cache row being unregistered. Pinned by `status_cache_tests::a_paused_operation_reports_paused_not_running` and `a_queued_operation_reports_queued`.

`lifecycle_status(id)` reads live RECORDS only, so `None` means one thing: the manager no longer tracks this id. It deliberately does not consult retained failures, which it cannot reach anyway — a failure is retained from the same cleanup that unregisters the status-cache row, so `get_operation_status` has already returned `None` by then. `snapshot()` is the one place the two sources are joined.

⚠️ **Lock order**: `get_operation_status` asks the manager BEFORE taking the status-cache read lock. That is the only place the two locks meet, and the busy-volume recompute already runs cache-lock-then-out, so nesting the manager lock inside the cache lock would close a cycle through data-safety code.

### Retained failures

**The exception to removal-on-terminal.** Nothing else can hold the failure of an operation that was backgrounded: the record is deleted on settle, and `write-error` only reaches a window that is listening at that moment — and the queue window being closed is the exact scenario this is for. So the manager keeps a bounded list of failures OUT OF BAND — `ManagerInner::failures`, a `VecDeque<OperationSnapshot>` capped at `FAILURE_CAPACITY = 20`, oldest evicted first — and `snapshot()` appends them after the live rows.

- **`free_and_remove` is untouched.** Admission, laning, the busy-volumes set, and settling behave exactly as before: a failed op frees its lane slots, cleans its caches, deletes its record, and admits the next op on the same `on_settled` it always did. Retention is a separate structure the settle path never consults. Pinned by `manager::tests::a_failed_op_frees_its_lane_and_admits_the_next_exactly_as_before`.
- **Recorded at the emit site.** `TauriEventSink::emit_error` calls `record_failure`, next to the `mcp::terminal_ops::record` line. ❌ Not in the `OperationEventSink` trait and not in `CollectorEventSink`: test sinks stay side-effect-free.
- **⚠️ A failure whose record is still LIVE is filtered out of the snapshot.** `emit_error` runs inside the op's own task, before `on_settled` removes the record, so for a moment the op is both running and failed. Emitting both rows would put one `operationId` in the list twice, and the queue window's keyed `{#each}` throws on a duplicate key. `record_failure` therefore stays silent whenever the record is live; the row surfaces on `on_settled`'s existing `emit_changed`, which is the honest moment anyway. It DOES emit on the other branch, where the record is already gone: no duplicate is possible without a record, and no `on_settled` is coming, so the row would otherwise sit unbroadcast until some unrelated emit — no toast, no chip, nothing until the queue window next opens. Pinned by `a_failure_with_no_live_record_broadcasts_itself`.
- **Not every `write-error` is a failure.** `WriteOperationError::Cancelled` (some volume paths emit it) and `ArchiveNeedsPassword` (a recoverable prompt the frontend answers and retries) are excluded by TYPED variant match, never by message text.
- **First error per id wins.** `write-error` can fire twice for one op — an inner handler emits and returns `Err`, then `mod.rs`'s safety net emits again — and the first one is what actually stopped it.
- **A retained row reports `supports_rollback: false`** and reuses the live record's source/destination summary, so it reads like the running row it replaces. If the record is already gone, it falls back to the event's own `operation_type` and no summary.
- **Only an explicit action clears one**: `dismiss_failed_operation(id)` or `dismiss_all_failed_operations()`. ❌ Never a timer, ❌ never a window close, ❌ never "the next operation starting". A 40-minute copy that failed while the user was away has to still be there.

**Decision: retention lives in the manager, not the frontend. Why.** The failure has to reach two separate webviews, and it can land while either is closed. A store in the queue window misses it outright (that window is closed in the exact scenario this exists for); a store in the main window can't be read by the queue window and would need a hand-rolled state server over `emitTo`, inverting the dependency. The operation log (`journal.rs`) is the durable history and stays that, but it records an `ExecutionStatus`, not the typed `WriteOperationError` the frontend's message pipeline needs, and browsing the past is a different job from a live notice. The manager already owns membership and already broadcasts it to both windows, so retention there needs no new event, no new listener, and seeds correctly through the existing `list_operations` on window open.

**Decision: runtime-only, capped at 20. Why.** The cap and its reasoning mirror `mcp::terminal_ops::CAPACITY`: enough that a user returning from lunch still finds what went wrong, bounded so a long batch session can't grow memory without limit. A restart clears the list, which is consistent with the rest of the manager's state and correct in kind — the operation log is where a failure lives permanently; this list only exists to make one visible right now.

### Naming a reversal on screen (`reverses`)

`OperationDescriptor::reverses` is `Some(original.kind)` on exactly one construction site, `rollback.rs`'s `spawn_managed_inverse`, and `None` everywhere else. It rides to `OperationSnapshot` (a static per-operation fact, so it belongs on the thin registry snapshot rather than the 200 ms tick) and is the only thing that tells a window "this running operation is an UNDO of a finished one, of THIS kind".

**Why the ORIGINAL kind and not the inverse.** `write_op_type(inverse_kind(kind))` is what the op RUNS as, and it can't say what the user gets: `Move` covers both undoing a move and undoing a trash, and `Delete` covers undoing a copy, a new file or folder, and a compress. The frontend feeds `reverses` to the same `OpKind` → variant map that worded the confirmation the user just answered, so the running bar cannot promise something different from the question (`apps/desktop/src/lib/file-operations/DETAILS.md` § "The running reversal is named from the SAME variant").

**Why not a phase.** `WriteOperationPhase::RollingBack` is NOT the signal: a cancelled copy cleaning up its own partials wears that phase too, and there the existing "Rolling back..." wording is honest, because it really is deleting. Reading the phase instead of this field is how a history reversal of a MOVE ends up telling someone their files are being deleted.

A retained failure keeps the field (`record_failure` copies it off the live record), so a reversal that stopped early still reads as a reversal on the row that explains why.

### Rollback availability (`supports_rollback`)

`OperationDescriptor::supports_rollback` says whether cancelling this op can also UNDO what it has written
(`cancel_write_operation(id, rollback = true)`), and it rides through to `OperationSnapshot` so the queue window can
offer Rollback on exactly the rows the progress dialog would. It's on the snapshot because the operation queue window is its
own webview: it never sees the source/destination volume ids the dialog decides from, and two surfaces disagreeing about
whether an operation is reversible is the kind of drift that ends with a button that lies.

Every construction site states its own verdict (a struct literal, so a new spawn path can't forget to decide):

- **`true`** — local copy/move via `start_write_operation` (`CopyTransaction` deletes the copies it made;
  `MoveTransaction` renames them back), and volume copy (`volume/cleanup.rs` deletes the destination copies).
- **`false`** — delete and trash (the files are already gone; there's nothing to put back); a SAME-volume move (a
  server-side rename-merge that stops without reversing); a CROSS-volume move, which copies and deletes the source per
  file and whose driver treats `RollingBack` exactly like `Stopped`, reporting `notRolledBack`; archive edits (an
  all-or-nothing temp+rename rewrite); instant metadata ops; and the operation-log rollback's own inverse op (rolling
  back a rollback would re-apply what the person just undid).

⚠️ The progress DIALOG doesn't read this flag yet: it decides from the volume ids it holds, disabling Rollback only for
a same-volume move. So it still offers Rollback on a cross-volume move, where the click only cancels. Pointing the
dialog at this flag (or teaching the cross-volume move driver to reverse) is the fix; until then, the operation queue window
is the honest one.

The flag is a property of the op's STRATEGY, so it never changes over the op's life. Whether Rollback is offered *right
now* is the UI's call on top of it: the queue row also requires a running/paused op that isn't already rolling back
(which it reads from the live `write-progress` phase, since rollback is an `OperationIntent`, not a `LifecycleStatus`).

### IPC

`list_operations` (the thin snapshot), `cancel_operation(id)`, `cancel_operations(ids)` (the queue window's "Cancel selected"), `pause_operation(id)` / `resume_operation(id)`, and `pause_all` / `resume_all`. Cancel routes through `cancel_operation`: a Queued op is dropped from the registry without ever spawning (`cancel_if_queued`); a Running/Paused op falls through to the existing `cancel_write_operation(id, rollback=false)` keep-partials path. Pause/resume flip BOTH the live `WriteOperationState` pause gate (so the driver parks) AND the manager record's `LifecycleStatus` (so the UI shows Paused), via `set_paused`, and both RETURN a `PauseOutcome` (`Applied` / `Deferred` / `NotApplicable`) that rides out through `bindings.ts` ([Pause / resume](#pause--resume)). Plus `dismiss_failed_operation(id)` / `dismiss_all_failed_operations()`, which drop retained failures and re-emit ([Retained failures](#retained-failures)). Registered in the `ipc.rs` manifest; `OperationSnapshot` / `LifecycleStatus` / `OperationsChanged` ride into `bindings.ts`. No capability change: manager commands go through the invoke handler, not the ACL.

### Pause / resume

The paused bit has TWO homes, kept in sync by the IPC layer: a `PauseGate` on `WriteOperationState` (the runtime gate the drivers honor) and the manager record's `LifecycleStatus::Paused` (what the UI sees in `operations-changed`). Pause is **orthogonal to `OperationIntent`** (which stays the cancel/rollback machine) — it never perturbs the validated `Running → RollingBack/Stopped` transitions — and it is **not a `WriteOperationPhase`** (a paused op may be mid-`Copying`).

- **`PauseGate`** (`operation_intent.rs`): a `paused: AtomicBool` plus a `std::sync::Condvar` (for the sync driver, which parks inside `spawn_blocking`) and a `tokio::sync::Notify` (for the async volume drivers). `pause()` sets the flag and opens the operation's human-wait clock; `resume()` clears the flag, closes the clock, and wakes both waiters; `wake()` wakes both WITHOUT clearing the flag (the cancel path uses it) but DOES close the clock — the operation is winding down, so nobody is being waited on any more, and a clock left open would make the rollback that follows measure no rate at all. `wait_while_paused_sync(&intent)` / `wait_while_paused_async(&intent).await` park while `paused && !cancelled` and return immediately on cancel.
- **Gate placement** (between-files boundaries, immediately AFTER the `is_cancelled` check so the data-safety ordering — cancel/skip before any destructive call — is preserved): both transfer drivers' per-source loop tops (`transfer_driver.rs`), and the delete-phase loops in both delete walkers (files then dirs, `delete/walker.rs`). The delete SCAN recursion is NOT gated (pausing mid-enumeration would freeze a half-counted "Scanning…"). The cross-volume streaming copy path ALSO parks BETWEEN CHUNKS via the `CheckpointStream` wrapper in `transfer/volume/strategy.rs` (the sync per-chunk `on_progress` callback can't `.await`, so the async stream decorator owns mid-file parking + a `yield_now`), so a paused single large file (e.g. MTP→local) stops mid-stream holding only its `.cmdr-tmp-<uuid>`. The local-FS sync chunk loop (`chunked_copy.rs`) still pauses only between files — it receives the cancel atom, not the `PauseGate`. Full rationale + scope: `transfer/DETAILS.md` § "Pause reaches between chunks".
- **Cancellation always wins over pause.** `cancel_write_operation` / `cancel_all_write_operations` flip the intent AND call `pause_gate.wake()`, so a paused, parked op unblocks, observes the non-`Running` intent, and bails through the existing keep-partials path (keeping already-copied files, deleting only the last partial). Without that wake a paused op parked on the condvar would never see the cancel.
- **A paused Running op keeps its lane slots** (`set_paused` never touches lanes), so a same-lane Queued op can't start and then fight it on resume. Resume runs NO admission pass (the op never freed its lanes). Pausing a Queued op is a v1 no-op (it isn't touching a device yet; it stays Queued and admits normally when its lanes free). Pinned by `manager::tests::{set_paused_flips_running_op_to_paused_and_keeps_its_lane, paused_running_op_does_not_admit_a_queued_same_lane_op}`.
- **The request reports what it did**, as a `PauseOutcome`: `Applied` (the record flipped), `Deferred` (a scan-waiting op, so the request is latched, see [The scan-wait](#the-scan-wait)), `AlreadyInState` (asked for what it already is, so the caller's intent holds and a retry isn't a refusal), `NotApplicable` (queued, over, or unknown — nothing changed and nothing is remembered). It travels the whole way out: `set_paused` → `pause_operation` / `resume_operation` → the IPC commands → `bindings.ts`. The MCP `queue` tool is the consumer that needs it, since an agent acts on the answer; the queue window ignores it and reads the live status from `operations-changed` instead.
- **A sweep reports counts, not a verdict.** `pause_all` / `resume_all` ask every eligible op, so they fold the per-op outcomes into a `PauseAllOutcome` (`applied` / `deferred` / `already_in_state` / `not_applicable`) and hand that out the same way, through the IPC commands into `bindings.ts`. "No operation was running", "three parked", and "one is still scanning" are three different situations, and a flat "OK" for all three is what sent an agent on believing a device had gone quiet. `took_effect_anywhere()` is the one derived question worth a name: `applied + deferred > 0`, since a latched pause HAS taken effect, just not yet. From a sweep, `not_applicable` can only be the settle race (the snapshot named an op that finished before the call landed) — a queued op never enters the walk. The fold is `impl FromIterator<PauseOutcome> for PauseAllOutcome`, which is what makes it testable without touching the process-global manager (`manager::tests::a_sweep_counts_every_outcome_it_collected`); ❌ never drive a real `pause_all()` from a test, it would park a sibling test's operation.
- **Concurrent copy path.** `copy_volumes_with_progress`'s `FuturesUnordered` path has no single between-files boundary, and its per-file `on_progress` callback stays cancel-only (pinned by `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`). But its in-flight files stream through the shared `stream_pipe_file`, so each parks between chunks via `CheckpointStream` when paused; the admission loop adds no new files while everyone is parked, so the batch effectively halts. Serial paths (local copy/move, cross-volume serial, delete) honor pause between files; the cross-volume paths additionally park between chunks. See `transfer/volume/DETAILS.md` § "Pause and the concurrent copy path".
- **Accepted resource asymmetry** (principle 5): `wait_while_paused_sync` parks the op's `spawn_blocking` pool thread for the whole pause — the same thing the deferred-start design avoids for *queued* ops. A paused Running op legitimately holds its lane and is rarer than queued ops, so v1 accepts this; many simultaneously-paused local ops could pressure the blocking pool. v2 may bound concurrent paused-and-parked ops if it proves real.
- **Connection-idle caveat** (document, don't fully solve in v1): a long pause holds SMB/MTP connections idle and may hit server/USB timeouts. v1 accepts that resume may surface a normal transient error (SMB already reconnects; MTP stale-handle has a one-shot retry). v2 adds keep-alive / explicit reconnect-on-resume.

### Existing single-op flow is unchanged

When nothing else touches the op's lanes (the common case), `spawn_managed` admits it on the registration's own admission pass — effectively an immediate spawn. The "register + return `operationId` immediately" contract holds: registration and the id return happen before any I/O, so the dialog opens even on a stalled mount. Every pre-existing write-op test passes through the manager path unchanged.

### Managed instant ops (`run_instant`)

Rename, make-folder, and make-file (`WriteOperationType::Rename` / `CreateFolder` / `CreateFile`) are **scan-free, near-instant, result-returning** metadata ops. They flow through `OperationManager::run_instant(descriptor, op)` instead of `spawn_managed`, so the "every write op goes through `spawn_managed`" framing above applies only to the streaming transfer/delete ops.

- **No lane, no admission queue — deliberate.** `run_instant` registers a `Running` record and marks its volumes busy, but reserves NO lane and runs NO admission pass. Lanes exist to stop two big *transfers* thrashing one device; a metadata syscall must never queue behind a multi-minute copy. An inline rename that hangs until its IPC timeout is worse than useless, and the MTP/SMB connection layer already serializes physical device access. It even ignores any lanes in its own descriptor (pinned by `manager::tests::run_instant_does_not_reserve_a_lane`). **Don't "clean this up" into `spawn_managed`** — that silently reintroduces lane-queuing for metadata syscalls, the regression this design forbids.
- **Runs inline and returns the op's result.** Unlike the fire-and-forget spawn path, `run_instant` awaits `op` inline and returns its `T` to the caller. The inline-rename editor and the new-file/new-folder dialogs need the result synchronously (new path for cursor placement / editor-open; conflict/timeout/success for the rename flow). The command layer wraps `run_instant` in its own IPC timeout; nothing inside spawns. Instant ops emit no `write-progress` / `write-complete` / `write-error` (the command return is the result channel) and no completion analytics (explicit no-op arms in `analytics::emit_completion_analytics`).
- **RAII cleanup on drop/panic is mandatory, not happy-path only.** The command wraps `run_instant` in a `tokio::time::timeout`, so a slow op that exceeds it makes the timeout **drop the `run_instant` future mid-`op.await`**; the async volume path can also panic. Either exit MUST still free the record AND unregister the busy status — else the eject guard sticks ON forever (the volume can never be ejected again) and a phantom `Running` row lingers. An `InstantTaskGuard` held across the `op.await` guarantees this: its `Drop` calls `free_and_remove` (record removal + `unregister_operation_status` → `recompute_and_emit_busy_volumes`) and re-emits `operations-changed`. The happy path calls `free_and_remove` + `emit_changed` explicitly, then `guard.disarm()`s so the Drop is a no-op. No admission pass on completion (instant ops reserve no lanes, so nothing waits on them). Pinned by `manager::tests::run_instant_releases_busy_and_record_when_{dropped_midflight,op_panics}`.
- **No `WriteOperationState`.** Instant ops have no intent/pause/conflict oneshot, so `run_instant` inserts none. Consequence: `cancel_operation` on an instant op is a safe no-op — `cancel_if_queued` is false for a Running op, then `cancel_write_operation` finds no state. Acceptable: instant ops finish before a human can cancel.
- **Queue surfacing.** They appear as a `Running` snapshot row that goes away almost immediately (the store prunes terminal/removed rows). A ~50 ms local rename may never render before it's pruned; a slow MTP rename shows a label + spinner with no progress bar (`fraction` is null). Local `root` ops cause NO busy-set churn (`root` is excluded), so inline-renaming local files won't flicker the eject menu; only volume ops mark busy.

## The scan-wait

A confirmed transfer is registered with the manager immediately, before its `TransferDialog` preview has finished
walking. That is what gives it an `operationId`, a queue row, its lanes, a busy-volume entry, and a place in the quit
gate from the moment the user confirms — the whole point, because before this a user could not background a scanning
transfer at all and the scan died with its dialog. The wait itself moved into the operation's own task
(`scan_bridge::await_claimed_preview`), which every deferred start calls first, BEFORE its journal open: an operation
that never got past its scan wrote nothing, so journaling one would record work that didn't happen.

**Claims, and why exactly one.** An operation claims its `preview_id` inside `spawn_managed`, before the record is even
inserted. The claim is what lets the progress bridge forward the walk's counts under the operation's id, what exempts a
settled result from TTL eviction while its owner sits Queued behind a busy lane (with `LANE_BUDGET = 1` that can be
well past five minutes), and what stops `cancel_scan_preview` from freeing a result an operation is about to read. A
SECOND operation naming the same id is refused and falls back to its own walk: `take_cached_scan_result` REMOVES what
it reads, so two claimants would race for one consumable result and the loser would silently get nothing. Not
hypothetical — the archive-password retry re-dispatches a new operation over the same sources, which is why
`dialog-state.svelte.ts` clears `previewId` on that path.

**The terminal outcome, not a completion pulse.** Both preview workers used to remove their in-flight state before
publishing a result, so an operation looking the preview up could find nothing and had four indistinguishable reasons:
complete-and-consumed, errored, cancelled, and never-existed. With `LANE_BUDGET = 1` "nothing there" is the common
case, not the rare one. `settle_preview` replaces the entry in one write with a `ScanOutcome`, readable afterwards.
⚠️ `Cancelled` comes from the worker's own cancel FLAG at its exit, ❌ never from which event fired: a genuinely
cancelled walk returns an error (the local walk's `on_cancelled` string, the volume path's
`"Scan failed: {VolumeError::Cancelled}"`), so classifying on the event would reach the operation as a failure whose
message merely says "cancelled", and recovering the truth from that message would be string-matching on the control
path. Both workers' arms were reconciled to match.

## Bounding the scan

A preview had no bound of any kind. That is survivable while a walk can only end, and it stops being survivable the
moment the volume under it can stop answering: a `stat` on a wedged kernel mount blocks until the mount is forced down,
observing no cancel flag and returning nothing, so the preview stayed in flight forever. The pre-confirm dialog spun on
`0 bytes / 0 files / 0 dirs`, and a transfer already confirmed waited with it, because it parks on the same preview.

**Why inactivity, not duration.** A legitimate scan of a large tree over SMB runs for minutes, so any total-duration cap
either cuts real work or is too generous to catch a wedge. What separates "slow but working" from "dead" is whether the
walk is still COUNTING: every entry a backend hands back is proof the far end answered. `ScanWatchdog::note_progress`
records that proof from both workers' progress paths (fed BEFORE the emit throttle on the volume path — the throttle is
a UI rate limit, not evidence about the device), and the watchdog fires only after `SCAN_INACTIVITY_LIMIT` (60 s) with
nothing counted.

**Why 60 s.** It sits above every bound the layers below own, so their better message wins whenever they have one: a
direct-SMB request gives up after ~50 s (20 s to the socket, then 30 s of server silence), and the IPC scan deadlines
are 30 s. What's left underneath is the case with no bound at all, and that's what this catches.

**Why the watchdog publishes.** It can't ask a wedged walk to stop and report, so it settles the preview
(`ScanOutcome::Error`) and emits `scan-preview-error` with `timed_out: true` itself, leaving the walk detached behind
it. It also sets the cancel flag as a courtesy: a walk that is merely slower than we believed reads it and stops working
for a dialog that has moved on. `claim_outcome` is the one-shot CAS both the watchdog and the worker pass through, so
exactly one publishes and a late walk can't contradict a timeout the user has already been shown. ❌ A new terminal path
in either worker owes that claim: skip it and two publishers race, which is the whole failure this CAS exists to make
unrepresentable.

**What it reaches.** The pre-confirm dialog renders an honest notice with a retry
(`src/lib/file-operations/transfer/DETAILS.md` § "When the dialog can't find out"). A CONFIRMED transfer's wait already
turns `ScanOutcome::Error` into that operation's failure (`await_claimed_preview`), so the same bound also ends the
post-confirm hang — with the watchdog's message, which is why that message is written for a person to read.

**The log is the other half.** `grep scan_preview` over the reporting user's multi-megabyte log returned nothing: the
module had two `debug!` calls, neither at scan start. Every preview now logs one INFO at start (sources + volume), a 5 s
DEBUG heartbeat with counts and time since the last one, and an INFO (or WARN, on a timeout) at the end, all under the
`scan_preview` target, so the next hang answers "did it start, did it progress, how did it end" without guessing.

**Testability.** `ScanPreviewEventSink` (mirroring `OperationEventSink`) keeps both workers off `tauri::AppHandle`, so
`scan_watchdog_tests.rs` can point a real walk at `test_support::WedgedVolume` — every future parks forever — and watch
the preview settle anyway. A network drop isn't repeatable; a volume that never answers is exactly repeatable and
reaches the same code.

**Trash is the one operation that doesn't wait.** `trashItemAtURL` is atomic per top-level item, so a trash walks
nothing: there is no second walk to serialize against and no cached result to consume, and waiting would be pure delay
— a long one on a big tree. `trash_files_start` therefore passes no `preview_id` and frees the preview outright,
because nothing downstream will read it and the dialog deliberately skips its own cleanup after a confirm (on the
DELETE path the operation DOES consume it). Pinned by `a_trash_frees_its_preview_instead_of_waiting_on_it`.

**Misses are always a re-walk, never a hang.** An unknown `preview_id` (evicted, or stale from a reloaded window) and a
refused claim both leave the operation with no claim at all, so it proceeds straight to its own foolproof scan.

**The progress bridge.** `scan-preview-progress` is keyed by `previewId` and carries no `operationId`, and nothing else
emits for an operation that is only waiting, so without a bridge every scan-phase surface would render zeros rather
than live counts. Each preview tick for a CLAIMED preview is republished as the owner's `write-progress` in
`phase: 'scanning'` (`scan_bridge::forward_scan_progress`), alongside — never instead of — the preview event, since a
pre-confirm dialog may still be watching the same preview by id. `files_total` / `bytes_total` stay 0 through the scan;
the index expectation rides `expected_*`, which every surface already treats as a hint.

⚠️ **The opening tick's ORDERING is load-bearing.** `row.progress` must stop being `null` immediately rather than at
the next `progress_interval_ms` boundary, which on a preview near its end may never arrive. But `spawn_managed` inserts
the record, runs admission, and only THEN calls `emit_changed()`, while the frontend store's `applyProgress`
early-returns for an id with no snapshot row yet. A tick that beats its own `operations-changed` is discarded and the
row stays blank — exactly the case the tick exists for. So `emit_initial_scan_tick` fires AFTER `emit_changed`, for a
`Queued` row as much as a `Running` one (the queued row renders the scan line too, and its task may not spawn for
minutes). `scan_bridge_tests` asserts the ordering against the manager's broadcast counter, not merely the tick's
existence.

**Pause is refused, and the refusal is LATCHED.** `set_paused` flips any `Running` record, and a scan-waiting operation
is `Running`, so without a rule the snapshot would say `Paused`, the dialog title would say "Paused", and the walk
would carry on at full speed — while `set_paused` deliberately keeps the lane slots, so a "paused" scan would hold its
lane indefinitely doing nothing. The refusal is one match arm on the record's `in_scan_wait` flag, and it is
observable everywhere it matters: no surface flips optimistically, so a deferred pause shows as "the status stayed
`running`, the button still says Pause". It reports itself as `PauseOutcome::Deferred`, which is what lets the MCP
`queue` tool answer "it pauses the moment it starts writing" rather than either lie.

The latch is the part that would otherwise ship as a real defect. `pause_all` walks `running_ids()` calling
`pause_operation`, which sets the driver's park gate only on `PauseOutcome::Applied` — so a bare refusal would drop the
request on the floor, and minutes later that one operation starts writing at full speed while every other operation is
paused and the user believes the device is free. The refusing arm records the request on the record, and
`end_scan_wait` applies it the moment the wait ends. Withdrawing (a resume during the scan) clears it the same way.
Nothing surfaces the PENDING pause: the row keeps saying "Running", then flips to "Paused" on its own when the write
would have begun. Showing "pause pending" would mean a new snapshot field and a new string, and the harm being fixed is
the silent full-speed write, not the surprise. Revisit if the delayed flip confuses anyone.

**Real parking was rejected**, and the volume path settles it rather than taste: a local walk could poll a pause flag
per entry the way it polls `cancelled`, but a volume scan sits inside `scan_for_copy_batch_with_progress` for a whole
batch, so there is no park point, and on MTP the batch can be the entire scan. Pausing would therefore work on the
volume kind that needs it least.

**Two leaks the wait creates, and their hooks.** (1) Every exit from the scan-wait has to reach `on_settled` or the row
leaks and its lane stays reserved; the operation's task owns that, not the detached scan worker, and `free_and_remove`
also abandons any claim still held (the panic / quit-drop net). (2) `cancel_if_queued` removes a Queued record WITHOUT
ever running its `DeferredStart`, which is where the wait and its cleanup live — so it calls `abandon_claim` explicitly.
A `Queued` op cancelled before admission is the ordinary case on a busy lane, and without the hook its walk would keep
going for an operation that no longer exists while its result sat until a TTL sweep. These are separate leaks and no
single test catches both.

**The lanes are reserved from confirm**, and this is the one genuinely contested decision here. For: with
`LANE_BUDGET = 1` a transfer confirmed while another runs on the same lane is admitted as `Queued` and the existing
auto-queue path backgrounds it with no new code; and admission is oldest-first, so operations run in the order the user
confirmed them, which they did NOT before (A dispatched only after its scan, so B, confirmed second with a finished
scan, took the lane first). Against: an operation scanning for three minutes holds a lane it is not writing to, so a
destination device can sit idle. Matching confirm order is a correctness property and better utilization is an
optimization, so reserve from confirm. Two-phase reservation (scan without lanes, request them at write start) costs
the invariant that `Running` means "admitted and holding its lanes" and adds a second admission point that can leak a
lane: reach for it only if the idle-device cost shows up in practice.

**Two behavior changes worth naming.** Eject is disabled from confirm rather than from first byte (the operation is
committed, so that is right, but it is new). And an operation confirmed onto a busy lane is `Queued` from registration,
so the progress dialog's one-shot `listOperations()` seed sees `queued` immediately and the auto-queue path mounts and
unmounts the modal within a frame or two, with a toast and a queue window. That flash is not new, only more frequent;
fixing it properly means moving dispatch out of the dialog.

**Archive routes await but do not reuse.** `copy_into.rs` plans its changeset with its own `WalkDir` and never calls
`take_cached_scan_result`, so `preview_id` is threaded through `route_archive_copy_into`, `compress_start`, and
`route_archive_delete` for the WAIT alone: it restores the serialization the frontend's scan-wait used to provide.
Without it, ⌥F5's sampling preview and the planner's walk would run down the same tree at once, which is what costs on
MTP and SMB. The duplicate walk itself is pre-existing and out of scope; teaching the archive changeset planner to seed
from a `CachedScanResult` is real work with its own correctness questions.

**Testing the scanning window.** E2E fixture trees are deliberately tiny and `data-scan-state` signals "counting done",
the opposite of what a scanning-phase test needs to hold, so `set_test_scan_preview_delay` (an IPC override behind the
`playwright-e2e` feature, falling back to `CMDR_E2E_SCAN_PREVIEW_DELAY_MS`) holds every worker at its starting line for
a set number of milliseconds. Per-test rather than per-process, so one spec's window doesn't slow the whole run.

## Routing a transfer

`routing.rs` answers the two questions every cross-volume transfer asks before it starts, and owns the fork it takes from the answers. It exists because that whole shape used to live inside the three `#[tauri::command]` bodies in `commands/file_system/volume_copy.rs`, which build a `TauriEventSink` at the edge — so it was reachable only from a window, and a backend caller (an approved suggestion group starting an op with its own injected sink) could reach it only by duplicating it.

- **`resolve_source_volume(volume_id, first_path)`** routes an archive-inner batch to its `ArchiveVolume`. One `source_volume_id` per batch means no straddle risk, so the first path decides. The returned `bool` is "inside an archive": a `.zip` FILE is a plain file, copied through its parent volume, so only a genuinely-inner path flips it. A cheap string pre-filter (`archive_boundary_candidate`) keeps a plain path off the parent-aware resolve entirely.
- **`resolve_dest_path(dest_volume, dest_path)`** expands `~` for LOCAL volumes only (on a share, `~` is an ordinary folder name) and then root-anchors. The transfer dialog's box is volume-relative (`/photos`, with the volume in a dropdown beside it) while a pane sends the absolute path it displays; `root_anchored` is idempotent, so neither caller has to know which it holds. Skipping it is what made a 360 GB move into an SMB subfolder fail instantly (ERR-XCP5Q): `SmbVolume` read `/photos` as a path outside its mount and answered `NotFound`.
- **`start_volume_copy` / `start_volume_move` / `start_volume_compress`** are the three entry points. Between them they cover the plain cross-volume engine, copy/move INTO a zip (one `{ add }` changeset), move OUT of one (the compound extract-then-delete op), and compress (seed an empty archive, then copy into it). Each takes an injected `Arc<dyn OperationEventSink>`; the IPC commands build the sink and pass it in, nothing more.

**A route that cannot hold a binding refuses the transfer rather than running it unbound.** The archive-changeset routes (copy or move INTO a zip, move OUT of one) plan their work from their own `WalkDir` instead of the per-source engine, so they have nowhere to apply an `ExpectedSources`. Dropping one silently would be the exact failure a binding exists to prevent, and nothing downstream would ever say so, which is why `route_cannot_hold_a_binding` turns it into an error the caller sees. Extract itself is unaffected: it resolves to an `ArchiveVolume` SOURCE and runs through `copy_between_volumes`, which is bound.

**Decision: an extract has no operation type of its own.**
**Why**: pulling entries out of a `.zip` is a copy whose SOURCE volume happens to be an `ArchiveVolume`, which `resolve_source_volume` arranges. `ArchiveSubkind::Extract` is an operation-log label describing what a copy was for, not a second driver. A `WriteOperationType::Extract` would need its own lane rules, its own conflict handling, and its own progress accounting, all of which already exist and already work.

**Decision: a compress over an existing archive overwrites it, and this module does not refuse.**
**Why**: the seed is unconditional and the prior bytes aren't retained, which makes it the one transfer here that can't be reversed at all. That is a fact to DISCLOSE before someone commits to it (the review surface's job), never a refusal in the engine: if a person can do it from the dialog, an operation they approved does the same thing. Same rule as § "Binding the sources".

## Binding the sources

`source_binding.rs`. An operation decided against files as they looked at review time can execute much later — an approved op sits queued behind a forty-minute copy on the same lane, and a suggestion waits until somebody clicks Approve. In between, a source can be edited, replaced, or swapped for a different file under the same name.

**`SourceFingerprint` is what lets an operation notice.** `Local { device, inode, content }` uses the identity the kernel maintains; `Remote { normalized_path, content }` stands in for it where no inode exists (SMB, MTP, an archive). Comparison is derived equality against a fresh capture, so a caller never writes the comparison itself.

**Decision: `LocalContent` / `RemoteContent` are `File { size, modified }` or `Directory`, not `Option`s.**
**Why**: a directory's own size and mtime move with every child write, so binding a proposed `delete ~/projects/cmdr/target/` to yesterday's directory mtime would refuse it after any build — while `(device, inode)` plus "still a directory" is a real identity worth holding it to. Two variants also make the two reasons a field could be empty impossible to confuse: "it's a folder, so there are no bytes" and "the backend didn't report a size" are different facts, and an `Option<u64>` doing both jobs is the same anti-pattern as `PerSource`'s empty `Vec` (§ "Key patterns", the `scan_sources_internal` decision). A directory that became a file mismatches on the variant alone.

**Decision: the EXPECTATION picks the namespace a source is checked in, not the call site.**
**Why**: one entry point can take two routes. `copy_between_volumes` hands a both-local transfer to `copy_files_start` and everything else to the Volume engine, and a caller that fingerprinted its sources a week ago cannot know which route the operation takes when its turn finally comes. Dispatching on the variant the caller actually holds keeps one binding correct on both routes, and leaves the capture rule a single sentence: **fingerprint with `capture_local` when the source volume has a `local_path()`, and with `capture_remote` when it does not.** The asymmetry that remains is honest: a `Local` expectation is settleable anywhere, while a `Remote` one needs the backend that owns it, so the sync local-only path drops it rather than guessing.

**The pre-flight runs INSIDE the operation's own task, not at approval.** `retain_bound_sources` (local, sync), `retain_bound_sources_remote` (Volume-backed, async), and `retain_bound_sources_with_sizes` (trash, whose `item_sizes` is positional against the sources and has to be filtered in lockstep) sit at the top of the four starters' handlers, after admission. The whole point of a fingerprint is to close the window between checking and acting; checking at approval and acting an hour later would reopen it as wide as it goes.

- **Binding is all-or-nothing.** A source the binding doesn't name is dropped, not waved through: a caller that supplies a partial map has a bug, and guessing "probably fine" means acting on a file nobody reviewed. A caller that wants no checking passes no `ExpectedSources` at all rather than an empty one.
- **An operation the binding empties completes; it does not fail.** Every source already went out as a `Skipped` item, so the caller has its per-source answers, and a failure dialog on top would be the engine editorializing about a decision the person is entitled to make differently.
- **`journal_snapshot()` lives on the fingerprint** because the journal's mtime column is Unix SECONDS while a local fingerprint holds nanoseconds. Journal the nanoseconds and every undo reports drift and refuses (silently disabling undo); journal nothing and identity rests on size alone, so a same-size replacement gets renamed back in place of the original. A directory answers `(None, None)`: it has no bytes of its own to be held to.

**Nothing in the engine asks who started an operation**, and a test holds it there. `expected_sources` is an `Option` a caller may supply; an unbound operation gets its sources back untouched, which is every user-started copy, move, delete, and trash. `approved_op_parity_tests.rs` pins the consequence: the same transfer run bound and unbound agrees on the destination tree, the completion counters, and every per-source event — a missing destination folder is created either way, an Overwrite overwrites either way, a Skip skips in the same place either way. `only_a_source_that_changed_is_treated_differently` states the one permitted difference positively, so the parity cases can't be read as "the binding does nothing".

## Per-source outcomes

`write-source-item-done` carries a `SourceItemOutcome` (`Done` / `Skipped` / `Failed`) beside `source_removed`.

**Decision: the outcome rides on that event rather than a sibling one.**
**Why**: "this source is finished with" and "how it ended" are one fact. Split across two events, every consumer wanting a per-source verdict — the queue's gradual deselection, the search-snapshot purge, the suggestion store writing `proposal_ops.status` — would have to join two streams and decide what a missing partner means.

**The LAST event a source gets is the verdict**, and both directions are pinned. A cross-filesystem move speaks twice for one source: `Done` when it finishes staging, then `Done` again when the source-delete phase removes it, or `Skipped` when the rename phase left it standing. Staging succeeding says nothing about where the item ended up, so a consumer recording a per-source status OVERWRITES rather than accumulating. `move_op_tests.rs`'s `a_cross_fs_move_that_{skipped_the_item_ends_on_skipped_not_done, took_the_item_ends_on_done}` pin both directions, so the rule can't degrade into "the second event is always a skip".

⚠️ **`source_removed` is a separate question from the outcome**, and it is the one the search-snapshot purge steers by (`apps/desktop/src/lib/search/snapshot-purge.ts`). A source skipped BECAUSE it vanished under the operation reports `Skipped` AND `source_removed: true`; a source that merely couldn't be stat'd reports `false`, because "we couldn't look" is not evidence a file is gone and a wrong `true` drops a row for a file the user can still open.

Where each non-`Done` outcome comes from — every one a place the engine already knew and used to say nothing:

- **The binding's pre-flight** (§ above) reports `Skipped` per dropped source, for all four verbs.
- **Trash reports `Failed` per item**, on both failure paths (a source it can't stat, and `trashItemAtURL` refusing). Trash is per-item by nature: one failure leaves the batch running, so the operation's own terminal event says nothing about that item.
- **A bulk-skipped copy source reports `Skipped`.** Those leave `scan_result.files` before `SourceItemTracker` is even built, so nothing downstream would otherwise speak for them.
- **A cross-FS move's rename-phase skip reports `Skipped`** in phase 4, which is what makes the last-event rule load-bearing rather than decorative.

The injected sink is the seam: a caller that wants per-source statuses wraps the sink it passes in, and the engine never names the `agent` module at all. That boundary is held by the `write-ops-isolation` check rather than by a rule here, so it fails a build instead of costing every session a line it might ignore.

## Archive edits

Editing a `.zip` (mkdir/mkfile/rename/delete inside, or copy/move INTO one) is an O(archive) temp+rename rewrite that
runs as a managed op, driven by the `archive_edit/` module: `archive_edit/CLAUDE.md` and its `DETAILS.md`.


## Busy-volumes set

Drives "disable Eject while an op reads from / writes to this device" so a disconnect can't truncate an in-flight file. Lives in `status_cache.rs`, alongside the cache it derives from: `recompute_and_emit_busy_volumes` reads that module's own `OPERATION_STATUS_CACHE`, and register / unregister are the only two places membership can change, so the two can't be separated without putting a private static on the wrong side of a module boundary.

The same two register/unregister functions also feed `crate::priority::transfers` (the per-volume "a transfer is
running" gauge indexing yields to — transfers trump indexing). ONE lifecycle choke point on purpose: a new op kind that
guards eject automatically also outranks indexing, and the panic-safe unregister paths below cover both signals. Don't
add a second feed site (see `../../priority/CLAUDE.md`).

- The manager registers an op's volume IDs busy (`register_operation_status(op_id, type, volume_ids)`) **only when it admits the op (Running)** — a Queued op isn't touching the device, so it marks nothing busy. Source **and** destination go in (a download from a phone is as corruptible as an upload to it). The manager's `on_settled` / `ManagedTaskGuard` Drop unregisters on every exit (including panic), so a finished or panicking op can't leave a volume stuck busy.
- The busy set is the union of every Running op's `volume_ids` **∪ external registrations**, minus `root` (never ejectable). `recompute_and_emit_busy_volumes` fires `volumes-busy-changed` only when membership changes — progress ticks don't churn it (`LAST_EMITTED_BUSY`). Membership-by-union means two concurrent transfers to one device keep it busy until both finish, with no manual refcount.
- **Where `volume_ids` come from**: the `OperationDescriptor` each spawn site hands the manager. The cross-volume entry points (`copy_between_volumes`, `move_between_volumes`, `move_within_same_volume`) and the volume-aware delete carry the IDs; the both-local branch of `copy_between_volumes` (a local→USB / DMG copy) passes both IDs through `copy_files_start` / `move_files_start` so the ejectable destination is still marked. The plain `copy_files` / `move_files` / `trash` commands pass an empty list — the unified transfer dialog only routes through them for same-`root` ops, where no ejectable volume is involved.
- **Consumers**: `busy_volume_ids()` backs the `get_busy_volume_ids` bootstrap command, the `eject_volume` server-side guard (refuses a busy volume — the real safety net, since the picker's disable is only UX), and the native breadcrumb-menu builder (renders the Eject item disabled with a ` (busy)` suffix). The frontend `volume-busy-store.svelte.ts` subscribes to `volumes-busy-changed` and exposes `isVolumeBusy(id)` to disable the picker's eject controls. `init_busy_volume_emitter(app)` wires the emitter at startup (`lib.rs`).
- **External (non-write-op) seam**: the drag-out file-promise fulfillment service (`native_drag::fulfillment`) marks the source volume busy while it streams a promise to a Finder destination, but it isn't a real write op (no `WRITE_OPERATION_STATE`, no progress events, no settle). The `pub(crate)` `register_external_volume_op(op_id, volume_ids)` / `release_external_volume_op(op_id)` pair (in `status_cache.rs`, surfaced through `state::` and re-exported from `mod.rs`) is the seam: it touches only the `OPERATION_STATUS_CACHE` half that `recompute_and_emit_busy_volumes` reads, registering under `WriteOperationType::Copy` (the type only affects `list_active_operations` diagnostics; the busy set is type-agnostic). The fulfillment side wraps it in an RAII guard so release fires on every exit path.

## Settle contract

`write-settled` fires exactly once per operation, after the spawned background task has fully torn down — including in-flight USB / network teardown that may briefly outlive the `write-cancelled` emit. The FE uses it to gate the "Cancelling…" dialog close so the user can't dispatch a new op against a still-tearing-down volume (the wedge mode that cancel propagation already shortens but doesn't eliminate).

**Ordering**: `write-settled` always fires AFTER the terminal outcome event (`write-complete` / `write-cancelled` / `write-error`) for the same `operation_id`. The BE guarantees this by placing the settle emit in a `WriteSettledGuard` RAII struct whose `Drop` runs at the very end of the spawn-task scope, AFTER all the conditional terminal-event emits.

**Guard pattern**: every op's deferred start (the future the manager spawns from each of the five entry points) constructs a `WriteSettledGuard` at the top, from the same injected `Arc<dyn OperationEventSink>` the rest of the op emits through. The guard's `Drop` impl calls `sink.emit_settled(...)`. This makes the emit panic-safe: even if the op body panics and the task exits via `JoinError`, the guard still drops during stack unwinding, so the FE never hangs waiting for a settle that never comes. `emit_settled` is a required `OperationEventSink` method (no default no-op), so a new sink can't silently swallow settle. See `settle_event_tests.rs::settled_fires_on_panic_unwind` for the safety-net pin.

**Cache-cleanup panic safety**: removal from `WRITE_OPERATION_STATE` + `OPERATION_STATUS_CACHE` must survive a panic, or the op lingers forever in `list_active_operations`. The manager owns this: `on_settled` removes both maps on the happy path, and the `ManagedTaskGuard` Drop (held by every spawned task, declared so it drops AFTER the `WriteSettledGuard`'s scope cleanup runs but frees caches before the settle emit) does it on unwind. The guard NEVER spawns in Drop — see [Operation manager](#operation-manager) § "Dequeue on settle". Pinned by `manager::tests::panicking_op_releases_its_lane_without_spawning_next`.

**Payload**: `{ operationId: String, operationType, volumeId: Option<String> }`. The `volume_id` is best-effort: filled with the source volume's display name for volume-aware ops (copy/move between volumes, volume delete), `None` for pure local-FS operations. The FE currently filters only by `operationId`; `volume_id` is for diagnostics and forward compatibility.

**Tests**: `settle_event_tests.rs` pins the guard's invariants (single fire, panic safety, ordering relative to the terminal event). `delete/volume_cancel_tests::volume_*_emits_write_settled_event` pin the integration shape against the volume-delete handler.

## Key decisions (shared)

**Decision**: Copy and cross-FS move pre-flight a destination per-file-size limit (FAT32's 4 GiB cap) right after the scan, before the first byte. `validation::validate_file_sizes_for_filesystem` classifies the destination via `crate::file_system::filesystem_kind` (macOS `statfs.f_fstypename` / Linux `/proc/mounts` → `FilesystemKind` → `MaxFileSize`) and, only when the cap is `Limited`, fails the whole operation with `WriteOperationError::FilesTooLargeForFilesystem` (up to 10 offenders, largest first, plus the true count).
**Why**: A FAT32 USB stick silently failed a 5 GB copy ~4 GB in. The gate is all-or-nothing and runs alongside the free-space check (`copy/mod.rs`, `transfer/move_op/cross_fs.rs`). It blocks **only** when certain: `Unlimited` (APFS/exFAT/NTFS/ext4/MTP) and `Unknown` (OS-mounted SMB, unrecognized) never block, so a false positive — worse than the mid-copy failure because it stops a copy that would have succeeded — can't happen. **exFAT must stay `Unlimited`** (it's the common big-USB format with no 4 GiB cap); only FAT32 (`msdos`/`vfat`) is `Limited`. Same-FS moves rename in place and never reach the gate. The kind → cap map in `filesystem_kind::FilesystemKind::max_file_size` is the single source of truth (the write guard, the error prose, and any future volume-picker display all read it). SMB FileSystemName detection (a `smb2`-crate `FileFsAttributeInformation` query) and the volume-picker filesystem display are scoped follow-ups.

**Decision**: Every scan reports **two** byte totals — `total_bytes` (write footprint, un-dedup'd) and `dedup_bytes` (`du`-equivalent, each inode once). Delete consumes `dedup_bytes`; copy/move consume `total_bytes`; the Copy dialog shows both.
**Why**: A hardlink contributes differently to the two operations. **Delete** frees an inode only when its last link is removed, so the bytes-freed number is the dedup'd one — counting every link would claim to free 80 GB when only 60 GB (cargo `target/`) actually frees. **Copy/move** materialize every hardlink as an independent file at the destination (hardlinks don't survive a cross-volume copy, and even a same-FS `cp` doesn't relink), so the bytes-written number — and the disk-space reservation — is the full write footprint. The earlier single-`total_bytes`-is-dedup'd design got delete right but silently regressed copy: the space check under-reserved (risking ENOSPC mid-copy) and the bar hit 100% early. Now `walk_dir_recursive` / `walk_cached_entries` / `scan_volume_recursive` / `LocalPosixVolume::scan_for_copy` / `scan_subtree_with_oracle` all track both, using a `seen_inodes: HashSet<u64>` (mirrors `indexing/scanner/mod.rs`, `nlink == 1` fast path, operation-scoped across source roots; **Unix-only**, where non-Unix has no `nlink()` so `dedup_bytes == total_bytes`). Volume backends populate `FileEntry::inode` only for `LocalPosixVolume` files with `nlink > 1` (MTP/SMB/InMemory leave it `None`, so dedup is a no-op and the two totals are equal). The **scan-phase** progress bar reports the dedup'd running total (it's compared against the indexer's inode-dedup'd `dir_stats` estimate, so reporting the write footprint would overshoot 100% on hardlink trees). The **delete** active phase sums per-entry `progress_bytes`/`VolumeDeleteEntry::progress_bytes` (= dedup'd) against the `dedup_bytes` denominator. The **copy** active phase credits full per-file `size` against the `total_bytes` denominator (no chunk scaling). The Copy dialog surfaces the gap with a one-line note ("X will be written; source is Y; the extra is hardlinked files…") via `dedup_bytes_total` on the scan-preview events — copy-only, since a same-FS move writes nothing. Pinned by `delete/hardlink_progress_tests.rs`, `delete/volume_hardlink_progress_tests.rs`, `transfer/hardlink_progress_tests.rs::copy_counts_write_footprint_for_hardlinks`, `scan.rs::tests::walker_dedupes_*`, `local_posix_test::test_scan_for_copy_dedupes_hardlinks_for_source_size_only`, and `transfer-dialog-utils.test.ts::shouldShowHardlinkNote`.

**Decision**: `WriteProgressEvent::with_scan_meta` is the only path that sets the scan-only fields (`current_dir`, `dirs_done`, `expected_files_total`, `expected_bytes_total`).
**Why**: 20+ emit sites construct `WriteProgressEvent` literals for active-phase events. Adding four optional fields to the struct would force every site to spell out their defaults, pure mechanical noise. The `new(...)` constructor takes the eight core counter fields and defaults the scan meta (`None` / `0`); the scan emit sites in `scan.rs`, `scan_preview.rs`, and `delete/walker.rs::scan_volume_recursive` opt in via `.with_scan_meta(current_dir, dirs_done, expected)`. Future scan-related fields go through the same builder. If a real refactor of the 20 literals to `new(...)` ever happens, the builder pattern still composes cleanly on top.

**Decision**: All write operations go through `OperationEventSink` instead of `tauri::AppHandle`, and the sink is constructed **only at the IPC edge** (`commands/file_system/write_ops.rs` + `commands/file_system/volume_copy.rs`), then injected all the way down.
**Why**: Decouples the copy/move/delete/trash orchestration from the Tauri framework. `TauriEventSink` wraps AppHandle for production; `CollectorEventSink` stores events for test assertions. The whole managed layer — `start_write_operation`, the four starters, the volume entry points (`copy_between_volumes` / `move_between_volumes` / `move_within_same_volume`), every `*_with_progress` function, and `WriteSettledGuard` — takes `&dyn OperationEventSink` / `Arc<dyn OperationEventSink>`, never an `AppHandle`. Each command builds `Arc::new(TauriEventSink::new(app))` once and passes it in (grep confirms zero `TauriEventSink::new` under `write_operations/`). This lets the full pipeline (multi-file copy, cancellation, conflict resolution, progress, the managed spawn path, and settle) run end-to-end under a `CollectorEventSink` with no Tauri runtime — see `tests.rs::injected_sink_receives_complete_and_settled_for_local_copy` and the trash unit tests (`delete/trash.rs::tests::trash_*_via_sink`). `state.emit_progress_via_sink` is the only progress-emit method — `emit_progress_via_app` is gone. The write-error safety-net arms in each deferred also route through `sink.emit_error(...)` rather than a string-named `app.emit("write-error", ...)`.

**Decision**: Scan preview reuses watched listings (the "fresh-listing oracle").
**Why**: Pre-flight scans for copy/move on MTP (and to a lesser degree SMB and big local trees) used to duplicate work the backend already had in `LISTING_CACHE`. Selecting 135 photos in a watched `/DCIM/Camera` (~15k entries) and pressing F5 would re-list the parent dir over USB just to look up size by name — ~17 s of "Verifying before copy…" while the listing was already fresh on the pane behind the dialog. `run_volume_scan_preview` now groups input sources by parent dir and consults `try_get_authoritative_listing(volume_id, parent)` first. On hit, sizes and `is_directory` flags come from the cached `FileEntry` for top-level files; top-level directories recurse via `scan_subtree_with_oracle`, which re-applies the oracle at every level (so a subfolder open in another pane also short-circuits). On miss, the call falls through to `volume.scan_for_copy_batch_with_progress(paths_in_group, ...)` — same code path as before — so MTP's parent-grouping and SMB's pipelined-stat optimizations still run for cold-cache parents. The local-FS walker (`walk_dir_recursive` in `scan.rs`) also takes an oracle check at the top of each recursive call, with `volume_id = "root"` plumbed through from `scan_sources_internal` and `run_scan_preview`. The freshness contract is bright-line at the watcher boundary: no "5 seconds is fresh enough" TTL, just "the volume's `listing_watch_coverage(path)` returned `EveryWriter`." See `file_system/listing/caching.rs::try_get_authoritative_listing` for the per-backend debounce windows that contract tolerates.

**Decision**: Copy and move are durable before they report complete: per-file `sync_data` (fdatasync) in chunked copy, plus an end-of-op targeted `fdatasync` pass over the transaction's recorded destinations for the strategies that don't flush themselves. Delete and trash don't sync at all.
**Why**: "Complete" must mean "durable on disk," not "buffered in the OS page cache." Without it, a user who copies to a USB stick / SD card and ejects (or the machine sleeps) right after "Copy finished" loses the file — and on a move it's gone from both source and dest. The flush is targeted, not a whole-machine `libc::sync()`: that global sync also stalled unrelated apps (AGENTS.md principle #5). The mechanism: (1) `transfer/chunked_copy.rs` calls `dst_file.sync_data()` per file, so each file is durable as it completes — a crash mid-batch on a long transfer leaves earlier files safe. (2) Before emitting `write-complete`, `durability::flush_created_destinations` emits a `Flushing`-phase progress event, then `fdatasync`s every recorded destination that wasn't already flushed, plus a best-effort `fsync` of each distinct parent directory so the rename-into-place (temp+rename / cross-FS staging) is durable too. It reuses `CopyTransaction.created_files` (no parallel dest-tracking) and skips an `already_synced: HashSet` of paths the strategy already made durable: chunked-synced files and APFS-clonefile / reflink dests (those share copy-on-write extents with the source, so a flush is moot). On macOS every produced-bytes path is either clonefile (moot) or chunked (already synced), so the end-of-op pass does no extra `fdatasync` there — its job on macOS is purely the honest `Flushing` UI state; on Linux it's the real flush for `copy_file_range` dests. Cross-FS move flushes the FINAL paths (Phase 3 renames staging → destination, so the staging entries in `created_files` are remapped to their final prefix before the pass — this also covers the Phase-3 `throwaway_tx` renames that aren't in the real transaction). Same-FS move (pure rename) writes no data at all, so it takes the sibling pass `flush_touched_directories`: one `fsync` per DIRECTORY the renames touched (each source parent and each destination parent), and none on the moved files — syncing those would cost an `F_FULLFSYNC` device barrier each for bytes nothing wrote. `transfer/DETAILS.md` § "Durability (flush before reporting complete)" has the measurements. The flush is best-effort on error: a failed `sync_data` is logged (`target: "write_durability"`), not propagated — the bytes are written either way and failing the whole op at the final flush is worse UX. Pinned by `transfer/copy_tests.rs::local_copy_emits_flushing_phase_before_complete` and `transfer/move_op/move_progress_tests.rs::cross_fs_local_move_emits_flushing_phase_before_complete`; FE label by `TransferProgressDialog.flushing.test.ts`. **Cross-volume copy/move landing on a local disk** (MTP → Local, SMB → Local, USB import) doesn't go through this local-FS engine — it flows through `LocalPosixVolume::write_from_stream`, which keeps the same promise by `sync_data`-ing each file (plus a best-effort parent-dir fsync for the directory entry) before it returns, so each file is durable as it completes. That path doesn't yet emit the `Flushing` UI phase (the volume copy/move handlers don't call `flush_created_destinations`); a follow-up could route them through the end-of-op pass for UI consistency, but the per-file `sync_data` already makes them durable.

**Decision**: `types.rs` is the floor of `write_operations` and imports no sibling. `state.rs` keeps its `operation_intent` + `scan_cache` + `status_cache` re-export facade, and `mod.rs` keeps its `transfer::*` + `delete::*` one.
**Why**: See § "Why `types` imports nothing" for the floor. The two surviving facades sit ABOVE the floor and point down, so neither can close a circle: `state` and `mod.rs` already depend on everything they re-export. Both front a broad name surface (`operation_intent` at ~35 sites across ~20 files, every cancellation check; the `scan_cache` types across `scan.rs`, `scan_preview.rs`, `validation.rs`, and two test files), which is a legitimate shape for a facade that costs nothing structurally.

## Shared gotchas

**Gotcha**: On macOS, never use `statvfs` alone for disk space checks; use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS will reclaim on demand. Using `statvfs` causes the "insufficient space" error to reject copies that would actually succeed, and shows a different available-space number than the status bar (which uses the NSURL API). `validate_disk_space` in `validation.rs` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

**Gotcha**: Volume-side `on_progress` callbacks report counts LOCAL to the current scan operation, not cumulative.
**Why**: `Volume::scan_for_copy_batch_with_progress` and `scan_subtree_with_oracle` both invoke `on_progress(count)` with a count local to the current `list_directory` call / subtree (starts at 1 each time). Forwarding that unchanged through `run_volume_scan_preview`'s closure made the FE's running tally drop visibly between parent groups, between sibling top-level dirs in a cache-hit branch, and between recursion frames inside `scan_subtree_with_oracle`. `run_oracle_aware_batch_scan` now wraps `on_progress` with a `baseline = aggregate.file_count` shift before each scan call (cold-cache batch + cache-hit subtree), and `scan_subtree_with_oracle` does the same at its own recursion site (`baseline = totals.file_count`). The visible FE count stays cumulative across the entire scan. Direct `on_progress(aggregate.file_count)` emit sites in `run_oracle_aware_batch_scan` (cache-hit per-file paths, fallthrough `scan_for_copy` after a name miss) stay unwrapped — they're already cumulative. Future scan call sites that delegate to a volume backend or to `scan_subtree_with_oracle` need the same baseline wrap.

**Gotcha**: Copy's bar/space-check use the write footprint (`total_bytes`), not the dedup'd source size — by design.
**Why**: A copy of a hardlink-heavy tree writes every link in full, so the bar fills against the write footprint and the disk-space check reserves it (the headline can legitimately read "80 GB" for a 60 GB-`du` `target/`). This is correct, not a bug — `scan_subtree_with_oracle` and `copy_volumes_with_progress` both carry the un-dedup'd `total_bytes` for copy, while the dedup'd `dedup_bytes` rides alongside purely to drive the dialog's clarifying note. Don't "fix" copy to show the dedup'd number: that would under-reserve disk space and stall the bar on dupes. The cross-volume copy path (`copy_volumes_with_progress` → `volume::strategy::copy_directory_streaming`) credits raw streamed bytes per file, which already equals the write footprint, so no dedup wiring is needed there. The one residual approximation: `dedup_bytes` over the cross-source-hardlink case (a file hardlinked into two separately-selected sources) counts twice, slightly understating the dedup savings shown in the note — safe direction, documented on `CopyScanResult::dedup_bytes`.

**Gotcha**: Volume disconnect mid-walk races with the oracle.
**Why**: The oracle returns `Some(entries)` when `listing_watch_coverage` reports `EveryWriter` at the moment of the check. Between that read and the recursive walk consuming the entries (and then issuing real `list_directory` calls for any sub-subfolders that aren't cached), the watcher can die (cable yanked, network drop). The synthesized totals for the cached level are correct — they reflect what the listing held — but recursion into now-disconnected sub-subfolders fails per-call, and the per-file copy/delete later then hits `DeviceDisconnected`-shaped errors instead of a single "device gone" message at the scan level. Same race that `scan_for_copy_batch` already had; the oracle doesn't widen it. Documented here so future investigation knows where to look.

## Dependencies

- `crate::file_system::volume`: `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `transfer/volume/copy.rs`)
- `crate::ignore_poison`: `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs), `xattr`, `exacl`, `filetime` (metadata preservation in `transfer/chunked_copy.rs`)

## Testing bar

This module's state machine (`state.rs`) is the spine of the cancel UX. Past investigations found one real production bug here ([commit `1de4255d`](../../../../../../docs/notes/speed-up-e2e-tests.md), lost-rollback on `Ok(())` arm) plus 30+ mutation-testing gaps that have since been pinned. New transitions or new cancel paths must:

1. **Drive the state machine through the public interface in tests.** Direct `state.intent.store(...)` mutation bypasses the validation guard and effectively dead-tests it. Pattern to copy: `state.rs::tests::test_cancel_via_public_path`.
2. **Cover both the happy path and the cancel-during-X race** for any new write-side operation. The Cancel-copy bug was specifically the `Ok(())` arm of the loop not re-checking intent.
3. **Add at least one E2E test** for user-visible flows (transfer dialogs, conflict policies); use `dispatchMenuCommand` for keyboard-shortcut triggers, see `docs/testing.md` § "❌ Synthesized F-key dispatches".
4. **Run `cargo mutants --file src/file_system/write_operations/<file>.rs`** after substantial changes; this module has ~85-90% mutation score per file and shouldn't regress. See `docs/testing.md` § "Process".

### Test isolation for `WRITE_OPERATION_STATE`

`cargo test` runs the crate's tests as threads in ONE process, so the `WRITE_OPERATION_STATE` map is shared by every
write-op test at once. `test_support::TestOperationGuard` owns one entry per test:

- **Unique key.** `register(tag)` / `register_state(tag, state)` mint a process-unique op id (tag + pid + counter), so a
  hardcoded literal can't collide with a sibling test. `register_as(op_id, state)` adopts an id the suite already
  generated (`transfer_driver`'s `unique_op_id`), for tests that thread the id through the call under test.
- **Panic-safe teardown.** `Drop` removes the entry, so an assertion that fails before a hand-rolled `remove` can't
  leave a corpse for the next test's `cancel_all_write_operations` to walk or `list_active_operations` to count. Pinned
  by `state::tests::guard_unregisters_its_state_even_when_the_test_body_panics`. Keep the guard on the stack: a
  `std::mem::forget` or a clone that outlives the test defeats it.

Same shape as `listing::caching_test_support::TestListingGuard` (over `LISTING_CACHE`) and
`indexing::tests::stress_test_helpers::TestInstanceGuard` (over `INDEX_REGISTRY`).

The operation-log journal slot is the exception to the unique-key pattern: it's a single value, not a keyed map, so
tests that install a journal SERIALIZE on `operation_log::TestJournalGuard` (one guard per test, lock held for the
test's duration, slot cleared on drop — see its doc for the multi-arm `hold_empty`/`set` shape and the non-reentrancy
deadlock warning). Never call `set_journal` directly from a test. Residual under plain `cargo test`: non-journal
write-op tests still journal their own ops into whatever DB is installed, so journal assertions stay scoped to the
test's own `op_id` (`journal_capture_tests::dir_volume_ids` joins dirs through the op's item rows for this reason).

#### ❌ Never drive a walk-everything mutator from a test

A unique key isolates a test that touches ONE entry. It does nothing for a function that walks the WHOLE registry:
`cancel_all_write_operations` stops every registered op, so a test calling it cancels whatever operations the tests
running beside it have in flight. That failure lands in the victim, not the culprit — a managed-op test seeing
`write-cancelled` (or a bare 0 events) where it expects `write-complete`, membership shifting run to run with
co-scheduling. It reads exactly like environment flake, which is how it survived: mis-blamed on load starvation once
and on a dependency upgrade once.

**The fix is a registry the test owns, not a lock around the tests.** `WriteOperationRegistry` (in `state.rs`) holds the
map and owns `cancel_all`; `WRITE_OPERATION_STATE` is one instance of it and `cancel_all_write_operations()` is a
one-line delegation. A test constructs its own `WriteOperationRegistry`, registers states in it, and calls
`cancel_all()` — the same production code, on a registry nothing else can see. Serializing the two suites behind a
mutex, `#[ignore]`ing the victim, or loosening its assertion would all have hidden the defect instead.

`state::tests::cancel_all_write_operations_walks_the_global_registry` is the ONE exception, because its subject IS the
global wiring. It's gated on `test_support::one_test_per_process()` (nextest's `NEXTEST_EXECUTION_MODE`), so it runs in
the sanctioned lane and skips under plain `cargo test`, where it would be the poisoner. Don't copy the gate to reach for
a global out of convenience; copy the private-registry pattern.

Lower-severity siblings, both currently harmless: `manager::force_admission_pass()` (test-only) runs a process-wide
admission pass, safe only because every manager test mints a unique `LaneKey`; and `watcher_test.rs`'s
`handle_directory_change` tests insert into `LISTING_CACHE` by hand with a tail `remove` instead of `TestListing`, so a
failing assertion leaks the entry (unique uuid keys keep it from poisoning anyone).

**A manager counter is never scoped by a unique op id.** `emit_count()` and `admission_pass_count()` count a WHOLE
manager's activity, so a sibling test spawning or settling ANY op moves them; a unique id, which is what keeps the
record-and-lane assertions honest, buys nothing here. A test asserting on either drives its own manager instead of
`manager()`: `private_manager()` for the lock-only paths, and `failures::leaked_manager()` (a `Box::leak`) when the test
has to `spawn_managed`, which takes `&'static self` because an admitted op's task outlives every borrow. Pair it with
`tests::gated_deferred_on`, so the synthetic op settles on the same private manager. The leak is one small struct per
test that asks for one. An equality assertion against the global counter passes only because nextest forks per test;
under plain `cargo test` it fails outright (`retained_failure_stays_hidden_until_the_record_settles` did).
The two `admission_pass_count()` waits in `tests.rs` stay on the global one: they wait for GROWTH, not an equality, so a
sibling's pass can only satisfy them early, never fail them.

**New subsystem state hangs off a struct, not a `static`.** These guards are the retrofit cost of a process-global; a
handle threaded through its callers needs none of it.

See also: `docs/testing.md` for the project-wide testing playbook.

## The network transfer suites

`webdav_transfer_integration_test.rs` and `sftp_transfer_integration_test.rs` copy real bytes between local disk and a
live fixture server through `copy_between_volumes`, which is the seam neither backend crate can reach: both directions
of an actual copy were once broken in the app while `cmdr-sftp`'s own Docker suite was fully green (a `supports_export`
predicate the crate never states, and a free-space pre-flight reading `NotSupported` as "no room").

- **The scenarios are backend-blind and live in `network_transfer_test_support.rs`.** Everything they touch is
  `dyn Volume`, so a claim proved against WebDAV is proved in the same words against SFTP and the two suites can't
  drift. Each backend file connects its own fixture, mints a scratch dir, and delegates.
- **❗ The cells themselves must stay in the two backend files, on the `webdav_integration_` / `sftp_integration_` name
  prefix.** The integration lane selects the app crate's Docker cells by NAME (`scripts/check/checks/fixture-lane-coverage.go`,
  enforced by `desktop-fixture-lane-coverage`), so a scenario promoted to a `#[tokio::test]` in the shared file would
  compile, look like coverage, and never run anywhere.
- **What the shared scenarios pin**: a nested directory tree lands intact in both directions (structure and bytes, as
  one `tree_fingerprint` comparison); a copy cancelled mid-upload leaves neither the user's filename nor a
  `.cmdr-tmp-*` sibling; an Overwrite answer travels out as a `write-conflict` and back through
  `resolve_write_conflict` and lands the source bytes; a PRE-EXISTING destination folder still probes each name under
  the concurrent driver (what a wrongly-`Created` `DirectoryCreation` would turn into a silent overwrite of every
  clashing file); and awkward names (`&`, `+`, `%`, `#`, non-ASCII) plus a zero-byte file survive a full round trip.
- **Nothing here waits on a stopwatch to decide a verdict.** The fixture stack is machine-wide and shared with every
  sibling worktree, so a wait that expires says "the containers were busy" at least as loudly as it says "the code is
  wrong". Two consequences to keep: a wait for a clash also ends when the operation SETTLES, so a destination probe that
  never happened fails as a sentence rather than as a timeout; and the cancel cell's premise is a chunk COUNT, not a
  duration.
- **The cancel cell holds the source at a chunk boundary rather than racing a timer.** Its `GatedUploadSource`
  (`network_gated_source_test_support.rs`, split out so neither file crosses the `file-length` warn threshold) hands
  out one chunk per semaphore permit, so the destination provably holds an open, incomplete staging sibling when the
  cancel lands; it then insists a `write-cancelled` was emitted and no `write-complete` was, so neither a copy that
  finished nor a run the host starved can satisfy the later claims for the wrong reason. It waits for TWO chunks, not
  one: the stream is polled again only once the destination has taken the previous chunk, so the second hand-out is what
  proves the first reached the server. That also keeps the cell inside the workspace-wide 8 s nextest cap,
  which a payload big enough to outrun a stopwatch would not.

## Bulk rename's hop log

`BulkRenameRecorder` (`rename/bulk.rs`) journals every filesystem hop the moment it lands, and the rows that never moved
once the run settles.

**Why per-hop rather than one pass at the end.** Journaling used to run after the whole batch, so a crash or force-quit
mid-batch left the renames done on disk, `operation_items` empty, `finalize_op` never run, and nothing for undo to
reverse (`restore_move` needs `ItemOutcome::Done` rows). The operation log's startup reconciliation only covers
`RollingBack` ops, so a crashed original stays `Running` with zero item rows forever. Recording as it lands is what makes
a partially-applied batch reversible.

**Why a rotation's temp hop gets its own row.** A cycle rotates through one same-directory temporary, and a case-only
rename does the same two-hop. Mid-rotation, one file's real name exists only at a `.cmdr-bulk-rename-*` path. Recording
`source → temp` as its own rollback unit means a crash there leaves the file findable by name and reversible in the
ordinary reverse-order replay. Consequence to expect: a two-file swap journals three rows, and a case-only rename two.
An in-process restore records its reversal hops too, so the log stays a faithful list and the reversals cancel out under
replay.

**The temp is NOT an `in_flight_temps` entry**, tempting as the shape looks. That ledger's next-launch sweep calls
`remove_file` on what it holds, which is right for the half-written `.cmdr-tmp-*` partials it exists for and destroys
data here: a rotation temp holds a COMPLETE user file under a private name.

**Skips are log entries, not rollback units.** Non-landing rows are journaled with `ItemOutcome::Skipped` / `Failed` so
the operation log stops claiming a smaller batch than the one that ran (they were dropped entirely before). They never
reach undo: `read_rollback_units_page` binds `ItemOutcome::Done`, which is why `restore_move`'s non-`Done` guard is
defensive rather than reachable. Pinned by
`a_skipped_row_is_logged_but_never_offered_to_undo_as_a_rollback_unit`.

## Testing the in-flight temp ledger

`in_flight_temps.rs` keeps ONE process-wide `STORE` for the whole test binary, and two rules follow from that. Ignore
either and the tests fail on load rather than on a break, which is worse than not having them.

- **Take `test_support::take_store()` (or `use_store_in`) for the WHOLE test body**, ❌ never for just the part that
  writes. Installing a log into the singleton redirects every `register` in the process into that file, from any
  thread, so two tests doing it at once put one test's records in the other's log — and leave a startup-sweep fixture
  replaying an empty log, sweeping nothing. The guard holds a `SINGLE_FILE` mutex that serializes them; releasing it
  early hands the singleton to the next test while this one is still recording. `simulate_process_exit()` is how a test
  detaches the process's handle (the crash it's reproducing) without giving the singleton back.
- **Assert about the path under test, ❌ never about the whole ledger.** `live_paths()` and the log file are shared with
  every transfer test that stages a write without holding the guard, so `live_paths().is_empty()` and
  `read_recorded(..).is_empty()` are assertions about the rest of the suite. Ask `contains(&subject)` instead; it pins
  the same regression.

**The sweep signals completion, so no test needs a deadline.** `init_and_sweep` returns a `SweepHandle`; the launch path
drops it (waiting there would block on a dead mount for minutes), and a test calls `.wait()` to join the sweep thread.
Joining also keeps the sweep from outliving the `TestDir` it is walking.
