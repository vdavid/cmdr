//! Pure path/name classifiers shared by the production signal-assembly
//! (`scheduler`/`signals`) and the test fixture generator (`fixtures`).
//!
//! These decide the categorical signals a folder's [`FolderSignals`] carries:
//! whether its name is denylisted, its path class, and whether it looks
//! hidden/system. Keeping them in ONE place is load-bearing: the fixtures doc
//! warns that the test stand-in and the real assembler "must agree on what each
//! signal means", and the only way to guarantee that is to share the code, not
//! re-derive it. All pure (values in, category out), so the classification is
//! unit-testable and matches between fixtures and production by construction.
//!
//! [`FolderSignals`]: super::scorer::FolderSignals

use super::scorer::PathClass;

/// The last path component (folder name). A path with no final component (the
/// root `/`) folds back to the whole string.
pub fn leaf_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Whether a folder name is on the known-unimportant denylist: a set-membership
/// check on the folded name against the project-wide system-dir exclude list
/// (`node_modules`, `.git`, caches, build output), never a substring match.
/// Reusing the indexer's `SYSTEM_DIR_EXCLUDES` keeps
/// importance and search agreeing on what counts as machine output.
pub fn is_denylisted(name: &str) -> bool {
    // Every entry is ASCII, so for an ASCII name the ASCII fold IS the Unicode fold and
    // the comparison needs no allocation. A non-ASCII name still takes the exact
    // `to_lowercase` path: it can fold ONTO an ASCII name (U+212A KELVIN SIGN lowercases
    // to `k`), so the fast path can't just answer `false`.
    if name.is_ascii() {
        return DENYLIST_FOLDED
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(name));
    }
    let folded = name.to_lowercase();
    DENYLIST_FOLDED.contains(&folded)
}

/// The denylist, folded ONCE for the process rather than per call. A full recompute
/// classifies every folder on a volume, so folding the whole list per folder cost the
/// walk one allocation per entry per folder — tens of millions on a NAS-sized volume,
/// for a list that never changes.
static DENYLIST_FOLDED: std::sync::LazyLock<Vec<String>> =
    std::sync::LazyLock::new(|| crate::SYSTEM_DIR_EXCLUDES.iter().map(|d| d.to_lowercase()).collect());

/// Whether a folder is hidden or system-owned: a dotfile name, or a path that
/// classifies as [`PathClass::SystemOrCache`]. A FLOOR override in the scorer.
pub fn is_hidden_or_system(path: &str, name: &str, home: &str) -> bool {
    name.starts_with('.') || matches!(path_class(path, home), PathClass::SystemOrCache)
}

/// Whether a folder floors ON ITS OWN — a denylisted name OR hidden/system. This
/// is the seed for the descendant-floor propagation: a folder that self-floors
/// floors every folder below it too (`under_floored_ancestor`). Kept here, shared
/// by the production walk and the fixtures/evals derivation, so the two agree on
/// exactly which folders anchor a floored subtree.
pub fn self_floors(path: &str, name: &str, home: &str) -> bool {
    is_denylisted(name) || is_hidden_or_system(path, name, home)
}

/// Given every folder path (in any order) and the home root, return the subset
/// that sits UNDER a self-flooring ancestor — the `under_floored_ancestor` signal
/// for each. A folder is under-floored when any PROPER ancestor of it self-floors
/// (denylisted / hidden / system), whether or not that ancestor is itself in
/// `paths`. Pure string + classifier math over the folder set, so a scenario
/// derives it identically to how the production walk does — the shared derivation
/// the `classify` must-know calls for.
///
/// The self-flooring folders themselves are NOT returned (they floor via their own
/// flag, not this one); only their descendants are. Detection walks each folder's
/// own ancestor path components rather than the sibling set, so a floored ancestor
/// missing from `paths` (a `node_modules` the index pruned but whose children
/// remain) still floors the descendants.
/// Only the corpus scenarios and the test fixtures need this: the app applies the
/// floor per folder as it scores, rather than pre-computing the set.
#[cfg(any(test, feature = "tooling"))]
pub fn under_floored_paths<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    home: &str,
) -> std::collections::HashSet<String> {
    let mut under = std::collections::HashSet::new();
    for path in paths {
        if under_floored_ancestor(path, home) {
            under.insert(path.to_string());
        }
    }
    under
}

