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

### Progress stays honest across a retry

An attempt restarts at byte zero, so a file's own counter legitimately goes backwards. What the user sees must not, and
the operation's total must not double-count. Both paths therefore report a file's HIGH-WATER mark:

- **Concurrent** (`make_concurrent_per_file_progress`): `last_file_bytes.fetch_max(...)`, ❌ never `swap`. A `swap`
  lowers the watermark on a restart and then credits the whole re-streamed prefix a second time — a silent over-count
  and a Size bar that reaches 100% before the copy does.
- **Serial** (`SerialLeafProgress`): a `leaf_high_water` for the in-flight leaf, reset in `on_leaf_complete` so the
  next (possibly much smaller) leaf measures from its own first byte. `on_leaf_complete` still adds the leaf's exact
  size once, so the end number is exact whatever the attempt count.

The file counter needs nothing: `on_file_complete` fires only after `stream_pipe_file` returns `Ok`.

### The file counter counts COMPLETED files, and the gap that opens is real

`on_file_complete` fires after the write returns, so with a window of `W` the destination can legitimately hold up to
`W - 1` more files than the counter shows. On 2026-07-31 that read as "5 of 764" against 10 files already on the NAS,
and the counter was right both times: five tasks had returned, the rest had bytes on the share but had not.

❌ **Don't "fix" the counter by crediting a file before its write returns.** Counting a file that a wedge, a cancel, or
a failed landing can still take away turns an honest bar into an optimistic one, and it would have claimed both of the
byte-incomplete phone backups the incident left behind as done. There is also no honest frontend-only fix: the
`write-progress` payload carries one `currentFile` and no in-flight set, so the gap cannot be reconstructed there. What
closes it is SURFACING the window — `TransferActivity::in_flight` on the same event, rendered by the stall notice's
in-flight line (`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "The stalled-transfer notice").
