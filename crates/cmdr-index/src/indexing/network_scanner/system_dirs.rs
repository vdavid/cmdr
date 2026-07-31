//! NAS system/snapshot directories the recursive size scan must not descend into.
//!
//! Synology, QNAP, NetApp, ZFS, netatalk, and Windows SMB shares all expose reserved
//! pseudo-directories that are catastrophic to recursively size:
//!
//! - **Snapshot trees** (`@Recently-Snapshot`, `#snapshot`, `.snapshot`, `.zfs`) hold full
//!   point-in-time copies of the whole share. Their bytes are hardlinked/deduped, so
//!   summing them is both ruinously expensive (the scanner re-walks the entire
//!   filesystem once per snapshot) AND wrong (the total isn't real consumed space).
//!   One real report: a NAS first-scan stalled near 50% grinding through
//!   `@Recently-Snapshot`, which alone reported 44 TB on a 10 TB volume.
//! - **Thumbnail/metadata sidecars** (`@eaDir`, `.@__thumb`, `.AppleDouble`) live inside
//!   *every* media folder, so a position-based ("only at share root") skip would miss
//!   them — they have to be matched at any depth.
//! - **Recycle bins** (`@Recycle`, `#recycle`, `$RECYCLE.BIN`, `Network Trash Folder`)
//!   and other system dirs are large and never what a size roll-up wants.
//!
//! **The bar for adding a name: it must be created by the vendor/protocol, documented,
//! and one no user would pick for a real folder.** A false positive hides a folder's
//! size, and (once the index is rebuilt against the new list) drops its subtree from the
//! index. So a name goes in only with a citation, and the SMB-visibility question is part
//! of it: ONTAP's `~snapshot` looks like an obvious candidate and is deliberately absent,
//! because an SMB2 client can never enumerate it, so a `~snapshot` you can actually SEE
//! is a user folder. Rationale and sources: `DETAILS.md`.
//!
//! We only SKIP RECURSION: the directory's own row is still indexed and stays listed and
//! navigable (a user can walk into `@Recycle` to restore a file); we just don't auto-walk
//! its subtree to compute a recursive size. Its size shows as unknown (`—`/`≥`), the
//! honest state, rather than `0 B`.
//!
//! An index built under an OLDER list still holds rows beneath these dirs, and no
//! reconcile can remove them (a reconcile only diffs the dirs it LISTS, and it never
//! lists these). So each index records the list it was BUILT against
//! ([`exclusion_list_fingerprint`], stamped when a scan truncates), and a mismatch makes
//! the next load rebuild the index from scratch instead of migrating it.
//!
//! Scope: the SMB/MTP side only — the home of these dirs. The `Volume`-trait network
//! scanner (`network_scanner/mod.rs`) applies it to both the fresh and the reconcile
//! walk, and the SMB live watcher (`transports/smb/watch.rs`) applies it to
//! `CHANGE_NOTIFY` too, so a live event can't put back what the scan won't write. The
//! local guarded walker has its own `should_exclude` and indexes a folder with one of
//! these names in FULL, which is why the stamp and the rebuild live on the network scan
//! path only. `FileEntry` carries no DOS hidden/system attribute, so matching the
//! canonical names is the available signal; if attributes are plumbed through later,
//! "hidden + system" would generalize this without a hardcoded list.

use crate::indexing::store::IndexStore;
use crate::indexing::writer::WriteMessage;

/// Canonical names of NAS system/snapshot directories, matched case-insensitively
/// (NAS shares are typically case-insensitive). Extend as new vendor conventions
/// surface; keep it to reserved, non-user-collidable names.
const EXCLUDED_DIR_NAMES: &[&str] = &[
    // Synology DSM
    "@eaDir",     // media-index thumbnails, in EVERY folder holding indexed media
    "#recycle",   // per-share recycle bin
    "#snapshot",  // per-share snapshot view ("Make snapshot visible")
    "@sharesnap", // share snapshots, volume root
    "@sharebin",  // unverified; no vendor doc found
    "@tmp",       // volume-root system dir
    // QNAP QTS
    "@Recently-Snapshot", // per-share snapshot view, on by default over SMB
    "@Recycle",           // per-share network recycle bin
    ".@__thumb",          // File Station / Multimedia Console thumbnail cache, any depth
    // NetApp ONTAP (NFS-side name; ONTAP's SMB view is `~snapshot`, which an SMB2
    // client can't enumerate at all, so a VISIBLE `~snapshot` is a user folder — see
    // `DETAILS.md` § "NAS snapshot/system dirs aren't recursed")
    ".snapshot",
    // Linux snapper / Btrfs default subvolume
    ".snapshots",
    // OpenZFS control dir (`.zfs/snapshot`), dataset root, hidden unless `snapdir=visible`
    ".zfs",
    // Netatalk / AFP sidecars
    ".AppleDouble", // resource forks, in EVERY folder
    ".AppleDB",
    ".AppleDesktop",
    "Network Trash Folder",
    "TheFindByContentFolder",
    "TheVolumeSettingsFolder",
    // macOS
    ".TemporaryItems",
    // Windows / SMB
    "$RECYCLE.BIN",
    "System Volume Information",
];

