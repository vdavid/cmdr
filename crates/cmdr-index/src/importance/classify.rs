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
use crate::IndexVolumeKind;

/// Whose absolute paths a volume's index stores, which decides whether the boot
/// disk's own layout rules ([`SYSTEM_ROOTS`], [`TEMP_ROOTS`]) apply. Typed from the
/// volume's kind ([`Disk::of`]), ❌ never from its id string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disk {
    /// The boot disk: index paths ARE the machine's paths, so `/Library` is the OS's.
    Boot,
    /// Any other volume (an external drive, a share): index paths are relative to
    /// the volume's root, so a top-level `Library` is the user's own folder.
    Other,
}

impl Disk {
    /// The disk a volume of this kind is. Only [`IndexVolumeKind::Local`] is the
    /// boot disk.
    pub fn of(kind: IndexVolumeKind) -> Self {
        match kind {
            IndexVolumeKind::Local => Disk::Boot,
            IndexVolumeKind::LocalExternal | IndexVolumeKind::Smb | IndexVolumeKind::Mtp | IndexVolumeKind::Adb => {
                Disk::Other
            }
        }
    }
}

/// What a path is classified against: the user's home (for the home-relative
/// rules) and the [`Disk`] the path lives on (for the boot disk's own roots).
#[derive(Debug, Clone, Copy)]
pub struct PathSpace<'a> {
    /// The user's home directory; empty when unresolved (no home-relative rule fires).
    pub home: &'a str,
    /// Whether the volume's paths are the boot disk's.
    pub disk: Disk,
}

impl<'a> PathSpace<'a> {
    /// A path space for `home` on `disk`.
    pub fn new(home: &'a str, disk: Disk) -> Self {
        Self { home, disk }
    }
}

/// Tests classify boot-disk paths almost everywhere, so a bare home reads as the
/// boot disk there. Test-only on purpose: production code has to say which disk a
/// path is on, or a share's `Library` would floor.
#[cfg(test)]
impl<'a> From<&'a str> for PathSpace<'a> {
    fn from(home: &'a str) -> Self {
        Self::new(home, Disk::Boot)
    }
}

/// See the `&str` conversion.
#[cfg(test)]
impl<'a> From<&'a String> for PathSpace<'a> {
    fn from(home: &'a String) -> Self {
        Self::new(home, Disk::Boot)
    }
}

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
/// Reusing `cmdr_fs::system_dirs::SYSTEM_DIR_EXCLUDES` keeps
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
static DENYLIST_FOLDED: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
    cmdr_fs::system_dirs::SYSTEM_DIR_EXCLUDES
        .iter()
        .map(|d| d.to_lowercase())
        .collect()
});

/// Whether a folder is hidden or system-owned: a dotfile name, or a path that
/// classifies as [`PathClass::SystemOrCache`]. A FLOOR override in the scorer.
pub fn is_hidden_or_system<'a>(path: &str, name: &str, space: impl Into<PathSpace<'a>>) -> bool {
    name.starts_with('.') || matches!(path_class(path, space), PathClass::SystemOrCache)
}

