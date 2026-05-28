# File viewer module (Rust backend)

Provides three backend strategies for serving file content line-by-line with instant open, virtual scrolling, and background search.

Frontend counterpart: [`apps/desktop/src/routes/viewer/CLAUDE.md`](../../../src/routes/viewer/CLAUDE.md) for the viewer route shell (window lifecycle, scroll/search composables) and [`apps/desktop/src/lib/file-viewer/CLAUDE.md`](../../../src/lib/file-viewer/CLAUDE.md) for the reusable open-viewer helper and binary-warning classifier.

## Key files

- `mod.rs`: public API, constants (1MB threshold, 256-line checkpoints, 8KB backward scan limit), `ViewerError` typed
  enum
- `session.rs`: session orchestration, backend switching, search state, per-read cancel registry (`active_reads`),
  encoding-switch (`set_encoding`), drain-and-swap-under-lock protocol via `pending_grew`, `read_range` and
  `cancel_read` entry points
- `range_read.rs`: backend-agnostic stitching of a `(line, offset) -> (line, offset)` range into one UTF-8 string,
  UTF-16 -> UTF-8 offset clamp (surrogate-safe), streaming via byte-offset seeks to keep `ByteSeek` honest
- `encoding.rs`: `FileEncoding` enum (UTF-8, UTF-8 with BOM, Windows-1252, ISO-8859-1, Mac Roman, US-ASCII, UTF-16 LE,
  UTF-16 BE), BOM + 64 KB heuristic detection, `NewlineScanner` with carry-byte state for UTF-16 chunked reads,
  `find_newlines` / `decode_line`, `same_byte_layout` predicate
- `full_load.rs`: loads entire file into `String` (<1MB files); decodes per `FileEncoding`
- `byte_seek.rs`: seeks by byte offset, scans backward for newline (instant open); ASCII-compatible encodings use the
  `memchr` fast path, UTF-16 uses `NewlineScanner` with byte-aligned reads
- `line_index.rs`: sparse newline index (1 checkpoint per 256 lines), SIMD-accelerated via `memchr` for
  ASCII-compatible encodings, `NewlineScanner`-driven for UTF-16; `extend_to(&self, new_size, cancel) -> Self` produces
  an extended backend by value
- `search_matcher.rs`: `Matcher` (literal or regex), `SearchMode`, `scan_line_with_matcher` helper. One matcher built
  per search; reused across every line. Huge-line chunking (1 MB windows, 256 byte overlap) lives here.
- `*_test.rs`: unit tests for each backend: UTF-8 edge cases, search highlighting, checkpoint math, range reads,
  cancellation, encoding detection, UTF-16 newline scanning, encoding-switch rebuild + drain-and-swap

## Backend selection logic

```rust
if file_size < 1MB {
    FullLoadBackend
} else {
    // Start with ByteSeek (instant)
    ByteSeekBackend
    // Spawn background thread to build LineIndex
    // Upgrade to LineIndexBackend when ready
}
```

## Tauri commands

- `viewer_open(path)` → `ViewerOpenResult` (session ID, metadata, initial lines, backend type)
- `viewer_get_lines(session_id, target_type, target_value, count)` → `LineChunk`
- `viewer_read_range(session_id, read_id, anchor, focus)` → `Result<String, ViewerError>`: reads a logical
  `(line, offset)` range as one UTF-8 string. Endpoints are `RangeEnd::Line { line, offset }` (UTF-16 code unit offset)
  or `RangeEnd::Eof` (used by ⌘A in ByteSeek-no-index mode). `read_id` is FE-allocated so cancel can land without an
  extra round-trip. The function holds the SESSIONS lock only long enough to clone the backend `Arc` and register the
  cancel flag; the read itself iterates outside the lock so other commands stay responsive.
- `viewer_cancel_read(session_id, read_id)` → flips the per-read cancel flag. No-op if the read already finished.
- `viewer_write_range_to_file(session_id, read_id, anchor, focus, dest_path)` → reads a logical range and writes it
  atomically to `dest_path` (temp+rename). Used by "Save as file…" in the copy dialogs. Same cancellation plumbing as
  `viewer_read_range`. Temp suffix includes the `read_id` for crash isolation.
