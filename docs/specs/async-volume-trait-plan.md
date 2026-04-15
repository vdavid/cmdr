# Async Volume trait refactor

## Goal

Make the `Volume` trait fully async. Eliminate all `Handle::block_on` bridges from MTP and SMB volumes. Migrate the
copy/move/delete pipelines to async with `CancellationToken`-based cancellation. The only remaining `block_on`-like
pattern should be `spawn_blocking` inside `LocalPosixVolume` (wrapping inherently blocking syscalls).

## Why

The Volume trait is sync today because it was designed around local filesystem ops. MTP and SMB are async internally but
bridge to sync via `Handle::block_on`. This creates:

1. **Nested runtime panics**: MTP source + SMB destination both call `block_on`, causing tokio to panic. Current
   workaround (Option E: channel-based MtpReadStream) works but is a band-aid.
2. **Invisible blocking**: `block_on` inside `spawn_blocking` blocks a thread pool thread. Fine for one operation, but N
   concurrent copies each blocking a thread is wasteful.
3. **Cancellation fragility**: 50+ `is_cancelled()` checks scattered through the write pipelines. Easy to miss a site.
   Async gives us `select!` + `CancellationToken` — composable and impossible to forget.
4. **Future volume types**: FTP, S3, WebDAV are all async. Each would need the same `block_on` bridge + channel
   workaround. Async trait makes them first-class.

## Constraints and principles

From `design-principles.md` and our conversation:

- **Elegance above all.** No hacks, no band-aids. The async trait should be the natural expression of what volumes ARE.
- **All actions > 1s should be immediately cancellable.** Async `select!` gives us this universally.
- **Radical transparency.** Progress reporting should be a first-class part of every operation, not bolted on.
- **Don't overengineer.** We're making the trait async, not building a generic plugin system. Concrete solutions for
  concrete problems.
- **Protect the user's data.** The Condvar → async channel migration for conflict resolution must be bulletproof. No
  race conditions, no lost resolutions.

## Scope

### In scope

- `Volume` trait: async methods via manual `Pin<Box<dyn Future>>` return types (no `async-trait` crate — see decisions)
- `VolumeReadStream` trait: `next_chunk` becomes `async fn`
- 4 implementors: `LocalPosixVolume`, `InMemoryVolume`, `MtpVolume`, `SmbVolume`
- **Volume** copy/move/delete pipelines (`volume_copy.rs`, `volume_move.rs`, `delete.rs`): `spawn_blocking` wrappers
  removed, loops become async
- Conflict resolution in `WriteOperationState`: `Condvar` → `tokio::sync::mpsc` channel. This is a shared struct used by
  ALL write operations (local and volume), so the migration touches `state.rs`, `helpers.rs`, `volume_copy.rs`,
  `volume_move.rs`, `volume_conflict.rs`, `volume_strategy.rs`, `mod.rs`, `trash.rs`, and all test files that construct
  `WriteOperationState`.
- Cancellation: `CancellationToken` for volume pipelines. **Local pipelines** (`copy.rs`, `move_op.rs`, `delete.rs` for
  local ops, `trash.rs`) keep `AtomicU8`-based `is_cancelled()` for now — they run in `spawn_blocking` where async
  cancellation doesn't help. This is a deliberate dual mechanism, not a bug. Tracked for future consolidation.
- Scan preview: volume path becomes async
- Space poller: becomes async
- All affected tests: `#[test]` → `#[tokio::test]`
- CLAUDE.md updates across all affected modules

### Out of scope

- Local file copy strategies (macOS `copyfile`, Linux `copy_file_range`): remain sync inside `spawn_blocking`.
- Local write pipelines (`copy.rs`, `move_op.rs` same-fs path, `trash.rs`): remain `spawn_blocking`-based. These use
  blocking syscalls directly and don't benefit from async. Migrating them is a separate, larger effort.
- `VolumeScanner` and `VolumeWatcher` sub-traits: own threading models (jwalk, FSEvents), don't benefit from async.
- Frontend changes: none needed. The Tauri IPC layer is already async.
- Consolidating `_with_progress` method variants: tracked as a separate follow-up refactor.

