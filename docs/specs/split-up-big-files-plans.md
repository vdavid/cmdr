# Large file splitting plan

## Why we're doing this

Large files cause problems:

- **Agent context limits**: Files over 800-1000 lines eat up context windows fast, making AI-assisted dev painful
- **Cognitive load**: Hard to understand, review, and maintain monolithic files
- **Merge conflicts**: Multiple devs touching the same giant file = pain

**Goal**: Keep files under ~800 lines. Split by domain/responsibility, not arbitrarily.

---

## Current status (2025-02-05)

Files over 500 lines, sorted by urgency. Excluding MCP module (separate effort) and test files.

| File                              | Lines | Priority | Status                           |
|-----------------------------------|------:|:--------:|----------------------------------|
| `mtp/connection.rs`               | 3,520 | 🔴 CRIT  | Was "done" but grew 1,260 lines! |
| `FilePane.svelte`                 | 1,860 | 🔴 HIGH  | Never started                    |
| `DualPaneExplorer.svelte`         | 1,414 |  🟡 MED  | Split: sorting/copy/folder/dialog extracted, L/R unified |
| `listing/operations.rs`           |   533 |  ✅ DONE | Split into reading.rs (275) + streaming.rs (418) + operations.rs (533) |
| `write_operations/volume_copy.rs` | 1,143 |  🟡 MED  | New file, already too big        |
| `CopyProgressDialog.svelte`       | 1,026 |  🟡 MED  | Never started                    |
| `commands/file_system.rs`         |   884 |  🟡 MED  | New file                         |
| `viewer/+page.svelte`             |   872 |  🟢 LOW  | Borderline, can wait             |
| `write_operations/scan.rs`        |   848 |  🟢 LOW  | Borderline                       |
| `KeyboardShortcutsSection.svelte` |   776 |  🟢 LOW  | Borderline                       |
| `(main)/+page.svelte`             |   725 |  🟢 LOW  | Borderline                       |
| `licensing/app_status.rs`         |   705 |  🟢 LOW  | Borderline                       |

**What actually got done from the old plan:**

- ✅ `smb_client.rs` → properly split into 5 modules
- ✅ `ai/manager.rs` → down to 656 lines (good enough)
- ✅ `tauri-commands.ts` → split into 9 files (largest now 563 lines)
- ✅ `MtpBrowser.svelte` → removed/refactored away entirely
- ⚠️ `mtp/connection.rs` → types.rs exists but connection.rs wasn't actually split

---

## Phase 1: Pure Rust extractions (zero risk)

Just moving code, no state/prop coupling. Do these first.

### 1.1 `mtp/connection.rs` (3,520 → ~800 lines) — CRITICAL

This is an emergency. 3.5k lines is absurd.

**Current structure** (estimated from size):

- Error types + Display impls (~200 lines)
- Manager struct + lifecycle (~300 lines)
- Path resolution + caching (~200 lines)
- Directory operations (~400 lines)
- File read/download (~400 lines)
- File write/upload (~400 lines)
- Mutation ops (delete/rename/move/mkdir) (~500 lines)
- Utilities (~200 lines)
- Tests (~900 lines)

**Split into:**

