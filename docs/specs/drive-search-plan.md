# Drive search plan

Whole-drive file search powered by the existing index DB. Users search by filename (wildcards), size, and date — results
appear instantly from an in-memory snapshot of the index. Optional AI mode translates natural language queries into
structured filters.

## Design decisions

### In-memory search index, loaded lazily

**Why**: The index DB has ~5M entries. SQLite `LIKE '%query%'` takes 1–3 seconds (full table scan). Loading all entries
into a `Vec` and searching with rayon gives sub-second results. But 5M entries × ~100 bytes ≈ 500–600 MB — too much to
keep resident permanently. So: load on dialog open, drop after idle. The 2–3 second load time happens while the user
sees "Loading index..." and can start typing their query.

**Memory estimate**: Each `SearchEntry` has two `String` fields (`name`: ~44 bytes avg with heap, `name_folded`: ~44
bytes), plus fixed fields (~33 bytes) = ~120 bytes × 5M ≈ 600 MB. If memory becomes a concern, a future optimization
can use an arena-allocated string buffer with offsets to eliminate per-String heap overhead, roughly halving usage. The
`memory_watchdog` (8 GB warn threshold) should also trigger search index eviction.

### Structured query model, not free-text SQL

**Why**: The backend receives a typed `SearchQuery` struct with optional fields (name pattern, size range, date range).
This is safe (no injection), composable (AI mode just fills the same struct), and simple to execute (single pass over
the in-memory Vec with rayon). The frontend owns the query building UI — the backend is a pure filter engine.

### AI translation happens in the backend

**Why**: The existing AI pattern routes all LLM calls through Rust IPC (`client::chat_completion`). The LLM returns
structured JSON, Rust parses it into an `AiSearchQuery` (with string dates), converts to a `SearchQuery` (with unix
timestamps), and returns both to the frontend. The frontend populates the structured filter UI with the parsed values
(radical transparency — user sees what the AI interpreted) and calls the normal `search_files` command. Two IPC calls:
`translate_search_query` → `search_files`. Clean separation.

### Dialog, not a panel or sidebar

**Why**: Search is a focused, transient task — find a file, go to it, done. A command-palette-style overlay matches this
usage pattern. It doesn't consume permanent screen real estate and dismisses on Escape or result selection, returning
focus to the file explorer.

### Path reconstruction at search time

**Why**: Search results need full paths for display and navigation, but the in-memory index only stores
`(id, parent_id, name, ...)`. Reconstructing paths by walking the parent chain is O(depth) per result. For 30 results
with average depth 8, that's ~240 lookups in a HashMap — microseconds. No need to store full paths in the index
(which would double memory usage).

### Icon IDs derived from extension + is_directory

**Why**: The frontend's `FileIcon` component needs an `iconId` string. For search results, we derive it the same way
the listing code does: `"dir"` for directories, `"ext:{extension}"` for files with extensions, `"file"` for
extensionless files. This avoids loading actual icons at search time (the icon cache handles the rest).

### Single-volume for now, multi-volume later

**Why**: The current codebase indexes only one volume — `start_indexing()` creates a single `IndexManager` for `/`, and
the `INDEXING` static holds one `IndexPhase`. There's no API to enumerate multiple volume DBs. Multi-volume search is a
future milestone. For v1, the search index loads from the single active volume DB via `ReadPool` (which already knows
the DB path). When multi-volume indexing is added, the search module just needs to iterate multiple DBs and merge.

## Non-goals

- **Content search** (searching inside files): Future milestone. The search query model has room for it (`content_pattern`
  field) but no indexing or implementation now.
- **Fuzzy filename matching**: Future enhancement. The `PatternType` enum is designed to grow (`Glob | Regex | Fuzzy`).
  Fuzzy scoring (for example, via `nucleo`) would slot into the same rayon scan as another predicate + scoring function.
- **Real-time result streaming**: The search completes in <200ms on a hot index. No need for streaming — return all
  results in one IPC response.
- **Saved searches / search history**: Out of scope for v1.

## Milestones

### Milestone 1: Backend search engine

Build the in-memory search index and `search_files` IPC command. Fully testable without any UI.

#### 1a. In-memory search index (`indexing/search.rs`, new file)

Create the search index data structure and lifecycle:

