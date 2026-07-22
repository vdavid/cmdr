//! NAS system/snapshot directories the recursive size scan must not descend into.
//!
//! Synology, QNAP, NetApp, and Windows SMB shares expose reserved pseudo-directories
//! that are catastrophic to recursively size:
//!
//! - **Snapshot trees** (`@Recently-Snapshot`, `#snapshot`, `.snapshot`) hold full
//!   point-in-time copies of the whole share. Their bytes are hardlinked/deduped, so
//!   summing them is both ruinously expensive (the scanner re-walks the entire
//!   filesystem once per snapshot) AND wrong (the total isn't real consumed space).
//!   One real report: a NAS first-scan stalled near 50% grinding through
//!   `@Recently-Snapshot`, which alone reported 44 TB on a 10 TB volume.
//! - **Thumbnail/metadata sidecars** (`@eaDir`, `.@__thumb`) live inside *every* media
//!   folder, so a position-based ("only at share root") skip would miss them — they
//!   have to be matched at any depth.
//! - **Recycle bins** (`@Recycle`, `#recycle`, `$RECYCLE.BIN`) and other system dirs
//!   are large and never what a size roll-up wants.
//!
//! These names are reserved vendor conventions (the `@` / `#` / `$` prefixes and
//! `System Volume Information` don't collide with real user folders), so a name match
//! is safe. We only SKIP RECURSION: the directory's own row is still indexed and stays
//! listed and navigable (a user can walk into `@Recycle` to restore a file); we just
//! don't auto-walk its subtree to compute a recursive size. Its size shows as unknown
//! (`—`/`≥`), the honest state, rather than `0 B`.
//!
//! Scope: applied by the `Volume`-trait network scanner (`network_scanner/mod.rs`) only,
//! which walks SMB/MTP shares — the home of these dirs. The local guarded walker has its
//! own `should_exclude`. `FileEntry` carries no DOS hidden/system attribute, so matching
//! the canonical names is the available signal; if attributes are plumbed through later,
//! "hidden + system" would generalize this without a hardcoded list.

/// Canonical names of NAS system/snapshot directories, matched case-insensitively
/// (NAS shares are typically case-insensitive). Extend as new vendor conventions
/// surface; keep it to reserved, non-user-collidable names.
const EXCLUDED_DIR_NAMES: &[&str] = &[
    // Synology
    "@eaDir",
    "@Recently-Snapshot",
    "@Recycle",
    "@sharesnap",
    "@sharebin",
    "@tmp",
    // QNAP
    "#recycle",
    "#snapshot",
    ".@__thumb",
    // NetApp / generic
    ".snapshot",
    ".snapshots",
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
            "$RECYCLE.BIN",
            "System Volume Information",
        ] {
            assert!(is_recursion_excluded_dir(name), "{name} should be excluded");
        }
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
        ] {
            assert!(!is_recursion_excluded_dir(name), "{name} should NOT be excluded");
        }
    }
}
