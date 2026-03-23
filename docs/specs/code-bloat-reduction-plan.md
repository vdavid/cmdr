# Code bloat reduction plan

Six targeted refactors to eliminate structural duplication found across the codebase. All are
mechanical — no behavior changes, no new features. Each milestone is independently shippable
and testable.

All file paths below are relative to `apps/desktop/`.

## Milestone 1: `start_write_operation` helper (write operations)

**File**: `src-tauri/src/file_system/write_operations/mod.rs`

**Problem**: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start` each repeat ~50 lines
of identical boilerplate: create `WriteOperationState`, register it in `WRITE_OPERATION_STATE` + `OPERATION_STATUS_CACHE`,
spawn `tokio::spawn` + `spawn_blocking`, clean up state on completion, and handle task panics.

**Why this matters**: Four copies of the same spawn/cleanup lifecycle is the single largest duplication in the codebase.
A bug fix in the cleanup path (or a new event, or a new safety check) must be applied in four places.

**Approach**: Extract a generic `start_write_operation` function that owns the shared lifecycle:

```rust
async fn start_write_operation<F>(
    app: tauri::AppHandle,
    operation_type: WriteOperationType,
    progress_interval_ms: u64,
    handler: F,
) -> Result<WriteOperationStartResult, WriteOperationError>
where
    F: FnOnce(tauri::AppHandle, String, Arc<WriteOperationState>) -> Result<(), WriteOperationError>
        + Send + 'static,
```

The handler receives **owned** values (`AppHandle`, `String` for operation ID, `Arc` for state) because the closure
crosses a `tokio::spawn` + `spawn_blocking` boundary and must be `'static`. Each `*_start` function becomes:
validate → call `start_write_operation(app, type, config.progress_interval_ms, |app, id, state| { ... })`.

**Per-operation differences captured by the closure**:
- `copy/move`: closure captures `sources`, `destination`, `config` (all owned/cloned as needed).
- `delete`: closure captures `sources`, `config`, and `Option<Arc<dyn Volume>>` (volume resolved before calling
  the helper). The volume resolution happens in `delete_files_start` between validation and the helper call.
  Verify that `dyn Volume: Send + Sync` (required because `Arc<dyn Volume>` crosses a `tokio::spawn` boundary).
- `trash`: closure captures `sources` and `item_sizes`. Note: `trash_files_with_progress` doesn't take `config` —
  it only uses `state` (which the helper creates from `progress_interval_ms`).

**Constraints**:
- Keep the log line in each caller (operation-specific context like source count).
- Don't pass `&WriteOperationConfig` into the helper — only extract `progress_interval_ms`. Each handler's
  `*_with_progress` function has its own config needs.

**Steps**:
1. Read `mod.rs` fully. Diff the four functions side by side to identify the exact shared skeleton vs. the
   per-operation differences (validation, handler call, log message).
2. Write `start_write_operation` in `mod.rs`. It creates the `WriteOperationState`, registers it, spawns the task,
   cleans up, and handles panics. The `operation_type` param is used in both `register_operation_status` AND the
   panic-handler's `WriteErrorEvent` — don't forget the latter.
3. Rewrite all four `*_start` functions to use it. Each becomes ~10-15 lines (validation + closure).
4. Run `cd apps/desktop/src-tauri && cargo nextest run` + `cargo clippy`. The existing integration tests in
   `integration_test.rs` and `tests.rs` cover the spawn/cleanup lifecycle.

**Testing**: Existing tests. No new tests needed — the refactor is purely structural and the existing test suite
covers copy/move/delete/trash start-to-finish.

---

## Milestone 2: `visible_entries` iterator helper (listing operations)

**File**: `src-tauri/src/file_system/listing/operations.rs`

**Problem**: The pattern `if include_hidden { entries.iter() } else { entries.iter().filter(|e| is_visible(e)) }` is
repeated in `get_file_range`, `get_total_count`, `get_max_filename_width`, `find_file_index`, `find_file_indices`,
`get_file_at`, `get_paths_at_indices`, `resort_listing` (3x), `get_listing_stats`, and `list_directory_start_with_volume`.