/// Whether the recursive size scan should NOT descend into a directory with this name.
///
/// `name` is a single path component (the directory's own name, not a path). Matched
/// case-insensitively against [`EXCLUDED_DIR_NAMES`].
pub(crate) fn is_recursion_excluded_dir(name: &str) -> bool {
    EXCLUDED_DIR_NAMES
        .iter()
        .any(|excluded| name.eq_ignore_ascii_case(excluded))
}

/// Whether this index still has to be REBUILT against the current exclusion list.
///
/// The stamp says which list the DB was last BUILT (truncated and rescanned)
/// against, so adding a name re-arms every existing network index without a schema
/// bump (which would throw away every index on the machine, including a 6.9M-entry
/// local one, to fix a network-only problem). An absent or stale stamp answers
/// "yes", and so does a read failure: a redundant rebuild costs a rescan, a skipped
/// one leaves rows today's scanner would never write.
pub(crate) fn index_predates_exclusion_list(conn: &rusqlite::Connection) -> bool {
    let stored = IndexStore::get_meta(conn, crate::indexing::store::SYSTEM_DIR_EXCLUSIONS_KEY);
    !matches!(stored, Ok(Some(ref v)) if *v == exclusion_list_fingerprint())
}

/// The message that stamps an index as built against the current exclusion list.
///
/// ❌ Send it ONLY right after a `TruncateData`. That's the one moment the DB
/// provably holds no row beneath an excluded dir; a reconcile never lists those
/// dirs, so it must never claim the current list.
pub(crate) fn exclusion_stamp_message() -> WriteMessage {
    WriteMessage::UpdateMeta {
        key: crate::indexing::store::SYSTEM_DIR_EXCLUSIONS_KEY.to_string(),
        value: exclusion_list_fingerprint(),
    }
}

