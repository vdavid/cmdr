# Copy safety hardening plan

Fixes three gaps found in the copy/move safety review: orphaned background operations, conflict deadlocks on
frontend teardown, and lack of automatic rollback on panic.

## Problem 1: Dialog closes before `operationId` is known (4A)

**Scenario:** User opens the progress dialog and immediately closes it (or presses Escape) before the Tauri IPC
round-trip returns the `operationId`. `handleCancel()` checks `if (!operationId)` and returns early. The backend
operation runs to completion silently — no UI, no way to cancel it.

**Fix: Track the pending promise and cancel after it resolves.**

In `TransferProgressDialog.svelte`:

1. Store the `startOperation()` promise in a module-level variable (`operationPromise`).
2. Add a `destroyed` flag, set in `onDestroy`.
3. When `handleCancel` is called with `operationId === null`, set `destroyed = true` and return. The `startOperation`
   function will check this flag after the IPC call resolves and immediately cancel the operation if set.
4. When `startOperation`'s IPC call resolves, check `destroyed`. If true, immediately cancel the new operation and
   return (don't replay buffered events, don't update state).
5. Update `onDestroy` to also set `destroyed = true` so the same logic applies when the component is destroyed for
   any reason.

This closes the window completely: if the user closes the dialog at any point before or after the operation ID arrives,
the operation gets cancelled as soon as the ID is known.

### Integration test: cancel before `operationId` arrives

Add a Rust integration test that exercises the `destroyed` flag path end-to-end:

1. Call `copy_files` (or `move_files`) with a valid source/destination via the backend directly (no frontend).
2. Immediately call `cancel_write_operation` with the returned operation ID before processing any progress events.
3. Assert: the operation emits a `Cancelled` result (not `Complete`).
4. Assert: no destination files remain (rollback happened).

This validates that cancellation works even when the cancel arrives in the narrow window right after the operation starts
— the same window the `destroyed` flag targets on the frontend side.

Also add a Vitest unit test for the frontend-side logic: mock the IPC call to `copyFiles` with a delayed resolution,
set `destroyed = true` before the promise resolves, and assert that `cancelWriteOperation` is called with the resolved
operation ID.

## Problem 2: Backend conflict deadlock when FE is destroyed (4B)

**Current state:** The condvar has a 300-second timeout (`helpers.rs:367`). The `onDestroy` handler cancels if
`conflictEvent && operationId`, but doesn't cancel in other teardown scenarios. Hot-reload, navigation, crash, or
memory pressure could all destroy the component without triggering a conflict-aware cancel.

**Two-layer fix:**

### Layer 1: Frontend — `beforeunload` cancels all active operations

Add a new backend command `cancel_all_write_operations` that iterates `WRITE_OPERATION_STATE` and cancels every active
operation (with rollback).

On the frontend, add a `beforeunload` listener (like the one in `log-bridge.ts:136`) that calls this command. This
covers hot-reload, tab close, window close, and navigation.

**Backend** (`state.rs`):

```rust
pub fn cancel_all_write_operations() {
    if let Ok(cache) = WRITE_OPERATION_STATE.read() {
        for (_, state) in cache.iter() {
            state.cancelled.store(true, Ordering::Relaxed);
            state.skip_rollback.store(false, Ordering::Relaxed); // rollback
            let _guard = state.conflict_mutex.lock();
            state.conflict_condvar.notify_all();
        }
    }
}
```

Wire it up as a Tauri command in `commands/file_system.rs`, expose in `write-operations.ts`, and call from a
`beforeunload` handler in the main layout or `+layout.svelte`.

### Layer 2: Make `onDestroy` unconditionally cancel

Change the `onDestroy` in `TransferProgressDialog.svelte` from:

```typescript
onDestroy(() => {
    if (conflictEvent && operationId) {
        void cancelWriteOperation(operationId, false)
    }
    cleanup()
})
```

To:

```typescript
onDestroy(() => {
    destroyed = true
    if (operationId) {
        void cancelWriteOperation(operationId, true) // rollback on unexpected teardown
    }
    cleanup()
})
```

This ensures that *any* component destruction (hot-reload, panic, whatever) cancels and rolls back. The `destroyed`
flag from Problem 1 handles the case where `operationId` is not yet known.

Note: Normal user-initiated cancel/rollback already calls `cleanup()` + `onCancelled()` before `onDestroy` fires,
so the `if (operationId)` check will be null by then (since `operationId` is set but the cleanup + callback already
happened). We should double-check this doesn't result in a double-cancel. The backend's `cancel_write_operation` is
idempotent (setting an already-set `AtomicBool` is a no-op, and the state is already removed from the cache after
the operation finishes), so a double-call is harmless.

### Layer 3 (defense in depth): Reduce the conflict condvar timeout

The 300s timeout in `helpers.rs:367` is a safety net for when the frontend is completely dead. 300 seconds is far too
long — a user watching their file manager hang for 5 minutes would be alarmed. Reduce to **30 seconds**. This is
still plenty of time for a user to decide on a conflict, and the actual resolution is instant (the dialog is always
visible while waiting).

## Problem 3: `CopyTransaction` doesn't auto-rollback on panic (5)

**Current state:** `CopyTransaction` has `rollback()` and `commit()` but no `Drop` impl. If the thread panics
inside the copy loop, created files are orphaned. The `commit()` method already takes `self` by value (consuming it),
but since there's no `Drop`, panic unwind just drops the vecs without cleaning up.

**Fix: Add `Drop` that calls `rollback()`, with a `committed` flag.**

```rust
pub(crate) struct CopyTransaction {
    pub created_files: Vec<PathBuf>,
    pub created_dirs: Vec<PathBuf>,
    committed: bool,
}

impl CopyTransaction {
    pub fn new() -> Self {
        Self {
            created_files: Vec::new(),
            created_dirs: Vec::new(),
            committed: false,
        }
    }

    pub fn commit(mut self) {
        self.committed = true;
        // Drop runs, sees committed = true, does nothing
    }

    pub fn rollback(&self) {
        // (same as current impl)
    }
}

impl Drop for CopyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            log::warn!(
                "CopyTransaction dropped without commit — rolling back {} files, {} dirs",
                self.created_files.len(),
                self.created_dirs.len(),
            );
            self.rollback();
        }
    }
}
```

This way:
- Normal success: `transaction.commit()` sets `committed = true`, `Drop` is a no-op.
- Error path: current code calls `transaction.rollback()` explicitly, then drops — `Drop` sees vecs are empty (already
  cleaned up by rollback), harmless.
- Panic: `Drop` fires during unwind, sees `committed = false`, rolls back. Files cleaned up.

The existing explicit `rollback()` calls in the error paths of `copy.rs` and `move_op.rs` can stay — they're harmless
when `Drop` also runs (rollback is idempotent since `remove_file` on an already-deleted file just returns an error
that's ignored with `let _`).

One subtlety: the explicit `rollback()` call in the error path borrows `&self`, but `Drop` also needs `&mut self`.
Since the explicit rollback happens before the `Drop`, and doesn't consume the transaction, this is fine — Rust's
drop runs after the value goes out of scope.