- `viewer_search_start(session_id, query, mode)` → starts background search. `mode = { useRegex, caseSensitive }`. An
  invalid regex pattern (parse error, exceeds size limits) or a multiline pattern (`(?s)`, literal newline, `\n`
  escape) makes the search status flip to `InvalidQuery { message }` synchronously; the worker isn't spawned. `(?m)`
  is fine because it only affects `^` / `$` within a line slice.
- `viewer_search_poll(session_id)` → `SearchPollResult` (matches, progress, status). `status` is a tagged union
  `{ status: "running" | "done" | "cancelled" | "idle" | "invalidQuery", message?: string }`.
- `viewer_search_cancel(session_id)` → cancels running search
- `viewer_close(session_id)` → frees resources (also signals every in-flight read to cancel)
- `viewer_setup_menu(label)`: builds viewer menu with word wrap item
- `viewer_set_word_wrap(label, checked)`: syncs menu state
- `viewer_get_encoding_options(session_id)` → `EncodingOptions`: current selection, detected encoding, and the full list
  of selectable encodings with their labels and groups. The FE renders the dropdown straight from this; no encoding list
  lives on the FE.
- `viewer_set_encoding(session_id, encoding)`: switches the active encoding. Instant when `same_byte_layout(current,
  new)` holds (UTF-8 ↔ Windows-1252 family): the active backend's `with_encoding(new)` method returns a fresh backend
  with only the encoding field swapped, no reindex. Otherwise snaps to ByteSeek immediately and rebuilds LineIndex in
  the background. The FE polls `viewer_get_status` for `is_indexing` to track the rebuild.
- `viewer_set_tail_mode(session_id, enabled)`: flips the per-session tail-mode flag. When true, watcher `Grew` events
  trigger an `extend_to` on the active backend so the open viewport auto-follows newly appended bytes. When false, the
  FE still hears `viewer:file-changed:<sid>` events and renders a persistent reload toast. Enabling also catches the
  backend up to the current on-disk size in one step.
- `viewer_reload(session_id)`: reopens the active backend against the file on disk under the session's current
  encoding. Called by the FE's reload toast and on rotation (`Shrunk` / `Replaced`).

## Tail mode + external-change watcher

The viewer watcher (`watcher.rs`) is a shared singleton modelled on the existing
`apps/desktop/src-tauri/src/file_system/watcher.rs`. One `notify-debouncer-full` debouncer per watched path; each
`ViewerSession` holds a single subscription via `VIEWER_WATCHER_MANAGER.subscribe(path)`. Dropping the subscription
unregisters the path. Debounce window is 300 ms.

Classification per debounce window:

- `Grew(new_size)` when `metadata.len()` grew vs. last-known.
- `Shrunk` when the size dropped (truncation, in-place reset).
- `Replaced` when the inode changed (rename + atomic replace, log rotation).
- `MetadataOnly` when nothing observable changed.

Per-session, a manager thread (`spawn_watcher_manager_thread`) consumes events on the subscription channel:

- Always emits `viewer:file-changed:<sid>` with `{ kind: "grew", newSize }` or `{ kind: "rotated" }`.
- `Grew` with `upgrading` or `rebuilding` in flight: queues `pending_grew = Some(new_size.max(prev))` (drain-and-swap
  protocol from § "Key decisions").
- `Grew` with no in-flight transition AND tail mode on: re-reads the current backend via `ArcSwap::load_full()` on
  every event (no cached `Arc`), calls `extend_to_boxed(new_size)`, and `backend.store(extended)`.
- `Shrunk` / `Replaced`: best-effort `reload(session_id)` which reopens the backend under the session's current
  encoding.

`extend_to_boxed` is a trait method on `FileViewerBackend` with backend-specific impls:

- `LineIndexBackend::extend_to(new_size, cancel)` opens the file, seeks to `self.total_bytes`, drives a
  `NewlineScanner` started at that offset over the new range, clones the checkpoint vec and appends new entries.