/// The `under_floored_ancestor` signal for ONE folder: whether any PROPER ancestor
/// of it self-floors (denylisted / hidden / system).
///
/// Pure path math — a folder's ancestors are exactly the prefixes of its own
/// absolute path, and each one's name is that prefix's last component — so this
/// needs neither the folder set nor the index. That is what lets an incremental
/// rescore compute the signal for a subtree it reads in isolation: the flooring
/// ancestor can sit far above the subtree root and still be seen.
pub fn under_floored_ancestor(path: &str, home: &str) -> bool {
    any_ancestor_self_floors(path, home)
}

/// Whether a folder floors purely by its PATH — it self-floors (denylisted /
/// hidden / system name) OR sits under a self-flooring ancestor. This is the
/// derive-on-read predicate the store's compaction leans on: a floored folder gets
/// no row, and the read API reconstructs its floored-ness from the path alone with
/// this, rather than storing a `0.0` blob. Pure string + name classification, no
/// index or listing data — the exact rule the recompute walk applies when deciding
/// to skip a row, so read and write agree by construction.
pub fn floors_by_path(path: &str, home: &str) -> bool {
    let name = leaf_name(path);
    self_floors(path, &name, home) || any_ancestor_self_floors(path, home)
}

/// Whether any PROPER ancestor directory of `path` self-floors. Walks the path's
/// own components from the second-to-last up, classifying each ancestor by its own
/// name + full ancestor path. The folder itself is excluded (start above it).
fn any_ancestor_self_floors(path: &str, home: &str) -> bool {
    let mut current = path;
    while let Some(pos) = current.rfind('/') {
        if pos == 0 {
            break; // reached the root `/`; no folder ancestor above it.
        }
        let ancestor = &current[..pos];
        let name = leaf_name(ancestor);
        if self_floors(ancestor, &name, home) {
            return true;
        }
        current = ancestor;
    }
    false
}

/// The project markers whose presence in a folder (or a descendant) marks it as
/// at/above a project root, raising the whole subtree. A set-membership check on
/// the folded child name.
pub const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "pom.xml",
    "build.gradle",
    "makefile",
    ".hg",
    ".svn",
];

/// Whether a folded child name is a project marker.
pub fn is_project_marker(folded_child_name: &str) -> bool {
    PROJECT_MARKERS.contains(&folded_child_name)
}

/// Classify a path into its [`PathClass`] prior, relative to the user's home.
///
/// A `~/Library` subtree and the system temp roots ([`TEMP_ROOTS`]) are
/// `SystemOrCache` (they stay low, and they FLOOR through
/// [`is_hidden_or_system`]); `Downloads`/`Desktop`/`Documents` and their subtrees
/// are `UserContent`; everything else is `Neutral`. `ProjectRoot` is NOT decided
/// here — it's the marker promotion [`path_class_with_marker`] layers on at
/// assembly time, since it depends on directory contents, not the path alone.
pub fn path_class(path: &str, home: &str) -> PathClass {
    if is_at_or_under_any(path, TEMP_ROOTS) {
        return PathClass::SystemOrCache;
    }
    for cache in HOME_CACHE_FOLDERS {
        if is_at_or_under(path, home, cache) {
            return PathClass::SystemOrCache;
        }
    }
    for content in HOME_CONTENT_FOLDERS {
        if is_at_or_under(path, home, content) {
            return PathClass::UserContent;
        }
    }
    PathClass::Neutral
}

/// Home-relative folders whose subtrees are `SystemOrCache` (so they also FLOOR).
const HOME_CACHE_FOLDERS: &[&str] = &["Library"];

/// Home-relative folders whose subtrees are `UserContent`.
const HOME_CONTENT_FOLDERS: &[&str] = &["Downloads", "Desktop", "Documents"];

