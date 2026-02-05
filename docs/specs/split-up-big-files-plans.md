# Large file splitting plans

Analysis of 10 files exceeding 800 lines. All will be split for better architecture.

## Summary table

| File                        | Lines | New modules                                                                  | Largest remaining |
|-----------------------------|-------|------------------------------------------------------------------------------|-------------------|
| `mtp/connection.rs`         | 2260  | errors.rs, types.rs, cache.rs, (operations.rs)                               | ~800 lines        |
| `tauri-commands.ts`         | 2149  | 9 modules by domain                                                          | ~200 lines        |
| `FilePane.svelte`           | 1777  | SelectionManager, MtpController, KeyboardHandler, DirectoryListing           | ~800 lines        |
| `DualPaneExplorer.svelte`   | 1692  | PathNavigation.ts, DialogManager, CopyOperations, Sorting, Keyboard, History | ~400 lines        |
| `operations.rs`             | 1641  | sorting.rs, caching.rs, metadata.rs, streaming.rs                            | ~300 lines        |
| `MtpBrowser.svelte`         | 1514  | mtp-errors.ts, MtpSelection, mtp-keyboard.ts, MtpFileOperations, mtp-format  | ~400 lines        |
| `CopyProgressDialog.svelte` | 1026  | ConflictDialog, progressDialogEvents.ts, ProgressDisplaySection              | ~400 lines        |
| `ai/manager.rs`             | 1024  | process.rs, extract.rs, download.rs, commands.rs                             | ~150 lines        |
| `smb_client.rs`             | 945   | smb_types.rs, smb_cache.rs, smb_util.rs, smb_smbutil.rs, smb_connection.rs   | ~300 lines        |
| `viewer/+page.svelte`       | 872   | SearchLogic.svelte, IndexingPoll.svelte, ScrollAndFetch.svelte               | ~300 lines        |

---

## 1. mtp/connection.rs (2260 lines) — HIGH

**Sections:** Error types (120L), data types (80L), caching (40L), manager lifecycle (200L), directory ops (190L), file
ops (325L), mutation ops (475L), utilities (160L), tests (500L).

**Recommended splits:**

- `errors.rs`: MtpConnectionError enum + Display/Error impls + tests (lines 25-146, 1766-2099)
- `types.rs`: MtpTransferProgress, MtpOperationResult, MtpObjectInfo, ConnectedDeviceInfo + tests (lines 149-229,
  2105-2182)
- `cache.rs`: PathHandleCache, ListingCache, CachedListing, TTL constants (lines 231-268)
- Optional `operations.rs`: download/upload/delete/create/rename/move (lines 771-1572)

**Coupling:** Manager.resolve_path_to_handle() used by operations; acquire_device_lock() universal.

**Order:** errors → types → cache (removes 500L), then operations.

---

## 2. tauri-commands.ts (2149 lines) — HIGH

**Sections:** Listing API (150L), file viewer (120L), file actions (80L), volumes (70L), permissions (30L), network
discovery (50L), SMB shares (70L), known shares (80L), keychain (100L), SMB mounting (50L), licensing (100L), scan
preview (60L), write operations (380L), AI (80L), settings (40L), MTP (400L).

**Recommended splits:**
| New file | Content | Lines |
|----------|---------|-------|
| `file-listing.ts` | Listing API | ~150 |
| `file-viewer.ts` | Viewer + search | ~120 |
| `storage.ts` | Volumes, space, permissions | ~120 |
| `networking.ts` | Network hosts, SMB, keychain, mounting | ~280 |
| `write-operations.ts` | Copy/move/delete + handlers | ~380 |
| `licensing.ts` | License commands | ~100 |
| `mtp.ts` | All MTP commands | ~400 |
| `ui-utilities.ts` | Icons, menus, clipboard | ~100 |
| `settings.ts` | Port checks, AI, watchers | ~120 |

**Coupling:** Shared types from `./file-explorer/types`. Type guards scattered—consolidate in `error-utils.ts`.

**Order:** write-operations → mtp (largest, isolated), then networking.

---

## 3. FilePane.svelte (1777 lines) — HIGH

**Sections:** Selection management (540L), MTP connection (120L), directory listing (140L), keyboard navigation (150L),
network/mounting (820L), state sync (50L).

**Recommended splits:**

- `SelectionManager.svelte`: selectedIndices, anchor, range selection, toggleSelectionAt, selectAll/deselectAll
- `MtpConnectionController.svelte`: MTP device-only logic, auto-connect, error parsing
- `KeyboardHandler.svelte`: handleBriefModeKeys, handleFullModeKeys, view-mode routing
- `DirectoryListing.ts`: loadDirectory(), event listener setup (pure function extraction)

