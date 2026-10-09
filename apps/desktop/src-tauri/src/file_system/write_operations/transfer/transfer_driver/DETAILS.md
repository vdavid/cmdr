# Shared transfer driver details

The scaffolding all four transfer cores run through, and the progress accounting they share. Read this before any
non-trivial work here: editing, planning, reorganizing, or advising. The transfers themselves are `../CLAUDE.md` and
`../DETAILS.md`.

## Sync and async are two siblings, not one generic

`copy_files_with_progress_inner` runs inside `spawn_blocking` with synchronous `std::fs`, and its closure captures
`&mut CopyTransaction` + `&mut HashSet<PathBuf>` + `&mut SourceItemTracker`. The three volume ops are async, awaiting
the `Volume` trait's `Pin<Box<dyn Future>>` methods. Being generic over both would force every sync caller through a
boxed-future allocation per source and lose the closure's `&mut` capture clarity, so `sync_driver.rs` and
`async_driver.rs` share types instead of a trait.

## Conflict resolution is closure-owned for sync, driver-owned for async

The async driver resolves the top-level conflict itself and never invokes the closure on a Skip; the sync driver hands
that to the closure, which is why point 2 of the data-safety contract is async-only. ❌ Don't unify the two by moving
resolution into the sync driver without moving the closure's `&mut` state with it.

## A `Cancelled` from any closure is a cancel, ❌ never a failure

Both drivers turn every closure `Err` into the loop's ending through `PostLoopIntent::stopped_by`: `Cancelled` ends it as
`PostLoopIntent::Cancelled`, anything else as `Failed`. The async driver has three closures that can say it, and the
resolver is the one that matters most: a cancel pressed while a Stop-mode clash is on screen drops the parked slot's
sender (`../../conflict_slot.rs::abandon`), so the resolver comes back `Err(Cancelled)` instead of a decision. The
engines emit `write-cancelled` only from their `Cancelled` arm (`../volume/move_cross.rs`, `../volume/move_same.rs`),
and the moves' outer wrappers only LOG a `Cancelled` error. So a `Failed(Cancelled)` emitted no terminal event at all,
and the progress dialog stayed parked on the stale prompt until the person answered it (a cross-volume move to an SMB
share in 0.51.0). Pinned by `async_driver_resolver_cancel_ends_the_loop_as_cancelled` and the two
`*_move_cancelled_at_a_conflict_prompt_emits_cancelled_event` tests in `../volume/move_cancel_tests.rs`.

### Progress stays honest across a retry, and across leaves that overlap

Three types, one per scope (`progress.rs`): `LeafProgressLedger` holds the operation's totals, `SourceProgress` is one
top-level source's view of it (carrying the name the events are labeled with, and the throttle clock), and
`LeafProgress` is one file's handle. The reported number is `finished + sum(in-flight leaves)`, read under ONE lock.

Two things that number must survive:

- **A retry.** An attempt restarts at byte zero, so a file's own counter legitimately goes backwards. `LeafProgress`
  holds its HIGH-WATER mark, so the bar doesn't, and `complete` adds the leaf's exact size once however many attempts
  it took. ❌ Never lower the mark on a restart: the re-streamed prefix would be credited twice, a silent over-count
  and a Size bar that reaches 100% before the copy does.
- **Leaves that overlap.** A directory source streams many files at once through `volume/merge_ctx.rs::FileWindow`, all
  reporting into the same ledger. ❌ Never give them one shared high-water slot: the bar then shows whichever leaf is
  furthest along, and the next leaf to finish — any leaf, however small — resets the slot and takes the big one's
  progress off the bar. On a 664 MB folder whose largest file was 259 MB that read as the Size bar falling from
  ~300 MB back to ~80 MB, once per completed file, while the bytes were moving fine.

`complete` swaps the leaf's in-flight share for its exact byte count under one lock, so the total can't be read
half-applied; a leaf that never lands withdraws its share on `Drop`, so a failed or cancelled file stops being reported
as delivered. Reading `finished` and `in_flight` from two separate atomics reintroduces the sawtooth in miniature, so
❌ don't split them.

The file counter needs nothing: `complete` fires only after `stream_pipe_file` returns `Ok`.

### The file counter counts COMPLETED files, and the gap that opens is real

`LeafProgress::complete` fires after the write returns, so with a window of `W` the destination can legitimately hold up to
`W - 1` more files than the counter shows. On 2026-07-31 that read as "5 of 764" against 10 files already on the NAS,
and the counter was right both times: five tasks had returned, the rest had bytes on the share but had not.

❌ **Don't "fix" the counter by crediting a file before its write returns.** Counting a file that a wedge, a cancel, or
a failed landing can still take away turns an honest bar into an optimistic one, and it would have claimed both of the
byte-incomplete phone backups the incident left behind as done. There is also no honest frontend-only fix: the
`write-progress` payload carries one `currentFile` and no in-flight set, so the gap cannot be reconstructed there. What
closes it is SURFACING the window — `TransferActivity::in_flight` on the same event, rendered by the stall notice's
in-flight line (`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "The stalled-transfer notice").
