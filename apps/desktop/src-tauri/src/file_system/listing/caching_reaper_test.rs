//! Orphan-listing backstop reaper tests.
//!
//! These pin the defense-in-depth reaper that catches listings whose explicit
//! `list_directory_end` IPC was never delivered. The crux is that the reaper keys on
//! `last_accessed_ms` (refreshed on every live-pane access), NOT `created_at` (stamped
//! once), so a long-open-but-still-used listing is never evicted. The positive test
//! proves a stale listing AND its watcher are torn down together; the negative test
//! proves a freshly-touched listing survives even when it was created long ago.
//!
//! Every wired sweep here goes through `reap_orphaned_listings_at_for`, scoped to
//! the ids this test owns. An unrestricted sweep would evict concurrently-running
//! tests' listings out of the process-global cache.

use super::caching::{LISTING_CACHE, ORPHAN_IDLE_WINDOW, orphan_ids, reap_orphaned_listings_at_for};
use super::caching_test_support::TestListing;
use crate::file_system::watcher::{WATCHER_MANAGER, start_watching};
use crate::ignore_poison::RwLockIgnorePoison;

fn is_watched(id: &str) -> bool {
    WATCHER_MANAGER.read_ignore_poison().watches.contains_key(id)
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
    // A real directory so `start_watching` can attach an FSEvents watcher. No
    // AppHandle is needed to STORE the watcher in WATCHER_MANAGER; the handle only
    // matters for emitting events, which this test doesn't exercise.
    let dir = tempfile::tempdir().unwrap();
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    // Last access at the epoch (ancient).
    let listing = TestListing::new().path(dir.path()).last_accessed_ms(0).insert("orphan");
    start_watching(listing.id(), dir.path()).expect("start_watching should succeed on a real dir");

    assert!(listing.is_cached(), "precondition: listing is cached");
    assert!(is_watched(listing.id()), "precondition: watcher is attached");

    // now = window → idle == window → orphan.
    let reaped = reap_orphaned_listings_at_for(window_ms, window_ms, &[listing.id()]);

    assert!(
        reaped.contains(&listing.id().to_string()),
        "reaper should report the orphaned listing"
    );
    assert!(!listing.is_cached(), "reaper must remove the cache entry");
    assert!(
        !is_watched(listing.id()),
        "reaper must tear down the watcher too (reusing list_directory_end's stop_watching)"
    );
}

// ---- reap: negative (recently-touched, long-created listing survives) --------

#[test]
fn reaper_keeps_recently_touched_listing_even_if_created_long_ago() {
    let dir = tempfile::tempdir().unwrap();
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;

    // Touched "just now" relative to the injected clock: last_accessed_ms == now. The
    // listing's `created_at` is real-now (set by the builder), but the reaper ignores
    // created_at entirely — only the fresh access stamp matters. Pick a `now` far past
    // the window so the only reason this listing survives is its fresh stamp, not a
    // small absolute clock.
    let now = 50 * window_ms;
    let listing = TestListing::new().path(dir.path()).last_accessed_ms(now).insert("live");
    start_watching(listing.id(), dir.path()).expect("start_watching should succeed on a real dir");

    let reaped = reap_orphaned_listings_at_for(now, window_ms, &[listing.id()]);

    assert!(
        !reaped.contains(&listing.id().to_string()),
        "a just-touched listing must NOT be reaped (this is the don't-evict-live guarantee)"
    );
    assert!(listing.is_cached(), "live listing's cache entry must survive");
    assert!(is_watched(listing.id()), "live listing's watcher must survive");
}

// ---- touch() refreshes the stamp so a long-open pane is never reaped ---------

#[test]
fn touch_rescues_a_listing_that_would_otherwise_be_orphaned() {
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let now = 50 * window_ms;

    // Start stale (last access at the epoch → idle == now == 50x window → orphan)...
    let listing = TestListing::new()
        .path("/no/watcher")
        .last_accessed_ms(0)
        .insert("touch");
    assert!(
        !orphan_ids(now, window_ms, std::iter::once((listing.id(), 0))).is_empty(),
        "precondition: a stamp of 0 is orphan-eligible at now = 50x window"
    );

    // ...then a live-pane access touches it (same path the read accessors take: hold
    // the cache read lock and stamp `last_accessed_ms` via `touch()`). `touch()` uses
    // the REAL clock (`epoch_millis_now()`), which in-process is a tiny value — far
    // below the injected `now` of 50x window. So the touched stamp's idle time at the
    // injected `now` is still ~50x window, which would STILL look orphaned under that
    // synthetic clock. To prove touch() works against the clock it actually uses, run
    // the reaper with the real clock + real window: the just-touched stamp is fresh.
    listing.with_listing(|cached| cached.touch());

    let reaped = reap_orphaned_listings_at_for(
        super::caching::epoch_millis_now(),
        ORPHAN_IDLE_WINDOW.as_millis() as u64,
        &[listing.id()],
    );
    assert!(
        !reaped.contains(&listing.id().to_string()),
        "touch() must reset the idle clock"
    );
    assert!(listing.is_cached(), "touched listing survives the sweep");
}

// ---- a sweep stays inside its own test ----------------------------------------

#[test]
fn a_reaper_sweep_leaves_a_sibling_tests_listing_alone() {
    // Pre-fix this failed: the wired reaper tests swept the WHOLE process-global
    // cache with an injected clock far past the idle window, so every other listing
    // test's fixture was evicted (watcher and all) mid-assertion. Two defenses now:
    // the sweep is scoped to this test's ids, and `TestListing` stamps
    // `last_accessed_ms` at NOW, so a fixture isn't orphan-eligible to begin with.
    let window_ms = ORPHAN_IDLE_WINDOW.as_millis() as u64;
    let sibling = TestListing::new().path("/sibling").insert("sibling");
    let mine = TestListing::new().path("/mine").last_accessed_ms(0).insert("mine");

    let reaped = reap_orphaned_listings_at_for(50 * window_ms, window_ms, &[mine.id()]);

    assert!(
        reaped.contains(&mine.id().to_string()),
        "my own stale listing is still reaped"
    );
    assert!(sibling.is_cached(), "a sibling test's listing survives my sweep");
}

// ---- the guard's own contract ------------------------------------------------

#[test]
fn guard_removes_its_listing_even_when_the_test_body_panics() {
    // Pins the panic-safety `TestListingGuard` exists for: an assertion that fails
    // before a hand-rolled `cache.remove(...)` used to leak the entry into every
    // later test's view of the process-global cache.
    let id = std::panic::catch_unwind(|| {
        let listing = TestListing::new().insert("panic-safety");
        let id = listing.id().to_string();
        assert!(listing.is_cached());
        panic!("simulated assertion failure while the listing is cached: {id}");
    })
    .expect_err("the closure should have panicked");
    let id = id
        .downcast_ref::<String>()
        .expect("panic payload is the formatted message")
        .rsplit(": ")
        .next()
        .expect("message ends with the listing id")
        .to_string();

    assert!(
        !LISTING_CACHE.read_ignore_poison().contains_key(&id),
        "Drop must remove the entry on unwind, not only on the happy path"
    );
}
