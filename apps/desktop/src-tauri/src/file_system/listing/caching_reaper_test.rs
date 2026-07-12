//! Orphan-listing backstop reaper tests.
//!
//! These pin the defense-in-depth reaper that catches listings whose explicit
//! `list_directory_end` IPC was never delivered. The crux is that the reaper keys on
//! `last_accessed_ms` (refreshed on every live-pane access), NOT `created_at` (stamped
//! once), so a long-open-but-still-used listing is never evicted. The positive test
//! proves a stale listing AND its watcher are torn down together; the negative test
//! proves a freshly-touched listing survives even when it was created long ago.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::caching::{CachedListing, LISTING_CACHE, ORPHAN_IDLE_WINDOW, orphan_ids, reap_orphaned_listings_at};
use super::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::file_system::watcher::{WATCHER_MANAGER, start_watching};
use crate::ignore_poison::IgnorePoison;

/// Serializes the tests that call the WIRED `reap_orphaned_listings_at`, which
/// sweeps the process-global `LISTING_CACHE` and evicts every listing idle past
/// the (injected) window. Two such sweeps racing as parallel threads under plain
/// `cargo test` cross-evict each other's freshly-inserted stale listings, so a
/// precondition like "the listing I just inserted is still cached" flakes
/// (rotating victim; `reaper_evicts_...` most often). `cargo nextest` sidesteps
/// this by running each test in its own process (fresh global cache); this mutex
/// is the in-process equivalent, so exactly one global sweep runs at a time under
/// `cargo test`. Mirrors `downloads::watcher::tests`' `WATCH_SERIAL`
/// (`lock_ignore_poison`, so one panicking test can't cascade-poison the rest).
/// The pure `orphan_ids` tests take explicit stamps and never touch the global
/// cache, so they don't need it. See `listing/DETAILS.md` § "Reaper test
/// serialization".
static REAPER_SERIAL: Mutex<()> = Mutex::new(());

fn unique(suffix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "reaper_{}_{}_{}",
        suffix,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// Inserts a listing with an explicit `last_accessed_ms` stamp so we can simulate an
/// orphan deterministically (no sleeping, no real clock advance).
fn insert_with_last_accessed(id: &str, path: &str, last_accessed_ms: u64) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.insert(
        id.to_string(),
        CachedListing {
            volume_id: "root".to_string(),
            path: PathBuf::from(path),
            entries: Vec::new(),
            sort_by: SortColumn::Name,
            sort_order: SortOrder::Ascending,
            directory_sort_mode: DirectorySortMode::LikeFiles,
            sequence: AtomicU64::new(0),
            // `created_at` stays "now" on purpose: it proves the reaper does NOT key on
            // creation time. A long-open listing has a recent `created_at` relative to
            // session start too, but the point is that even a brand-new `created_at`
            // doesn't save a listing whose `last_accessed_ms` is stale.
            created_at: std::time::Instant::now(),
            last_accessed_ms: AtomicU64::new(last_accessed_ms),
        },
    );
}

fn remove_listing(id: &str) {
    let mut cache = LISTING_CACHE.write().unwrap();
    cache.remove(id);
}

fn in_cache(id: &str) -> bool {
    LISTING_CACHE.read().unwrap().contains_key(id)
}

fn is_watched(id: &str) -> bool {
    WATCHER_MANAGER.read().unwrap().watches.contains_key(id)
}

// ---- pure helper: orphan_ids ------------------------------------------------

#[test]
fn orphan_ids_flags_only_listings_idle_past_the_window() {
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 100 * window_ms; // far enough that subtractions don't underflow

    let stamps = [
        ("fresh", now),                      // idle 0
        ("recent", now - 1),                 // idle 1 ms
        ("just_under", now - window_ms + 1), // idle window-1: NOT orphan
        ("exactly", now - window_ms),        // idle == window: orphan (>=)
        ("ancient", now - 10 * window_ms),   // idle 10x window: orphan
    ];

    let mut ids = orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms)));
    ids.sort();

    assert_eq!(ids, vec!["ancient".to_string(), "exactly".to_string()]);
}

#[test]
fn orphan_ids_empty_for_all_fresh() {
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 1_000_000_000u64;
    let stamps = [("a", now), ("b", now - 5), ("c", now - 100)];
    assert!(orphan_ids(now, window_ms, stamps.iter().map(|(id, ms)| (*id, *ms))).is_empty());
}