**Why this matters**: The duplication makes it easy to forget the filter (a bug), and the `get_file_range`
`include_hidden=false` path collects all visible entries into a Vec before slicing — O(n) allocation for what should
be O(window).

**Approach**: Extract a helper that returns a `Box<dyn Iterator>`:

```rust
fn visible_entries<'a>(
    entries: &'a [FileEntry],
    include_hidden: bool,
) -> Box<dyn Iterator<Item = &'a FileEntry> + 'a> {
    if include_hidden {
        Box::new(entries.iter())
    } else {
        Box::new(entries.iter().filter(|e| is_visible(e)))
    }
}
```

**Also fix `get_file_range` (hidden path only)**: Replace the collect-then-slice with `.skip(start).take(count)`:

```rust
let entries: Vec<FileEntry> = visible_entries(&listing.entries, include_hidden)
    .skip(start)
    .take(count)
    .cloned()
    .collect();
```

Note: the `include_hidden=true` path is already optimal (direct slice). The unified iterator approach is slightly
less optimal for that path (iterator vs direct slice), but the difference is negligible for UI-driven calls.

**Which sites use the iterator vs. collect-then-index**:
- **Iterator-friendly** (use `visible_entries` directly): `get_file_range`, `get_total_count` (`.count()`),
  `get_max_filename_width` (`.map().collect()`), `find_file_index` (`.position()`), `find_file_indices` (`.enumerate()`),
  `list_directory_start_with_volume` (`.count()`).
- **Need collected Vec for indexing by position**: `get_file_at` (`.nth(index)` works but is O(n) — acceptable since
  this is called once per user action, not in a loop), `get_paths_at_indices` (iterates `selected_indices` and needs
  random access — keep collecting), `resort_listing` (3 sites — all collect for position lookups),
  `get_listing_stats` (indexes by `idx` from `selected_indices` — keep collecting).

**Steps**:
1. Add `visible_entries` function near `is_visible` at the top of the file.
2. Replace each occurrence, choosing iterator vs. collect based on the table above.
3. Fix `get_file_range` to use skip/take instead of collect-then-slice.
4. Run `cd apps/desktop/src-tauri && cargo nextest run` + `cargo clippy`.

**Testing**: Existing tests cover all these functions.

---

## Milestone 3: `IoResultExt` trait (write operation error mapping)

**Files**: `copy.rs`, `move_op.rs`, `delete.rs`, `scan.rs`, `helpers.rs`, `chunked_copy.rs`, `copy_strategy.rs`,
`linux_copy.rs`, `macos_copy.rs` (all under `src-tauri/src/file_system/write_operations/`)

**Problem**: `.map_err(|e| WriteOperationError::IoError { path: X.display().to_string(), message: e.to_string() })`
appears 30+ times across these files.

**Why this matters**: It's 3-4 lines of noise at every I/O call site. The pattern is identical — only the path varies.

**Approach**: Add an extension trait in `types.rs` (where `WriteOperationError` lives):

```rust
pub(super) trait IoResultExt<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError> {
        self.map_err(|e| WriteOperationError::IoError {
            path: path.display().to_string(),
            message: e.to_string(),
        })
    }
}
```

Usage: `fs::remove_file(&path).with_path(&path)?;`

**Constraints**:
- Several call sites map non-`io::Error` types (for example, `CString::new` returns `NulError`, some ObjC calls in
  `macos_copy.rs` return custom errors). Those stay as manual `.map_err()`.
- The trait is `pub(super)` — internal to the write_operations module.
- Some call sites use `&file_info.path` (a `PathBuf`), others use `source` (a `&Path`). The trait takes `&Path`,
  which `PathBuf` derefs to, so both work.
- `volume_strategy.rs` maps to `VolumeError::IoError`, not `WriteOperationError::IoError` — doesn't apply.

**Steps**:
1. Add the trait to `types.rs`.
2. Go file by file (including `macos_copy.rs`), replacing each `.map_err(|e| WriteOperationError::IoError { ... })`
   with `.with_path(&path)`. Skip sites that don't map `io::Error`.
3. Run `cd apps/desktop/src-tauri && cargo nextest run` + `cargo clippy`.

