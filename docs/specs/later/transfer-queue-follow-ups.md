# What the transfer queue still owes

The lane-based queue shipped: one ordered admission queue with atomic multi-lane reservation, pause and resume that
reach mid-file, a standalone queue window, and Pause + Queue controls on the progress dialog.
`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Operation manager" is the canonical account of
admission, lanes, settling, and the pause model; `transfer/DETAILS.md` § "Pause reaches between chunks" owns the
mid-file half.

Five things are open, each independent of the others.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## 1. A lane still admits exactly one operation

`LANE_BUDGET` is a `const usize = 1` in `manager.rs`: one operation per lane, so a second copy to the same NAS waits for
the first even when the server would happily serve both.

**Why it's a localized change**: `lane_use` is a count map rather than a set, chosen for exactly this, so only the
free-check has to compare against a per-lane number instead of the constant. The shape is a `Volume::lane_budget()`
(default 1) beside the existing `lane_key()`.

**❗ Don't confuse this with `Volume::max_concurrent_ops()`**, which already exists and already varies per backend. That
one sizes the file window WITHIN one transfer (`transfer/volume/copy.rs::transfer_concurrency`, and the user-facing
`network.smbConcurrency` setting rides it). The lane budget is a different question: how many separate OPERATIONS may
hold one device at once.

**The motivating case has moved.** The original one was FTP's five simultaneous connections, and FTP is parked
(`docs/notes/ftp-crate-evaluation-2026-08-22.md` § "Is FTP worth doing at all"). What's left is SFTP and SMB, where a
server does carry more than one operation, plus finer local-device detection so two copies on genuinely different
physical disks stop sharing a mount-root lane key.

**One risk is smaller than it was**: `smb2` 0.19.0 bounds outstanding write payload connection-wide at 32 MiB, so a lane
budget above 1 on an SMB device can no longer multiply into hundreds of megabytes of uncancellable in-flight writes.

## 2. A long pause can still leave a connection to time out

A paused transfer parks between chunks and keeps its source stream and its backend connection open, so a pause long
enough to cross a server or USB idle timeout can surface a transient error on resume. Today's contract accepts that: SMB
reconnects and MTP has a one-shot stale-handle retry.

**The MTP half of this is already gone**, and the reason is worth knowing before anyone builds a keep-alive: an MTP read
is a sequence of bounded windows that hold nothing between them, so a paused MTP copy releases the device without any
release-and-reopen machinery (`transfer/DETAILS.md` § "Pause reaches between chunks", the park-in-place paragraphs).

**What's left** is SMB and SFTP, and the two differ. SMB's `smb2` has an ECHO keep-alive already running; the open
question there is only whether a paused transfer should hold its handle across a long pause. SFTP has no keep-alive at
all in this stack (`crates/cmdr-sftp/DETAILS.md` § "Coming back"), so an explicit reconnect-on-resume is the likelier
shape.

**Trigger**: a real report of a resume that errors after a long pause. Both backends already recover, so this buys
smoothness rather than correctness.

## 3. A paused operation parks a blocking-pool thread

`wait_while_paused_sync` parks the operation's `spawn_blocking` thread for the whole pause, the very thing the
deferred-start design avoids for queued operations. `write_operations/DETAILS.md` § "Pause / resume" records this as an
accepted asymmetry: a paused Running operation legitimately holds its lane, and it is rarer than a queued one.

**The open question is bounded and empirical**: does many-simultaneously-paused-operations pressure on the blocking pool
ever show up? Bound the count if it does. ❌ Don't build the bound speculatively; the accepted-tradeoff note exists so
the next agent doesn't mistake it for an oversight.

## 4. The queue can't be reordered

No drag-to-reorder, no "run next", no priority bump. Admission walks a single FIFO `order` vector under the manager
lock, so reordering the queue is reordering that vector; the cost is the queue-window UI, not the backend.

## 5. The queue doesn't survive a restart

The registry is in-memory, so a crash or quit drops operations that were queued and never started. The queue window's
capability file (`src-tauri/capabilities/queue.json`) records the no-persistence choice deliberately, dropping
`store:default` for it.

**What it would take**: persist enough of each pending operation's descriptor to reconstruct its `DeferredStart`, then
offer to resume on next launch. It interacts with mid-file resume, so a persisted operation that was already running
reopens the question of what to do with its `.cmdr-tmp-<uuid>`.

## Settled while re-deriving this, so nobody re-opens it

- **Mid-large-file pause shipped.** A paused cross-volume copy stops between chunks, not only between files, via the
  `CheckpointStream` decorator that `stream_pipe_file` wraps the source stream in. Resume semantics are settled too:
  park in place, keep the offset, no reopen. The local-FS sync chunk loop (`transfer/chunked_copy.rs`) is the one path
  that still pauses only between files, because it receives the cancel atom rather than the `PauseGate`.
- **The concurrent copy path halts on pause.** Its per-file callback is deliberately cancel-only (pinned by
  `transfer_driver::tests::concurrent_per_file_callback_is_cancel_only_not_pause_aware`), but every in-flight file parks
  between chunks through the same `CheckpointStream` and the admission loop adds none while they're parked, so the batch
  stops. `transfer/volume/DETAILS.md` § "Pause and the concurrent copy path" owns it.
- **Rollback is not coming to the queue window on its own.** The backend machinery exists and the MCP `queue` tool
  already exposes it behind the `IfRollback` approval gate, so an agent can roll back today. The queue window stays
  cancel-only by choice (`routes/queue/+page.svelte` says so at its per-row comment), and cancel-keep-partials covers
  what people actually ask for. Build a rollback affordance only if the demand turns out to be real.
