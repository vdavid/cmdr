# Write operations task list

Implementation checklist for [write-operations.md](./write-operations.md). Complete all items to achieve production
readiness.

## Phase 1: Safety-critical fixes

These must be done first. Without them, user data is at risk.

### 1.1 macOS copyfile integration (spec R1)

- [x] Create `apps/desktop/src-tauri/src/file_system/macos_copy.rs` module
- [x] Add FFI bindings for `copyfile()`, `copyfile_state_*` functions (R1.2)
- [x] Implement `copy_file_native(src, dst, flags)` wrapper
- [x] Add progress callback support via `copyfile_state_set` (R1.3)
- [x] Wire progress callback to emit `write-progress` events
- [x] Test: verify xattrs preserved (`xattr -l` before/after) - test_copy_preserves_xattrs
- [ ] Test: verify ACLs preserved (`ls -le` before/after)
- [ ] Test: verify clonefile used on APFS (check with `stat` - same inode blocks)

### 1.2 Symlink handling (spec R4)

- [x] Update copy to use `COPYFILE_NOFOLLOW_SRC` flag
- [x] Test: copy symlink, verify it's a symlink not a file
- [x] Test: copy broken symlink, verify preserved
- [x] Add symlink loop detection in recursive scan (R4.3)
- [x] Test: create symlink loop, verify error not infinite recursion - test_copy_detects_symlink_loop

### 1.3 Atomic cross-FS moves (spec R2)

- [x] Implement staging directory pattern (R2.1):
  - [x] Create `.cmdr-staging-{uuid}` in destination
  - [x] Copy into staging
  - [x] Rename from staging to final (atomic)
  - [x] Delete sources
  - [x] Remove staging dir
- [x] Implement rollback on failure (R2.2):
  - [x] On error, delete staging directory
  - [x] Verify source files untouched
- [ ] Test: simulate failure mid-copy, verify no orphans
- [ ] Test: simulate failure mid-rename, verify rollback

### 1.4 Copy rollback (spec R3)

- [x] Add `CopyTransaction` struct to track created files/dirs
- [x] On copy error, call `transaction.rollback()`
- [ ] Test: create 10 files, fail on 5th, verify first 4 cleaned up
- [ ] Test: create nested dirs, fail mid-copy, verify dirs cleaned up

## Phase 2: UX improvements

### 2.1 Large file progress (spec R6)

- [x] Implement progress callback in `copy_file_native()`
- [x] Query `COPYFILE_STATE_COPIED` for bytes copied
- [x] Emit progress events during single file copy
- [ ] Test: copy 500MB file, verify progress events every 200ms
- [ ] Fallback: implement `copy_file_chunked()` for non-macOS (R6.1)

### 2.2 Conflict handling (spec R5)

- [x] Add `ConflictResolution` enum to config (R5.1)
- [x] Implement `Skip` mode: log skip, continue
- [x] Implement `Overwrite` mode: remove destination first
- [x] Implement `Rename` mode: append " (1)", " (2)", etc.
- [x] Add `WriteConflictEvent` for `Stop` mode (R5.2)
- [x] Add `resolve_write_conflict` command
- [ ] Test: batch copy with conflicts, verify each mode

### 2.3 Error messages (spec R10)

- [x] Update `WriteOperationError` variants with context fields
- [x] Add `user_message()` method returning human-friendly strings
- [x] Include volume name in disk space errors
- [x] Include actionable hints (e.g., "Check permissions in Finder")
- [x] Test: trigger each error, verify message is helpful

## Phase 3: Frontend integration

### 3.1 TypeScript types (spec R7.1)

- [x] Add interfaces to `apps/desktop/src/lib/tauri-commands.ts`:
  - [x] `WriteOperationConfig`
  - [x] `WriteOperationStartResult`
  - [x] `WriteProgressEvent`
  - [x] `WriteCompleteEvent`
  - [x] `WriteErrorEvent`
  - [x] `WriteCancelledEvent`
  - [x] `WriteConflictEvent`
  - [x] `WriteOperationError` (discriminated union)
