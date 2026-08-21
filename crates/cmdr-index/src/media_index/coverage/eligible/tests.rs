//! The eligible cache: building it, refilling it from a pass's own walk, patching the
//! dirs a live tick re-walked, and deduplicating concurrent cold builds.

use std::collections::HashSet;

use super::*;

fn img(path: &str) -> ImageEntry {
    ImageEntry {
        path: path.to_string(),
        mtime: Some(1),
        size: Some(2),
        kind: crate::media_index::predicate::MediaKind::Image,
    }
}

fn touched(dirs: &[&str]) -> HashSet<String> {
    dirs.iter().map(|d| d.to_string()).collect()
}

/// A `WalkedDirs` over a synthetic set. Production can only get one out of a real scoped
/// walk (that's the point of the type); a unit test over the pure patch arithmetic mints
/// one directly.
fn walked(dirs: &HashSet<String>) -> WalkedDirs<'_> {
    WalkedDirs::for_test(dirs)
}

#[test]
fn build_counts_aggregates_per_folder_and_total() {
    let counts = build_counts(&[img("/p/a.jpg"), img("/p/b.jpg"), img("/q/c.jpg")]);
    assert_eq!(counts.total, 3);
    assert_eq!(counts.per_folder.get("/p").copied(), Some(2));
    assert_eq!(counts.per_folder.get("/q").copied(), Some(1));
}

#[test]
fn patch_updates_only_the_touched_dir_and_moves_total() {
    // /a re-walked to 1 image (was 3); /b is untouched. total moves by the /a delta only.
    let existing = FolderImageCounts {
        per_folder: [("/a".to_string(), 3u64), ("/b".to_string(), 5)].into_iter().collect(),
        total: 8,
    };
    let patched = patch_counts(&existing, walked(&touched(&["/a"])), &[img("/a/x.jpg")]);
    assert_eq!(patched.per_folder.get("/a").copied(), Some(1), "/a re-counted");
    assert_eq!(patched.per_folder.get("/b").copied(), Some(5), "/b untouched");
    assert_eq!(patched.total, 6, "total moved by the /a delta (3 → 1)");
}

#[test]
fn patch_drops_a_dir_that_fell_to_zero() {
    // Every qualifying image left /a (the tick walked it and found none) ⇒ /a leaves
    // `per_folder` (which only holds folders with ≥ 1), and total drops by its old count.
    let existing = FolderImageCounts {
        per_folder: [("/a".to_string(), 3u64), ("/b".to_string(), 5)].into_iter().collect(),
        total: 8,
    };
    let patched = patch_counts(&existing, walked(&touched(&["/a"])), &[]);
    assert!(!patched.per_folder.contains_key("/a"), "/a dropped at zero");
    assert_eq!(patched.per_folder.get("/b").copied(), Some(5));
    assert_eq!(patched.total, 5);
}

#[test]
fn patch_adds_a_newly_qualifying_dir() {
    // A touched dir absent from the cache (a folder's first qualifying image) is added.
    let existing = FolderImageCounts {
        per_folder: [("/b".to_string(), 5u64)].into_iter().collect(),
        total: 5,
    };
    let patched = patch_counts(
        &existing,
        walked(&touched(&["/a"])),
        &[img("/a/x.jpg"), img("/a/y.jpg")],
    );
    assert_eq!(patched.per_folder.get("/a").copied(), Some(2), "/a added");
    assert_eq!(patched.total, 7);
}

#[test]
fn replace_then_patch_round_trips_through_the_global_cache() {
    // A unique volume id keeps this isolated from the process-global cache other tests use.
    let vid = "coverage-test-replace-patch";
    replace_from_entries(vid, &[img("/a/x.jpg"), img("/a/y.jpg"), img("/b/z.jpg")]);
    let after_replace = COUNTS.lock_ignore_poison().get(vid).cloned().expect("cached");
    assert_eq!(after_replace.total, 3);
    assert_eq!(after_replace.per_folder.get("/a").copied(), Some(2));

    // A live tick re-walks /a and finds one image now: the cache patches /a in place.
    patch_touched_dirs(vid, walked(&touched(&["/a"])), &[img("/a/x.jpg")]);
    let after_patch = COUNTS.lock_ignore_poison().get(vid).cloned().expect("cached");
    assert_eq!(after_patch.per_folder.get("/a").copied(), Some(1), "/a patched");
    assert_eq!(after_patch.per_folder.get("/b").copied(), Some(1), "/b untouched");
    assert_eq!(after_patch.total, 2);
    invalidate(vid);
}

#[test]
fn patch_is_a_noop_without_a_cached_volume() {
    // No cached counts yet ⇒ the patch does nothing (the next preview builds them fresh),
    // never inserting a partial (touched-dirs-only) entry that would undercount the volume.
    let vid = "coverage-test-patch-noop";
    invalidate(vid);
    patch_touched_dirs(vid, walked(&touched(&["/a"])), &[img("/a/x.jpg")]);
    assert!(
        !COUNTS.lock_ignore_poison().contains_key(vid),
        "a patch with nothing cached inserts nothing"
    );
}

#[test]
fn concurrent_cold_builds_run_the_walk_once() {
    // The cold build is an O(entries) index walk costing gigabytes of transient heap on
    // a multi-million-entry volume. Several callers can go cold at once (the volume-state
    // poll, the slider preview, the reclaim preview all land within milliseconds of a
    // launch), so N concurrent callers must NOT each run their own walk — they queue on
    // the volume's build and the losers find the cache warm.
    let vid = "coverage-test-concurrent-build";
    invalidate(vid);
    let builds = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let threads = 8;
    let barrier = Arc::new(std::sync::Barrier::new(threads));

    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let builds = Arc::clone(&builds);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                // Every thread arrives before any of them looks at the cache, so they all
                // genuinely race the cold path.
                barrier.wait();
                let counts = get_or_build_with(vid, || {
                    builds.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    // Stand in for the walk's duration, so a racing caller would have
                    // time to start a second one.
                    // allowed-test-sleep: the fake walk latency IS the subject — without it the racers could serialize by luck and pass an un-deduplicated build
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    Some(FolderImageCounts {
                        per_folder: [("/photos".to_string(), 3u64)].into_iter().collect(),
                        total: 3,
                    })
                });
                counts.expect("built").total
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().expect("thread"), 3, "every caller gets the counts");
    }

    assert_eq!(
        builds.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the concurrent callers share ONE walk"
    );
    invalidate(vid);
}