## Milestones

### Milestone 1: Foundation (trait + implementors + tests compile)

**Intention**: Get the trait compiling as async and all 4 implementors updated. Tests that directly call Volume methods
(for example `in_memory_test.rs`, `local_posix_test.rs`) are converted to `#[tokio::test]` in this milestone, because
they won't compile otherwise. Callers in production code use temporary `block_on` bridges.

Steps:

1. **No `async-trait` crate.** On Rust 1.94, `async fn in trait` is stable for static dispatch. For `dyn Volume` (which
   we need for `Arc<dyn Volume>`), we use manual desugaring: methods that need async return
   `Pin<Box<dyn Future<Output = T> + Send + '_>>`. Methods that are trivially sync (`name()`, `root()`,
   `supports_streaming()`, `supports_watching()`, `supports_export()`, etc.) STAY as regular `fn` — no async overhead
   for identity accessors. This avoids the `async-trait` dependency entirely and keeps the trait readable.

   **Why not `async-trait`**: On current Rust stable, `async fn in dyn trait` is not natively supported but manual
   boxing works. `async-trait` is a macro that does the same boxing with nicer syntax but adds a proc-macro dependency
   and makes error messages harder to read. Manual boxing is ~20 methods of
   `-> Pin<Box<dyn Future<Output = ...> + Send + '_>>` but each implementor just wraps the body in
   `Box::pin(async { ... })`. When Rust stabilizes `async fn in dyn trait`, we do a mechanical simplification pass.

2. **Split Volume methods into sync and async categories:**
   - **Stay sync** (return concrete values from struct fields, no I/O): `name()`, `root()`, `supports_watching()`,
     `supports_local_fs_access()`, `supports_export()`, `supports_streaming()`, `local_path()`, `space_poll_interval()`,
     `smb_connection_state()`, `on_unmount()`, `scanner()`, `watcher()`, `inject_error()`
   - **Become async** (do I/O or need to be cancellable): `list_directory()`, `list_directory_with_progress()`,
     `get_metadata()`, `exists()`, `is_directory()`, `create_file()`, `create_directory()`, `delete()`, `rename()`,
     `notify_mutation()`, `scan_for_copy()`, `scan_for_copy_batch()`, `scan_for_conflicts()`, `export_to_local()`,
     `export_to_local_with_progress()`, `import_from_local()`, `import_from_local_with_progress()`, `get_space_info()`,
     `open_read_stream()`, `write_from_stream()`

3. **Convert `VolumeReadStream` to async.** `next_chunk` becomes async (manual boxing:
   `fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = ...> + Send + '_>>`). `total_size()` and `bytes_read()` stay
   sync (they return cached values). This is the key change that prevents nested-runtime panics by design.

4. **Update `LocalPosixVolume`.** Async methods clone the fields they need from `self`, then enter
   `Box::pin(async move { spawn_blocking(move || { ... }).await.unwrap() })`. The `spawn_blocking` closure requires
   `'static`, so you CANNOT capture `&self` — clone `self.root`, `self.name`, etc. before the closure. Pattern:

   ```rust
   fn list_directory(&self, path: &Path) -> Pin<Box<dyn Future<Output = Result<...>> + Send + '_>> {
       let root = self.root.clone();
       let path = path.to_path_buf();
       Box::pin(async move {
           spawn_blocking(move || { /* use root, path — owned, 'static */ }).await.unwrap()
       })
   }
   ```

   `name()` and `root()` stay sync — they return `&self.name` and `&self.root` directly.

5. **Update `InMemoryVolume`.** Async methods wrap in `Box::pin(async { sync_body })`. Bodies unchanged (no I/O).
   Mechanical.

6. **Update `MtpVolume`.** Remove ALL `Handle::block_on` calls. Replace with direct `.await` on the
   `MtpConnectionManager` async methods. Remove the `runtime_handle` field. Remove the `SendSyncProgress` hack. The
   channel-based `MtpChannelStream` (Option E) is replaced — since `next_chunk` is now async, `MtpReadStream` can call
   `download.next_chunk().await` directly. No channel needed.

