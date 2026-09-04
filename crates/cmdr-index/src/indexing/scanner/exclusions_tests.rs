//! Tests for the scan exclusion policy: the two tiers, the root-position
//! pseudo-filesystem rule and its corroboration probe, and the exclusion-policy
//! stamp that re-arms an index when the policy's contents change.

use super::*;

/// Every external-mount prefix MUST also be a boot-disk exclusion prefix.
/// That's the invariant read routing rests on: a path under one of these is a
/// subtree `root`'s scan skips, so the external drive's own index is its sole
/// owner. If someone drops `/Volumes/` from `EXCLUDED_PREFIXES`, `root` would
/// start indexing external drives AND routing would still divert them — this
/// test fails loudly before that ships.
#[test]
fn external_mount_prefixes_are_excluded() {
    for prefix in EXTERNAL_MOUNT_PREFIXES {
        assert!(
            EXCLUDED_PREFIXES.contains(prefix),
            "{prefix} must be in EXCLUDED_PREFIXES so root's scan disowns the mount",
        );
    }
}

/// Nothing on this machine is a File Provider domain root or a Unix root.
fn no_probe(_path: &str) -> bool {
    false
}

/// Everything is a Unix root (paired with a specific domain probe, this isolates
/// the root-POSITION half of the rule).
fn every_dir_is_a_unix_root(_path: &str) -> bool {
    true
}

/// A directory named after a Linux pseudo-filesystem is skipped when it sits
/// DIRECTLY at the volume root of a Unix-like filesystem, in every scope: the
/// boot disk's `/proc`, an external drive's `/Volumes/X/proc`, an MTP-style scan
/// root's. This is what keeps an Android phone's `proc/<pid>/task/<tid>/…` tree
/// out of the index; before it, only the boot volume's absolute `/proc` prefix
/// was caught.
#[test]
fn pseudo_fs_at_a_unix_like_volume_root_is_skipped_in_every_scope() {
    let unix_root = |scope: ExclusionScope| scope.with_probes(no_probe, every_dir_is_a_unix_root);
    for name in PSEUDO_FS_BASENAMES {
        assert!(
            should_exclude(&format!("/{name}"), &unix_root(ExclusionScope::boot_disk())),
            "{name} at the boot root",
        );
        assert!(
            should_exclude(
                &format!("/Volumes/USB/{name}"),
                &unix_root(ExclusionScope::mount_rooted("/Volumes/USB")),
            ),
            "{name} at a mount root",
        );
        assert!(
            should_exclude(
                &format!("mtp://mtp-PIXEL9/65537/{name}"),
                &unix_root(ExclusionScope::mount_rooted("mtp://mtp-PIXEL9/65537")),
            ),
            "{name} at an MTP scan root",
        );
    }
}

/// The name alone is NOT enough. Someone's Dropbox with a top-level `dev` folder
/// (a very ordinary name for a real folder) must keep being indexed: excluding it
/// would drop it from sizes with no error at all, which is worse than a slow walk.
///
/// So the rule also demands corroboration that the root really is a Unix-like
/// filesystem: all three of `proc`, `sys`, and `dev` present as siblings. A cloud
/// folder has none of the other two, an Android root has all three.
#[test]
fn a_cloud_folder_named_dev_is_not_mistaken_for_a_pseudo_filesystem() {
    const DROPBOX: &str = "/Users/me/Library/CloudStorage/Dropbox";
    fn dropbox_is_a_domain_root(path: &str) -> bool {
        path == DROPBOX
    }
    // A real domain root, but its only pseudo-fs-shaped child is `dev`.
    let scope = ExclusionScope::boot_disk().with_probes(dropbox_is_a_domain_root, no_probe);

    for name in PSEUDO_FS_BASENAMES {
        assert!(
            !should_exclude(&format!("{DROPBOX}/{name}"), &scope),
            "{name} in a cloud drive is a user folder, not a pseudo-filesystem",
        );
    }
}

