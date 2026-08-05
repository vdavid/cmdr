//! What a walk refuses to descend into: the structural exclusion policy's on/off
//! switch, and the volume boundary.
//!
//! Both are [`WalkPolicy`], and both matter most on the SEARCH walk, which is the
//! one walk nobody chose the ground for: a person names a scope, the coverage
//! answer turns it into frontier roots, and the walk goes wherever those lead. A
//! scoped search of `/` would otherwise walk `/private/var` and `/proc`, and a
//! search of a folder holding a mounted drive would write that drive's rows into
//! this volume's index.

use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;

use super::test_fixtures::{ensure_path_in_db, scan_test_tempdir, setup_writer};
use super::*;
use crate::indexing::IndexPathSpace;
use crate::indexing::store::{IndexStore, resolve_path};

// ── Fixture ──────────────────────────────────────────────────────────

/// A real temp tree plus an index over it, with the chain down to the tree root
/// seeded so a walk can resolve its root.
struct Tree {
    dir: tempfile::TempDir,
    db_path: PathBuf,
    _db_dir: tempfile::TempDir,
    writer: IndexWriter,
}

impl Tree {
    fn new() -> Self {
        let (writer, db_path, db_dir) = setup_writer();
        let dir = scan_test_tempdir();
        ensure_path_in_db(&db_path, dir.path(), &writer);
        Self {
            dir,
            db_path,
            _db_dir: db_dir,
            writer,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    /// Whether the index holds a row for this path at all.
    fn has_row(&self, relative: &str) -> bool {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        resolve_path(&conn, &self.path(relative).to_string_lossy())
            .expect("resolve")
            .is_some()
    }

    /// Every row under the tree root, by path, so a test can say exactly what a
    /// walk wrote.
    fn rows(&self) -> Vec<String> {
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        let root_id = resolve_path(&conn, &self.dir.path().to_string_lossy())
            .expect("resolve")
            .expect("the tree root is seeded");
        let mut out = Vec::new();
        let mut stack = vec![(root_id, String::new())];
        while let Some((id, prefix)) = stack.pop() {
            for row in IndexStore::list_children_on(id, &conn).expect("list children") {
                let path = if prefix.is_empty() {
                    row.name.clone()
                } else {
                    format!("{prefix}/{}", row.name)
                };
                if row.is_directory {
                    stack.push((row.id, path.clone()));
                }
                out.push(path);
            }
        }
        out.sort();
        out
    }

    /// The row a frontier node always has by the time a walk reaches it — the
    /// coverage answer found it by descending into its parent's listing, and
    /// `cover::bootstrap::ensure_walkable` materializes the chain otherwise.
    fn seed_walk_root(&self, relative: &str) {
        ensure_path_in_db(&self.db_path, &self.path(relative), &self.writer);
    }

    fn cover(&self, relative: &str) -> ScanSummary {
        self.cover_in(relative, &IndexPathSpace::root())
    }

    fn cover_in(&self, relative: &str, space: &IndexPathSpace) -> ScanSummary {
        self.seed_walk_root(relative);
        let summary = cover_subtree(
            &self.path(relative),
            space,
            &self.writer,
            None,
            &CancellationToken::new(),
            &WalkHeartbeat::new(),
        )
        .expect("the walk runs");
        self.writer.flush_blocking().expect("flush");
        summary
    }

    fn cover_with_devices(&self, relative: &str, device_of: fn(&Path) -> Option<u64>) -> ScanSummary {
        self.seed_walk_root(relative);
        let summary = cover_subtree_with_device_probe(
            &self.path(relative),
            &IndexPathSpace::root(),
            &self.writer,
            &CancellationToken::new(),
            device_of,
        )
        .expect("the walk runs");
        self.writer.flush_blocking().expect("flush");
        summary
    }
}

// ── The exclusion switch ─────────────────────────────────────────────

/// The search walk applies the structural policy to what it discovers, so a
/// scoped search doesn't fill the index with `.Spotlight-V100` and friends.
///
/// The junk tier is the one that fires on any volume; the boot-disk absolute
/// prefixes (`/private/var`, `/proc`, …) come off the same call and the same
/// scope, which `exclusions::tests` covers path by path.
#[test]
fn a_search_walk_skips_a_structurally_excluded_child() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("scope/.Spotlight-V100/inner")).expect("dirs");
    std::fs::write(tree.path("scope/.Spotlight-V100/inner/junk.db"), "x").expect("file");
    std::fs::write(tree.path("scope/real.txt"), "x").expect("file");

    let summary = tree.cover("scope");

    assert_eq!(tree.rows(), vec!["scope".to_string(), "scope/real.txt".to_string()]);
    assert_eq!(summary.total_entries, 1, "only the real file was walked");
    assert!(
        !tree.has_row("scope/.Spotlight-V100"),
        "an excluded directory gets NO row: a row nothing ever lists would sit in \
         the coverage frontier forever and re-offer itself to every later search"
    );
}