7. **Update `SmbVolume`.** Remove ALL `Handle::block_on` and `with_smb` bridge calls. Create an async `with_smb_async`
   that handles connection-state transitions and error mapping. Switch from `std::sync::Mutex` to `tokio::sync::Mutex`
   for the SMB session. Call `smb2::SmbClient` methods directly with `.await`.

   **Cancellation safety for `tokio::sync::Mutex`**: Ensure cancellation (via `select!`) only happens BETWEEN SMB
   operations, not while the lock is held. Pattern: acquire lock, do SMB operation, release lock, THEN check
   cancellation. Never `select!` around a lock-holding future.

8. **Convert tests that call Volume methods directly.** `in_memory_test.rs`, `local_posix_test.rs`, `smb.rs` inline
   tests, `mtp.rs` inline tests, `inmemory_test.rs`, write operation `tests.rs` — these all call Volume methods and must
   be `#[tokio::test]` with `.await`.

9. **Temporary bridge for production callers.** At call sites in non-test code (listing pipeline, copy pipeline,
   commands), add `Handle::current().block_on(volume.method())` so the codebase compiles. These are explicitly temporary
   and get removed in milestone 2. Mark each with `// TEMPORARY: remove in async-volume milestone 2`.

**Testing**: Run `./scripts/check.sh` after each implementor to catch regressions immediately.

**Estimated scope**: ~1200-1600 lines changed (includes test conversions).

### Milestone 2: Callers migrate to async

**Intention**: Remove all temporary `block_on` bridges from production callers. Volume pipelines become async loops.
Conflict resolution migrates from `Condvar` to async channels.

Steps:

1. **`WriteOperationState` migration.** Replace `conflict_condvar: std::sync::Condvar` +
   `conflict_mutex: std::sync::Mutex<bool>` with
   `conflict_tx: Option<tokio::sync::mpsc::Sender<ConflictResolutionResponse>>` and
   `conflict_rx: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<ConflictResolutionResponse>>>`. The channel is
   created when a conflict is detected, not at state construction time.

   **All** code that constructs `WriteOperationState` must be updated: `volume_copy.rs`, `volume_move.rs`, `mod.rs`,
   `trash.rs`, `volume_strategy.rs` tests, `tests.rs`, `volume_move.rs` tests.

   The `pending_resolution: RwLock<Option<...>>` field becomes redundant — the resolution value travels through the
   channel. Remove it and all reads/writes to it.

   The `resolve_write_conflict` Tauri command (in `commands/file_system/write_ops.rs`) sends on the channel instead of
   notifying the condvar. `cancel_write_operation` drops the sender (or sends a cancel sentinel) so the awaiting copy
   loop unblocks.

2. **Volume copy pipeline (`volume_copy.rs`).** Remove `spawn_blocking`. `copy_volumes_with_progress` becomes
   `async fn`. Each Volume method call gets `.await`. Progress emission stays the same (atomics + throttled events via
   `OperationEventSink`).

   **Cancellation**: Add `CancellationToken` to the volume write operation state. The per-file loop uses:

   ```rust
   tokio::select! {
       result = copy_single_path(...) => { handle(result) }
       _ = cancel_token.cancelled() => { break; }
   }
   ```

   `cancel_write_operation` triggers the token. The `OperationIntent` state machine stays for modeling
   cancel-vs-rollback business logic.

3. **Volume conflict resolution (`volume_conflict.rs`).** `resolve_volume_conflict` becomes async. Awaits on
   `tokio::time::timeout(30s, conflict_rx.recv())` instead of `Condvar::wait_timeout_while`.

4. **Copy strategy (`volume_strategy.rs`).** `copy_single_path` becomes `async fn`. Streaming:
   `source.open_read_stream().await`, `dest.write_from_stream(...).await`. Local FS branches:
   `source.export_to_local().await` (internally `spawn_blocking`).

5. **Volume move pipeline (`volume_move.rs`).** Same pattern. Remove `spawn_blocking`, await volume methods.

6. **Volume delete pipeline (`delete.rs`).** `delete_volume_files_with_progress` becomes async.

7. **Scan preview (`scan_preview.rs`).** `run_volume_scan_preview` becomes async.

