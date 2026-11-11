//! The durable-enough, live-applied network-enrichment policy state: the per-volume
//! SMB opt-in, the "always index" overrides, and the transient paused-volume set.
//!
//! **Why a settings-seeded global, not a fourth per-volume store.** The opt-in and
//! the overrides are user config (a handful of volumes/folders), not per-image data,
//! so they ride the existing sparse settings store (`mediaIndex.*` keys, FE-owned)
//! rather than a new SQLite DB with its own writer thread. `media_index::scheduler`
//! runs off the IPC thread and consults this on every pass, so the values live in a
//! process-global seeded from `load_settings` at startup and live-applied through the
//! `media_index_set_*` commands (the standard backend-affecting-setting pattern). The
//! paused set is purely runtime (a disconnect marker), never persisted.

use std::collections::HashSet;
use std::sync::LazyLock;
use std::sync::RwLock;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

/// The user-set network-enrichment policy: which volumes are opted into background
/// SMB enrichment, and which volumes/folders are "always index" overrides.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NetworkEnrichConfig {
    /// Volume ids the user opted into background network enrichment (off by default;
    /// turning on the master toggle does NOT auto-enrich network volumes).
    pub opted_in_volumes: HashSet<String>,
    /// Volume ids marked "always index": enrich regardless of the importance
    /// threshold (a NAS archive scores low on navigation-based importance, so without
    /// this its photos defer forever — media_index Decision 6).
    pub always_index_volumes: HashSet<String>,
    /// Absolute folder paths (OS-mount form, e.g. `/Volumes/naspi/Photos`) marked
    /// "always index": every image at or under one enriches regardless of importance.
    pub always_index_folders: HashSet<String>,
    /// Absolute folder paths the user EXCLUDED from photo-search indexing (the privacy
    /// complement to the opt-in — a sensitive high-importance folder like
    /// `~/Documents/IDs`). A hard veto: an image at or under an excluded folder never
    /// enriches, even under an "always index" override. Plan § Privacy.
    pub excluded_folders: HashSet<String>,
}

impl NetworkEnrichConfig {
    /// Whether `volume_id` is opted into background network enrichment.
    pub fn is_opted_in(&self, volume_id: &str) -> bool {
        self.opted_in_volumes.contains(volume_id)
    }

    /// Whether an image at OS path `os_path` on `volume_id` is covered by an
    /// "always index" override (the whole volume, or an ancestor folder).
    pub fn covers(&self, volume_id: &str, os_path: &str) -> bool {
        self.always_index_volumes.contains(volume_id)
            || self.always_index_folders.iter().any(|f| path_is_within(os_path, f))
    }

    /// Whether an "always index" override could cover ANY file directly inside the
    /// folder `dir` — the DIRECTORY-level question a pass asks before it walks a
    /// directory at all, where [`Self::covers`] answers the per-file one.
    ///
    /// True on three counts: the whole volume is an override, an ancestor folder (or
    /// `dir` itself) is one, or an override entry names something at or under `dir`.
    /// That third count is what makes this a provable SUPERSET of `covers` over `dir`'s
    /// children: `covers` is a prefix test, so `covers(v, "{dir}/{name}")` can only be
    /// true where `covers(v, dir)` is, or where the override entry IS `"{dir}/{name}"`,
    /// and an entry at or under `dir` is exactly that case. A filter built on this can
    /// therefore never drop a file the per-file gate would have taken, whatever an
    /// override entry turns out to name.
    pub(crate) fn may_cover_within(&self, volume_id: &str, dir: &str) -> bool {
        self.covers(volume_id, dir) || self.always_index_folders.iter().any(|f| path_is_within(f, dir))
    }

    /// Whether an image at OS path `os_path` is under a user-excluded folder — a hard
    /// veto that beats any "always index" override (the privacy complement).
    pub fn is_excluded(&self, os_path: &str) -> bool {
        self.excluded_folders.iter().any(|f| path_is_within(os_path, f))
    }

    /// Whether this EXACT folder is on the chosen-folder list. Distinct from
    /// [`Self::covers`]: the folder context menu offers "remove" only for a real entry,
    /// never for a folder some ancestor covers (removing what isn't there would be a
    /// lie). Trailing slashes don't change membership.
    pub fn is_chosen_folder(&self, os_path: &str) -> bool {
        let path = os_path.trim_end_matches('/');
        self.always_index_folders
            .iter()
            .any(|f| f.trim_end_matches('/') == path)
    }