/// A stable fingerprint of [`EXCLUDED_DIR_NAMES`], persisted per index under
/// `store::SYSTEM_DIR_EXCLUSIONS_KEY`.
///
/// Content-derived on purpose: ADDING a name changes the fingerprint, so every
/// existing index rebuilds itself on the next load with no version constant for
/// anyone to forget to bump. FNV-1a rather than `DefaultHasher` because the value
/// goes to disk and must not shift with a toolchain upgrade.
fn exclusion_list_fingerprint() -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for name in EXCLUDED_DIR_NAMES {
        for byte in name.to_ascii_lowercase().bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_known_nas_system_dirs() {
        for name in [
            "@eaDir",
            "@Recently-Snapshot",
            "@Recycle",
            "#recycle",
            "#snapshot",
            ".snapshot",
            ".zfs",
            ".AppleDouble",
            "Network Trash Folder",
            ".TemporaryItems",
            "$RECYCLE.BIN",
            "System Volume Information",
        ] {
            assert!(is_recursion_excluded_dir(name), "{name} should be excluded");
        }
    }

    /// ONTAP renders `.snapshot` as `~snapshot` over SMB, but an SMB2 client can't
    /// enumerate it even with `showsnapshot` on: it's reachable only by typing the
    /// path. So a `~snapshot` that shows up in a listing is a USER folder, and
    /// excluding the name would be pure false positive — which now also means
    /// deleting that folder's indexed rows. Deliberately absent, pinned here so
    /// nobody "completes the set" later.
    #[test]
    fn does_not_exclude_the_smb_invisible_ontap_snapshot_name() {
        assert!(!is_recursion_excluded_dir("~snapshot"));
        assert!(!is_recursion_excluded_dir("~snapshtable"));
    }

    #[test]
    fn matches_case_insensitively() {
        assert!(is_recursion_excluded_dir("@eadir"));
        assert!(is_recursion_excluded_dir("@RECENTLY-SNAPSHOT"));
        assert!(is_recursion_excluded_dir("system volume information"));
    }

    #[test]
    fn keeps_ordinary_dirs() {
        for name in [
            "photos",
            "Dori-Dropbox",
            "videos",
            "eaDir",
            "recycle",
            "snapshot",
            "@myfiles",
            "zfs",
            "AppleDouble",
            "Trash",
            "Temporary Items",
            "Network Trash Folder Archive",
        ] {
            assert!(!is_recursion_excluded_dir(name), "{name} should NOT be excluded");
        }
    }

    /// A fresh temp DB carrying the index schema, for the stamp tests.
    fn temp_index() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("index.db");
        IndexStore::open(&db_path).expect("create index schema");
        (dir, db_path)
    }

    /// Every index that predates the stamp (i.e. every index built before this
    /// mechanism, which is where the 10.9M stale rows live) must be rebuilt.
    #[test]
    fn an_unstamped_index_predates_the_exclusion_list() {
        let (_dir, db_path) = temp_index();
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert!(
            index_predates_exclusion_list(&conn),
            "an index with no stamp was built under unknown rules and must be rebuilt"
        );
    }

    /// And once it's been rebuilt, it must be left alone: the rebuild costs a full
    /// rescan, so re-arming on every launch would rescan forever.
    #[test]
    fn a_stamped_index_is_not_rebuilt_again() {
        let (_dir, db_path) = temp_index();
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        let WriteMessage::UpdateMeta { key, value } = exclusion_stamp_message() else {
            panic!("the stamp must be a meta write");
        };
        IndexStore::update_meta(&conn, &key, &value).expect("stamp");

        assert!(
            !index_predates_exclusion_list(&conn),
            "an index built against the current list must not rebuild again"
        );
    }

    /// **The whole point of the rebuild.** An index built under an older list
    /// carries rows beneath a dir today's scanner never walks; the rebuild's
    /// truncate is what sheds them, and the stamp is what stops it happening again.
    #[test]
    fn a_rebuild_sheds_the_rows_under_an_excluded_dir_and_stamps_the_index() {
        let (_dir, db_path) = temp_index();
        {
            let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
            let excluded =
                IndexStore::insert_entry_v2(&conn, 1, "@Recently-Snapshot", true, false, None, None, None, None)
                    .expect("insert excluded dir");
            IndexStore::insert_entry_v2(
                &conn,
                excluded,
                "hoarded.bin",
                false,
                false,
                Some(9),
                Some(9),
                None,
                None,
            )
            .expect("insert a row the scanner would never write today");
            assert!(index_predates_exclusion_list(&conn), "test setup: an unstamped index");
        }

        // What a `Rebuild` scan sends before it walks (`lifecycle/network_scan.rs`).
        let writer = crate::indexing::writer::IndexWriter::spawn(&db_path, crate::NoopEventSink::shared())
            .expect("spawn writer");
        writer.send(WriteMessage::TruncateData).expect("truncate");
        writer.send(exclusion_stamp_message()).expect("stamp");
        writer.flush_blocking().expect("flush");
        writer.shutdown();

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            IndexStore::get_entry_count(&conn).expect("count"),
            1,
            "only the root sentinel survives a rebuild: every stale row is gone"
        );
        assert!(
            !index_predates_exclusion_list(&conn),
            "the rebuilt index is stamped, so it doesn't rebuild again on the next load"
        );
    }

    /// Growing the list re-arms every existing index, because the stamp records
    /// the list's contents rather than a bare "done" flag.
    #[test]
    fn a_stamp_from_an_older_list_re_arms_the_rebuild() {
        let (_dir, db_path) = temp_index();
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(
            &conn,
            crate::indexing::store::SYSTEM_DIR_EXCLUSIONS_KEY,
            "0123456789abcdef",
        )
        .expect("stamp an older list");

        assert!(
            index_predates_exclusion_list(&conn),
            "a stamp from a different list must re-arm the rebuild"
        );
    }

    /// The fingerprint is content-derived so that GROWING the list re-arms every
    /// existing index, with no version constant to forget to bump.
    #[test]
    fn the_fingerprint_tracks_the_list_contents() {
        assert_eq!(
            exclusion_list_fingerprint(),
            exclusion_list_fingerprint(),
            "the fingerprint must be stable across calls; it's persisted on disk"
        );
        assert_eq!(
            exclusion_list_fingerprint().len(),
            16,
            "a fixed-width hex digest keeps the meta value tidy"
        );
        // Same hash function, one extra name ⇒ a different digest.
        let hash_of = |names: &[&str]| {
            const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
            const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
            let mut h = FNV_OFFSET;
            for name in names {
                for b in name.to_ascii_lowercase().bytes().chain(std::iter::once(b'\n')) {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(FNV_PRIME);
                }
            }
            format!("{h:016x}")
        };
        assert_ne!(
            hash_of(&["@eaDir"]),
            hash_of(&["@eaDir", ".zfs"]),
            "adding a name must change the fingerprint, so every index re-prunes"
        );
    }
}
