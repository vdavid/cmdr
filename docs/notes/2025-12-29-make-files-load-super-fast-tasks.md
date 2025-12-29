# Make files load super fast: task list

Related: [2025-12-30-load-files-super-fast-spec.md](./2025-12-30-load-files-super-fast-spec.md)

---

## Phase 1: FileDataStore (ADR-009) ✅

Goal: Eliminate Svelte reactivity freeze by keeping file data outside reactive state.

- [x] Create `FileDataStore` class (plain JS, not Svelte)
    - [x] `files: FileEntry[]` — plain array storage
    - [x] `totalCount: number` — for scrollbar sizing
    - [x] `maxFilenameWidth: number` — for Brief mode horizontal scrollbar
    - [x] `getRange(start, end): FileEntry[]` — for virtual scroll
    - [x] `setFiles(entries: FileEntry[]): void` — bulk set
    - [x] `appendFiles(entries: FileEntry[]): void` — for chunked loading
    - [x] `clear(): void` — on navigation
    - [x] `onUpdate` callback mechanism — notify components of changes

- [x] Implement Brief mode width calculation
    - [x] Use `canvas.measureText()` to measure filename widths
    - [x] Calculate in `measureFilenameWidths()` function

- [x] Update `FilePane.svelte` to use FileDataStore
    - [x] Create store instance per pane
    - [x] Replace `allFilesRaw` with store
    - [x] Store provides `getAllFiltered()` for current implementation (full virtual scroll getRange coming in Phase 2)
    - [x] `storeVersion` reactive trigger for component updates

- [ ] Update `BriefList.svelte` and `FullList.svelte` (deferred to Phase 2)
    - [ ] Accept `totalCount` prop for scrollbar sizing
    - [ ] Accept `maxFilenameWidth` prop (Brief mode)
    - [ ] On scroll, call back to parent for new visible range

- [x] Remove old `filesVersion` pattern
    - [x] Delete `filesVersion` state variable (replaced with `storeVersion`)
    - [x] Delete direct `allFilesRaw` mutations

---

## Phase 2: Two-phase metadata loading

Goal: Show files fast with core data, load extended metadata in background.

- [ ] Split `FileEntry` into core vs extended fields
    - [ ] Core: name, path, isDirectory, isSymlink, size, modifiedAt, createdAt, permissions, owner, group, iconId
    - [ ] Extended: addedAt, openedAt (macOS-specific)
    - [ ] Add `extendedMetadataLoaded: boolean` flag

- [ ] Update Rust `list_directory()` to support phased loading
    - [ ] New command: `list_directory_core()` — fast stat() only
    - [ ] New command: `list_directory_extended()` — macOS metadata
    - [ ] Add `// TODO: Apply sort criteria here` placeholder for future sorting

- [ ] Update Rust session management
    - [ ] `list_directory_start` returns count + starts background loading
    - [ ] `list_directory_next_chunk` returns core data chunks
    - [ ] New: `list_directory_get_extended` returns extended metadata

- [ ] Update `FileDataStore` for extended data
    - [ ] `mergeExtendedData(entries: PartialFileEntry[]): void` — merge by path
    - [ ] Track which items have extended data loaded

- [ ] Update UI to handle missing extended metadata
    - [ ] Show placeholder or omit fields if `extendedMetadataLoaded === false`
    - [ ] Update display when extended data arrives

---

## Phase 3: Sorting infrastructure (placeholders)

Goal: Prepare for future sorting feature.

- [ ] Add sorting placeholders in Rust
    - [ ] `// TODO: Apply sort criteria here` before chunking
    - [ ] Document that first chunk should contain "best" files for current sort

- [ ] Add sorting placeholders in FileDataStore
    - [ ] `sortBy(criteria): void` method (stub)
    - [ ] Note: Re-sorting requires re-requesting from backend (sorted order affects which files are "first")

---

## Phase 4: Cancellation (future optimization)

Goal: Stop wasting backend resources when user navigates away quickly.

- [ ] Add cancellation flag in Rust session
    - [ ] `is_cancelled: AtomicBool` in session struct
    - [ ] Check flag periodically in `list_directory()` loop
    - [ ] Exit early if cancelled

- [ ] Wire up cancellation from frontend
    - [ ] `list_directory_end_session` sets cancellation flag
    - [ ] Background thread checks flag and exits

- [ ] Consider using `tokio` for proper async cancellation (complex, evaluate ROI)

---

## Phase 5: Performance tuning

Goal: Optimize chunk sizes and timing.

- [ ] Benchmark different chunk sizes (1000, 2500, 5000, 10000)
    - [ ] Measure IPC overhead vs. perceived responsiveness
    - [ ] Document optimal size for different directory sizes

- [ ] Consider dynamic chunk sizing
    - [ ] Smaller first chunk for faster initial display
    - [ ] Larger subsequent chunks for efficiency

- [ ] Profile and optimize `canvas.measureText()` if needed
    - [ ] Batch measurements
    - [ ] Use `requestIdleCallback` for large directories

---

## Cleanup

- [ ] Remove temporary benchmarking code from `FilePane.svelte` (if any remains)
- [ ] Remove temporary benchmarking code from `operations.rs` (if any remains)
- [ ] Update ADR-009 status if superseded by this work
- [ ] Update `docs/notes/2025-12-28-dir-load-bench-findings.md` with new results

---

## Verification

- [ ] Test with 1k files — should feel instant
- [ ] Test with 5k files — should feel fast (<500ms to first content)
- [ ] Test with 20k files — should feel responsive (<1s to first content)
- [ ] Test with 50k files — should match Commander One (~3s total)
- [ ] Test rapid navigation (enter folder, immediately leave) — no freeze
- [ ] Test hidden files toggle — responsive
- [ ] Test both Brief and Full view modes