/// The system temp roots, whose whole subtrees are machine scratch space rather
/// than something the user works in.
///
/// Both the `/private`-prefixed and the bare spellings are listed because macOS
/// firmlinks `/tmp` to `/private/tmp` and `/var` to `/private/var`: the index
/// stores whichever spelling the walk produced, and a classifier that knew only one
/// of them would score the same directory two different ways.
///
/// Evidence (2026-09-03, read out of the live `importance-root.db`): before these
/// were listed, `/private/tmp` stored at `score=0.898` with
/// `pathClass=projectRoot`, because Claude Code writes background task output under
/// `/private/tmp/claude-501/...` and the marker promotion fired on it. That held the
/// agent's wake interest above its `0.7` hot threshold continuously.
const TEMP_ROOTS: &[&str] = &[
    "/tmp",
    "/private/tmp",
    "/var/tmp",
    "/private/var/tmp",
    "/var/folders",
    "/private/var/folders",
];

/// The path prefixes under which one more component names a mounted volume, so
/// `/Volumes/backup` is a volume root while `/Volumes/backup/photos` is an ordinary
/// folder inside it. Per-platform, since the mount layout is.
#[cfg(target_os = "macos")]
const MOUNT_PREFIXES: &[&str] = &["/Volumes/"];
/// See the macOS arm.
#[cfg(target_os = "linux")]
const MOUNT_PREFIXES: &[&str] = &["/mnt/", "/media/"];
/// See the macOS arm. No mount convention we recognize, so only `/` is a root.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const MOUNT_PREFIXES: &[&str] = &[];

/// Whether `path` is the root of a volume: the disk root `/`, or exactly one
/// component under a [`MOUNT_PREFIXES`] entry.
pub fn is_volume_root(path: &str) -> bool {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return true; // the disk root `/`.
    }
    MOUNT_PREFIXES.iter().any(|prefix| {
        trimmed
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/'))
    })
}

/// A folder's [`PathClass`] once the project-marker promotion is applied: a folder
/// holding a marker (here or below) normally takes `ProjectRoot`, the strongest
/// prior.
///
/// Three kinds of path are exempt, because a marker there says nothing about the
/// folder being a place the user works:
///
/// - **`$HOME` itself.** A `.git` or `Makefile` sitting directly in the home
///   directory means dotfiles. The exemption only drops the promotion; `$HOME`
///   still scores on its ordinary signals. ❌ Never make `$HOME` FLOOR instead: the
///   floor propagates through [`under_floored_ancestor`] to every folder below it
///   and switches the whole feature off.
/// - **A volume root** ([`is_volume_root`]). A marker at `/` or at a mount point
///   would promote a whole disk.
/// - **A `SystemOrCache` path.** Machine scratch stays machine scratch; a
///   `package.json` written into a temp directory doesn't make it a project. These
///   floor either way, so the promotion only wrote a misleading `projectRoot` into
///   the stored signals.
///
/// An ordinary project directory is untouched: `~/projects/thing` with a `.git`
/// still promotes, and so does a project sitting UNDER a volume root.
pub fn path_class_with_marker(path: &str, home: &str, has_project_marker: bool) -> PathClass {
    let class = path_class(path, home);
    if !has_project_marker || !marker_can_promote(path, home, class) {
        return class;
    }
    PathClass::ProjectRoot
}

/// Whether a project marker found at `path` may raise it to `ProjectRoot`. The
/// exemptions are documented on [`path_class_with_marker`].
fn marker_can_promote(path: &str, home: &str, class: PathClass) -> bool {
    !matches!(class, PathClass::SystemOrCache) && path.trim_end_matches('/') != home && !is_volume_root(path)
}

// ── The scoring policy a store's rows were computed under ────────────

/// Bump when a classification RULE changes in code rather than in one of the lists
/// below: the fingerprint folds the lists in by content, but it can't see a change
/// to how they're applied.
///
/// Starts at `2`: `1` names the rules in the builds before any stamp existed, which
/// an absent [`SCORING_POLICY_KEY`](super::store::SCORING_POLICY_KEY) already stands
/// for.
///
/// `2`: the temp roots became `SystemOrCache`, and the project-marker promotion
/// stopped firing at `$HOME`, at a volume root, and on a `SystemOrCache` path.
const SCORING_RULES_VERSION: &str = "2";