**Testing**: Existing tests. Error paths are tested by the integration tests (permission denied, etc.).

---

## Milestone 4: Input handler factory (search dialog)

**File**: `src/lib/search/SearchDialog.svelte`

**Problem**: 11 nearly identical event handler functions (lines 446–506) follow the pattern:
`extract value from event target → call setter → optionally call scheduleSearch()`.

**Why this matters**: 60 lines of mechanical boilerplate that obscures the actual search logic.

**Approach**: Two factory functions — one for text inputs, one for typed selects:

```typescript
function inputHandler(setter: (v: string) => void, search = true) {
    return (e: Event) => {
        setter((e.target as HTMLInputElement).value)
        if (search) scheduleSearch()
    }
}

function selectHandler<T extends string>(setter: (v: T) => void, search = true) {
    return (e: Event) => {
        setter((e.target as HTMLSelectElement).value as T)
        if (search) scheduleSearch()
    }
}
```

Then in the template: `oninput={inputHandler(setNamePattern)}`, `onchange={selectHandler<SizeUnit>(setSizeUnit)}`,
`oninput={inputHandler(setAiPrompt, false)}`.

**About search-state.svelte.ts getter/setter pairs**: Svelte 5's `$state` in `.svelte.ts` modules requires
getter/setter functions for cross-module reactivity (module-level `$state` can't be directly re-exported as reactive).
The getter/setter pattern is a Svelte 5 constraint, not bloat. **Leave them as-is.**

**Steps**:
1. Add `inputHandler` and `selectHandler` factories in the `<script>` block.
2. Replace all 11 handlers with factory calls.
3. Inline `toggleCaseSensitive` and `toggleExcludeSystemDirs` if they're just negation + search.
4. Run `pnpm vitest run` in `apps/desktop` + manual smoke test of the search dialog (type in each field, verify
   search triggers, verify select dropdowns work).

**Testing**: Existing Vitest tests cover `buildSearchQuery`, `parseSizeToBytes`, etc. The handler refactor needs
a manual test of the search dialog.

---

## Milestone 5: `FileEntry` default construction (listing reading)

**Files**: `src-tauri/src/file_system/listing/metadata.rs` and `reading.rs`, plus other construction sites

**Problem**: `FileEntry` has 20 fields, and construction sites in `reading.rs` (3 sites), `mtp/connection/directory_ops.rs`
(2 sites), `file_system/volume/in_memory.rs` (2-3 sites), and `file_system/mock_provider.rs` (1+ sites) each spell out
all 20 fields. 8-10 of those fields are always `None`/default at construction time.

**Why this matters**: Adding a new field to `FileEntry` means updating 8+ construction sites. Easy to miss one.

**Approach**: Add `FileEntry::new()` that takes the essential fields and defaults the rest:

```rust
impl FileEntry {
    pub(crate) fn new(name: String, path: String, is_dir: bool, is_symlink: bool) -> Self {
        Self {
            icon_id: get_icon_id(is_dir, is_symlink, &name),
            name, path,
            is_directory: is_dir, is_symlink,
            size: None, physical_size: None,
            modified_at: None, created_at: None,
            added_at: None, opened_at: None,
            permissions: 0,
            owner: String::new(), group: String::new(),
            extended_metadata_loaded: false,
            recursive_size: None, recursive_physical_size: None,
            recursive_file_count: None, recursive_dir_count: None,
        }
    }
}
```

Callers use struct update syntax for the fields that differ:

```rust
FileEntry {
    size: if metadata.is_file() { Some(metadata.len()) } else { None },
    physical_size,
    modified_at: modified,
    created_at: created,
    permissions: metadata.permissions().mode(),
    owner,
    group,
    ..FileEntry::new(name, path, is_dir, is_symlink)
}
```

**Why not `#[derive(Default)]`**: `Default` would give `icon_id: ""`, `is_directory: false`, etc. — semantically
wrong defaults. A named constructor is more intentional, and `get_icon_id` needs the `name`/`is_dir`/`is_symlink`
values.

**Gotcha — broken-entry fallback**: The broken-entry path in `reading.rs` uses custom `icon_id` values
(`"symlink-broken"` / `"file"`) different from what `get_icon_id` would compute. Use struct update syntax to
override `icon_id`:

```rust
FileEntry {
    icon_id: if is_symlink { "symlink-broken".to_string() } else { "file".to_string() },
    extended_metadata_loaded: true,
    ..FileEntry::new(name, path, false, is_symlink)
}
```

Note: `FileEntry::new` computes `icon_id` internally, which gets discarded when overridden. This wastes one small
string allocation per broken entry — acceptable since broken entries are rare edge cases.

**Skip site**: `src-tauri/src/stubs/mtp.rs` defines a separate `FileEntry` struct (different fields, no `recursive_*`).
Don't touch it.

**Steps**:
1. `grep -r 'FileEntry {' src-tauri/src/` to find ALL construction sites. Expect 8+ production sites AND many test
   files (`sorting_test.rs`, `caching_test.rs`, `watcher_test.rs`, `hidden_files_test.rs`, etc.) — the test files
   likely have more construction sites than production code.
2. Add `FileEntry::new()` in `metadata.rs`.
3. Rewrite each construction site (production and test) to use `FileEntry::new(...)` + struct update syntax.
4. Run `cargo nextest run -p cmdr` + `cargo clippy`.

**Testing**: Existing tests. The constructor is a pure refactor — no logic changes.

---

## Milestone 6: `with_savepoint` helper (index store)

**File**: `src-tauri/src/indexing/store.rs`

**Problem**: `insert_entries_v2_batch` and `upsert_dir_stats_by_id` each repeat the same savepoint boilerplate:
`SAVEPOINT name` → closure → `RELEASE name` on success / `ROLLBACK TO name` on error.

**Why this matters**: Small duplication (2 occurrences), but the pattern is error-prone — getting the savepoint
name wrong or forgetting to rollback on error would silently corrupt data.

**Approach**: Extract a helper function:

```rust
/// Runs `f` inside a SQLite savepoint. Releases on success, rolls back on error.
///
/// SAFETY: `name` is interpolated into SQL. Only pass hardcoded string literals.
fn with_savepoint<F, T>(conn: &Connection, name: &str, f: F) -> Result<T, IndexStoreError>
where
    F: FnOnce(&Connection) -> Result<T, IndexStoreError>,
{
    conn.execute_batch(&format!("SAVEPOINT {name}"))?;
    match f(conn) {
        Ok(val) => {
            conn.execute_batch(&format!("RELEASE {name}"))?;
            Ok(val)
        }
        Err(e) => {
            // Rollback failure is intentionally silenced — the savepoint may already
            // be released or the connection may be in an error state.
            let _ = conn.execute_batch(&format!("ROLLBACK TO {name}"));
            Err(e)
        }
    }
}
```

**Constraints**:
- The helper is a free function (not an `IndexStore` method) since it only needs `&Connection`.
- Only called with hardcoded names (`"insert_entries"`, `"upsert_stats"`).

**Steps**:
1. Add `with_savepoint` as a module-level function in `store.rs`.
2. Rewrite `insert_entries_v2_batch` and `upsert_dir_stats_by_id` to use it.
3. Run `cd apps/desktop/src-tauri && cargo nextest run indexing` + `cargo clippy`.

**Testing**: Existing tests cover both batch insert and dir stats upsert paths.

---

## Execution order

All six milestones are independent — they touch different files with no semantic overlaps. If executing in parallel
(for example, two agents), avoid pairing M1 + M3 since both touch the `write_operations` module and may cause merge
conflicts in `use` statements. All other pairings are safe.

Suggested order if sequential (easiest wins first, build confidence):

1. **M6** (savepoint) — smallest, most mechanical
2. **M3** (IoResultExt) — straightforward trait, many call sites but simple replacement
3. **M5** (FileEntry::new) — small, self-contained
4. **M2** (visible_entries) — moderate, includes the skip/take performance fix
5. **M4** (input handlers) — frontend, needs manual testing
6. **M1** (start_write_operation) — largest, most impactful, benefits from confidence built in earlier milestones

## Final checks

After all milestones: `./scripts/check.sh` (runs all lints, tests, and checks).