/// A rebuild walk does NOT, and that difference is the switch.
///
/// `scan_subtree`'s two callers (`reconcile/verifier.rs` and
/// `watch/event_loop/verification.rs`) each ask `should_exclude` about the
/// directory before handing it over, so a rebuild is a re-read of ground somebody
/// already gated — while a search walk's roots come from a coverage answer that
/// looked at nothing.
#[test]
fn a_rebuild_walk_indexes_what_its_caller_already_gated() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("scope/.Spotlight-V100")).expect("dirs");
    std::fs::write(tree.path("scope/.Spotlight-V100/junk.db"), "x").expect("file");
    ensure_path_in_db(&tree.db_path, &tree.path("scope"), &tree.writer);

    scan_subtree(
        &tree.path("scope"),
        &IndexPathSpace::root(),
        &tree.writer,
        &CancellationToken::new(),
    )
    .expect("the rebuild runs");
    tree.writer.flush_blocking().expect("flush");

    assert!(
        tree.has_row("scope/.Spotlight-V100"),
        "the rebuild indexes the directory it was pointed at, whole"
    );
}

// ── The volume boundary ──────────────────────────────────────────────

/// A device probe that puts a "mount" at every directory named `mounted`, so a
/// test can build a boundary in a temp tree without one.
fn mount_at_directories_named_mounted(path: &Path) -> Option<u64> {
    if path.components().any(|c| c.as_os_str() == "mounted") {
        Some(2)
    } else {
        Some(1)
    }
}

/// A search walk cuts at a volume boundary: another filesystem is mounted inside
/// the scope, and its rows belong to its own volume's index (Decision 4 makes one
/// volume the broadest scope a search can have).
#[test]
fn a_search_walk_cuts_where_another_volume_is_mounted() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("scope/mounted/deep")).expect("dirs");
    std::fs::write(tree.path("scope/mounted/deep/theirs.txt"), "x").expect("file");
    std::fs::write(tree.path("scope/ours.txt"), "x").expect("file");

    let summary = tree.cover_with_devices("scope", mount_at_directories_named_mounted);

    assert_eq!(tree.rows(), vec!["scope".to_string(), "scope/ours.txt".to_string()]);
    assert_eq!(summary.total_entries, 1, "nothing on the other volume was walked");
    assert!(
        !tree.has_row("scope/mounted"),
        "and the mount point itself gets no row either: its bytes belong in the \
         other volume's dir_stats, not this one's"
    );
}

/// The pin comes from the WALK's root, not the volume's, and that's what stops a
/// walk rooted inside a mount from cutting away every one of its own children,
/// listing the root, and reading as fully covered while holding nothing.
///
/// Same failure `ExclusionTier` exists to prevent for the mount-rooted scan, one
/// rule over.
#[test]
fn a_walk_rooted_inside_a_mount_still_covers_it() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("mounted/deep")).expect("dirs");
    std::fs::write(tree.path("mounted/deep/theirs.txt"), "x").expect("file");

    let summary = tree.cover_with_devices("mounted", mount_at_directories_named_mounted);

    assert_eq!(summary.total_entries, 2, "deep/ and deep/theirs.txt");
    assert!(tree.has_row("mounted/deep/theirs.txt"));
}

/// A directory whose device can't be read is NOT treated as a boundary. The walk
/// descends, its read fails, and `visit_read_error` reports that honestly —
/// rather than this rule guessing and silently dropping a subtree.
#[test]
fn an_unreadable_device_is_not_a_boundary() {
    fn no_device_anywhere(_path: &Path) -> Option<u64> {
        None
    }

    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("scope/inner")).expect("dirs");
    std::fs::write(tree.path("scope/inner/found.txt"), "x").expect("file");

    let summary = tree.cover_with_devices("scope", no_device_anywhere);

    assert_eq!(summary.total_entries, 2, "inner/ and inner/found.txt");
}

/// ⚠️ A File Provider domain root is NOT a volume boundary (Decision 16).
/// Dropbox, iCloud Drive, and Google Drive report the same device as `$HOME` and
/// belong to the boot volume's scope, so the walk descends into them; the guarded
/// walker's stall detection is what makes a disconnected one safe. ❌ Don't
/// repurpose the domain-root probe as a cut — it answers where a volume ROOT
/// sits, for the pseudo-filesystem rule, and nothing else.
#[test]
fn a_file_provider_domain_is_walked_into_rather_than_cut_at() {
    let tree = Tree::new();
    std::fs::create_dir_all(tree.path("scope/CloudStorage/dev")).expect("dirs");
    std::fs::write(tree.path("scope/CloudStorage/dev/mine.rs"), "x").expect("file");

    // Every directory answers "yes, I'm a domain root", and none of them looks
    // like a Unix filesystem — which is exactly a cloud provider's tree.
    let space =
        IndexPathSpace::root().with_exclusion_scope(ExclusionScope::boot_disk().with_probes(|_| true, |_| false));
    let summary = tree.cover_in("scope", &space);

    assert_eq!(summary.total_entries, 3, "CloudStorage/, dev/, dev/mine.rs");
    assert!(
        tree.has_row("scope/CloudStorage/dev/mine.rs"),
        "a developer's `dev` folder inside a provider tree stays indexed"
    );
}

