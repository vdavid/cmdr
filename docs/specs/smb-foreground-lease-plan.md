# A background SMB upload stands aside for the folder you're actually waiting on

**Problem.** Browsing an SMB share while a local → SMB transfer runs is sluggish. A yield mechanism exists and works for
large files, but two gaps make it miss the common cases.

**Gap 1: the foreground signal decays before the listing it protects finishes.** "Is the user browsing this share?" is
answered by a timestamp stamped once at listing-command entry
(`apps/desktop/src-tauri/src/commands/file_system/listing.rs:248`) and read back with a 500 ms decay
(`TRANSFER_FOREGROUND_IDLE_THRESHOLD`, `crates/cmdr-smb/src/volume/foreground_yield.rs`). Nothing refreshes it while the
listing runs, and SMB returns a whole listing in one round trip (`crates/cmdr-smb/src/volume/query.rs`
`list_directory_with_progress_impl` calls `on_progress` exactly once, after the wire work is done), so there is no
per-batch hook either. A listing that takes seconds is protected for its first half-second; the upload then resumes and
competes for the rest of the user's wait. The measurement that motivated the whole feature (`6d9df62d7`) was a 40-entry
folder taking 10.7 s.

**Gap 2: files under 4 MiB never yield at all.** The destination arm is gated on `MIN_PROGRESS_FLOOR_BYTES` (4 MiB,
`transfer/volume/strategy.rs`), and each file gets a fresh `CheckpointStream` with the counter at zero
(`transfer/checkpoint_stream.rs`). A file smaller than the floor can never satisfy it. Copying a folder of photos or
documents to a NAS yields zero times.

**Impact.** Both gaps make the protection weakest exactly where it is needed, and neither looks broken from outside: the
feature is wired, the tests pass, and browsing is still slow. Reported by a user; unresolved.

---

## Milestone 1: a listing holds a lease, so "busy" is a fact instead of an estimate

Replace the guess with an exact holder. This is the shape the MTP path already has (a per-device gate with a real
in-flight count); SMB was assumed to be unable to do it because many requests multiplex over one connection. That is
true of the connection, but a listing is a scoped operation with a beginning and an end, so it can hold a lease.

1. Add a per-volume in-flight lease count to `ForegroundActivity` (`apps/desktop/src-tauri/src/priority/foreground.rs`)
   alongside the existing timestamp, plus a guard type that releases on drop. RAII is load-bearing: the lease must come
   back on the error path, on panic, and on task drop, not by remembering to call something.
2. `foreground_pending` becomes `leases > 0 || !idle_for(threshold)`. The timestamp keeps its one good job: the
   post-listing debounce so a burst of arrow-key presses reads as one continuous action. ❌ Do not delete the timestamp.
3. Thread the lease count through the seams the timestamp already travels: the `UserActivity` trait
   (`crates/cmdr-fs/src/volume/host/activity.rs`), `AppUserActivity` / `AppHostPolicy`
   (`apps/desktop/src-tauri/src/priority/host_policy.rs`), and the SMB reader
   (`crates/cmdr-smb/src/volume/foreground_yield.rs`).
4. Take the lease in the spawned listing task in `apps/desktop/src-tauri/src/file_system/listing/streaming.rs` (the
   `tokio::spawn` around `read_directory_with_progress`), so it spans the actual listing rather than the command call
   that returns immediately. Keep the existing entry-point stamp: it covers the non-streaming path and seeds the
   debounce.

**Decide and document**: the index scan reads the same per-volume signal with a 2 s threshold
(`crates/cmdr-index/src/indexing/network_scanner/scan_pace.rs`), so it will now also back off for a listing's full
duration. That is intended, and it is a behavior change in a second consumer. Say so in the docs.

**Bounded scope.** ❌ Do not touch the transfer's park loop (that is milestone 2), the min-progress floor (milestone 3),
or any other IPC command. In particular ❌ do not add foreground stamping to `path_exists`, `get_file_range`, or
`refresh_listing`: each is reached by background callers (a 2 s deleted-directory poll that is not skipped for SMB, the
MCP pane mirror, the tag sweep, a post-transfer refresh), and stamping there would pin a share permanently busy. That
was investigated and rejected.