```rust
struct SearchEntry {
    id: i64,
    parent_id: i64,
    name: String,         // original filename (for display and path reconstruction)
    name_folded: String,  // pre-normalized for case-insensitive matching
    is_directory: bool,
    size: Option<u64>,
    modified_at: Option<u64>,
}

struct SearchIndex {
    entries: Vec<SearchEntry>,
    id_to_index: HashMap<i64, usize>,  // entry ID → position in entries Vec (for path reconstruction)
    generation: u64,                    // writer generation at load time
}
```

**Load**: `load_search_index(pool: &ReadPool) -> SearchIndex`. Uses `pool.with_conn()` to get a read connection
(consistent with enrichment and verifier patterns). Platform-conditional SQL:
- **macOS**: `SELECT id, parent_id, name, name_folded, is_directory, size, modified_at FROM entries` — `name_folded`
  column exists in schema.
- **Linux**: `SELECT id, parent_id, name, is_directory, size, modified_at FROM entries` — compute `name_folded` in Rust
  via `store::normalize_for_comparison(&name)` during the row-reading loop. On Linux, `name_folded` equals `name`
  (identity function — Linux filesystems are case-sensitive). Glob matching is therefore case-sensitive on Linux,
  matching ext4/btrfs behavior.

Store entries in a `Vec` (rayon parallel iteration is fastest over contiguous memory). Build `id_to_index` HashMap
during load. Takes ~2–3 seconds for 5M rows.

**Global state**: `static SEARCH_INDEX: LazyLock<Mutex<Option<SearchIndexState>>>` where:
```rust
struct SearchIndexState {
    index: Arc<SearchIndex>,
    idle_timer: Option<tokio::task::AbortHandle>,
    load_cancel: Option<Arc<AtomicBool>>,  // for cancelling in-progress loads
}
```
The `Arc` allows search to proceed without holding the mutex. Load replaces the `Option`; idle timeout drops it.

