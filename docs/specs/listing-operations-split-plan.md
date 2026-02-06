# Listing operations.rs split plan

## Goal

Split `listing/operations.rs` (1,407 lines) into three files in the 400–600 range, improving architecture along the way.

## Extractions

### 1. `reading.rs` (~445 lines) — low-level disk I/O

Move these functions:
- `list_directory()` — full metadata read
- `list_directory_core()` — fast core-only read
- `get_single_entry()` — single path metadata
- `process_dir_entry()` — single DirEntry → FileEntry
- `get_extended_metadata_batch()` — macOS extended metadata

**Bonus**: Deduplicate `list_directory()` / `list_directory_core()`. They share ~90% of code.
Refactor so `list_directory()` calls `list_directory_core()` then fills in macOS metadata.

### 2. Expand `streaming.rs` (~412 lines) — async streaming implementation

Move these functions (next to existing streaming types):
- `list_directory_start_streaming()` — spawns background task
- `read_directory_with_progress()` — the actual background worker
- `cancel_listing()` — sets cancellation flag

### 3. `operations.rs` remains (~590 lines) — synchronous FE-facing API

Keeps:
- Types: `ListingStartResult`, `ResortResult`, `ListingStats`
- Lifecycle: `list_directory_start`, `list_directory_start_with_volume`, `list_directory_end`
- Cache API: `get_file_range`, `get_total_count`, `get_max_filename_width`, `find_file_index`, `get_file_at`, `get_paths_at_indices`
- Resort: `resort_listing`
- Stats: `get_listing_stats`
- Internal watcher accessors: `get_listing_entries`, `update_listing_entries`, `get_listings_by_volume_prefix`

## Execution order

1. Create `reading.rs` with moved functions
2. Deduplicate `list_directory` / `list_directory_core`
3. Move streaming impl into `streaming.rs`
4. Clean up `operations.rs` imports
5. Update `mod.rs` re-exports
6. Run `./scripts/check.sh --rust`
7. Update split plan checklist