- `ByteSeekBackend::extend_to` returns a fresh `ByteSeekBackend` with the updated size field.
- `FullLoadBackend::extend_to_boxed` returns `ViewerError::Io` — the session is responsible for escalating FullLoad →
  ByteSeek before any append crosses `FULL_LOAD_THRESHOLD`.

## Gotchas (tail mode)

**Watcher subscribe happens AFTER `SESSIONS.insert` but BEFORE the upgrade spawn.** `notify-debouncer-full::new_debouncer`
plus `debouncer.watch` need the session already in `SESSIONS` so the manager thread can look it up. They run before the
upgrade thread spawn so the watcher captures any append that lands during the upgrade window; otherwise an append
arriving between SESSIONS.insert and the watcher's first event would be observed by no one (the upgrade has already
stored its LineIndex covering only the pre-append EOF, and no later FS event ever fires for that one append). Pinned by
`tail_mode_on_extends_backend_when_watcher_reports_grew` and `test_append_during_upgrade_not_dropped`.

**Tail-extend race against an encoding rebuild.** `apply_tail_extend` snapshots the active backend `Arc`, drops the
SESSIONS lock, runs `extend_to_boxed` (multi-second on a multi-MB append), then re-acquires the lock. If an encoding
rebuild installed a fresh backend during the unlocked window, storing the stale extend would clobber it. The fix:
snapshot via `ArcSwap::load_full()` BEFORE the extend; after the extend, re-load and compare with `Arc::ptr_eq`. On
mismatch, discard the stale extend and re-queue the EOF into `pending_grew` so the rebuild's drain (or a follow-up FS
event) still catches up. Pinned by `test_tail_extend_during_encoding_rebuild_discards_stale_extend`.

**`watcher.rs` canonicalises paths** so `/var/folders/...` (the tempfile path on macOS) and `/private/var/folders/...`
(the equivalent without the symlink) collapse into the same registration. `test_only_emit` walks the same stored
canonical paths.

**Watcher subscription is process-wide and shared.** Two sessions on the same path share one debouncer; the subscriber
list is keyed by path. Dropping the last subscription unwatches the path.

**Manager thread polls with a 200 ms timeout.** This is the only path that lets `close_session` make the manager exit
when the file is idle (no events). Without it, `recv` would block forever and the thread would leak.

## Key decisions

**Decision**: Three-backend architecture (FullLoad / ByteSeek / LineIndex) instead of one general-purpose backend.
**Why**: The core constraint is that opening a file must feel instant regardless of size. FullLoad is simplest and gives perfect random access, but loading a 1 GB file into memory is unacceptable. ByteSeek opens any file in O(1) time but can't seek by line number (only by byte offset or fraction). LineIndex gives O(1) line seeking but requires a full file scan first. The three-tier approach gives instant open (ByteSeek), then upgrades to precise line navigation (LineIndex) once the background scan finishes.

**Decision**: ByteSeek-to-LineIndex upgrade happens in a background thread with a 5-second timeout.
**Why**: On fast SSDs, indexing a 1 GB file takes ~2 seconds and the upgrade is seamless. But on slow disks or network drives, indexing could take minutes. The 5s timeout prevents the indexer from hammering a slow volume indefinitely. If it times out, the session stays in ByteSeek mode. The user can still scroll (via fraction seeking) and search, they just don't get exact line numbers.

**Decision**: Search always uses a fresh `ByteSeekBackend` instance in a separate thread, even when the session uses `LineIndex`.
**Why**: Search is a streaming full-file scan regardless of backend. The line index doesn't help find text matches. Using `ByteSeekBackend` for search keeps the search thread independent of the session's primary backend, avoiding lock contention. Opening a fresh file handle also means search doesn't interfere with the user scrolling in the main session.

**Decision**: `SearchMatch.column` and `.length` use UTF-16 code units instead of byte or char offsets.
**Why**: The frontend is JavaScript, where `String.prototype.length` and `String.prototype.substring()` count UTF-16 code units. If the backend returned byte offsets or Unicode scalar offsets, the frontend would need to convert on every match highlight, which is error-prone for text with emoji or CJK characters. Matching the JS string model eliminates an entire class of off-by-one bugs in the highlight rendering.