**Generation tracking**: New `pub(super) static WRITER_GENERATION: AtomicU64` in `writer.rs`, initialized to 1 (not 0,
to avoid ambiguity with a freshly constructed search index). Bumped via `fetch_add(1, Relaxed)` on every mutation:
`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`. On `TruncateData`, set to current + 1 via
`fetch_add(1, Relaxed)` (not reset to 0 — that would match a stale search index's initial generation). The search index
stores the generation it was loaded at. On `search_files`, if generations don't match, reload in background (serve
results from stale index — it's still mostly correct). This is an entirely new counter — nothing like it exists in the
writer today.

**Idle timeout**: Two-layer strategy for robustness:
1. Frontend sends `release_search_index` (IPC) when dialog closes → starts a 5-min timer.
2. Server-side backstop: if no `search_files` or `prepare_search_index` call arrives within 10 minutes (regardless of
   whether `release_search_index` was called), drop the index. This handles frontend crashes.

Any new search call cancels both timers and reloads if needed.

**Cancellation**: The index load checks an `AtomicBool` every ~100K rows. If the user closes the dialog before loading
completes, `release_search_index` sets the flag, the load aborts, and no partial index is stored. This follows the
design principle: "All actions longer than ~1 second should be immediately cancelable."

#### 1b. Search execution

`search(index: &SearchIndex, query: &SearchQuery) -> SearchResult` — pure function, no side effects.

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    name_pattern: Option<String>,      // glob: "*.pdf", regex: "Q[1-4].*\\.pdf"
    pattern_type: PatternType,         // default: Glob
    min_size: Option<u64>,             // bytes
    max_size: Option<u64>,
    modified_after: Option<u64>,       // unix timestamp
    modified_before: Option<u64>,
    is_directory: Option<bool>,        // None = both, Some(true) = dirs only, Some(false) = files only
    limit: u32,                        // default 30
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum PatternType {
    #[default]
    Glob,
    Regex,
    // Future: Fuzzy
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResult {
    entries: Vec<SearchResultEntry>,
    total_count: u32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchResultEntry {
    name: String,
    path: String,         // full reconstructed path (for navigation)
    parent_path: String,  // display path (~ replaces home dir prefix)
    is_directory: bool,
    size: Option<u64>,
    modified_at: Option<u64>,
    icon_id: String,
}
```

**Pattern matching**: All matching uses `regex::Regex` internally. At query time:
- **Glob** (`PatternType::Glob`): Convert to regex via `glob_to_regex()` — a small helper that escapes regex
  metacharacters (`.` → `\\.`), converts `*` → `.*` and `?` → `.`, then wraps in `^...$` for full-match semantics.
  Compile with `regex::RegexBuilder::new().case_insensitive(cfg!(target_os = "macos"))`.
- **Regex** (`PatternType::Regex`): Use the pattern directly. Compile with the same case-insensitivity flag.

Apply against `name_folded`. If no `name_pattern` is provided, all entries match the name filter (only size/date
filters apply).

The `regex` crate compiles patterns to a DFA — matching each of 5M strings is O(n) per string with very low constants,
same performance as a custom glob matcher. The crate is already a transitive dependency (used by many Rust ecosystem
crates); add it as a direct dependency.

**Invalid regex handling**: If the user-provided regex doesn't compile, return an error immediately (don't scan).
The frontend shows "Invalid pattern" inline. Glob-to-regex conversion always produces valid regex.

**Parallel scan**: Use `rayon::par_iter()` over `index.entries`. Each thread applies all filters and collects matches
into a thread-local `Vec`. Merge results, sort by relevance:
1. Exact name match (entire filename matches query exactly)
2. Name starts with query pattern (prefix match)
3. Most recently modified (recency is a better relevance signal than path depth)

Take first `limit`, count total.

**Path reconstruction**: Use `index.id_to_index` HashMap for O(1) parent lookups. For each of the `limit` result
entries (not all matches), walk up the `parent_id` chain collecting `name` values, join with `/`, prepend volume mount
point. Cache the home dir prefix for `~` replacement. O(depth) per result, ~240 total lookups for 30 results.

**Directory sizes in results**: For directory results among the 30 returned, fetch `dir_stats` via
`ReadPool` + `store::get_dir_stats_batch_by_ids()` (existing batch API, PK lookup, lock-free). This avoids storing
`dir_stats` for all entries in the search index. One batch call for up to 30 IDs — microseconds.

#### 1c. IPC commands (`commands/search.rs`, new file)

Three commands:

```rust
#[tauri::command]
pub async fn prepare_search_index() -> Result<PrepareResult, String>
// Called when the search dialog opens. Starts loading the index in the background.
// Returns immediately with { ready: bool, entryCount: u64 }.
// If already loaded and fresh, returns { ready: true, entryCount: N }.
// If loading, returns { ready: false, entryCount: 0 }.
// Emits "search-index-ready" event when load completes.

#[tauri::command]
pub async fn search_files(query: SearchQuery) -> Result<SearchResult, String>
// If index not loaded yet, returns Ok(SearchResult { entries: [], total_count: 0 }) — not an error.
// The frontend distinguishes "no results" from "not loaded" via the isIndexReady flag from prepare_search_index.
// Resets the idle/backstop timer on each call.

#[tauri::command]
pub async fn release_search_index()
// Called when the search dialog closes. Starts the 5-min idle timer.
// Also sets the cancellation flag to abort any in-progress load.
```

All three use `#[serde(rename_all = "camelCase")]` on their request/response types to match the frontend conventions.
Add a serde round-trip test to verify serialization matches.

#### 1d. Tests

- **Unit tests in `search.rs`**: glob matching (exact, prefix, suffix, contains, `?` wildcard, case-insensitive on
  macOS), regex matching (alternation `Q[1-4]`, complex patterns, case-insensitive flag), invalid regex error handling,
  glob-to-regex conversion, size filters (min, max, both), date filters (after, before, range), combined filters,
  empty query (returns first N by recency), limit and total_count, path reconstruction.
- **Integration test**: load index from a real SQLite DB (use the existing test helpers to build a tree), search, verify
  results.
- **Serde round-trip test**: `serde_json::to_string` / `from_str` on `SearchQuery` and `SearchResult` to verify
  `camelCase` rename works and `Option` fields serialize as `null`.

#### 1e. CLAUDE.md and docs

- Add a `search.rs` entry to the indexing CLAUDE.md module structure table.
- Add a "Search" section to `docs/architecture.md`.

### Milestone 2: Search dialog (manual mode)

Build the frontend dialog with structured filter inputs and result display.

#### 2a. Command registration

Add to `command-registry.ts`:
```ts
{
    id: 'search.open',
    name: 'Search files',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘F', '⌥F7'],
}
```

Add `case 'search.open':` to `handleCommandExecute` in `+page.svelte` — sets `showSearchDialog = true`. If the search
dialog is already open, this is a no-op (don't re-open).

Add to `dialog-registry.ts`:
```ts
{ id: 'search', description: 'Whole-drive file search' }
```

Add to native menu in `menu.rs`:
- Add `search.open` to `menu_id_to_command` and `command_id_to_menu_id` mappings.
- Add "Search files" menu item to the Edit menu with `⌘F` accelerator.

Add `'search.open'` to the `menuCommands` array in `shortcuts-store.ts` to prevent double-execution (native menu
accelerator + JS shortcut dispatch).

#### 2b. SearchDialog component (`src/lib/search/SearchDialog.svelte`)

The dialog follows the command palette pattern (custom overlay, not `ModalDialog`) because:
- It needs a search input at the top, not a title bar — `ModalDialog`'s title/drag pattern doesn't fit.
- It needs custom keyboard handling (arrow keys for results, Tab between sections) that would fight `ModalDialog`'s
  Escape/focus management.
- The command palette already proves this pattern works and is battle-tested.

Since we're not using `ModalDialog`, we must manually call `notifyDialogOpened('search')` on mount and
`notifyDialogClosed('search')` on destroy for MCP tracking.

**Layout** (all values use design system tokens):

```
┌─────────────────────────────────────────────────────────────────┐
│ 🔍 [ filename pattern input                      ] [✨ Ask AI] │  ← row 1: main input
│ Size: [any ▾] [        ]    Modified: [any ▾] [          ]     │  ← row 2: filters (always visible)
│─────────────────────────────────────────────────────────────────│
│ 📄 report-q4.pdf      ~/Documents/Finance                2 MB  │  ← results (scrollable)
│ 📄 invoice.pdf         ~/Downloads                      340 KB  │
│ 📁 reports             ~/Projects/cmdr              1.2 GB  15  │  ← dirs show recursive size + file count
│ ...                                                             │
│─────────────────────────────────────────────────────────────────│
│ 30 of 1,247 results                                             │  ← status bar
└─────────────────────────────────────────────────────────────────┘
```

**Overlay**: `position: fixed; inset: 0; backdrop-filter: blur(2px); background: rgba(0,0,0, 0.5);
z-index: var(--z-modal)`. Dialog at `padding-top: 10vh`, centered, `width: 680px`. Background:
`var(--color-bg-secondary)`, border: `1px solid var(--color-border-strong)`, radius: `var(--radius-lg)`,
shadow: `var(--shadow-lg)`.

**Input row**: Input with `--font-size-md`, placeholder "Filename pattern (use * and ? as wildcards)". "Ask AI" button
is a mini secondary button, only visible when `ai.provider !== 'off'`. Keyboard: `⌘L` toggles AI mode.

**Filter row**: Always visible. Compact inline layout:
- **Size**: dropdown `[any | ≥ | ≤ | between]` + size input(s) with unit selector `[KB | MB | GB]`. Inputs hidden when
  "any" selected.
- **Modified**: dropdown `[any | after | before | between]` + date input(s). Inputs hidden when "any" selected.

Dropdowns and inputs use `--font-size-sm`, mini styling to keep the row compact.

**Search trigger**: Live search with **200ms debounce**. The search fires on any input change (name pattern, filter
dropdown, size/date value). The debounce prevents IPC pile-up during typing. Enter also triggers an immediate search
(bypasses debounce). This gives the best UX — results update as you type with minimal latency.

**Results list**: `max-height: 400px; overflow-y: auto`. Each result row is a CSS grid:
`grid-template-columns: 16px 1fr auto auto` (icon, name+path, size, date/count).
- File icon: 16px, using `FileIcon` component.
- Filename: `--font-size-sm`, `--color-text-primary`. Glob match highlight using `--color-highlight`.
- Parent path: `--font-size-sm`, `--color-text-tertiary`, truncated with ellipsis.
- Size: `--font-size-sm`, using `formatSizeTriads` for colored size display (reuse from `FullList`).

**Keyboard navigation**: Arrow keys move cursor through results (highlight with `--color-accent-subtle`). Enter on a
result: close dialog, navigate the active pane to the file's parent directory, place cursor on the file. Escape closes
the dialog. Tab cycles between input, filter fields, and results. `stopPropagation` on all keydown events.

**Status bar**: `--font-size-sm`, `--color-text-tertiary`. Left: "N of M results". Right: loading state
("Loading index (2.4M entries)..." → "Ready").

#### 2c. Search state management (`src/lib/search/search-state.svelte.ts`)

Reactive state for the search dialog:

```ts
interface SearchState {
    // UI state
    isOpen: boolean
    isIndexReady: boolean
    indexEntryCount: number
    isSearching: boolean

    // Query fields (bound to UI inputs)
    namePattern: string
    sizeFilter: 'any' | 'gte' | 'lte' | 'between'
    sizeValue: string          // user input, parsed to bytes before search
    sizeUnit: 'KB' | 'MB' | 'GB'
    sizeValueMax: string       // for 'between' mode
    sizeUnitMax: 'KB' | 'MB' | 'GB'
    dateFilter: 'any' | 'after' | 'before' | 'between'
    dateValue: string          // ISO date string
    dateValueMax: string       // for 'between' mode

    // Results
    results: SearchResultEntry[]
    totalCount: number
    cursorIndex: number        // keyboard cursor position in results

    // AI mode
    isAiMode: boolean
    aiStatus: string           // '', 'Calling local LLM...', 'Building query...', etc.
}
```

#### 2d. IPC wrappers (`src/lib/tauri-commands/search.ts`, new file)

```ts
export async function prepareSearchIndex(): Promise<PrepareResult>
export async function searchFiles(query: SearchQuery): Promise<SearchResult>
export async function releaseSearchIndex(): Promise<void>
```

Types in `ipc-types.ts`:
```ts
type PatternType = 'glob' | 'regex'
interface SearchQuery { namePattern?: string, patternType: PatternType, minSize?: number, maxSize?: number, ... }
interface SearchResult { entries: SearchResultEntry[], totalCount: number }
interface SearchResultEntry { name: string, path: string, parentPath: string, ... }
```

Re-export from `index.ts`. Listen for `"search-index-ready"` event.

#### 2e. Navigation on result selection

When the user selects a result (Enter or click):

1. Close the search dialog.
2. Extract `path` from the selected `SearchResultEntry`.
3. Compute parent directory and filename from `path`.
4. Use the existing `FilePane.navigateToPath(parentDir, selectName)` API — it already supports navigating to a directory
   and placing the cursor on a specific file by name. No new explorer API needed.

#### 2f. Tests

- **Vitest**: SearchDialog component rendering, filter state changes, keyboard navigation, result selection callback.
- **Manual testing**: Open dialog, type patterns, verify results, navigate to result, verify ⌘F and ⌥F7 both work,
  verify ⌘F while dialog is open doesn't re-open, verify Escape closes.

### Milestone 3: AI mode

Add natural language search powered by the configured LLM provider.

#### 3a. Prerequisite: refactor `client::chat_completion`

The current `chat_completion` in `ai/client.rs` has hardcoded system prompt, `temperature: 0.6`, `max_tokens: 150`,
and `top_p: 0.95` — tuned for folder suggestions, not search query translation. Refactor to accept options:

```rust
struct ChatCompletionOptions {
    system_prompt: String,
    temperature: f32,
    max_tokens: u32,
    top_p: f32,
}

async fn chat_completion(
    backend: &AiBackend,
    user_prompt: &str,
    options: &ChatCompletionOptions,
) -> Result<String, AiError>
```

Update the existing `get_folder_suggestions` caller to pass the current defaults as explicit options
(`temperature: 0.6`, `max_tokens: 150`, `top_p: 0.95`).

#### 3b. Backend: `translate_search_query` IPC command

Add to `commands/search.rs`:

```rust
/// Intermediate struct for LLM output — uses ISO date strings and explicit pattern type.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiSearchQuery {
    name_pattern: Option<String>,
    pattern_type: Option<String>,     // "glob" or "regex" — LLM specifies which it used
    min_size: Option<u64>,
    max_size: Option<u64>,
    modified_after: Option<String>,   // ISO date "2025-01-01"
    modified_before: Option<String>,
    is_directory: Option<bool>,
}

#[tauri::command]
pub async fn translate_search_query(natural_query: String) -> Result<SearchQuery, String>
```

This calls `client::chat_completion` with `ChatCompletionOptions { temperature: 0.3, max_tokens: 200, top_p: 0.9,
system_prompt }`. Lower temperature + tighter `top_p` for deterministic JSON output. The system prompt (inject today's
date as ISO 8601 `YYYY-MM-DD`):

```
You translate natural language file search queries into structured JSON filters.

Return ONLY a JSON object with these optional fields:
- "namePattern": a filename pattern. Use glob (*, ?) for simple cases, regex for complex ones.
- "patternType": "glob" or "regex" — specify which format you used for namePattern.
- "minSize": size in bytes (e.g., 1048576 for 1 MB)
- "maxSize": size in bytes
- "modifiedAfter": ISO date string (e.g., "2025-01-01")
- "modifiedBefore": ISO date string
- "isDirectory": true for folders only, false for files only, omit for both

Examples:
"large pdfs" → {"namePattern": "*.pdf", "patternType": "glob", "minSize": 10485760}
"quarterly reports" → {"namePattern": "(?i).*(?:Q[1-4]|quarterly).*\\.pdf", "patternType": "regex"}
"photos from last month" → {"namePattern": "*.jpg", "patternType": "glob", "modifiedAfter": "2026-02-15"}
"folders bigger than 1gb" → {"isDirectory": true, "minSize": 1073741824}

Today's date is {today}. Return ONLY the JSON, no explanation.
```

Parse response with `serde_json::from_str::<AiSearchQuery>`. Convert ISO dates to unix timestamps — use a simple
manual parser (`split('-')` → year/month/day → compute days since epoch) to avoid adding `chrono` as a dependency for a
single conversion. On parse failure, return an error string for the frontend: "Couldn't understand that query. Try
rephrasing or use the manual filters."

Return the `SearchQuery` (with timestamps) plus the original `AiSearchQuery` field values so the frontend can display
the human-readable dates in the filter UI.

#### 3c. Frontend: AI mode toggle

When the user clicks "Ask AI" or presses `⌘L` inside the search dialog:
1. `isAiMode = true`
2. Input placeholder changes to "Describe what you're looking for..."
3. On Enter:
   - `aiStatus = "Calling local LLM..."` (or `"Sending your query to {provider}..."` for cloud — be specific per
     design principles: radical transparency)
   - Call `translateSearchQuery(input.value)`
   - `aiStatus = "Getting response..."`
   - On success: populate the structured filter fields with the returned values. Brief highlight animation using
     `--color-accent-subtle` on changed fields (radical transparency — user sees what the AI interpreted).
   - `aiStatus = "Running your search..."`
   - Call `searchFiles(builtQuery)`
   - Display results, `aiStatus = ""`
   - On failure: show the error inline below the input in `--color-text-secondary`, keep AI mode active

#### 3d. Tests

- **Rust unit test**: `AiSearchQuery` deserialization from various JSON shapes (missing fields, all fields, bad dates,
  `patternType` "glob" vs "regex" vs missing/defaulting to glob). ISO date → timestamp conversion.
- **Manual testing**: Test with local LLM and cloud provider, verify filter population, verify error handling.

### Milestone 4: Polish and integration

#### 4a. Index not available state

When indexing is disabled, not started, or still scanning:
- The search dialog still opens but shows a message: "Drive index not ready. Search is available after the initial scan
  completes." with the scan progress if available.
- The "Ask AI" button is hidden.
- The input and filters are disabled.

#### 4b. Accessibility

- `role="dialog"`, `aria-labelledby` on the dialog.
- `aria-label` on all input fields (name pattern, size, date).
- Result list: `role="listbox"`, each result `role="option"`, `aria-selected` for cursor.
- Status bar: `aria-live="polite"` for result count updates.
- Focus management: input auto-focused on open, focus trapped within dialog.

#### 4c. CLAUDE.md updates

- Create `src/lib/search/CLAUDE.md` for the frontend search module.
- Update `indexing/CLAUDE.md` with the search index section, `WRITER_GENERATION` docs.
- Update `docs/architecture.md` with a Search row in the frontend and backend tables.

#### 4d. Run all checks

`./scripts/check.sh --include-slow` — ensure no regressions in existing tests, linting, or coverage.

## Edge cases

- **Empty index**: No entries in DB (fresh install, scan not complete). `load_search_index` returns an empty Vec.
  `search_files` returns 0 results. UI shows appropriate message.
- **Index loading while user types**: The dialog opens, `prepare_search_index` fires, user types and hits Enter before
  the index is ready. Frontend shows "Index still loading..." in status bar. Auto-runs search when `search-index-ready`
  event arrives (if user has typed a query).
- **Very broad queries**: `*` as name pattern with no other filters matches all 5M entries. `total_count` is 5M, but
  only 30 results are returned. User sees "30 of 5,000,000 results" and narrows their search.
- **Network volumes**: The search index only covers locally indexed volumes. Results from network mounts won't appear.
  No special handling needed — the index simply doesn't contain those entries.
- **Result file was deleted**: User navigates to a search result, but the file no longer exists (index is slightly
  stale). The pane navigates to the parent directory. The cursor won't land on the file. Acceptable — the verifier
  catches drift on navigation.
- **Dialog close during load**: User opens search, starts loading, closes immediately. `release_search_index` sets the
  cancellation flag, load aborts, no memory wasted on a partial index.
- **⌘F while dialog is open**: No-op (dialog already open). `handleCommandExecute` checks `showSearchDialog` first.
- **⌘F in text inputs elsewhere**: The `search.open` command is in the `menuCommands` array, so it's handled by the
  native menu accelerator and excluded from the JS shortcut dispatch map. No double-execution.
- **Multiple volumes**: Not supported in v1 (only one volume is indexed). Future milestone.
- **TruncateData during search**: Writer bumps generation. Next search detects mismatch and triggers a background
  reload. Stale results served until reload completes.

## Files changed (summary)

### New files
- `apps/desktop/src-tauri/src/indexing/search.rs` — in-memory search index, search execution, glob-to-regex, pattern matching
- `apps/desktop/src-tauri/src/commands/search.rs` — IPC commands (prepare, search, release, translate)
- `apps/desktop/src/lib/search/SearchDialog.svelte` — search dialog component
- `apps/desktop/src/lib/search/search-state.svelte.ts` — reactive search state
- `apps/desktop/src/lib/search/CLAUDE.md` — module docs
- `apps/desktop/src/lib/tauri-commands/search.ts` — typed IPC wrappers

### Modified files
- `apps/desktop/src-tauri/Cargo.toml` — add `regex` as direct dependency (already a transitive dep)
- `apps/desktop/src-tauri/src/indexing/mod.rs` — export search module
- `apps/desktop/src-tauri/src/indexing/writer.rs` — add `WRITER_GENERATION` AtomicU64, bump on all mutations
- `apps/desktop/src-tauri/src/indexing/CLAUDE.md` — add search.rs docs, WRITER_GENERATION docs
- `apps/desktop/src-tauri/src/ai/client.rs` — refactor `chat_completion` to accept `ChatCompletionOptions`
- `apps/desktop/src-tauri/src/ai/suggestions.rs` — update caller to pass explicit options
- `apps/desktop/src-tauri/src/commands/mod.rs` — register search commands
- `apps/desktop/src-tauri/src/main.rs` or `lib.rs` — register command handlers
- `apps/desktop/src/lib/commands/command-registry.ts` — add `search.open` command
- `apps/desktop/src/lib/ui/dialog-registry.ts` — add search dialog ID
- `apps/desktop/src/lib/shortcuts/shortcuts-store.ts` — add `'search.open'` to `menuCommands`
- `apps/desktop/src/routes/(main)/+page.svelte` — add search dialog rendering + command handling
- `apps/desktop/src/lib/tauri-commands/index.ts` — re-export search commands
- `apps/desktop/src/lib/tauri-commands/ipc-types.ts` — search types
- `apps/desktop/src-tauri/src/menu/mod.rs` — add search menu item + ID mappings
- `docs/architecture.md` — add search row