/// Same corroboration on a `/Volumes/X` mount root: a `dev` folder at the top of
/// someone's USB stick or backup drive stays indexed.
#[test]
fn a_folder_named_dev_at_a_mount_root_is_not_mistaken_for_a_pseudo_filesystem() {
    let scope = ExclusionScope::mount_rooted("/Volumes/Backup").with_probes(no_probe, no_probe);

    for name in PSEUDO_FS_BASENAMES {
        assert!(
            !should_exclude(&format!("/Volumes/Backup/{name}"), &scope),
            "{name} at the root of a plain drive is a user folder",
        );
    }
}

/// The corroboration probe itself, against real directories: a temp dir holding
/// all three of `proc`, `sys`, and `dev` reads as a Unix-like root; the same dir
/// with only `dev` does not, and neither does a symlink standing in for `proc`
/// (an Android root has a symlink `d` alongside its real `proc`/`sys`/`dev`).
#[test]
fn the_unix_root_probe_needs_all_three_real_directories() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().to_string_lossy().into_owned();

    std::fs::create_dir(dir.path().join("dev")).expect("create dev");
    assert!(!has_pseudo_fs_trio(&root), "`dev` alone is just a folder name");

    std::fs::create_dir(dir.path().join("sys")).expect("create sys");
    assert!(!has_pseudo_fs_trio(&root), "two of three is still not a Unix root");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("sys"), dir.path().join("proc")).expect("symlink proc");
        assert!(!has_pseudo_fs_trio(&root), "a symlink named proc is not the real thing");
        std::fs::remove_file(dir.path().join("proc")).expect("remove the symlink");
    }

    std::fs::create_dir(dir.path().join("proc")).expect("create proc");
    assert!(has_pseudo_fs_trio(&root), "all three present reads as a Unix-like root");
}

/// The rule keys on root POSITION, not on the name: an ordinary folder that
/// happens to be called `proc` (or `dev`, or `sys`) deeper in the tree stays
/// indexed. `~/projects/myapp/proc` is somebody's source directory.
#[test]
fn pseudo_fs_below_the_volume_root_stays_indexed() {
    for name in PSEUDO_FS_BASENAMES {
        assert!(
            !should_exclude(
                &format!("/Users/me/projects/myapp/{name}"),
                &ExclusionScope::boot_disk()
            ),
            "{name} deep on the boot disk is an ordinary folder",
        );
        assert!(
            !should_exclude(
                &format!("/Volumes/USB/a/{name}"),
                &ExclusionScope::mount_rooted("/Volumes/USB"),
            ),
            "{name} one level below a mount root is an ordinary folder",
        );
        // A child INSIDE the skipped tree isn't matched by this rule either
        // (the scanner never descends into a skipped dir, so nothing else needs it).
        assert!(
            !should_exclude(
                &format!("/{name}/1/task"),
                &ExclusionScope::mount_rooted("/Volumes/USB")
            ),
            "{name}'s children aren't matched by the root-position rule",
        );
    }
}

/// A File Provider domain root (Dropbox, Google Drive, iCloud Drive, MacDroid)
/// counts as a volume root, so the phone's `proc` tree MacDroid grafts under
/// `~/Library/CloudStorage/MacDroid-…` is skipped: the phone's root really is a
/// Unix root (its listing carries `proc`, `sys`, AND `dev` among `bin`, `etc`,
/// `sdcard`, …). Both probes are injected, so this needs neither a real provider
/// domain nor a phone attached.
#[test]
fn pseudo_fs_at_a_file_provider_domain_root_is_skipped() {
    const DOMAIN: &str = "/Users/me/Library/CloudStorage/MacDroid-pixel";
    fn fake_domain_probe(path: &str) -> bool {
        path == DOMAIN
    }
    let scope = ExclusionScope::boot_disk().with_probes(fake_domain_probe, every_dir_is_a_unix_root);

    assert!(
        should_exclude(&format!("{DOMAIN}/proc"), &scope),
        "a domain root's proc tree is a volume-root pseudo-filesystem",
    );
    // Same shape one level deeper is an ordinary folder: the parent isn't a domain root.
    assert!(
        !should_exclude(&format!("{DOMAIN}/sdcard/proc"), &scope),
        "only the domain root itself is a volume root",
    );
    // And with the real (macOS xattr) probe, an ordinary folder is never a domain root.
    assert!(
        !should_exclude(&format!("{DOMAIN}/proc"), &ExclusionScope::boot_disk()),
        "an unmarked parent is not a volume root",
    );
}