**Decision**: One `Matcher` (literal or regex) is compiled at `search_start` and reused for every line in the file.
**Why**: The regex crate's `Regex::new` builds the NFA / lazy DFA up front. Recompiling per line would tank throughput
on million-line files. `Matcher::Literal` carries the pre-lowercased haystack form when case-insensitive so each line
scan only pays for the per-line `to_lowercase()` (skipped entirely in the case-sensitive path). The `find_matches`
callback returns `ControlFlow` so the backend can stop at the match limit or on a cancel signal without scanning the
rest of the line.

**Decision**: Reject cross-line regex patterns (`(?s)`, literal `\n`, `\n` escape) at build time; accept `(?m)`.
**Why**: Our search engine streams line by line, so a cross-line pattern silently never matches. A clear error at
build time is better UX than "no matches" for a query that can never match. `(?m)` only changes the meaning of `^` /
`$` within the current slice; it doesn't cross newlines, so it's safe.

**Decision**: Per-pattern memory caps (`size_limit = 8 MB`, `dfa_size_limit = 8 MB`) on regex builds.
**Why**: The watchdog assumes the per-call cost of `Regex::find_iter` stays bounded by the `regex` crate's linear-time
guarantee. The guarantee holds while the lazy DFA stays under `dfa_size_limit`. A pathological pattern that would blow
the budget at scan time is rejected at build time via `MatcherBuildError::InvalidRegex("regex too complex")`.

**Decision**: Per-match cancellation inside the scan loop, not just per-chunk / per-line.
**Why**: A 1 MB line with 100,000 matches would block cancellation for seconds without per-match cancel. The
`scan_line_with_matcher` helper checks the cancel flag once per match (inside the `Matcher::find_matches` callback)
and breaks out via `ControlFlow::Break(())`. This solves the "many matches on a moderate line" case. The watchdog
covers the orthogonal "single `iter.next()` call took too long" case.

**Decision**: Huge-line chunking with 1 MB windows and 256-byte overlap.
**Why**: Lines longer than 1 MB are rare but real (machine-generated JSON, minified JS). Without chunking, search on a
5 MB line allocates a 5 MB `to_lowercase()` buffer for every line scan in case-insensitive mode AND blocks
per-line cancellation. The chunked scan keeps the working set bounded and a needle straddling a chunk boundary is
still found exactly once: matches starting in `[0, chunk_len - overlap)` are reported, matches starting in the overlap
belong to the next chunk.

**Decision**: `ViewerSession.backend` is `Arc<ArcSwap<Box<dyn FileViewerBackend>>>`, not `Arc<dyn …>` or
`RwLock<Box<dyn …>>`.
**Why**: Background threads (ByteSeek → LineIndex upgrade, encoding rebuild, future tail-mode `extend_to`) need to
replace the active backend without taking a write lock on the `get_lines` read path. Each backend is immutable; readers
hold an `Arc<Box<dyn FileViewerBackend>>` from `load_full()` and finish their call against whichever backend was current
at load time. Mid-swap there's no torn read because the old `Arc` is only dropped when the last reader releases it. A
`RwLock` would either block readers on a heavy rebuild or force the rebuild to copy data into the lock; `ArcSwap` is
both lock-free for readers and zero-copy for the writer.

**Decision**: ISO-8859-1 is decoded via a manual 1:1 byte → codepoint table, NOT via `encoding_rs::WINDOWS_1252`.
**Why**: `encoding_rs` aliases the ISO-8859-1 label to Windows-1252. The two disagree on the `0x80-0x9F` range:
Windows-1252 reassigns `0x80` to `€` (U+20AC), while strict ISO-8859-1 leaves the byte as the C1 control code U+0080.
Users selecting "Western (ISO-8859-1)" expect the strict mapping. The decoder is a single byte-to-char loop (10 lines of
code), small enough to colocate with `decode_line` rather than add a new dependency. Pinned by
`decode_line_iso_8859_1_keeps_c1_control_codes`.

