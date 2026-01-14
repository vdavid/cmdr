# Write operations task list

Implementation checklist for [write-operations.md](./write-operations.md). Complete all items to achieve production
readiness.

## Phase 1: Safety-critical fixes

These must be done first. Without them, user data is at risk.

### 1.1 macOS copyfile integration (spec R1)

- [ ] Create `apps/desktop/src-tauri/src/file_system/macos_copy.rs` module
- [ ] Add FFI bindings for `copyfile()`, `copyfile_state_*` functions (R1.2)
- [ ] Implement `copy_file_native(src, dst, flags)` wrapper
- [ ] Add progress callback support via `copyfile_state_set` (R1.3)
- [ ] Wire progress callback to emit `write-progress` events
- [ ] Test: verify xattrs preserved (`xattr -l` before/after)
- [ ] Test: verify ACLs preserved (`ls -le` before/after)
- [ ] Test: verify clonefile used on APFS (check with `stat` - same inode blocks)

### 1.2 Symlink handling (spec R4)

- [ ] Update copy to use `COPYFILE_NOFOLLOW_SRC` flag
- [ ] Test: copy symlink, verify it's a symlink not a file
- [ ] Test: copy broken symlink, verify preserved
- [ ] Add symlink loop detection in recursive scan (R4.3)
- [ ] Test: create symlink loop, verify error not infinite recursion

### 1.3 Atomic cross-FS moves (spec R2)

- [ ] Implement staging directory pattern (R2.1):
  - [ ] Create `.cmdr-staging-{uuid}` in destination
  - [ ] Copy into staging
  - [ ] Rename from staging to final (atomic)
  - [ ] Delete sources
  - [ ] Remove staging dir
- [ ] Implement rollback on failure (R2.2):
  - [ ] On error, delete staging directory
  - [ ] Verify source files untouched
- [ ] Test: simulate failure mid-copy, verify no orphans
- [ ] Test: simulate failure mid-rename, verify rollback

### 1.4 Copy rollback (spec R3)

- [ ] Add `CopyTransaction` struct to track created files/dirs
- [ ] On copy error, call `transaction.rollback()`
- [ ] Test: create 10 files, fail on 5th, verify first 4 cleaned up
- [ ] Test: create nested dirs, fail mid-copy, verify dirs cleaned up

## Phase 2: UX improvements

### 2.1 Large file progress (spec R6)

- [ ] Implement progress callback in `copy_file_native()`
- [ ] Query `COPYFILE_STATE_COPIED` for bytes copied
- [ ] Emit progress events during single file copy
- [ ] Test: copy 500MB file, verify progress events every 200ms
- [ ] Fallback: implement `copy_file_chunked()` for non-macOS (R6.1)

### 2.2 Conflict handling (spec R5)

- [ ] Add `ConflictResolution` enum to config (R5.1)
- [ ] Implement `Skip` mode: log skip, continue
- [ ] Implement `Overwrite` mode: remove destination first
- [ ] Implement `Rename` mode: append " (1)", " (2)", etc.
- [ ] Add `WriteConflictEvent` for `Stop` mode (R5.2)
- [ ] Add `resolve_write_conflict` command
- [ ] Test: batch copy with conflicts, verify each mode

### 2.3 Error messages (spec R10)

- [ ] Update `WriteOperationError` variants with context fields
- [ ] Add `user_message()` method returning human-friendly strings
- [ ] Include volume name in disk space errors
- [ ] Include actionable hints (e.g., "Check permissions in Finder")
- [ ] Test: trigger each error, verify message is helpful

## Phase 3: Frontend integration

### 3.1 TypeScript types (spec R7.1)

- [ ] Add interfaces to `apps/desktop/src/lib/tauri-commands.ts`:
  - [ ] `WriteOperationConfig`
  - [ ] `WriteOperationStartResult`
  - [ ] `WriteProgressEvent`
  - [ ] `WriteCompleteEvent`
  - [ ] `WriteErrorEvent`
  - [ ] `WriteCancelledEvent`
  - [ ] `WriteConflictEvent`
  - [ ] `WriteOperationError` (discriminated union)
- [ ] Add command wrappers:
  - [ ] `copyFiles()`
  - [ ] `moveFiles()`
  - [ ] `deleteFiles()`
  - [ ] `cancelWriteOperation()`
  - [ ] `resolveWriteConflict()`
- [ ] Run `pnpm svelte-check` - must pass

### 3.2 Event helpers (spec R7.2)

- [ ] Add `onWriteProgress()` subscription helper
- [ ] Add `onWriteComplete()` subscription helper
- [ ] Add `onWriteError()` subscription helper
- [ ] Add `onWriteCancelled()` subscription helper
- [ ] Add `onWriteConflict()` subscription helper

## Phase 4: Testing

### 4.1 Integration tests (spec R9.1)

Create `apps/desktop/src-tauri/src/file_system/write_operations_integration_test.rs`:

**Copy tests:**
- [ ] `test_copy_single_file`
- [ ] `test_copy_directory_recursive`
- [ ] `test_copy_preserves_permissions`
- [ ] `test_copy_preserves_symlinks`
- [ ] `test_copy_preserves_xattrs`
- [ ] `test_copy_handles_broken_symlink`
- [ ] `test_copy_detects_symlink_loop`
- [ ] `test_copy_rollback_on_failure`

**Move tests:**
- [ ] `test_move_same_fs_uses_rename`
- [ ] `test_move_cross_fs_uses_staging`
- [ ] `test_move_cross_fs_atomic`

**Delete tests:**
- [ ] `test_delete_recursive`
- [ ] `test_delete_preserves_on_error`

**Cancellation tests:**
- [ ] `test_cancellation_mid_copy`
- [ ] `test_cancellation_mid_delete`

**Conflict tests:**
- [ ] `test_conflict_stop_mode`
- [ ] `test_conflict_skip_mode`
- [ ] `test_conflict_overwrite_mode`

**Progress tests:**
- [ ] `test_large_file_progress`

**Edge case tests:**
- [ ] `test_concurrent_operations`
- [ ] `test_special_characters_in_paths`
- [ ] `test_long_paths`
- [ ] `test_empty_directory`
- [ ] `test_readonly_source`
- [ ] `test_readonly_destination`

### 4.2 Stress tests (spec R9.2)

- [ ] `test_many_small_files` (10,000 files)
- [ ] `test_large_file` (1GB)
- [ ] `test_deep_nesting` (50 levels)
- [ ] `test_wide_directory` (50,000 files)

### 4.3 Run all tests

- [ ] `./scripts/check.sh --check rust-tests` passes
- [ ] `./scripts/check.sh --check clippy` passes
- [ ] `./scripts/check.sh --check rustfmt` passes

## Phase 5: Performance (optional)

Only after all above is complete and measured.

### 5.1 Benchmarking

- [ ] Create benchmark comparing current vs Finder for:
  - [ ] 1000 small files copy
  - [ ] 1GB file copy
  - [ ] Same-FS move of 10,000 files
- [ ] Document results in `docs/artifacts/notes/write-operations-benchmarks.md`

### 5.2 Parallel delete (spec R8.1)

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