- [x] Add command wrappers:
  - [x] `copyFiles()`
  - [x] `moveFiles()`
  - [x] `deleteFiles()`
  - [x] `cancelWriteOperation()`
  - [x] `resolveWriteConflict()`
- [x] Run `pnpm svelte-check` - must pass

### 3.2 Event helpers (spec R7.2)

- [x] Add `onWriteProgress()` subscription helper
- [x] Add `onWriteComplete()` subscription helper
- [x] Add `onWriteError()` subscription helper
- [x] Add `onWriteCancelled()` subscription helper
- [x] Add `onWriteConflict()` subscription helper

## Phase 4: Testing

### 4.1 Integration tests (spec R9.1)

Create `apps/desktop/src-tauri/src/file_system/write_operations_integration_test.rs`:

**Copy tests:**
- [x] `test_copy_single_file`
- [x] `test_copy_directory_recursive`
- [x] `test_copy_preserves_permissions`
- [x] `test_copy_preserves_symlinks`
- [x] `test_copy_preserves_xattrs`
- [x] `test_copy_handles_broken_symlink`
- [x] `test_copy_detects_symlink_loop`
- [ ] `test_copy_rollback_on_failure`

**Move tests:**
- [x] `test_move_same_fs_uses_rename`
- [ ] `test_move_cross_fs_uses_staging`
- [ ] `test_move_cross_fs_atomic`

**Delete tests:**
- [x] `test_delete_recursive` (test_delete_directory_manually)
- [ ] `test_delete_preserves_on_error`

**Cancellation tests:**
- [ ] `test_cancellation_mid_copy`
- [ ] `test_cancellation_mid_delete`

**Conflict tests:**
- [x] `test_conflict_stop_mode` (test_conflict_resolution_default)
- [x] `test_conflict_skip_mode` (test_conflict_skip_mode_config)
- [x] `test_conflict_overwrite_mode` (test_conflict_overwrite_mode_config)

**Progress tests:**
- [ ] `test_large_file_progress`

**Edge case tests:**
- [ ] `test_concurrent_operations`
- [x] `test_special_characters_in_paths`
- [x] `test_long_paths`
- [x] `test_empty_directory`
- [x] `test_readonly_source`
- [x] `test_readonly_destination`

### 4.2 Stress tests (spec R9.2)

- [ ] `test_many_small_files` (10,000 files)
- [ ] `test_large_file` (1GB)
- [ ] `test_deep_nesting` (50 levels)
- [ ] `test_wide_directory` (50,000 files)

### 4.3 Run all tests

- [x] `./scripts/check.sh --check rust-tests` passes
- [x] `./scripts/check.sh --check clippy` passes
- [x] `./scripts/check.sh --check rustfmt` passes

## Phase 5: Architectural improvements

Critical fixes and enhancements based on post-implementation review.

### 5.1 Safe overwrite (temp+rename pattern)

- [x] Refactor `apply_resolution()` for Overwrite mode:
  - [x] Copy source to `dest.cmdr-tmp-{uuid}` in same directory
  - [x] Rename original dest to `dest.cmdr-backup-{uuid}`
  - [x] Rename temp to final dest path
  - [x] Delete backup file
  - [x] Handle errors at each step with appropriate recovery
- [ ] Test: overwrite file, verify original preserved if copy fails
- [ ] Test: overwrite file, verify both files intact if rename fails

### 5.2 Async sync at end of operation

- [x] Add `sync()` call after all writes complete
- [x] Make it async so user sees "complete" immediately
- [ ] Test: verify sync is called (mock or observe syscall)

### 5.3 Dry-run mode with streaming conflicts