**Decision**: `FileEncoding` detection runs the UTF-16 parity heuristic BEFORE the UTF-8 fast path.
**Why**: ASCII text encoded as UTF-16 (interleaved with `0x00` bytes) is technically valid UTF-8 — every `0x00` is a
legal U+0000 codepoint — so `std::str::from_utf8(buf).is_ok()` would misclassify it as UTF-8 and decode to a stream of
ASCII chars with NUL gaps. The 30% zero-byte parity threshold is restrictive enough that real UTF-8 text never trips
it, while ASCII-dominant UTF-16 streams hit nearly 100% in the matching slot.

**Decision**: UTF-16 LE ↔ BE swap is NOT instant; both go through a background rebuild.
**Why**: `same_byte_layout(a, b)` requires both encodings to be ASCII-newline-compatible, which UTF-16 isn't. Even when
two UTF-16 encodings share a BOM length, any codepoint above U+007F puts the `0x0A` byte at a different file offset
under each byte order, so the newline index is invalid for the new encoding. The only "free" UTF-16 swap is identity,
which we short-circuit at the top of `set_encoding`.

**Decision**: Drain-and-swap-under-lock protocol for both the ByteSeek → LineIndex upgrade and the encoding rebuild.
**Why**: A watcher `Grew(eof)` event that arrives between the rebuild thread reading the on-disk EOF and storing the
new backend would be silently dropped: the new backend covers only the pre-rebuild EOF, and the watcher never re-fires
for the same file. The fix queues such events in `session.pending_grew: Mutex<Option<u64>>`. The rebuild thread's swap
critical section drains the queue, optionally `extend_to`s the new backend to the queued EOF, `ArcSwap::store`s it, and
clears the `upgrading` / `rebuilding` flag — all inside one `pending_grew` lock. The watcher writers also lock
`pending_grew`, so they physically can't write between the rebuild's read-and-clear and its store. Reused by both
upgrade and encoding-rebuild paths. Pinned by `test_append_during_encoding_rebuild_not_dropped` (milestone 3 will land
the watcher itself plus the analogue `test_append_during_upgrade_not_dropped`).

**Decision**: Sticky `SearchStatus::Cancelled` under a single mutex critical section, plus a watchdog.
**Why**: The per-match cancel in the search loop covers the cooperative case. For the pathological case (runaway regex
inside a single `iter.next()`), the watchdog forces `Cancelled` within 1 s. To avoid the worker clobbering the
watchdog's verdict when it eventually finishes naturally, the worker's final-status write is conditional under the
same mutex: if it sees `Cancelled`, it leaves it. Tests `test_worker_done_after_watchdog_cancelled_is_sticky` and
`test_watchdog_forces_cancel_when_worker_ignores_flag` pin this contract.

**Decision**: `SearchMatch.byte_offset` stores the byte offset of the line start for each match.
**Why**: In ByteSeek mode (when line indexing timed out), search returns exact line numbers but the virtual scroll uses estimated line counts for fraction-based seeking. The byte offset lets the frontend convert to scroll position via `(byteOffset / totalBytes) * estimatedTotalLines`, which is the same fraction the virtual scroll uses for fetching. Without this, navigating to a search match scrolls to the wrong part of the file.

**Decision**: Sparse checkpoints every 256 lines instead of indexing every line.
**Why**: Indexing every line in a 100M-line file would need ~800 MB of offset data (8 bytes each). At 256-line intervals, the same file needs ~3 MB. The trade-off is that seeking to a specific line requires reading forward up to 255 lines from the nearest checkpoint, which takes <1ms on any modern disk, well within the 16ms frame budget for 60fps scrolling.

**Decision**: `ViewerError` is a `serde(tag = "kind")` enum exported through `specta::Type`, not stringified into
`IpcError`. **Why**: the copy flow specifically needs to distinguish `Cancelled` (user pressed Escape, show nothing)
from `TimedOut` (read exceeded 60 s, offer Retry) from `OutOfRange` / `Io` (real failure). String matching on the
message would break the no-string-classification rule (AGENTS.md) and silently break when copy changes. The typed enum
flows through `tauri-specta` to `bindings.ts` as a discriminated union; the frontend's `viewerReadRange` wrapper
returns `{ ok, error }` and the page matches on `error.kind`.

