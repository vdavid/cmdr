# Conflict resolution `Sender` can be re-installed after `cancel_write_operation` drops it

**Severity:** low
**Lens:** B — Concurrency
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/state.rs:391-419` (cancel) plus the helpers that install the sender (`helpers.rs:425`-ish wait-for-resolution path)

## What
`cancel_write_operation` transitions the intent atomic and then drops the conflict-resolution sender:

```rust
state.intent.store(target as u8, Ordering::Relaxed);
state.backend_cancel.store(true, Ordering::Release);
let _ = state.conflict_resolution_tx.lock().unwrap().take();
```

The order is: (1) flip intent, (2) flip backend cancel, (3) take the conflict-resolution sender. A worker hitting a conflict mid-flight installs a fresh sender in `state.conflict_resolution_tx` and then blocks on the receiver. The install side does:

1. Check `is_cancelled(&state.intent)` — if cancelled, bail.
2. Install a new `(tx, rx)` pair into `state.conflict_resolution_tx`.
3. Block on `rx.blocking_recv()`.

If `cancel_write_operation` runs *between* step 1 (the worker's intent check, which passes) and step 2 (the worker stores its new sender), the worker overwrites the cancel-cleared slot with a fresh sender. The cancel's `take()` already executed against `None`. The worker then blocks on the receiver — and nothing ever sends on it because the cancel already happened. Stuck operation, settle event never fires until the broader spawn-task scope exits via the intent check on its next iteration.

The MTP `backend_cancel` flip is the main reason this is "low" not "medium": the long-running USB calls do check `backend_cancel` and would eventually bail, propagating an error up through `helpers::run_cancellable`, which is the path that eventually unblocks the spawn task. But the conflict-receive itself doesn't poll `backend_cancel`; it relies on the sender being dropped, which it can't if a new sender was installed after the take.

## Why it matters
The settle-guard's `Drop` won't fire until the spawn task exits, which can't happen until the conflict-resolve receive unblocks. The frontend's "Cancelling…" dialog waits for `write-cancelled` + `write-settled` (per `write_operations/CLAUDE.md` § Settle contract). A hanging receive means the dialog stays up indefinitely, exactly the wedge the settle contract was built to prevent.

The race window is narrow (a couple of instructions between the intent check and the sender install), so this is empirically unlikely to trigger. But the architecture's correctness story is "cancel always settles," and a window — even a narrow one — where it doesn't is worth flagging.

## Evidence
- `state.rs:412-417`: cancel sequence.
- `helpers.rs:411-430` (approximate, the `wait_for_conflict_resolution` shape): install-then-receive pattern.

## Suggested fix
Two options:

1. **Atomic install-with-check**: inside the install helper, after constructing the `(tx, rx)`, re-check `is_cancelled(&state.intent)` while holding the `conflict_resolution_tx` lock; if cancelled, drop the new sender immediately and return an error. This closes the race because the cancel either ran before the install (intent check fails) or runs after (the install holds the same mutex the cancel needs to drop the sender).

2. **Channel-based cancel signal**: in addition to `backend_cancel`, have a `tokio::sync::watch` or per-op `Notify` that all blocking-receive sites also select on. The conflict path becomes `select! { _ = rx => ..., _ = cancel_notify.notified() => Err(Cancelled) }`, and cancel just notifies.

Option 1 is the smaller change. The mutex order in cancel needs to be tightened: `let mut guard = tx.lock().unwrap(); intent.store(...); backend_cancel.store(...); let _ = guard.take();` so the cancel side holds the lock throughout the transition.

## Notes
The audit caller documented the broader "race between cancel + finalize" pattern; this is one specific instance. Hard to reproduce reliably without instrumentation; consider a stress test that loops "start op, fire conflict, fire cancel in the same tick" 10k times under address sanitizer or with a Loom-style scheduler.