- [x] Add `dryRun: boolean` to `WriteOperationConfig`
- [x] Add `ScanProgressEvent` type:
  - [x] `operationId`, `filesFound`, `bytesFound`, `conflictsFound`, `currentPath`
- [x] Emit `ScanProgressEvent` every ~300ms during scan
- [x] Stream conflicts as they're found via `scan-conflict` events
- [x] When dry-run completes, emit `dry-run-complete` event with `DryRunResult`:
  - [x] `filesTotal`, `bytesTotal`, `conflicts` (list, max 200 sampled)
  - [x] `conflictsTotal` (exact count, may differ from list length)
- [x] Same-FS moves: still do conflict scan (fast `exists()` checks)
- [x] Test: serialization tests for all new types (12 tests added)
- [ ] Test: dry-run integration with actual files

### 5.4 Operation status query APIs

- [x] Add `listActiveOperations()` command
- [x] Add `getOperationStatus(operationId)` command
- [x] Store operation state in memory (operation_id → status)
- [x] Test: serialization tests for OperationStatus and OperationSummary (4 tests added)
- [ ] Test: list operations shows running operation (integration test)
- [ ] Test: get status returns current progress (integration test)

### 5.5 TypeScript types for new features

- [x] Add `ScanProgressEvent` interface
- [x] Add `DryRunResult` interface
- [x] Add `ConflictInfo` interface
- [x] Add `OperationStatus` interface
- [x] Add `OperationSummary` interface
- [x] Add `dryRun` to `WriteOperationConfig`
- [x] Add `listActiveOperations()` wrapper
- [x] Add `getOperationStatus()` wrapper
- [x] Add `onScanProgress()` event helper
- [x] Add `onScanConflict()` event helper
- [x] Add `onDryRunComplete()` event helper
- [x] Run `pnpm svelte-check` - must pass

## Phase 6: Performance (optional)

Only after all above is complete and measured.

### 6.1 Benchmarking

- [ ] Create benchmark comparing current vs Finder for:
  - [ ] 1000 small files copy
  - [ ] 1GB file copy
  - [ ] Same-FS move of 10,000 files
- [ ] Document results in `docs/artifacts/notes/write-operations-benchmarks.md`

### 6.2 Parallel delete (spec R8.1)

Only if benchmarks show it's beneficial:

- [ ] Add `parallel_delete` feature flag
- [ ] Implement parallel unlink with rayon
- [ ] Benchmark: HDD vs SSD, small vs large files
- [ ] Document when to enable/disable

## Definition of done

All boxes checked AND:

1. [ ] Manual test: copy a folder with Photos.app library, open in Photos, verify intact
2. [ ] Manual test: copy folder to USB drive, verify all files accessible
3. [ ] Manual test: move large folder to different volume, cancel midway, verify source intact
4. [ ] Manual test: delete folder with readonly file inside, verify error message helpful
5. [ ] `ls -l@` on copied file shows same xattrs as original
6. [ ] Copied symlinks are symlinks (not dereferenced files)
7. [ ] Same-FS move of 100,000 file folder completes in <1 second

## Files to create/modify

| File | Action |
|------|--------|
| `src/file_system/macos_copy.rs` | Create - FFI bindings and native copy |
| `src/file_system/write_operations.rs` | Modify - use macos_copy, add staging, rollback |
| `src/file_system/write_operations_integration_test.rs` | Create - integration tests |
| `src/file_system/mod.rs` | Modify - add macos_copy module |
| `src/lib/tauri-commands.ts` | Modify - add TS types and commands |
| `docs/features/write-actions.md` | Update - document new features |

## Estimated effort

| Phase | Effort |
|-------|--------|
| Phase 1 (Safety) | 2-3 days |
| Phase 2 (UX) | 1-2 days |
| Phase 3 (Frontend) | 0.5 day |
| Phase 4 (Testing) | 2-3 days |
| Phase 5 (Performance) | 1 day (optional) |
| **Total** | **6-9 days** |