**Robustness bar.** A listing that never returns must not hold a lease forever. Confirm and document the bound: the
`smb2` transport has a 30 s response deadline and a 20 s send deadline, and the park is capped independently (milestone
3's cap stays), so a held lease slows an upload but can never stop it.

---

## Milestone 2: the parked upload wakes on the lease instead of polling every 50 ms

Depends on milestone 1.

Today a parked upload re-checks the foreground signal every `DEST_PARK_POLL_SLICE` (50 ms, `checkpoint_stream.rs`),
because a written-down timestamp has nothing to notify on. With a lease count there is a real event to wait for.

1. Give the lease count a notify (`tokio::sync::Notify` or equivalent) fired when the count reaches zero.
2. Rework the park loop to wait on `{ lease-count-hits-zero, cancel, park hard cap }` rather than a poll slice.
3. **Be honest about the debounce.** After the last lease drops, the timestamp debounce still has to elapse, and nothing
   notifies a timestamp going stale. Wait out that remainder with ONE computed sleep to the deadline, ❌ not a
   reintroduced poll loop. A single `sleep` to a known instant is a proper wait; a 50 ms tick is not.
4. Apply the same treatment to the source-side arm (`wait_until_foreground_idle`) if it shares the mechanism, but ❌ do
   not change MTP's own gate: that has a real holder already and is not the problem being solved.

**Non-negotiable**: cancel-awareness must survive exactly as it is today. A cancel while parked has to unblock promptly
in every path. Prove it with the existing cancel tests plus one for the new wait shape.

**Bounded scope.** ❌ Do not change any threshold, cap, or floor value in this milestone. It is a wait-mechanism swap,
and behavior at the boundaries must be observably identical apart from wake latency.

---

## Milestone 3: small files stand aside, without weakening the write-handle guarantee

Independent of milestones 1 and 2; do it last so it lands on top of a settled foreground signal.

The 4 MiB floor and the 1 s park cap (`DEST_FOREGROUND_YIELD_HARD_CAP`) both exist for one reason: an upload holds an
open SMB write handle across a pause, and a server can reap an idle handle. On the single-shot compound path that reason
does not apply. `crates/cmdr-smb/src/volume/streams.rs` drains the source stream fully into a buffer and only then sends
CREATE+WRITE+FLUSH+CLOSE as one frame, so during the drain (which is where the checkpoints happen) nothing is open on
the server.

1. Pass the already-computed single-shot answer into `CheckpointStream`. `resolve_staging` runs a couple of lines before
   the stream is wrapped in `transfer/volume/strategy.rs`, so no new probe or round trip is needed. Check whether
   `resolved_staging == SingleShot` is the right carrier or whether the raw `write_is_single_shot` boolean should be
   captured separately: `resolve_staging` only returns `SingleShot` when the request was `Stage`, so the enum may
   under-report. Prefer whichever is exact.
2. On that path, skip the min-progress floor in the destination arm, so a small file can yield.
3. **Keep the 1 s hard cap.** This is a deliberate correction to an earlier draft of this plan that proposed removing
   it. A small file's whole drain is well under a second, so the cap never binds and removing it buys nothing; meanwhile
   the single-shot decision and the actual write happen at different moments, and a reconnect between them could change
   the server's negotiated `max_write_size` and move the file onto the streaming path, which does hold a handle open.
   Keeping the cap makes that race harmless. ❌ Do not remove or raise the cap.
4. Net effect: no data-safety change at all. An existing guard is narrowed to the cases it was written for.

**Lean on the existing guarantee.** `write_is_single_shot` and `write_from_stream` are required to branch on the same
predicate (`fits_one_compound_write`), because disagreement would leave a truncated file under the user's real filename.
That contract is what makes the flag trustworthy; reference it rather than restating it.

**Bounded scope.** ❌ Do not add a pre-file yield gate in `stream_pipe_file`. That is the follow-up for the 1 MiB–4 MiB
band, it is a genuine throughput-versus-responsiveness trade, and it needs benchmarking against real hardware first.
Note it as deferred; do not build it.

---

## Definition of done

- Each milestone is red-first where the change is testable, and the red step is actually observed.
- `pnpm check` green per milestone; `pnpm check --include-slow` green at the end.
- Colocated `CLAUDE.md` / `DETAILS.md` updated wherever behavior or rationale moved, per `AGENTS.md`. In particular the
  "Decision/Why a timestamp, not a gate" rationale in `transfer/DETAILS.md` is now partly wrong and must be rewritten to
  describe the lease.
- ❌ No milestone tags ("M1", "Milestone 2") left in code, comments, or docs.