8. **Listing pipeline (`streaming.rs`).** `read_directory_with_progress` for the volume path becomes async. The local
   path (direct `std::fs::read_dir`) stays in `spawn_blocking`.

9. **Space poller (`space_poller.rs`).** `volume.get_space_info().await`.

10. **Commands** (`commands/rename.rs`, `commands/file_system/listing.rs`, `commands/file_system/volume_copy.rs`,
    `commands/file_system/write_ops.rs`, `commands/mtp.rs`). Remove `spawn_blocking` wrappers, call volume methods with
    `.await`. Note: `resolve_write_conflict` in `write_ops.rs` must become async to send on the channel.

11. **Remove all `// TEMPORARY: remove in async-volume milestone 2` bridges.**

**Testing**: Each step should pass `./scripts/check.sh`. The `CollectorEventSink` + `InMemoryVolume` tests exercise the
copy pipeline without Tauri or real I/O.

**Estimated scope**: ~800-1200 lines changed.

### Milestone 3: Cleanup + docs

**Intention**: Remove all dead code from the sync era, update documentation.

Steps:

1. **Remove dead code:**
   - `MtpChannelStream` (Option E channel bridge) — replaced by direct async `MtpReadStream`
   - `runtime_handle` fields on MtpVolume and SmbVolume
   - `SendSyncProgress` hack on MtpVolume
   - `with_smb` sync bridge on SmbVolume (keep only async `with_smb_async`)
   - `Handle::block_on` usage in volume code (verify zero remaining)
   - `_guard = handle.enter()` hacks

2. **Update CLAUDE.md files:**
   - `file_system/volume/CLAUDE.md`: Update architecture diagram, remove "MTP threading" section, update key decisions
     (sync→async rationale), remove nested-`block_on` gotcha, document async method categories (sync vs async), document
     cancellation safety for tokio::sync::Mutex
   - `file_system/write_operations/CLAUDE.md`: Update data flow diagram (no `spawn_blocking` for volume ops), update
     conflict resolution docs (Condvar → channel), note dual cancellation mechanism (CancellationToken for volume ops,
     AtomicU8 for local ops)
   - `mtp/CLAUDE.md`: Remove `block_on` references, update MtpVolume description
   - `AGENTS.md`: Update debugging section if `RUST_LOG` patterns changed

3. **Verify**: `grep -rn 'block_on' src/ --include='*.rs'` should show zero hits in volume code and MTP/SMB code. Only
   hits should be in `LocalPosixVolume` (via `spawn_blocking`, which is different) and non-volume code (indexing
   verifier, etc.).

**Estimated scope**: ~200-400 lines changed.

## Key decisions

**Decision**: Manual `Pin<Box<dyn Future>>` desugaring, not `async-trait` crate **Why**: Rust 1.94 supports
`async fn in trait` for static dispatch. For `dyn Volume` we need boxed futures. `async-trait` is a proc-macro that does
the same thing with nicer syntax but: (a) adds a dependency, (b) makes compiler errors harder to read, (c) forces ALL
methods to be async (even `name()` → boxed future for returning `&str`). Manual boxing lets us keep simple accessors
sync and only box methods that do I/O. When Rust stabilizes `async fn in dyn trait`, we simplify to native syntax.

**Decision**: Split methods into sync and async categories **Why**: `name()`, `root()`, `supports_streaming()` etc. are
identity functions that return struct fields. Making them async would add heap allocation for zero benefit. Keeping them
sync means callers don't pay `.await` overhead for trivial accessors. The async methods are the ones that actually do
I/O. This is the same pattern as `std::io::Read` vs `tokio::io::AsyncRead` — not everything needs to be async.

**Decision**: `name()` and `root()` stay as `fn` returning `&str` / `&Path` **Why**: These return borrowed references
from struct fields. Async methods with `Pin<Box<dyn Future>>` return types need `+ '_` lifetime annotations, and
`spawn_blocking` inside `LocalPosixVolume` can't return borrows from self. By keeping these sync, we avoid lifetime
complexity and boxing overhead for identity accessors.

