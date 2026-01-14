# Write operations: review and concerns

Post-implementation review of the write operations feature.

## Good

### 1. Frontend convenience

- Clean async API with typed events (`copyFiles()`, `moveFiles()`, `deleteFiles()`)
- Event helpers abstract the Tauri listen boilerplate (`onWriteProgress()`, etc.)
- Operation IDs allow tracking multiple concurrent operations
- TypeScript types provide full type safety

### 2. UX/Transparency

- Progress events include phase, current file, file counts, byte counts
- Conflict events show which file is newer and size difference
- Configurable progress interval to balance responsiveness vs overhead

### 3. Speed

- macOS copyfile(3) with APFS clonefile support - optimal for same-volume copies
- Same-FS moves use instant `rename()` - O(1) regardless of file count
- Progress callback uses configurable intervals to reduce overhead
- Cross-FS moves use staging pattern to ensure atomicity

### 4. Safety

- `CopyTransaction` struct tracks created files for rollback on failure
- Staging directory pattern for cross-FS moves protects source until copy completes
- Symlink loop detection prevents infinite recursion
- Source validation before operations begin

### 5. Architecture and testing

- 56 tests passing covering serialization, basic operations, edge cases
- Clean separation between Tauri commands and core logic
- Conflict resolution modes (Stop, Skip, Overwrite, Rename) provide flexibility

---

## Concerns

### 1. Frontend convenience

#### Awkward conflict resolution flow

##### Problem
The conflict resolution requires frontend to coordinate `onWriteConflict` listener with `resolveWriteConflict()` calls, managing state across async boundaries. This is error-prone and requires careful state management.

##### Solution ideas
- Provide a higher-level abstraction that handles the event/response coordination
- Or change to a polling model where frontend queries for pending conflicts
- Or use a callback-based API where conflict handler is passed upfront

##### Recommendation
Defer until frontend integration reveals actual pain points.

#### No operation status query

##### Problem
If frontend crashes or reconnects, it loses context about in-progress operations. No way to query "what operations are running and what's their status?"

##### Solution ideas
- Add `getOperationStatus(operationId)` command
- Add `listActiveOperations()` command
- Store operation state persistently

##### Recommendation
Add `listActiveOperations()` and `getOperationStatus()` commands before shipping.

#### No dry-run mode

##### Problem
Users can't preview what will happen (especially conflicts) before committing to an operation.

##### Solution ideas
- Add `dryRun: boolean` option to config that scans and returns conflicts without executing
- Return a "plan" object showing what would happen

##### Recommendation
Consider adding for v2, not critical for initial release.

#### No batch/queue support

##### Problem
If user wants to queue multiple operations, frontend must manage this entirely.

##### Solution ideas
- Add operation queue on backend
- Or provide explicit guidance for frontend queueing

##### Recommendation
Defer - frontend can manage this initially.

### 2. UX/Transparency

#### No throughput/speed reporting

##### Problem
Users can't see how fast the operation is going (MB/s), which is useful for large transfers.

##### Solution ideas
- Add `bytesPerSecond` field to progress events
- Calculate rolling average over last N seconds

##### Recommendation
Add `bytesPerSecond` to `WriteProgressEvent` - simple enhancement.

#### No ETA

##### Problem
Frontend must calculate ETA from bytes_done/bytes_total and elapsed time.

##### Solution ideas
- Calculate ETA on backend and include in progress events
- Or document that frontend should calculate this

##### Recommendation
Document that frontend should calculate ETA - keeps backend simpler.

#### No upfront conflict preview

##### Problem
User doesn't know how many conflicts exist until they hit them one by one (in Stop mode).

##### Solution ideas
- Scan for conflicts during the Scanning phase
- Return conflict list before starting actual copy
- Add dry-run mode (see above)

##### Recommendation
Consider for v2 alongside dry-run mode.

#### Technical phase names

##### Problem
Phase names like "Scanning", "Copying", "Deleting" are technical. Users might prefer "Preparing...", "Copying files...", etc.

##### Solution ideas
- Change enum values to be more user-friendly
- Or let frontend map technical names to user-friendly strings

##### Recommendation
Let frontend handle display strings - keeps backend API stable.

### 3. Speed

#### No parallel file copying

##### Problem
Many-small-files scenarios could benefit from parallel copying, especially on SSDs.

##### Solution ideas
- Use rayon for parallel file iteration
- Add `parallelism` config option
- Research: how do other file managers handle this?

##### Recommendation
Research needed - benchmark first to see if this is actually a bottleneck.