    /// Whether an ANCESTOR (never the folder itself) is on the chosen-folder list, so
    /// this folder is already covered and adding it would write a redundant entry.
    pub fn is_covered_by_parent_folder(&self, os_path: &str) -> bool {
        !self.is_chosen_folder(os_path) && self.always_index_folders.iter().any(|f| path_is_within(os_path, f))
    }
}

/// Whether `path` is `ancestor` itself or lives under it: a component-boundary prefix (so
/// `/Photos2` isn't "within" `/Photos`, and a trailing slash doesn't matter), compared under
/// the platform's filesystem name rules. Those rules are the drive index's own
/// [`normalize_for_comparison`](crate::indexing::store::normalize_for_comparison), the
/// folding behind its `platform_case` collation: on macOS the NFC and NFD forms of a name
/// and a case-only difference name the same folder; elsewhere the comparison is byte-exact,
/// as ext4 and btrfs are.
///
/// The ONE folder matcher the exclusion veto, the read-time exclusion filter, the purge, and
/// the "always index" overrides share, so none of them can disagree about what a folder
/// holds.
pub(crate) fn path_is_within(path: &str, ancestor: &str) -> bool {
    let ancestor = ancestor.trim_end_matches('/');
    if ancestor.is_empty() {
        return true; // "/" (or empty) is an ancestor of everything
    }
    // Byte-identical is the common case, and answering it needs no allocation.
    if is_component_prefix(path, ancestor) {
        return true;
    }
    is_component_prefix(
        &crate::indexing::store::normalize_for_comparison(path),
        &crate::indexing::store::normalize_for_comparison(ancestor),
    )
}

/// `path` is `ancestor`, or continues past it at a `/`. Folding never adds or removes a
/// `/`, so the boundary holds on the normalized forms too.
fn is_component_prefix(path: &str, ancestor: &str) -> bool {
    path == ancestor || path.strip_prefix(ancestor).is_some_and(|rest| rest.starts_with('/'))
}

/// The process-global config, seeded from settings at startup and live-applied.
static CONFIG: LazyLock<RwLock<NetworkEnrichConfig>> = LazyLock::new(|| RwLock::new(NetworkEnrichConfig::default()));

/// Volume ids whose enrichment paused on a disconnect (unmount). Purely runtime: a
/// paused volume keeps every completed row and resumes on reconnect (never GC'd, never
/// marked failed — that's the disconnect data-safety line).
static PAUSED: LazyLock<RwLock<HashSet<String>>> = LazyLock::new(|| RwLock::new(HashSet::new()));

/// Replace the whole config (startup seed + live-apply of a bulk change).
pub fn set_config(config: NetworkEnrichConfig) {
    *CONFIG.write_ignore_poison() = config;
}

/// A snapshot of the current config (cheap clone; consulted once per pass).
pub fn snapshot() -> NetworkEnrichConfig {
    CONFIG.read_ignore_poison().clone()
}

/// Set or clear a volume's background-enrichment opt-in (live-applied).
pub fn set_opted_in(volume_id: &str, opted_in: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if opted_in {
        cfg.opted_in_volumes.insert(volume_id.to_string());
    } else {
        cfg.opted_in_volumes.remove(volume_id);
    }
}

/// Whether `volume_id` is opted into background network enrichment.
pub fn is_opted_in(volume_id: &str) -> bool {
    CONFIG.read_ignore_poison().is_opted_in(volume_id)
}

/// Set or clear a whole-volume "always index" override (live-applied).
pub fn set_always_index_volume(volume_id: &str, always: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if always {
        cfg.always_index_volumes.insert(volume_id.to_string());
    } else {
        cfg.always_index_volumes.remove(volume_id);
    }
}

/// Set or clear a folder "always index" override (live-applied). `folder` is an
/// absolute OS-mount path.
pub fn set_always_index_folder(folder: &str, always: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if always {
        cfg.always_index_folders.insert(folder.to_string());
    } else {
        cfg.always_index_folders.remove(folder);
    }
}

/// Set or clear a folder photo-search EXCLUSION (live-applied). `folder` is an
/// absolute path; every image at or under it is skipped (privacy veto).
pub fn set_excluded_folder(folder: &str, excluded: bool) {
    let mut cfg = CONFIG.write_ignore_poison();
    if excluded {
        cfg.excluded_folders.insert(folder.to_string());
    } else {
        cfg.excluded_folders.remove(folder);
    }
}

/// Whether an image at OS path `os_path` is under a user-excluded folder (a hard
/// privacy veto, beats any override).
pub fn is_excluded(os_path: &str) -> bool {
    CONFIG.read_ignore_poison().is_excluded(os_path)
}