| New file           | Content                                       | ~Lines |
|--------------------|-----------------------------------------------|-------:|
| `errors.rs`        | MtpConnectionError enum, Display, Error impls |    200 |
| `cache.rs`         | PathHandleCache, ListingCache, TTL logic      |    200 |
| `directory_ops.rs` | list_objects, resolve_path_to_handle          |    400 |
| `file_ops.rs`      | download_file, upload_file, read operations   |    800 |
| `mutation_ops.rs`  | delete, rename, move, create_folder           |    500 |
| `tests.rs`         | All tests (can stay large, it's tests)        |    900 |
| `connection.rs`    | Manager struct, lifecycle, utilities          |    500 |

**Order**: errors → cache → directory_ops → file_ops → mutation_ops → move tests

### 1.2 `listing/operations.rs` (1,407 → ~400 lines)

**Split into:**

| New file        | Content                                             | ~Lines |
|-----------------|-----------------------------------------------------|-------:|
| `sorting.rs`    | SortColumn, SortOrder, sort_entries()               |    250 |
| `cache.rs`      | LISTING_CACHE, CachedListing, virtual scroll API    |    350 |
| `metadata.rs`   | FileEntry construction, icon detection, owner/group |    300 |
| `streaming.rs`  | ListingStatus, streaming start/cancel               |    300 |
| `operations.rs` | Orchestration, public API                           |    200 |

**Order**: sorting → metadata → cache → streaming

### 1.3 `write_operations/volume_copy.rs` (1,143 → ~500 lines)

**Split into:**

| New file                | Content                                               | ~Lines |
|-------------------------|-------------------------------------------------------|-------:|
| `volume_strategy.rs`    | Cross-volume strategy selection, local vs MTP routing |    300 |
| `progress_reporting.rs` | Progress event emission, rate calculation             |    250 |
| `volume_copy.rs`        | Main copy flow, conflict handling                     |    500 |

### 1.4 `commands/file_system.rs` (884 → ~400 lines)

**Split into:**

| New file                  | Content                           | ~Lines |
|---------------------------|-----------------------------------|-------:|
| `commands/listing.rs`     | Directory listing commands        |    300 |
| `commands/write.rs`       | Copy/move/delete command handlers |    300 |
| `commands/file_system.rs` | Path utilities, status queries    |    280 |

---

## Phase 2: Svelte component extractions (moderate coupling)

These need careful prop/callback design. Extract pure logic first, then UI chunks.

### 2.1 `FilePane.svelte` (1,860 → ~700 lines)

**Extract:**

| New file               | Content                               | Approach                              |
|------------------------|---------------------------------------|---------------------------------------|
| `selection-logic.ts`   | Selection state, range select, toggle | Pure TS module with state object      |
| `keyboard-handlers.ts` | Brief/Full mode key handlers          | Pure functions taking state+callbacks |
| `DirectoryLoader.ts`   | loadDirectory(), event setup          | Pure async functions                  |

Keep in FilePane: UI markup, lifecycle, state binding, scrolling

### 2.2 `DualPaneExplorer.svelte` (1,550 → ~600 lines)

PathNavigation.ts already extracted. Continue:

| New file                   | Content                                      | Approach         |
|----------------------------|----------------------------------------------|------------------|
| `dialog-state.ts`          | Dialog visibility flags, show/hide functions | Pure TS          |
| `copy-operations.ts`       | Copy initiation logic, MTP upload/download   | Pure TS          |
| `sorting-handlers.ts`      | Sort state management (dedupe L/R!)          | Pure TS          |
| `CopyDialogManager.svelte` | All copy-related dialogs                     | Svelte component |

**Bug to fix**: L/R handlers are 90% identical. Unify them.

### 2.3 `CopyProgressDialog.svelte` (1,026 → ~500 lines)

| New file                          | Content                                        | Approach           |
|-----------------------------------|------------------------------------------------|--------------------|
| `ConflictResolutionDialog.svelte` | Conflict UI, resolution handlers               | Separate component |
| `progress-events.ts`              | Event listener setup, filtering by operationId | Pure TS            |
| `progress-calculations.ts`        | Stage derivation, time estimates, formatting   | Pure TS            |

---

## Phase 3: Lower priority (do when touching these files)

These are borderline (700-900 lines). Don't prioritize, but split if you're already modifying them.

- `viewer/+page.svelte` (872) → Extract search logic, virtual scroll
- `write_operations/scan.rs` (848) → Extract conflict detection
- `KeyboardShortcutsSection.svelte` (776) → Extract shortcut editing logic
- `(main)/+page.svelte` (725) → Extract keyboard handler
- `licensing/app_status.rs` (705) → Fine as-is for now

---

## Execution checklist

### Phase 1 (Rust)

- [x] `mtp/connection.rs` — errors.rs
- [x] `mtp/connection.rs` — cache.rs
- [x] `mtp/connection.rs` — directory_ops.rs
- [x] `mtp/connection.rs` — file_ops.rs
- [x] `mtp/connection.rs` — mutation_ops.rs
- [x] `mtp/connection.rs` — move tests
- [x] `listing/operations.rs` — sorting.rs
- [x] `listing/operations.rs` — metadata.rs
- [x] `listing/operations.rs` — cache.rs (as caching.rs)
- [x] `listing/operations.rs` — streaming.rs (types + impl)
- [x] `listing/operations.rs` — reading.rs (disk I/O, deduped list_directory/list_directory_core)
- [ ] `write_operations/volume_copy.rs` — split
- [ ] `commands/file_system.rs` — split

### Phase 2 (Svelte/TS)

- [ ] `FilePane.svelte` — selection-logic.ts
- [ ] `FilePane.svelte` — keyboard-handlers.ts
- [ ] `FilePane.svelte` — DirectoryLoader.ts
- [x] `DualPaneExplorer.svelte` — copy-operations.ts + new-folder-operations.ts
- [x] `DualPaneExplorer.svelte` — sorting-handlers.ts (L/R unified!)
- [x] `DualPaneExplorer.svelte` — DialogManager.svelte (replaces planned CopyDialogManager + dialog-state)
- [ ] `CopyProgressDialog.svelte` — ConflictResolutionDialog.svelte
- [ ] `CopyProgressDialog.svelte` — progress-events.ts
- [ ] `CopyProgressDialog.svelte` — progress-calculations.ts

### Phase 3 (opportunistic)

- [ ] `viewer/+page.svelte`
- [ ] `write_operations/scan.rs`
- [ ] `KeyboardShortcutsSection.svelte`
- [ ] `(main)/+page.svelte`

---

## Notes

- **Don't over-split**: 400-600 line files are fine. We're avoiding 1000+ monsters, not creating 50-line fragments.
- **Tests can be large**: Test files over 1000 lines are okay—they're not read as often and don't need splitting.
- **MCP excluded**: Another agent is restructuring that module separately.
- **Run checks after each split**: `./scripts/check.sh --rust` or `--svelte` to catch breakage early.