#### Full upfront scan

##### Problem
We scan all files before starting copy, even if user might cancel early. For huge directories this adds latency.

##### Solution ideas
- Stream scan results while copying begins
- Or add a "quick start" mode that estimates based on first N files

##### Recommendation
Defer - upfront scan provides accurate progress which is valuable.

#### No pipelining

##### Problem
Scan and copy are sequential phases. Could potentially overlap.

##### Solution ideas
- Start copying files as they're discovered during scan
- Requires more complex progress tracking

##### Recommendation
Defer - adds significant complexity for marginal gain.

### 4. Safety

#### CRITICAL: Overwrite deletes destination before copy

##### Problem
Current code in `apply_resolution()`:
```rust
ConflictResolution::Overwrite => {
    fs::remove_file(dest_path)?;  // DELETES DESTINATION
    Ok(Some(dest_path.to_path_buf()))  // THEN we try to copy
}
```
If copy fails after delete, user loses both files. This is **worse than Finder**.

##### Solution ideas
1. Copy to temp file first, then atomic rename
2. Rename destination to `.backup`, copy, then delete backup
3. Use macOS `exchangedata()` or `renamex_np()` with `RENAME_EXCHANGE`

##### Recommendation
**Fix immediately before shipping.** Use temp+rename pattern.

#### No fsync after writes

##### Problem
After copying, data may be in OS write cache. Power loss could lose data.

##### Solution ideas
- Call `fsync()` after each file copy
- Or call `sync()` after entire operation
- Research: does copyfile(3) handle this?

##### Recommendation
Research needed - check if copyfile(3) provides durability guarantees.

#### No checksum verification

##### Problem
No verification that copied data matches source. Silent corruption possible.

##### Solution ideas
- Optional checksum verification after copy
- Use `fcopyfile()` flags if available
- Add `verify: boolean` config option

##### Recommendation
Consider for v2 - adds significant overhead.

#### Race conditions with external modifications

##### Problem
If another process modifies files during our operation, results are undefined.

##### Solution ideas
- Lock files during operation (platform-specific)
- Detect modifications and warn/abort
- Document as known limitation

##### Recommendation
Document as known limitation - same as Finder behavior.

#### Partial batch failure handling

##### Problem
If copying 10 files and file 5 fails, what happens to files 1-4? Currently they're rolled back, but is that always desired?

##### Solution ideas
- Add `stopOnError: boolean` config (current behavior is true)
- Add `continueOnError` mode that skips failed files
- This overlaps with conflict resolution modes

##### Recommendation
Current behavior (rollback on failure) is safe. Document it clearly.

### 5. Architecture and testing

#### No end-to-end tests with Tauri runtime

##### Problem
All tests are unit/integration tests without actual Tauri app handle. The async event coordination is untested.

##### Solution ideas
- Create Tauri test harness
- Use mock app handle
- Manual testing protocol

##### Recommendation
Create manual testing protocol before shipping. Automated e2e tests for v2.

#### No failure injection tests

##### Problem
No tests for disk full, permission denied mid-operation, etc.

##### Solution ideas
- Mock filesystem for failure injection
- Use temp filesystem with quota limits
- Test on actual full disk

##### Recommendation
Add basic failure injection tests before shipping.

#### No concurrent operation tests

##### Problem
Multiple simultaneous operations not tested. Potential for state corruption.

##### Solution ideas
- Add concurrent operation tests
- Stress test with many parallel operations

##### Recommendation
Add basic concurrent operation test before shipping.

#### No race condition tests

##### Problem
External file modifications during operation not tested.

##### Solution ideas
- Test with concurrent file modifications
- Document expected behavior

##### Recommendation
Document expected behavior, defer testing to v2.

---

## Tests

Phase 1 remaining:
- ACL preservation test
- Clonefile verification test
- Mid-copy/mid-rename failure simulation tests
- Rollback tests (need actual failure injection)

Phase 4 remaining:
- test_copy_rollback_on_failure - needs failure injection
- test_move_cross_fs_uses_staging / test_move_cross_fs_atomic - need cross-FS setup
- test_delete_preserves_on_error - needs failure injection
- Cancellation tests - need async test harness with Tauri app handle
- test_large_file_progress - needs big-files test data
- test_concurrent_operations - needs async test harness
- Stress tests - need generated test data

These remaining tests require either:
1. A proper Tauri test harness with app handle for async operations
2. Generated big-files test data (which the generator now supports)
3. Failure injection mechanisms