/// Whether this EXACT folder is on the chosen-folder list (the folder context menu's
/// remove-vs-add decision).
pub fn is_chosen_folder(os_path: &str) -> bool {
    CONFIG.read_ignore_poison().is_chosen_folder(os_path)
}

/// Whether an ancestor folder (never this one) is on the chosen-folder list.
pub fn is_covered_by_parent_folder(os_path: &str) -> bool {
    CONFIG.read_ignore_poison().is_covered_by_parent_folder(os_path)
}

/// Mark a volume paused (its enrichment stopped on a disconnect; resumes on reconnect).
pub(crate) fn mark_paused(volume_id: &str) {
    PAUSED.write_ignore_poison().insert(volume_id.to_string());
}

/// Clear a volume's paused marker (it reconnected / completed a pass).
pub(crate) fn clear_paused(volume_id: &str) {
    PAUSED.write_ignore_poison().remove(volume_id);
}

/// Whether a volume is currently paused (disconnected mid-enrichment).
pub fn is_paused(volume_id: &str) -> bool {
    PAUSED.read_ignore_poison().contains(volume_id)
}

/// The enrichment veto for an image a network pass reaches at `os_path` under its live
/// `mount_root`: excluded when a folder excludes it at the live root OR at the same
/// index-relative path under any root the share was recorded under (`recorded_roots`), so a
/// share remounted under a new name keeps an exclusion set under its old one. Reads the live
/// exclusion list, like [`is_excluded`]; `read/exclusion.rs` applies the same union to a read.
pub(crate) fn is_excluded_at_known_roots(os_path: &str, mount_root: &str, recorded_roots: &[String]) -> bool {
    let config = CONFIG.read_ignore_poison();
    if config.is_excluded(os_path) {
        return true;
    }
    let Some(index_path) = super::fetch::os_folder_to_index_prefix(os_path, mount_root) else {
        return false;
    };
    recorded_roots
        .iter()
        .any(|root| config.is_excluded(&super::fetch::os_join(root, &index_path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resets the process-global config on drop, so a failing test can't leave a folder
    /// excluded for the next one.
    struct ResetConfig;

    impl Drop for ResetConfig {
        fn drop(&mut self) {
            set_config(NetworkEnrichConfig::default());
        }
    }

    #[test]
    fn a_remounted_share_stays_excluded_at_the_root_its_exclusion_was_set_under() {
        // The folder was excluded while the share mounted at /Volumes/naspi; today it's at
        // /Volumes/naspi-1. The veto has to judge the image at every root the volume is known
        // by, or the next pass re-reads the folder the person excluded.
        let _reset = ResetConfig;
        set_config(NetworkEnrichConfig {
            excluded_folders: ["/Volumes/naspi/Photos".to_string()].into_iter().collect(),
            ..NetworkEnrichConfig::default()
        });
        let recorded = ["/Volumes/naspi".to_string()];
        assert!(is_excluded_at_known_roots(
            "/Volumes/naspi-1/Photos/scan.jpg",
            "/Volumes/naspi-1",
            &recorded
        ));
        assert!(!is_excluded_at_known_roots(
            "/Volumes/naspi-1/Docs/scan.jpg",
            "/Volumes/naspi-1",
            &recorded
        ));
        assert!(
            is_excluded_at_known_roots("/Volumes/naspi/Photos/scan.jpg", "/Volumes/naspi", &[]),
            "the live root alone still counts"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn path_within_matches_across_unicode_normalization_forms() {
        // macOS hands out NFD names, while a typed or stored folder can be NFC. The same
        // folder in either form has to match, or an exclusion of `Útikönyv` misses its files.
        use unicode_normalization::UnicodeNormalization;
        let nfc_folder: String = "/Users/me/Útikönyv".nfc().collect();
        let nfd_path: String = "/Users/me/Útikönyv/scan.jpg".nfd().collect();
        assert!(
            !nfd_path.starts_with(&nfc_folder),
            "premise: the two forms differ byte for byte"
        );
        assert!(
            path_is_within(&nfd_path, &nfc_folder),
            "an NFC folder covers an NFD path"
        );
        let nfd_folder: String = "/Users/me/Útikönyv".nfd().collect();
        let nfc_path: String = "/Users/me/Útikönyv/scan.jpg".nfc().collect();
        assert!(path_is_within(&nfc_path, &nfd_folder), "and the other way round");
        let sibling: String = "/Users/me/Útikönyv2/scan.jpg".nfd().collect();
        assert!(
            !path_is_within(&sibling, &nfc_folder),
            "still component-safe after normalizing"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn path_within_ignores_case_where_the_filesystem_does() {
        // APFS and SMB shares on macOS treat `IDs` and `ids` as one folder.
        assert!(path_is_within("/Users/me/ids/passport.jpg", "/Users/me/IDs"));
        assert!(path_is_within("/VOLUMES/NASPI/photos/a.jpg", "/Volumes/naspi/Photos/"));
        assert!(!path_is_within("/Users/me/IDs2/a.jpg", "/users/me/ids"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn path_within_is_byte_exact_where_the_filesystem_is() {
        // ext4 and btrfs keep `IDs` and `ids` as two folders, so folding them would exclude a
        // folder nobody named.
        assert!(!path_is_within("/home/me/ids/passport.jpg", "/home/me/IDs"));
    }

    #[test]
    fn path_within_is_trailing_slash_safe() {
        assert!(path_is_within("/Volumes/naspi/Photos", "/Volumes/naspi/Photos"));
        assert!(path_is_within("/Volumes/naspi/Photos/a.jpg", "/Volumes/naspi/Photos"));
        assert!(path_is_within("/Volumes/naspi/Photos/a.jpg", "/Volumes/naspi/Photos/"));
        // A sibling that shares a name prefix is NOT within.
        assert!(!path_is_within("/Volumes/naspi/Photos2/a.jpg", "/Volumes/naspi/Photos"));
        assert!(!path_is_within("/Volumes/other/a.jpg", "/Volumes/naspi/Photos"));
    }

    #[test]
    fn covers_matches_whole_volume_or_ancestor_folder() {
        let cfg = NetworkEnrichConfig {
            opted_in_volumes: HashSet::new(),
            always_index_volumes: ["smb-vol".to_string()].into_iter().collect(),
            always_index_folders: ["/Volumes/naspi/Photos".to_string()].into_iter().collect(),
            excluded_folders: HashSet::new(),
        };
        // Whole-volume override.
        assert!(cfg.covers("smb-vol", "/anything/x.jpg"));
        // Folder override on a different volume.
        assert!(cfg.covers("other", "/Volumes/naspi/Photos/2026/x.jpg"));
        assert!(!cfg.covers("other", "/Volumes/naspi/Docs/x.jpg"));
    }

    #[test]
    fn may_cover_within_keeps_the_dir_holding_an_override_entry() {
        // The boundary the directory filter rests on. An override entry names a folder
        // in practice, but `covers` is a plain prefix test, so it would ALSO cover a
        // file whose path is that entry exactly. A dir filter built on `covers` alone
        // would drop that file's parent and lose it; this keeps the parent.
        let cfg = NetworkEnrichConfig {
            always_index_folders: ["/Users/dave/Photos".to_string()].into_iter().collect(),
            ..NetworkEnrichConfig::default()
        };
        assert!(cfg.may_cover_within("vol", "/Users/dave/Photos"), "the entry itself");
        assert!(cfg.may_cover_within("vol", "/Users/dave/Photos/2026"), "a dir under it");
        assert!(
            cfg.may_cover_within("vol", "/Users/dave"),
            "and its PARENT, because the entry could name a file directly inside it"
        );
        assert!(cfg.may_cover_within("vol", "/"), "up to the root, for the same reason");
        // A dir on neither side of the entry's chain is dropped: that's the whole point.
        assert!(!cfg.may_cover_within("vol", "/Users/dave/Docs"));
        assert!(
            !cfg.may_cover_within("vol", "/Users/dave/Photos2"),
            "a name-prefix sibling"
        );
    }

    #[test]
    fn chosen_membership_is_exact_and_parent_coverage_is_not() {
        let cfg = NetworkEnrichConfig {
            always_index_folders: ["/Users/dave/Photos/".to_string()].into_iter().collect(),
            ..NetworkEnrichConfig::default()
        };
        // The exact entry, trailing slash either side.
        assert!(cfg.is_chosen_folder("/Users/dave/Photos"));
        assert!(cfg.is_chosen_folder("/Users/dave/Photos/"));
        assert!(!cfg.is_covered_by_parent_folder("/Users/dave/Photos"));
        // A child is covered, but isn't itself an entry.
        assert!(!cfg.is_chosen_folder("/Users/dave/Photos/2026"));
        assert!(cfg.is_covered_by_parent_folder("/Users/dave/Photos/2026"));
        // An unrelated folder is neither.
        assert!(!cfg.is_chosen_folder("/Users/dave/Docs"));
        assert!(!cfg.is_covered_by_parent_folder("/Users/dave/Docs"));
    }
}