// ── Which walk gets which rules ──────────────────────────────────────

/// A full scan pins no device, deliberately: it bounds itself by path prefix
/// (`/Volumes/` on the boot disk) and pinning it would silently change what a
/// boot index contains. Only the search walk pays the per-directory probe.
#[test]
fn only_the_search_walk_pins_a_device() {
    let tree = Tree::new();
    let space = IndexPathSpace::root();
    let pinned = |mode| WalkPolicy::with_device_probe(mode, &space, tree.dir.path(), |_| Some(1)).device;

    assert_eq!(pinned(ScanRoot::Virgin), Some(1), "the search walk stays on its device");
    assert_eq!(pinned(ScanRoot::Volume), None, "a full scan doesn't");
    assert_eq!(pinned(ScanRoot::Rebuild), None, "and neither does a rebuild");
}

/// What the boundary probe costs: the same real tree walked with the pin on and
/// with it off, so the difference IS the per-directory `lstat`.
///
/// Not a correctness check, so it's `#[ignore]`d. Run it in RELEASE, where the
/// difference is honest, after a warm-up pass over the tree:
///
/// ```sh
/// CMDR_BOUNDARY_BENCH_ROOT=/Applications \
///   cargo test -p cmdr-index --release --lib -- --ignored --nocapture measure_boundary_probe
/// ```
///
/// The result is recorded in `docs/notes/cover-walk-primitive-2026-08-05.md`.
#[test]
#[ignore = "benchmark over a real tree; run manually with --nocapture"]
fn measure_boundary_probe() {
    use std::io::Write;

    fn no_pin(_path: &Path) -> Option<u64> {
        None
    }

    let root = PathBuf::from(
        std::env::var("CMDR_BOUNDARY_BENCH_ROOT").expect("set CMDR_BOUNDARY_BENCH_ROOT to a real directory to walk"),
    );
    assert!(root.is_dir(), "{} isn't a directory", root.display());
    let mut out = std::io::stderr();

    // Each run gets its own empty index, and both include the writer drain, so
    // neither is measuring the other's queue.
    let measure = |device_of: fn(&Path) -> Option<u64>| {
        let (writer, db_path, _db_dir) = setup_writer();
        ensure_path_in_db(&db_path, &root, &writer);
        let started = Instant::now();
        let summary = cover_subtree_with_device_probe(
            &root,
            &IndexPathSpace::root(),
            &writer,
            &CancellationToken::new(),
            device_of,
        )
        .expect("the walk runs");
        writer.flush_blocking().expect("flush");
        let elapsed = started.elapsed();
        writer.shutdown();
        (elapsed, summary.total_dirs)
    };

    measure(no_pin); // warm the page cache
    let (without, dirs) = measure(no_pin);
    let (with, _) = measure(device_of);

    let per_dir = with.saturating_sub(without).as_nanos() as f64 / dirs.max(1) as f64;
    let _ = writeln!(
        out,
        "\n── {} ──\n  no pin: {without:>10.2?}\n     pin: {with:>10.2?}  over {} \
         \n verdict: {per_dir:.0} ns per directory, {:+.1}% wall clock",
        root.display(),
        pluralize(dirs, "dir"),
        100.0 * (with.as_secs_f64() / without.as_secs_f64() - 1.0),
    );
}

/// The real probe reads a real device, and tells two filesystems apart.
///
/// Every other boundary test injects a probe, so without this one the production
/// `device_of` could return a constant and nothing would notice (`cargo mutants`
/// found exactly that). `/dev` is the mount that's a different filesystem from
/// `/` on both platforms we run on: devfs on macOS, devtmpfs on Linux.
#[test]
#[cfg(unix)]
fn the_real_device_probe_tells_two_filesystems_apart() {
    let root = device_of(Path::new("/")).expect("`/` has a device");
    let dev = device_of(Path::new("/dev")).expect("`/dev` has a device");

    assert_ne!(root, dev, "a mounted filesystem is a different device from its parent");
    assert_eq!(
        device_of(Path::new("/")),
        Some(root),
        "and the answer is the path's, not a counter"
    );
    assert_eq!(
        device_of(Path::new("/no-such-path-cmdr-boundary-probe")),
        None,
        "a path that isn't there has no device to report"
    );
}

/// A walk whose own root has no readable device pins nothing, rather than pinning
/// `None` and cutting every child it finds.
#[test]
fn a_root_with_no_readable_device_pins_nothing() {
    let space = IndexPathSpace::root();
    let policy = WalkPolicy::with_device_probe(ScanRoot::Virgin, &space, Path::new("/gone"), |_| None);

    assert_eq!(policy.device, None);
    assert!(
        !policy.leaves_the_volume(Path::new("/gone/child")),
        "with no pin, nothing is a boundary"
    );
}
