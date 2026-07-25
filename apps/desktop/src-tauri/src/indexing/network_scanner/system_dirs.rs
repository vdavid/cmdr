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
//! and one no user would pick for a real folder.** A false positive doesn't just hide a
//! size — it makes the prune (`writer/prune.rs`) delete that folder's indexed rows. So a
//! name goes in only with a citation, and the SMB-visibility question is part of it:
//! ONTAP's `~snapshot` looks like an obvious candidate and is deliberately absent,
//! because an SMB2 client can never enumerate it, so a `~snapshot` you can actually SEE
//! is a user folder. Rationale and sources: `DETAILS.md`.
//!
//! We only SKIP RECURSION: the directory's own row is still indexed and stays listed and
//! navigable (a user can walk into `@Recycle` to restore a file); we just don't auto-walk
//! its subtree to compute a recursive size. Its size shows as unknown (`—`/`≥`), the
//! honest state, rather than `0 B`.
//!
//! Scope: applied by the `Volume`-trait network scanner (`network_scanner/mod.rs`) only,
//! which walks SMB/MTP shares — the home of these dirs. The local guarded walker has its
//! own `should_exclude`, and indexes a folder with one of these names in FULL, which is
//! why the prune is gated on the volume kind. `FileEntry` carries no DOS hidden/system
//! attribute, so matching the canonical names is the available signal; if attributes are
//! plumbed through later, "hidden + system" would generalize this without a hardcoded list.

use crate::indexing::lifecycle::state::IndexVolumeKind;
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

/// The prune message for a volume of `kind`, or `None` when that kind's scanner
/// doesn't apply this exclusion at all.
///
/// **The gate is the whole point, and this is the only way in from outside
/// `network_scanner`.** The exclusion is a `Volume`-trait-scanner rule
/// ([`is_recursion_excluded_dir`]); the LOCAL guarded walker has its own
/// `should_exclude` and indexes a folder called `.snapshot` or `@eaDir` in full.
/// Pruning a local index against this list would delete rows the local scanner
/// really did produce, which is real user data disappearing from the index.
/// ❌ Never hand-build the message elsewhere.
pub(crate) fn prune_message_for_kind(kind: IndexVolumeKind) -> Option<WriteMessage> {
    kind.is_trait_scanned().then(prune_message)
}

/// The ungated prune message, for use INSIDE the trait scanner, which by
/// definition only ever runs for a trait-scanned volume. Everyone else goes
/// through [`prune_message_for_kind`].
pub(super) fn prune_message() -> WriteMessage {
    WriteMessage::PruneExcludedSubtrees {
        excluded_dir_names: EXCLUDED_DIR_NAMES.iter().map(|n| (*n).to_string()).collect(),
        fingerprint: exclusion_list_fingerprint(),
    }
}

/// A stable fingerprint of [`EXCLUDED_DIR_NAMES`], persisted per index under
/// `store::EXCLUDED_SUBTREES_PRUNED_KEY`.
///
/// Content-derived on purpose: ADDING a name changes the fingerprint, so every
/// existing index re-prunes itself on the next load with no version constant for
/// anyone to forget to bump. FNV-1a rather than `DefaultHasher` because the value
/// goes to disk and must not shift with a toolchain upgrade.
pub(crate) fn exclusion_list_fingerprint() -> String {
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

    /// The prune deletes indexed rows, so the kind gate is a data-safety control:
    /// the LOCAL walker indexes `@eaDir` / `.snapshot` folders in full, so pruning
    /// a locally-scanned index against this list would delete real user data.
    #[test]
    fn only_trait_scanned_volumes_get_a_prune_message() {
        for kind in [IndexVolumeKind::Smb, IndexVolumeKind::Mtp] {
            assert!(
                prune_message_for_kind(kind).is_some(),
                "{kind:?} is trait-scanned, so it applies the exclusion and can be pruned"
            );
        }
        for kind in [IndexVolumeKind::Local, IndexVolumeKind::LocalExternal] {
            assert!(
                prune_message_for_kind(kind).is_none(),
                "{kind:?} uses the local walker, which indexes these folders in full: never prune it"
            );
        }
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