**Coupling:** cursorIndex, listingId, selectedIndices read/written by multiple subsystems. Callback object pattern
recommended.

**Order:** Selection (self-contained) → MTP connection (domain-specific) → Keyboard (has ref deps).

---

## 4. DualPaneExplorer.svelte (1692 lines) — HIGH

**Sections:** State management (80L), sorting (100L, duplicated L/R), volume/path nav (90L), keyboard (180L),
lifecycle (270L), copy operations (420L), new folder (70L), public API (640L).

**Recommended splits:**
| New file | Content |
|----------|---------|
| `PathNavigation.ts` | determineNavigationPath, resolveValidPath (pure TS) |
| `DialogManager.svelte` | 5 dialog show/props pairs + handlers |
| `CopyOperations.svelte` | Local copy, MTP upload/download, progress, 3 dialog types |
| `SortingManager.svelte` | L/R sort handlers (eliminate duplication) |
| `KeyboardDispatcher.svelte` | handleKeyDown, handleKeyUp, function key routing |

**Major issue:** L/R handlers 90-100% identical (sort, volume, MTP error). Bug fixes require 2 changes.

**Order:** PathNavigation.ts (pure, safe) → DialogManager → CopyOperations → SortingManager.

---

## 5. operations.rs (1641 lines) — HIGH

**Sections:** Sorting (245L), FileEntry/metadata (195L), listing cache/virtual scroll (330L), resort (105L), watcher
integration (20L), two-phase metadata (265L), streaming (305L), stats (100L).

**Recommended splits:**
| New file | Content | Lines |
|----------|---------|-------|
| `sorting.rs` | SortColumn, SortOrder, sort_entries(), extract_extension_for_sort | 19-264 |
| `caching.rs` | LISTING_CACHE, CachedListing, virtual scrolling API, resort_listing | 46-946 |
| `metadata.rs` | FileEntry, get_icon_id(), owner/group caching, process_dir_entry | 44-1234 |
| `streaming.rs` | ListingStatus, streaming types, list_directory_start_streaming, cancel_listing | 1240-1539 |

**Coupling:** LISTING_CACHE ↔ watcher, sorting ↔ caching, streaming → caching.

**Order:** sorting → caching → metadata → streaming.

---

## 6. MtpBrowser.svelte (1514 lines) — HIGH

**Sections:** Error handling (105L), selection (90L), keyboard/nav (105L), file operations (195L), transfers (105L),
core state (190L), formatting (15L), UI (420L).

**Recommended splits:**

- `mtp-errors.ts`: extractErrorMessage, isFatalMtpError, getErrorType (reusable across MTP components)
- `MtpSelection.svelte`: selectedIndices, anchor, range selection logic
- `mtp-keyboard.ts`: handleArrowKeys, handleSelectionKeys, handleActionKeys
- `MtpFileOperations.ts`: Delete/rename/newfolder dialogs + operation logic
- `mtp-format.ts`: formatSize, formatDate

**Coupling:** loadDirectory() deeply coupled to state; selection bound to display offset logic.

**Order:** mtp-errors.ts (pure) → MtpSelection → mtp-keyboard.ts → MtpFileOperations.ts.

---

## 7. CopyProgressDialog.svelte (1026 lines) — HIGH

**Sections:** State management (100L), event listeners (120L), conflict resolution (15L), error formatting (25L),
operation control (80L), dialog interaction (60L), stage calculation (15L), UI (210L), styling (390L).

**Recommended splits:**
| New file | Content | Priority |
|----------|---------|----------|
| `ConflictDialog.svelte` | Conflict resolution UI + handleConflictResolution | HIGH |
| `progressDialogEvents.ts` | Event handlers with operationId filtering | HIGH |
| `progressDialogState.ts` | State derivation, stage calculation, formatErrorMessage | MEDIUM |
| `ProgressDisplaySection.svelte` | Normal progress view (stages, bar, stats) | HIGH |

**Coupling:** Event handlers mutate shared state; conflict UI needs conflictEvent passed as props.

**Order:** ConflictDialog (orthogonal) → progressDialogEvents → ProgressDisplaySection.

---

## 8. ai/manager.rs (1024 lines) — HIGH

**Sections:** State management (50L), user-facing commands (150L), process lifecycle (200L), archive extraction (120L),
download management (315L).

**Recommended splits:**

- `process.rs`: start_server_inner, stop_process, is_process_alive, find_available_port (lines 765-964)
- `extract.rs`: extract_bundled_llama_server, extract_llama_server (lines 535-657)
- `download.rs`: do_download, download_file, cleanup_partial (lines 445-760)
- `commands.rs`: All Tauri commands + AiModelInfo (lines 149-300)
- Keep `manager.rs`: init, shutdown, state persistence, orchestration