**Decision**: Session map (`SESSIONS`) is a global `LazyLock<Mutex<HashMap>>` rather than Tauri managed state.
**Why**: Same reasoning as the AI manager. Viewer sessions need to be accessed from background threads (search, indexing) that don't have an `AppHandle`. A global makes the session cache accessible from any context without threading an `AppHandle` through every call chain.

## Gotchas

- **SESSIONS is unbounded**: grows with each `viewer_open`. Must call `viewer_close` when window closes (not automatic).
- **LineIndex build is async**: `ViewerSession` upgrades backend when ready. Frontend sees backend type change via status query.
- **Search state per session**: only one search can run per session. Starting a new search cancels the previous one.
- **`search_cancel` must not null `session.search`**: the cancel sets the cancel flag; the spawned search thread sees
  it and writes `SearchStatus::Cancelled` into the same `SearchState`. If `search_cancel` nulled `session.search` first,
  the thread's write would land in a dropped state and `search_poll` would return `Idle` instead of `Cancelled`, so the FE
  would never see the cancellation. The next `search_start` atomically replaces the `SearchState`, so `Cancelled` is
  cleared naturally when a new search begins. See `session.rs::search_cancel` and its test
  `tests::test_search_cancel_surfaces_cancelled_status`.
- **UTF-16 offsets for JS compatibility**: `SearchMatch.column` and `.length` are in UTF-16 code units, matching JS `String.substring()`.
- **ByteSeek backward scan limit**: 8KB max. If newline not found, line starts at scan boundary (truncated).
- **LineIndex memory**: O(total_lines / 256) for checkpoints. For a 100M line file: ~390K checkpoints × 8 bytes = ~3MB.
- **`read_range` cancellation is per-read, not session-wide**: each `read_range` call inserts an `Arc<AtomicBool>` into
  `session.active_reads` keyed by the FE-allocated `read_id`. `cancel_read(session_id, read_id)` flips that one flag.
  Per-read (not session-wide) for the same reason as `search_cancel`: a session-wide flag would race against concurrent
  reads and against reads that complete just as the user starts a new one.
- **`read_range` advances by byte offset after the first chunk, not by line number**: ByteSeek's `SeekTarget::Line(N)`
  resolves to `N * 80` bytes (no line index), so a multi-chunk read keyed by line number would misalign as soon as line
  lengths drift from the 80-byte estimate. `range_read.rs` keys the first chunk by line, then by `byte_offset = chunk
  end` for every subsequent chunk. All three backends honour byte-offset seeks exactly.
- **UTF-16 surrogate clamp at the IPC boundary**: `clamp_utf16_offset_to_byte` rounds offsets that land between the
  high and low surrogate of an astral codepoint down to the codepoint start. This guarantees the returned slice is
  always valid UTF-8.
- **`range_read` checks the cancel flag inside the per-line loop, not just between chunks**: the inner check fires
  every 256 lines OR every 64 KB of emitted output, whichever first. Without the inner check, a 4096-line chunk of
  4 KB/line files (16 MB) would be uninterruptible. Same lesson as `search_cancel`'s per-chunk progress reporting.
- **CRLF: line readers keep `\r` AS PART of the returned line string.** All three backends (`byte_seek.rs:118`,
  `full_load.rs:43`, `line_index.rs:172`) split only on `\n` and slice up to the `\n` byte; the `\r` stays with the
  line. So `line.len()` already includes the `\r` for CRLF files, and `range_read`'s `chunk_end_offset += line.len()
  + 1` accounts only for the single `\n` delimiter byte. No drift on LF or CRLF. Pinned by
  `read_range_full_load_crlf_preserves_carriage_returns` in `session_test.rs`. If a future change makes line readers
  strip `\r`, the byte-offset arithmetic in `range_read.rs` needs the same change.

## Performance targets

- **Open latency**: <10ms for any file size (ByteSeek), <50ms for 1GB file after LineIndex builds
- **Scroll latency**: <16ms (60fps) for 50-line fetch
- **Search**: ~500MB/s (SIMD-accelerated), progress updates every 10MB
