# LISTING_CACHE / WATCHER_MANAGER have no backstop eviction for orphaned listings

**Severity:** medium **Lens:** G — Resource hygiene **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/file_system/listing/operations.rs:120-131` (`list_directory_end`)
`apps/desktop/src-tauri/src/file_system/listing/caching.rs:40` (`LISTING_CACHE`, no cap/TTL)
`apps/desktop/src-tauri/src/file_system/watcher.rs:44` (`WATCHER_MANAGER`, no cap/TTL)
`apps/desktop/src-tauri/src/lib.rs:1026-1034` (main-window `Destroyed`: no listing cleanup)

## What

A listing's only removal path from `LISTING_CACHE` and its watcher's only removal path from `WATCHER_MANAGER` is an
explicit `list_directory_end(listing_id)` IPC, which the frontend fires from `FilePane.loadDirectory` (superseding nav)
and `FilePane.onDestroy`. There is no TTL, no entry cap, and no backend reaper. Unlike the file viewer — which has a
`WindowEvent::Destroyed` branch in `lib.rs` that calls `close_session_for_window` precisely because the titlebar-X path
skips the FE close IPC — the listing subsystem has no equivalent backend safety net. Any listing for which
`list_directory_end` is never delivered (a frontend exception between `listDirectoryStart` and the cleanup wiring, a
`$effect` teardown that throws, a future code path that forgets the call) pins its `CachedListing` (a full
`Vec<FileEntry>` — up to 50k+ entries) plus a live `notify-debouncer-full` OS watcher for the rest of the session.

## Why it matters

Each leaked listing holds an entire directory's worth of `FileEntry` structs in memory AND a live FSEvents/notify
watcher on that directory. Over a multi-day session, a handful of missed cleanups (e.g. a thrown handler during rapid
navigation, an HMR-adjacent edge, an MTP/SMB error mid-handshake) accumulate into pinned multi-megabyte entry vectors
and orphaned OS watchers that never release. The architecture-patterns doc itself acknowledges the failure mode by name
— `caching::snapshot_listings()` exists so `cmdr://state` can "surface orphan listings (started but not bound to a
pane)" — which means orphans are a known, observable condition, yet nothing reclaims them automatically. A single-writer
contract with no defense-in-depth is brittle for a process designed to run for days.

## Evidence

```rust
// operations.rs — the ONLY removal path
pub fn list_directory_end(listing_id: &str) {
    stop_watching(listing_id);                                   // WATCHER_MANAGER.remove
    crate::file_system::listing::diff_emitter::drop_pending(listing_id);
    if let Ok(mut cache) = LISTING_CACHE.write() {
        cache.remove(listing_id);                                // LISTING_CACHE.remove
    }
}
```

```rust
// caching.rs — unbounded map, no TTL, no cap
pub(crate) static LISTING_CACHE: LazyLock<RwLock<HashMap<String, CachedListing>>> =
    LazyLock::new(...);
```

```rust
// lib.rs — main-window Destroyed cleans AI/MCP/mDNS but NOT listings/watchers
if let tauri::WindowEvent::Destroyed = event && window.label() == "main" {
    ai::manager::shutdown();
    mcp::stop_mcp_server();
    network::mdns_discovery::stop_discovery();
    // (no LISTING_CACHE / WATCHER_MANAGER sweep — viewer-* labels get one below, main does not)
}
```

## Suggested fix

Add a lightweight backstop that bounds the orphan set without changing the happy-path contract. Two complementary
options: (1) On a successful new `list_directory_start_streaming`, opportunistically reap any cached listing whose
`created_at` is older than a generous TTL (e.g. 30 min) AND that no live pane references — `CachedListing` already
carries `created_at` and `snapshot_listings()` already computes age, so this is cheap and self-limiting. (2) Cap
`WATCHER_MANAGER.watches` at a sane ceiling (a UI can realistically show only a handful of panes/tabs) and evict the
oldest watcher when exceeded, logging a warning so a real leak surfaces. Keep `list_directory_end` as the primary path;
the reaper is purely defense-in-depth, mirroring the search index's idle/backstop timers and the viewer's
`Destroyed`-window net.

## Notes

In production, main-window destroy == process exit, so the missing main-window sweep doesn't leak across the app's death
— the exposure is strictly the in-session orphan accumulation, not a shutdown leak. The viewer subsystem is the proof
that the team already treats "FE close IPC not delivered" as a real, handled risk; the listing subsystem is the larger
and longer-lived cousin that lacks the same net.