/// A stable fingerprint of the classification policy a store's weights were
/// computed under, persisted per volume under `store::SCORING_POLICY_KEY`.
///
/// Content-derived over every list that decides a folder's categorical signals,
/// plus [`SCORING_RULES_VERSION`] for the rules that aren't a list. A stored value
/// that differs means the rows predate the policy this build applies, so the volume
/// needs a full recompute rather than being trusted: a classification change moves
/// scores, and nothing else would ever revisit the ~189,000 rows a scored volume
/// holds (observed on the local `root` volume, 2026-09-03). Mirrors the index's
/// `scanner::exclusion_policy_fingerprint`, down to sharing its mixing function.
///
/// ❌ The scorer's [`Weights`](super::scorer::Weights) are deliberately NOT folded
/// in. Every row persists the `FolderSignals` it was computed from, so a weight
/// change re-weights the stored signals without a rescan; only a change to the
/// SIGNALS themselves invalidates a row.
pub fn scoring_policy_fingerprint() -> String {
    crate::fingerprint::fingerprint_of(&scoring_policy_parts())
}

/// The policy's contents, flattened for hashing. Each list is preceded by its own
/// label, so moving a name from one list to another changes the fingerprint even
/// though the flat set of names didn't. Split out from
/// [`scoring_policy_fingerprint`] so a test can perturb one list and check the
/// stamp moves.
fn scoring_policy_parts() -> Vec<&'static str> {
    let mut parts: Vec<&str> = vec!["rules", SCORING_RULES_VERSION, "temp_roots"];
    parts.extend_from_slice(TEMP_ROOTS);
    parts.push("mount_prefixes");
    parts.extend_from_slice(MOUNT_PREFIXES);
    parts.push("home_cache");
    parts.extend_from_slice(HOME_CACHE_FOLDERS);
    parts.push("home_content");
    parts.extend_from_slice(HOME_CONTENT_FOLDERS);
    parts.push("project_markers");
    parts.extend_from_slice(PROJECT_MARKERS);
    parts.push("denylist");
    parts.extend_from_slice(crate::SYSTEM_DIR_EXCLUDES);
    parts
}