/// `is_on_mounted_external_volume` accepts a mounted-external path (mount root
/// and anything beneath it) and rejects boot-disk and cloud-drive paths.
#[test]
fn mounted_external_volume_detection() {
    #[cfg(target_os = "macos")]
    {
        assert!(is_on_mounted_external_volume("/Volumes/NONAME"));
        assert!(is_on_mounted_external_volume("/Volumes/NONAME/sub/deep"));
    }
    #[cfg(target_os = "linux")]
    {
        assert!(is_on_mounted_external_volume("/media/usb"));
        assert!(is_on_mounted_external_volume("/mnt/data/sub"));
    }
    // Boot-disk and cloud-drive paths are NOT on an external mount.
    assert!(!is_on_mounted_external_volume("/Users/me/project"));
    assert!(!is_on_mounted_external_volume(
        "/Users/me/Library/CloudStorage/Dropbox/x"
    ));
    assert!(!is_on_mounted_external_volume("/"));
}

// ── The exclusion-policy stamp ───────────────────────────────────

/// A fresh temp DB carrying the index schema, for the stamp tests.
fn temp_index() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("index.db");
    IndexStore::open(&db_path).expect("create index schema");
    (dir, db_path)
}

/// An index with no stamp was built under unknown rules, so nothing in it may
/// be trusted as covered. The alternative — assuming the current policy —
/// would quietly hide every subtree an older policy excluded.
#[test]
fn an_unstamped_index_predates_the_exclusion_policy() {
    let (_dir, db_path) = temp_index();
    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert!(index_predates_exclusion_policy(&conn));
}

/// The scan-start sequence stamps the index for real, through the writer. What
/// this pins is the wiring: a message that never reaches the DB would leave
/// every search walking its whole scope forever, with nothing failing.
#[test]
fn a_truncating_walk_stamps_the_policy_through_the_writer() {
    let (_dir, db_path) = temp_index();
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert!(index_predates_exclusion_policy(&conn), "test setup: an unstamped index");
    }

    // What `lifecycle/manager/start.rs` and `lifecycle/network_scan.rs` send
    // before a fresh walk.
    let writer =
        crate::indexing::writer::IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    writer.send(WriteMessage::TruncateData).expect("truncate");
    writer.send(exclusion_policy_stamp_message()).expect("stamp");
    writer.flush_blocking().expect("flush");
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert!(
        !index_predates_exclusion_policy(&conn),
        "a walk under the current policy leaves the index trustworthy"
    );
}

/// Editing any of the lists re-arms every existing index, because the stamp
/// records the policy's CONTENTS rather than a bare "done" flag. That's what
/// makes REMOVING a name safe: the subtrees it used to hide can't stay
/// invisible behind a stale claim of coverage.
#[test]
fn a_stamp_from_a_different_policy_re_arms_the_walk() {
    let (_dir, db_path) = temp_index();
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "0123456789abcdef")
        .expect("stamp an older policy");
    assert!(index_predates_exclusion_policy(&conn));
}

/// The fingerprint is a pure function of compile-time constants, so it can't
/// drift between the read that decides and the write that stamps.
#[test]
fn the_policy_fingerprint_is_stable() {
    assert_eq!(exclusion_policy_fingerprint(), exclusion_policy_fingerprint());
    assert_eq!(exclusion_policy_fingerprint().len(), 16, "a 64-bit FNV-1a in hex");
}