**Decision**: Replace `Condvar` with `tokio::sync::mpsc::channel(1)`, not `Notify` **Why**: `Notify` is a signal (wake
up), not a value channel. Conflict resolution carries a `ConflictResolutionResponse` payload. `mpsc::channel(1)`
naturally carries the value. Timeout is `tokio::time::timeout(30s, rx.recv())`, matching the current
`wait_timeout_while(30s)` behavior.

**Decision**: Condvar migration touches ALL write operations, not just volume ops **Why**: `WriteOperationState` is a
shared struct used by local copy, local move, trash, and volume operations. Its `conflict_condvar` field must change
globally. However, local operations continue using `spawn_blocking` and `AtomicU8` for their cancellation — they don't
gain `CancellationToken`. This creates a dual mechanism which is acceptable because local ops are inherently sync
(blocking syscalls) and async cancellation wouldn't help them.

**Decision**: Local pipeline conflict resolution uses `blocking_recv()`, not `.await` **Why**: `resolve_conflict` in
`helpers.rs` is called from local copy/move pipelines inside `spawn_blocking`. You can't `.await` inside
`spawn_blocking`. `tokio::sync::mpsc::Receiver::blocking_recv()` is designed for exactly this case — it blocks the
current thread (fine inside `spawn_blocking`) and receives from the channel. The volume pipeline uses the async
`.recv().await`. Same channel type, two recv styles depending on context. This is a safe, well-documented tokio pattern.

**Decision**: Keep `OperationIntent` state machine, add `CancellationToken` alongside **Why**: `OperationIntent`
(`Running → RollingBack → Stopped`) models user intent (cancel vs rollback). This is business logic, not a cancellation
mechanism. `CancellationToken` handles the mechanical "stop the async loop" part. The two work together:
`cancel_write_operation` transitions the intent AND triggers the token.

**Decision**: `SmbVolume` switches from `std::sync::Mutex` to `tokio::sync::Mutex` **Why**: With async methods, lock
contention means awaiting (not blocking a thread). `tokio::sync::Mutex` yields to the runtime instead of blocking.
**Cancellation safety**: `select!` must never cancel a future that holds the lock. Structure code so the lock is
acquired, operation completes, lock drops, THEN cancellation is checked.

**Decision**: `LocalPosixVolume` I/O methods use `spawn_blocking` **Why**: `async { fs::read_dir(path) }` still blocks
the calling task's thread. `spawn_blocking` moves blocking work to a dedicated pool. This keeps the tokio runtime
responsive for MTP/SMB operations that genuinely yield.

**Decision**: Milestone 1 includes test conversions (not deferred to milestone 3) **Why**: Tests that call Volume
methods directly won't compile after the trait changes. Converting them in milestone 1 ensures the test suite stays
green throughout the refactor.

## Risks

**Risk**: Condvar → async channel migration has race conditions in conflict resolution **Mitigation**: The existing
`CollectorEventSink` + `InMemoryVolume` tests cover conflict scenarios. Add specific tests: resolution arrives before
await, resolution arrives after timeout, cancel during conflict wait, rollback during conflict.

**Risk**: `spawn_blocking` overhead for `LocalPosixVolume` high-frequency operations **Mitigation**: Benchmark
`list_directory` before and after. If overhead > 5%, batch into one `spawn_blocking` per listing. Expected: negligible
(<1% for dirs with >100 entries).

**Risk**: Large diff size **Mitigation**: Milestone structure. Mechanical changes (`.await`, `Box::pin`,
`#[tokio::test]`) clearly separated from behavioral changes (conflict resolution, cancellation).

**Risk**: `tokio::sync::Mutex` cancellation leaves SMB in inconsistent state **Mitigation**: Cancellation boundaries are
between operations (after lock is released), never while holding the lock. Enforce via code structure:
`{ let guard = mutex.lock().await; guard.do_thing().await; }` — the guard drops before `select!` can cancel.

## Checking

After each milestone, run:

```bash
./scripts/check.sh
```

This covers clippy, rustfmt, and all Rust + Svelte tests. Additionally:

- Run Docker SMB integration tests for SmbVolume changes
- Manual test: MTP → SMB copy with progress + cancellation (the original use case)