/// Whether `path` IS one of `roots` or sits under one: the absolute-path
/// counterpart of [`is_at_or_under`], for the roots that don't live under the home
/// directory. Guarded the same way, so `/var/tmpfoo` doesn't match `/var/tmp`.
fn is_at_or_under_any(path: &str, roots: &[&str]) -> bool {
    roots.iter().any(|root| {
        path.strip_prefix(root)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// Whether `path` IS `{home}/{folder}` or sits under it. Compares by stripping prefixes
/// rather than building the candidate path: a full recompute classifies every folder on
/// the volume, so a formatted string per candidate per folder is millions of throwaway
/// allocations a pass.
fn is_at_or_under(path: &str, home: &str, folder: &str) -> bool {
    let Some(rest) = path
        .strip_prefix(home)
        .and_then(|rest| rest.strip_prefix('/'))
        .and_then(|rest| rest.strip_prefix(folder))
    else {
        return false;
    };
    rest.is_empty() || rest.starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylist_is_folded_set_membership() {
        assert!(is_denylisted("node_modules"));
        assert!(is_denylisted("NODE_MODULES"), "folded, so case doesn't matter");
        assert!(is_denylisted(".git"));
        assert!(!is_denylisted("my_project"));
        // Set-membership, not substring: a name CONTAINING a denylisted word isn't denylisted.
        assert!(!is_denylisted("node_modules_backup"));
    }

    #[test]
    fn path_class_matches_the_fixture_rules() {
        let home = "/Users/test";
        assert_eq!(path_class("/Users/test/Downloads", home), PathClass::UserContent);
        assert_eq!(
            path_class("/Users/test/Documents/invoices", home),
            PathClass::UserContent
        );
        assert_eq!(path_class("/Users/test/Library/Caches", home), PathClass::SystemOrCache);
        assert_eq!(path_class("/Users/test/projects/webapp", home), PathClass::Neutral);
    }

    #[test]
    fn hidden_or_system_covers_dotfiles_and_library() {
        let home = "/Users/test";
        assert!(is_hidden_or_system("/Users/test/.config", ".config", home));
        assert!(is_hidden_or_system("/Users/test/Library/Caches", "Caches", home));
        assert!(!is_hidden_or_system("/Users/test/Downloads", "Downloads", home));
    }

    #[test]
    fn project_markers_are_folded_membership() {
        assert!(is_project_marker(".git"));
        assert!(is_project_marker("cargo.toml"));
        assert!(is_project_marker("package.json"));
        assert!(!is_project_marker("readme.md"));
    }

    #[test]
    fn self_floors_covers_denylist_and_hidden_system() {
        let home = "/Users/test";
        assert!(
            self_floors("/Users/test/proj/node_modules", "node_modules", home),
            "a denylisted folder self-floors"
        );
        assert!(
            self_floors("/Users/test/.config", ".config", home),
            "a dotfile self-floors"
        );
        assert!(
            self_floors("/Users/test/Library/Caches", "Caches", home),
            "a system/cache folder self-floors"
        );
        assert!(
            !self_floors("/Users/test/projects/webapp", "webapp", home),
            "an ordinary folder doesn't self-floor"
        );
    }

    #[test]
    fn under_floored_paths_marks_descendants_of_a_floored_ancestor() {
        let home = "/Users/test";
        let paths = [
            "/Users/test/projects/webapp",
            "/Users/test/projects/webapp/node_modules",
            "/Users/test/projects/webapp/node_modules/react",
            "/Users/test/projects/webapp/node_modules/react/cjs",
            "/Users/test/projects/webapp/.git",
            "/Users/test/projects/webapp/.git/refs/heads",
            "/Users/test/Documents/invoices",
        ];
        let under = under_floored_paths(paths.iter().copied(), home);

        // Descendants of node_modules and .git are under-floored.
        assert!(under.contains("/Users/test/projects/webapp/node_modules/react"));
        assert!(under.contains("/Users/test/projects/webapp/node_modules/react/cjs"));
        assert!(under.contains("/Users/test/projects/webapp/.git/refs/heads"));

        // The self-flooring anchors themselves are NOT in the set (they floor via
        // their own flag, not this one).
        assert!(!under.contains("/Users/test/projects/webapp/node_modules"));
        assert!(!under.contains("/Users/test/projects/webapp/.git"));

        // Folders outside any floored subtree are untouched.
        assert!(!under.contains("/Users/test/projects/webapp"));
        assert!(!under.contains("/Users/test/Documents/invoices"));
    }

    #[test]
    fn temp_roots_classify_as_system_or_cache() {
        let home = "/Users/test";
        for root in TEMP_ROOTS {
            assert_eq!(
                path_class(root, home),
                PathClass::SystemOrCache,
                "{root} is a temp root"
            );
        }
        assert_eq!(
            path_class("/private/tmp/claude-501/session", home),
            PathClass::SystemOrCache,
            "anything under a temp root is system/cache too"
        );
        assert_eq!(
            path_class("/private/var/folders/xx/yy/T", home),
            PathClass::SystemOrCache,
            "the macOS per-user $TMPDIR lives under /private/var/folders"
        );
    }

    #[test]
    fn a_temp_root_floors_itself_and_its_whole_subtree() {
        let home = "/Users/test";
        assert!(self_floors("/private/tmp", "tmp", home), "a temp root self-floors");
        assert!(
            under_floored_ancestor("/private/tmp/claude-501/scratchpad", home),
            "a folder under a temp root floors via the ancestor rule"
        );
        assert!(floors_by_path("/private/tmp/claude-501/scratchpad", home));
        assert!(floors_by_path("/private/var/folders/xx/yy/T/build", home));
    }

    #[test]
    fn a_temp_root_prefix_doesnt_capture_a_lookalike_sibling() {
        let home = "/Users/test";
        assert_eq!(
            path_class("/var/tmpfoo", home),
            PathClass::Neutral,
            "/var/tmpfoo is not /var/tmp"
        );
        assert!(!floors_by_path("/var/tmpfoo/project", home));
        assert_eq!(path_class("/tmpfoo", home), PathClass::Neutral);
        assert_eq!(path_class("/private/tmpfoo", home), PathClass::Neutral);
    }

    #[test]
    fn a_marker_doesnt_promote_home_or_a_volume_root() {
        let home = "/Users/test";
        assert_eq!(
            path_class_with_marker(home, home, true),
            PathClass::Neutral,
            "a .git in $HOME means dotfiles, not a project the user works in"
        );
        assert_eq!(
            path_class_with_marker("/", home, true),
            PathClass::Neutral,
            "a marker at the boot volume root doesn't promote the whole disk"
        );
        for root in MOUNT_PREFIXES {
            let mount = format!("{root}backup");
            assert_eq!(
                path_class_with_marker(&mount, home, true),
                PathClass::Neutral,
                "{mount} is a volume root"
            );
        }
    }

    #[test]
    fn home_does_not_floor_even_when_its_promotion_is_suppressed() {
        // Flooring $HOME would propagate through `under_floored_ancestor` to the
        // whole home directory and disable the feature.
        let home = "/Users/test";
        assert!(!self_floors(home, "test", home));
        assert!(!floors_by_path(home, home));
        assert!(!floors_by_path("/Users/test/projects/webapp", home));
    }

    #[test]
    fn a_marker_still_promotes_a_real_project_dir() {
        let home = "/Users/test";
        assert_eq!(
            path_class_with_marker("/Users/test/projects-git/vdavid/cmdr", home, true),
            PathClass::ProjectRoot,
            "an ordinary project dir keeps the strongest prior"
        );
        assert_eq!(
            path_class_with_marker("/Volumes/backup/repos/thing", home, true),
            PathClass::ProjectRoot,
            "a project UNDER a volume root still promotes"
        );
    }

    #[test]
    fn volume_roots_are_the_disk_root_and_one_level_under_a_mount_prefix() {
        assert!(is_volume_root("/"));
        for prefix in MOUNT_PREFIXES {
            assert!(is_volume_root(&format!("{prefix}backup")), "{prefix}backup");
            assert!(
                !is_volume_root(&format!("{prefix}backup/photos")),
                "one level under a mount prefix only"
            );
        }
        assert!(!is_volume_root("/Users/test"));
        assert!(!is_volume_root("/Users"));
    }

    /// The stamp is stable within a build. An unstable one would rescore every
    /// volume on every launch, which is the expensive direction of this mechanism
    /// failing.
    #[test]
    fn the_scoring_policy_fingerprint_is_stable() {
        assert_eq!(scoring_policy_fingerprint(), scoring_policy_fingerprint());
        assert_eq!(scoring_policy_fingerprint().len(), 16, "a 64-bit FNV-1a in hex");
    }

    /// Editing any list the classifiers read moves the fingerprint, which is the
    /// whole mechanism: a store scored under the old list re-arms with no version
    /// constant for anyone to forget to bump. Simulated over the real `parts`
    /// shape, since the constants themselves can't be mutated at runtime.
    #[test]
    fn adding_a_name_to_any_policy_list_moves_the_fingerprint() {
        let baseline = scoring_policy_fingerprint();
        for label in [
            "temp_roots",
            "mount_prefixes",
            "home_cache",
            "home_content",
            "project_markers",
            "denylist",
        ] {
            let mut parts = scoring_policy_parts();
            // Insert a new name right after the perturbed list's own label.
            let at = parts.iter().position(|p| *p == label).expect("label present") + 1;
            parts.insert(at, "/a-newly-listed-name");
            assert_ne!(
                crate::fingerprint::fingerprint_of(&parts),
                baseline,
                "a name added to {label} has to change the stamp"
            );
        }
        // And so does a rules change that no list can see.
        let mut parts = scoring_policy_parts();
        parts[1] = "a-later-rules-version";
        assert_ne!(crate::fingerprint::fingerprint_of(&parts), baseline);
    }

    #[test]
    fn under_floored_detects_an_ancestor_absent_from_the_path_set() {
        // The ancestor `node_modules` isn't in the input list (say the index pruned
        // it), but its descendant still floors — detection walks the path's own
        // components, not the sibling set.
        let home = "/Users/test";
        let paths = ["/Users/test/x/node_modules/pkg/dist"];
        let under = under_floored_paths(paths.iter().copied(), home);
        assert!(under.contains("/Users/test/x/node_modules/pkg/dist"));
    }
}