**Coupling:** Global MANAGER mutex accessed from 10+ functions. Consider StateHandle trait.

**Order:** process.rs → extract.rs → download.rs → commands.rs.

---

## 9. smb_client.rs (945 lines) — HIGH

**Sections:** Types & errors (70L), cache layer (75L), public API (40L), smb-rs core (150L), smbutil fallback (210L),
connection utilities (100L), error classification (30L), share filtering (65L), tests (115L).

**Recommended splits:**

- `smb_types.rs`: ShareInfo, AuthMode, ShareListResult, ShareListError + Display (lines 16-84)
- `smb_cache.rs`: SHARE_CACHE, TTL logic, cache operations (lines 86-160)
- `smb_util.rs`: is_auth_error, classify_error, NDR extraction, disk share filtering (lines 738-832)
- `smb_smbutil.rs`: macOS smbutil wrapper, 3 cfg variants (lines 359-566)
- `smb_connection.rs`: establish_smb_connection, try_list_shares_* (lines 637-736)

**Coupling:** Cache uses ShareListResult (move to types). Error classification used by both smb-rs and smbutil.

**Order:** smb_types → smb_cache → smb_util → smb_smbutil.

---

## 10. viewer/+page.svelte (872 lines)

**Sections:** Virtual scrolling (150L), search (240L), indexing polling (80L), keyboard (70L), window lifecycle (80L),
UI (280L).

**Recommended splits:**

- `SearchLogic.svelte`: Search polling, match navigation, highlight logic
- `IndexingPoll.svelte`: Indexing status monitor
- `ScrollAndFetch.svelte`: Virtual scroll + line fetching

**Coupling:** Search shares state with keyboard handler. Indexing shares state with line fetching.

**Note:** Smallest file in the list but still benefits from extraction. Search module (240L) is self-contained and
frequently modified. Virtual scrolling extraction requires care to avoid prop-drilling performance overhead.

---

## Execution plan

All 10 files should be split. Ordered by risk (safest first):

### Phase 1: Pure TS/Rust modules (no state coupling)

These are zero-risk extractions—just moving code:

- [x] `mtp/connection.rs` → errors.rs, types.rs, cache.rs
- [x] `smb_client.rs` → smb_types.rs, smb_cache.rs, smb_util.rs, smb_smbutil.rs, smb_connection.rs
- [ ] `operations.rs` → sorting.rs, caching.rs, metadata.rs, streaming.rs
- [x] `ai/manager.rs` → process.rs, extract.rs, download.rs, commands.rs
- [x] `tauri-commands.ts` → file-listing.ts, file-viewer.ts, storage.ts, networking.ts, write-operations.ts, licensing.ts, mtp.ts, ui-utilities.ts, settings.ts
- [x] `DualPaneExplorer.svelte` → PathNavigation.ts (pure utility extraction first)

### Phase 2: Svelte component extractions (moderate coupling)

These require careful prop/callback design:

- [ ] `FilePane.svelte` → SelectionManager.svelte, MtpConnectionController.svelte, KeyboardHandler.svelte, DirectoryListing.ts
- [ ] `MtpBrowser.svelte` → mtp-errors.ts, MtpSelection.svelte, mtp-keyboard.ts, MtpFileOperations.ts, mtp-format.ts
- [ ] `CopyProgressDialog.svelte` → ConflictDialog.svelte, progressDialogEvents.ts, progressDialogState.ts, ProgressDisplaySection.svelte
- [ ] `viewer/+page.svelte` → SearchLogic.svelte, IndexingPoll.svelte, ScrollAndFetch.svelte

### Phase 3: Complex Svelte extractions (high coupling, design-sensitive)

- [ ] `DualPaneExplorer.svelte` → DialogManager.svelte, CopyOperations.svelte, SortingManager.svelte,
    KeyboardDispatcher.svelte, HistoryNavigation.svelte

---

## New file count

| Original file             | New modules               |
|---------------------------|---------------------------|
| mtp/connection.rs         | 4 (connection.rs + 3 new) |
| tauri-commands.ts         | 9                         |
| FilePane.svelte           | 4                         |
| DualPaneExplorer.svelte   | 6                         |
| operations.rs             | 5                         |
| MtpBrowser.svelte         | 6                         |
| CopyProgressDialog.svelte | 5                         |
| ai/manager.rs             | 5                         |
| smb_client.rs             | 6                         |
| viewer/+page.svelte       | 4                         |
| **Total**                 | **54 files** (from 10)    |