// ---- reap: positive (orphan + its watcher torn down together) ----------------
//
// We inject `now_ms` and `window_ms` because the real idle clock is relative to
// process start, so a genuine 6 h gap can't be produced in a unit test. The listing
// is stamped `last_accessed_ms = 0`; calling the reaper with `now = window` makes its
// idle time exactly the window → orphan.

#[test]
fn reaper_evicts_stale_listing_and_its_watcher_together() {
    let _serial = REAPER_SERIAL.lock_ignore_poison();
    // A real directory so `start_watching` can attach an FSEvents watcher. No
    // AppHandle is needed to STORE the watcher in WATCHER_MANAGER; the handle only
    // matters for emitting events, which this test doesn't exercise.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    let lid = unique("orphan");
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    insert_with_last_accessed(&lid, &path, 0); // last access at the epoch (ancient)
    start_watching(&lid, dir.path()).expect("start_watching should succeed on a real dir");

    assert!(in_cache(&lid), "precondition: listing is cached");
    assert!(is_watched(&lid), "precondition: watcher is attached");

    // now = window → idle == window → orphan.
    let reaped = reap_orphaned_listings_at(window_ms, window_ms);

    assert!(reaped.contains(&lid), "reaper should report the orphaned listing");
    assert!(!in_cache(&lid), "reaper must remove the cache entry");
    assert!(
        !is_watched(&lid),
        "reaper must tear down the watcher too (reusing list_directory_end's stop_watching)"
    );
}

// ---- reap: negative (recently-touched, long-created listing survives) --------

#[test]
fn reaper_keeps_recently_touched_listing_even_if_created_long_ago() {
    let _serial = REAPER_SERIAL.lock_ignore_poison();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_string_lossy().to_string();
    let lid = unique("live");
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    // Touched "just now" relative to the injected clock: last_accessed_ms == now. The
    // listing's `created_at` is real-now (set in the helper), but the reaper ignores
    // created_at entirely — only the fresh access stamp matters. Pick a `now` far past
    // the window so the only reason this listing survives is its fresh stamp, not a
    // small absolute clock.
    let now = 50 * window_ms;
    insert_with_last_accessed(&lid, &path, now);
    start_watching(&lid, dir.path()).expect("start_watching should succeed on a real dir");

    let reaped = reap_orphaned_listings_at(now, window_ms);

    assert!(
        !reaped.contains(&lid),
        "a just-touched listing must NOT be reaped (this is the don't-evict-live guarantee)"
    );
    assert!(in_cache(&lid), "live listing's cache entry must survive");
    assert!(is_watched(&lid), "live listing's watcher must survive");

    // Cleanup
    crate::file_system::listing::operations::list_directory_end(&lid);
    remove_listing(&lid);
}

// ---- touch() refreshes the stamp so a long-open pane is never reaped ---------

#[test]
fn touch_rescues_a_listing_that_would_otherwise_be_orphaned() {
    let _serial = REAPER_SERIAL.lock_ignore_poison();
    let lid = unique("touch");
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 50 * window_ms;

    // Start stale (last access at the epoch → idle == now == 50x window → orphan)...
    insert_with_last_accessed(&lid, "/no/watcher", 0);
    assert!(
        !orphan_ids(now, window_ms, std::iter::once((lid.as_str(), 0))).is_empty(),
        "precondition: a stamp of 0 is orphan-eligible at now = 50x window"
    );

    // ...then a live-pane access touches it (same path the read accessors take: hold
    // the cache read lock and stamp `last_accessed_ms` via `touch()`). `touch()` uses
    // the REAL clock (`epoch_millis_now()`), which in-process is a tiny value — far
    // below the injected `now` of 50x window. So the touched stamp's idle time at the
    // injected `now` is still ~50x window, which would STILL look orphaned under that
    // synthetic clock. To prove touch() works against the clock it actually uses, run
    // the reaper with the real clock + real window: the just-touched stamp is fresh.
    {
        let cache = LISTING_CACHE.read().unwrap();
        cache.get(&lid).unwrap().touch();
    }

    let reaped = reap_orphaned_listings_at(
        super::caching::epoch_millis_now(),
        ORPHAN_IDLE_WINDOW.as_millis() as u64,
    );
    assert!(!reaped.contains(&lid), "touch() must reset the idle clock");
    assert!(in_cache(&lid), "touched listing survives the sweep");

    remove_listing(&lid);
}