/// Whether a folder floors ON ITS OWN — a denylisted name OR hidden/system. This
/// is the seed for the descendant-floor propagation: a folder that self-floors
/// floors every folder below it too (`under_floored_ancestor`). Kept here, shared
/// by the production walk and the fixtures/evals derivation, so the two agree on
/// exactly which folders anchor a floored subtree.
pub fn self_floors<'a>(path: &str, name: &str, space: impl Into<PathSpace<'a>>) -> bool {
    is_denylisted(name) || is_hidden_or_system(path, name, space)
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
pub fn under_floored_paths<'a, 'h>(
    paths: impl IntoIterator<Item = &'a str>,
    space: impl Into<PathSpace<'h>>,
) -> std::collections::HashSet<String> {
    let space = space.into();
    let mut under = std::collections::HashSet::new();
    for path in paths {
        if under_floored_ancestor(path, space) {
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
pub fn under_floored_ancestor<'a>(path: &str, space: impl Into<PathSpace<'a>>) -> bool {
    any_ancestor_self_floors(path, space.into())
}

/// Whether a folder floors purely by its PATH — it self-floors (denylisted /
/// hidden / system name) OR sits under a self-flooring ancestor. This is the
/// derive-on-read predicate the store's compaction leans on: a floored folder gets
/// no row, and the read API reconstructs its floored-ness from the path alone with
/// this, rather than storing a `0.0` blob. Pure string + name classification, no
/// index or listing data — the exact rule the recompute walk applies when deciding
/// to skip a row, so read and write agree by construction.
pub fn floors_by_path<'a>(path: &str, space: impl Into<PathSpace<'a>>) -> bool {
    let space = space.into();
    let name = leaf_name(path);
    self_floors(path, &name, space) || any_ancestor_self_floors(path, space)
}

/// Whether any PROPER ancestor directory of `path` self-floors. Walks the path's
/// own components from the second-to-last up, classifying each ancestor by its own
/// name + full ancestor path. The folder itself is excluded (start above it).
fn any_ancestor_self_floors(path: &str, space: PathSpace<'_>) -> bool {
    let mut current = path;
    while let Some(pos) = current.rfind('/') {
        if pos == 0 {
            break; // reached the root `/`; no folder ancestor above it.
        }
        let ancestor = &current[..pos];
        let name = leaf_name(ancestor);
        if self_floors(ancestor, &name, space) {
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
/// These are `SystemOrCache` (they stay low, and they FLOOR through
/// [`is_hidden_or_system`]): the system temp roots ([`TEMP_ROOTS`]), the OS and
/// package-manager roots ([`SYSTEM_ROOTS`]), the home's own caches and package
/// stores ([`HOME_CACHE_FOLDERS`]), and the insides of an app bundle or library package
/// ([`BUNDLE_SUFFIXES`], [`LIBRARY_PACKAGE_SUFFIXES`]). `Downloads`/`Desktop`/`Documents` and their subtrees are
/// `UserContent`; everything else is `Neutral`. `ProjectRoot` is NOT decided here —
/// it's the marker promotion [`path_class_with_marker`] layers on at assembly time,
/// since it depends on directory contents, not the path alone.
///
/// The absolute roots are the boot disk's own layout, so they apply only on
/// [`Disk::Boot`]: on a share, `/Library` is a top-level folder of the user's.
/// ❌ They also never classify `$HOME`, anything under it, or an ancestor of it: a
/// home living under one (Fedora Silverblue's `/var/home/x`) would otherwise floor,
/// and the floor would switch the whole feature off.
pub fn path_class<'a>(path: &str, space: impl Into<PathSpace<'a>>) -> PathClass {
    let PathSpace { home, disk } = space.into();
    if disk == Disk::Boot
        && !is_on_home_branch(path, home)
        && (is_at_or_under_any(path, TEMP_ROOTS) || is_at_or_under_any(path, SYSTEM_ROOTS))
    {
        return PathClass::SystemOrCache;
    }
    if is_inside_package(path) {
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

/// Home-relative folders whose subtrees are `SystemOrCache` (so they also FLOOR):
/// the per-user Library, and package stores the user never files into (`go/pkg`
/// held 59,199 of the root volume's 193,861 scored rows on 2026-10-26, its module
/// cache at 0.84). Dot-prefixed stores (`.cargo`, `.npm`, `.rustup`) already floor
/// as hidden.
const HOME_CACHE_FOLDERS: &[&str] = &["Library", "go/pkg"];

/// Absolute roots whose subtrees are machine-owned: the OS (`/System`, `/Library`,
/// `/usr`, `/bin`, `/sbin`, `/private/etc`, `/private/var`, and their firmlinked
/// spellings) and the package managers (`/opt/homebrew`, MacPorts' `/opt/local`,
/// `/nix`). Homebrew on Intel lives under `/usr/local`, which `/usr` covers.
///
/// Evidence (live `importance-root.db`, 2026-10-26): `/opt/homebrew/lib` stored at
/// 0.85, `/Library/User Template/Non_localized/Downloads` at 0.42, and `/usr/share`
/// at 0.77, crowding real folders out of the agent's top list.
///
/// Boot disk only ([`Disk::Boot`]): an external or network volume's index stores
/// volume-relative paths, where a top-level `Library` is the user's.
const SYSTEM_ROOTS: &[&str] = &[
    "/System",
    "/Library",
    "/usr",
    "/bin",
    "/sbin",
    "/cores",
    "/etc",
    "/private/etc",
    "/var",
    "/private/var",
    "/opt/homebrew",
    "/opt/local",
    "/nix",
];

/// Folder-name suffixes of an app bundle, whose `Contents` (and everything under it)
/// is the app's own machinery, never somewhere to file things. The bundle folder
/// itself keeps its ordinary class, and a project merely NAMED like one
/// (`getcmdr.app/src`) has no `Contents` and stays untouched. Evidence: the
/// LibreOffice, Xcode, and IntelliJ bundles held 56,655 scored rows (2026-10-26).
const BUNDLE_SUFFIXES: &[&str] = &[".app"];

/// The folder a bundle's machinery lives in, directly under the bundle.
const BUNDLE_CONTENTS: &str = "Contents";

/// Folder-name suffixes of a library package: the package is the user's (it shows
/// as one item in Finder), but everything INSIDE it is the owning app's database.
/// No `Contents` layer, so any descendant counts. Evidence: with the other rules in
/// place, `Photos Library.photoslibrary/database/search/...` and
/// `Music Library.musiclibrary` internals were the next junk in the folded top 15 of
/// the live root volume (2026-10-26).
const LIBRARY_PACKAGE_SUFFIXES: &[&str] = &[".photoslibrary", ".musiclibrary", ".tvlibrary"];

/// Whether `path` sits inside an app's machinery: at or under a `<name>.app/Contents`
/// folder, or strictly under a library package. Walks the path's components with no
/// allocation (it runs per folder in every pass).
fn is_inside_package(path: &str) -> bool {
    let has_suffix =
        |name: &str, suffixes: &[&str]| suffixes.iter().any(|suffix| ends_with_ignore_ascii_case(name, suffix));
    let mut previous: Option<&str> = None;
    for component in path.split('/') {
        if let Some(parent) = previous
            && ((component == BUNDLE_CONTENTS && has_suffix(parent, BUNDLE_SUFFIXES))
                || (!component.is_empty() && has_suffix(parent, LIBRARY_PACKAGE_SUFFIXES)))
        {
            return true;
        }
        previous = Some(component);
    }
    false
}

/// `name.ends_with(suffix)`, ASCII-case-insensitively (`Foo.APP` is a bundle too).
fn ends_with_ignore_ascii_case(name: &str, suffix: &str) -> bool {
    name.len() >= suffix.len()
        && name.is_char_boundary(name.len() - suffix.len())
        && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Whether `path` is `$HOME`, under it, or one of its ancestors: the paths the
/// absolute [`SYSTEM_ROOTS`] and [`TEMP_ROOTS`] must never claim.
fn is_on_home_branch(path: &str, home: &str) -> bool {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return false;
    }
    is_at_or_above_home(path, home) || path.strip_prefix(home).is_some_and(|rest| rest.starts_with('/'))
}

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
#[cfg(target_os = "windows")]
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

/// How far below a folder a project marker still counts for it: `0` is a marker
/// directly in the folder (a repo root, or a monorepo member with its own
/// `package.json`), `1` is a direct child that is a project root (a folder of
/// projects like `~/projects/`, which is where the user picks one).
///
/// Two levels up is already a generic container (`~/Desktop`, `/Users`,
/// `~/go/pkg/mod/github.com`), where a marker says nothing about the folder. With no
/// reach limit, one buried `go.mod` promoted every ancestor to `ProjectRoot`
/// (live `importance-root.db`, 2026-10-26: `/Users` 0.87, `~/go/pkg/mod` 0.84).
///
/// ❌ Changing it changes stored signals: bump [`SCORING_RULES_VERSION`].
pub const MARKER_REACH: u8 = 1;

/// A folder's marker depth as the signals carry it: `Some(0)` for a direct marker,
/// otherwise the nearest marker below (`nearest_below`, a distance of at least `1`)
/// when it's within [`MARKER_REACH`], else `None`. The ONE place the reach applies,
/// shared by production assembly, the incremental's marker guard, and the fixtures.
pub fn marker_depth(has_direct_marker: bool, nearest_below: Option<u8>) -> Option<u8> {
    if has_direct_marker {
        return Some(0);
    }
    nearest_below.filter(|depth| *depth <= MARKER_REACH)
}

/// A folder's [`PathClass`] once the project-marker promotion is applied.
/// `marker_depth` is [`marker_depth`]'s output: a direct marker (`Some(0)`) takes
/// `ProjectRoot`, the strongest prior; a marker one level below promotes only a
/// `Neutral` folder (a folder of projects), so a repo dropped on the Desktop doesn't
/// relabel `~/Desktop`.
///
/// Three kinds of path are exempt, because a marker there says nothing about the
/// folder being a place the user works:
///
/// - **`$HOME` and every ancestor of it** (`/Users`). A `.git` or `Makefile`
///   sitting directly in the home directory means dotfiles. The exemption only drops
///   the promotion; these still score on their ordinary signals. ❌ Never make
///   `$HOME` FLOOR instead: the floor propagates through [`under_floored_ancestor`]
///   to every folder below it and switches the whole feature off.
/// - **A volume root** ([`is_volume_root`]). A marker at `/` or at a mount point
///   would promote a whole disk.
/// - **A `SystemOrCache` path.** Machine scratch stays machine scratch; a
///   `package.json` written into a temp directory doesn't make it a project. These
///   floor either way, so the promotion only wrote a misleading `projectRoot` into
///   the stored signals.
///
/// An ordinary project directory is untouched: `~/projects/thing` with a `.git`
/// still promotes, and so does a project sitting UNDER a volume root.
pub fn path_class_with_marker<'a>(path: &str, space: impl Into<PathSpace<'a>>, marker_depth: Option<u8>) -> PathClass {
    let space = space.into();
    let class = path_class(path, space);
    let promotes = match marker_depth {
        Some(0) => true,
        Some(depth) if depth <= MARKER_REACH => matches!(class, PathClass::Neutral),
        _ => false,
    };
    if !promotes || !marker_can_promote(path, space.home, class) {
        return class;
    }
    PathClass::ProjectRoot
}

/// Whether a project marker found at `path` may raise it to `ProjectRoot`. The
/// exemptions are documented on [`path_class_with_marker`].
fn marker_can_promote(path: &str, home: &str, class: PathClass) -> bool {
    !matches!(class, PathClass::SystemOrCache) && !is_at_or_above_home(path, home) && !is_volume_root(path)
}

/// Whether `path` IS `$HOME` or one of its ancestors (`/Users`, `/`). An empty
/// `home` (unresolved) has no ancestors.
fn is_at_or_above_home(path: &str, home: &str) -> bool {
    let path = path.trim_end_matches('/');
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return false;
    }
    home.strip_prefix(path)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
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
///
/// `3`: a project marker reaches only [`MARKER_REACH`] levels up (the signal became
/// `marker_depth`), a marker below promotes only a `Neutral` folder, the promotion
/// skips every ancestor of `$HOME`, an app bundle's `Contents` and a library package's insides are `SystemOrCache`,
/// and the absolute roots never claim the home branch.
///
/// `4`: the absolute roots (system and temp) apply only on the boot disk ([`Disk::Boot`]); a
/// share or external drive stores volume-relative paths.
const SCORING_RULES_VERSION: &str = "4";

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
    parts.push("system_roots");
    parts.extend_from_slice(SYSTEM_ROOTS);
    parts.push("bundle_suffixes");
    parts.extend_from_slice(BUNDLE_SUFFIXES);
    parts.push("library_package_suffixes");
    parts.extend_from_slice(LIBRARY_PACKAGE_SUFFIXES);
    parts.push("home_cache");
    parts.extend_from_slice(HOME_CACHE_FOLDERS);
    parts.push("home_content");
    parts.extend_from_slice(HOME_CONTENT_FOLDERS);
    parts.push("project_markers");
    parts.extend_from_slice(PROJECT_MARKERS);
    parts.push("denylist");
    parts.extend_from_slice(cmdr_fs::system_dirs::SYSTEM_DIR_EXCLUDES);
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
mod tests;
