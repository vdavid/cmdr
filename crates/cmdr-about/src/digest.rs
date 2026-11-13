//! What a folder holds, and what an app leaves in the Library.
//!
//! Everything here comes from the drive index, never from a walk the card
//! started, so each part carries the index's coverage. The host hands over the
//! rows it read; the rules about them (which children lead, what kind a name is,
//! what a folder's name and neighbors make it, which Library folders are an
//! app's, what share of a folder sat untouched) live here.

use std::cmp::Ordering;

use serde::Serialize;

use crate::fact::{Fact, UnknownWhy};
use crate::owner::{OwnerPlace, OwnerQuestion, belongs_to};
use crate::payload::{RecursiveSize, SizeCoverage};
use crate::sniff::{SniffedKind, claimed_kind};
use crate::text::ItemText;

/// How many children, and how many of the largest files under a folder, the
/// digest names. The host asks the search core for this many.
pub const SHOWN_CHILDREN: usize = 8;

/// How long a file sits unchanged before the digest counts it as untouched: a
/// year, in seconds. The host asks for files modified before `now` minus this.
pub const UNCHANGED_FOR_SECS: i64 = 365 * 24 * 60 * 60;

impl FolderDigest {
    /// The digest from the folder's direct children as the index lists them, and
    /// the parts the host read beside them.
    ///
    /// Keeps the [`SHOWN_CHILDREN`] biggest children (a known size before an
    /// unknown one, then by name, so the order never flickers between two
    /// reads), and counts the kinds over every child.
    pub fn of(
        children: Vec<SizedChild>,
        largest_files: Fact<Vec<SizedChild>>,
        unchanged_for_a_year: Fact<ByteShare>,
        class: FolderClass,
    ) -> Self {
        let direct_children = children.len() as u64;
        let mut counts: Vec<KindCount> = Vec::new();
        for child in &children {
            let kind = ChildKind::of_name(child.name.as_str(), child.is_folder);
            match counts.iter_mut().find(|count| count.kind == kind) {
                Some(count) => count.count += 1,
                None => counts.push(KindCount { kind, count: 1 }),
            }
        }
        counts.sort_by(|a, b| b.count.cmp(&a.count).then(a.kind.cmp(&b.kind)));
        let largest_files = match largest_files {
            Fact::Known(files) => Fact::Known(biggest(files)),
            other => other,
        };
        Self {
            direct_children,
            biggest_children: biggest(children),
            largest_files,
            kind_mix: counts,
            unchanged_for_a_year,
            class,
        }
    }
}

/// The [`SHOWN_CHILDREN`] biggest, biggest first.
fn biggest(mut children: Vec<SizedChild>) -> Vec<SizedChild> {
    children.sort_by(|a, b| by_size(&a.size, &b.size).then_with(|| a.name.as_str().cmp(b.name.as_str())));
    children.truncate(SHOWN_CHILDREN);
    children
}

/// Biggest first, a known size before one Cmdr doesn't know.
fn by_size(a: &Fact<RecursiveSize>, b: &Fact<RecursiveSize>) -> Ordering {
    match (a.known(), b.known()) {
        (Some(a), Some(b)) => b.logical.cmp(&a.logical),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// A folder's contents at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FolderDigest {
    /// How many entries sit directly in the folder. Zero is a real answer: the
    /// folder is empty.
    pub direct_children: u64,
    /// The biggest direct children, biggest first.
    pub biggest_children: Vec<SizedChild>,
    /// The largest files anywhere under the folder, largest first. Unknown when
    /// the search arena isn't loaded.
    pub largest_files: Fact<Vec<SizedChild>>,
    /// The kinds of the direct children, most common first.
    pub kind_mix: Vec<KindCount>,
    /// The share of the folder's bytes that haven't changed in a year.
    pub unchanged_for_a_year: Fact<ByteShare>,
    /// What kind of folder its name and place make it.
    pub class: FolderClass,
}

/// One child with its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SizedChild {
    /// Its name, for showing.
    pub name: ItemText,
    /// Its full path, exactly as the volume has it: for navigating there, ❌ never
    /// for showing.
    pub path: String,
    /// Whether it's a folder.
    pub is_folder: bool,
    /// Its size: its own for a file, the whole subtree for a folder.
    pub size: Fact<RecursiveSize>,
}

/// How many direct children are of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KindCount {
    /// The kind, by extension class.
    pub kind: ChildKind,
    /// How many children are of it.
    pub count: u64,
}

/// Extensions of source code and styles, which the real-type table counts as
/// plain text.
const CODE_EXTENSIONS: [&str; 26] = [
    "js", "mjs", "cjs", "ts", "tsx", "jsx", "rs", "go", "py", "rb", "c", "h", "cc", "cpp", "hpp", "swift", "java",
    "kt", "css", "scss", "html", "htm", "svelte", "vue", "php", "cs",
];

impl ChildKind {
    /// What a child is, from its name alone (no byte read: the digest runs over
    /// thousands of children). An `.app` folder is an app.
    pub(crate) fn of_name(name: &str, is_folder: bool) -> ChildKind {
        let extension = name
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map(|(_, extension)| extension.to_ascii_lowercase());
        if is_folder {
            return match extension.as_deref() {
                Some("app") => ChildKind::App,
                _ => ChildKind::Folder,
            };
        }
        let Some(extension) = extension else {
            return ChildKind::Other;
        };
        if CODE_EXTENSIONS.contains(&extension.as_str()) {
            return ChildKind::Code;
        }
        match claimed_kind(&extension) {
            Some(SniffedKind::Image) => ChildKind::Image,
            Some(SniffedKind::Video) => ChildKind::Video,
            Some(SniffedKind::Audio) => ChildKind::Audio,
            Some(SniffedKind::Pdf | SniffedKind::Document | SniffedKind::Text) => ChildKind::Document,
            Some(SniffedKind::Archive | SniffedKind::DiskImage | SniffedKind::InstallerPackage) => ChildKind::Archive,
            Some(SniffedKind::Script | SniffedKind::OtherCode) => ChildKind::Code,
            Some(SniffedKind::MachO | SniffedKind::Elf | SniffedKind::WindowsProgram) => ChildKind::App,
            Some(SniffedKind::Font | SniffedKind::Database | SniffedKind::Other | SniffedKind::Unrecognized) | None => {
                ChildKind::Other
            }
        }
    }
}

/// A direct child's kind, read from its extension alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChildKind {
    /// A folder.
    Folder,
    /// A photo or other image.
    Image,
    /// A video.
    Video,
    /// A sound file.
    Audio,
    /// A text, office, or PDF document.
    Document,
    /// An archive or a disk image.
    Archive,
    /// Source code or a script.
    Code,
    /// An app or an executable.
    App,
    /// Anything else.
    Other,
}

/// A share of a folder's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ByteShare {
    /// The bytes in the share.
    pub bytes: u64,
    /// The bytes it's a share of.
    pub of: u64,
    /// Whether the share may be bigger: Cmdr counted the largest of the files in
    /// it, not all of them. Said in the same words as the share ("at least").
    pub at_least: bool,
}

impl ByteShare {
    /// The share of the folder's bytes in files untouched for a year:
    /// `unchanged_bytes` summed over the untouched files the index answered for,
    /// `every_file_counted` when that was all of them, against the folder's own
    /// total.
    ///
    /// A folder whose total is a lower bound has no honest share: the untouched
    /// bytes may sit in the part the index never listed, and a share of an
    /// unknown total is a guess. The share never exceeds the total (the two come
    /// from the same index, read a moment apart).
    pub fn unchanged(unchanged_bytes: u64, every_file_counted: bool, folder: &Fact<RecursiveSize>) -> Fact<ByteShare> {
        match folder {
            Fact::Known(total) if total.coverage != SizeCoverage::LowerBound => Fact::Known(ByteShare {
                bytes: unchanged_bytes.min(total.logical),
                of: total.logical,
                at_least: !every_file_counted,
            }),
            Fact::Known(_) | Fact::Pending => Fact::unknown(UnknownWhy::NotIndexed),
            Fact::Unknown(unknown) => Fact::unknown(unknown.why()),
            Fact::NotApplicable(why) => Fact::NotApplicable(*why),
        }
    }
}

/// What kind of folder its name and its neighbors make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FolderClass {
    /// Nothing special.
    Plain,
    /// A cache: whatever wrote it writes it again.
    Cache,
    /// Installed dependencies (`node_modules`): a package manager fetches them
    /// again.
    Dependencies,
    /// Build output (`target`, `dist`): a build tool makes it again.
    BuildOutput,
    /// Version-control data (`.git`).
    VersionControl,
}

/// Names that make a folder what it is wherever it sits: no one names their own
/// folder `node_modules` or `__pycache__`.
const BY_NAME_ALONE: [(&str, FolderClass); 13] = [
    (".git", FolderClass::VersionControl),
    (".hg", FolderClass::VersionControl),
    (".svn", FolderClass::VersionControl),
    ("node_modules", FolderClass::Dependencies),
    ("bower_components", FolderClass::Dependencies),
    (".next", FolderClass::BuildOutput),
    (".nuxt", FolderClass::BuildOutput),
    (".svelte-kit", FolderClass::BuildOutput),
    ("DerivedData", FolderClass::BuildOutput),
    ("__pycache__", FolderClass::Cache),
    (".pytest_cache", FolderClass::Cache),
    (".mypy_cache", FolderClass::Cache),
    (".ruff_cache", FolderClass::Cache),
];

/// The project files a Python environment sits beside.
const PYTHON_PROJECT: &[&str] = &["pyproject.toml", "requirements.txt", "Pipfile", "setup.py", "uv.lock"];

/// Names a person also gives their own folders (`build`, `target`, `out`), which
/// count only beside the project file of the tool that writes them.
const BESIDE_A_MARKER: [(&str, FolderClass, &[&str]); 8] = [
    ("target", FolderClass::BuildOutput, &["Cargo.toml", "pom.xml"]),
    (
        "dist",
        FolderClass::BuildOutput,
        &["package.json", "pyproject.toml", "setup.py"],
    ),
    (
        "build",
        FolderClass::BuildOutput,
        &[
            "package.json",
            "build.gradle",
            "build.gradle.kts",
            "CMakeLists.txt",
            "setup.py",
        ],
    ),
    ("out", FolderClass::BuildOutput, &["package.json"]),
    (".build", FolderClass::BuildOutput, &["Package.swift"]),
    (".venv", FolderClass::Dependencies, PYTHON_PROJECT),
    ("venv", FolderClass::Dependencies, PYTHON_PROJECT),
    ("Pods", FolderClass::Dependencies, &["Podfile"]),
];

impl FolderClass {
    /// What a folder named `name` is, asking `beside` whether a sibling of a given
    /// name sits next to it (the host answers from the index, never the disk).
    ///
    /// ❗ A name a person might give their own folder (`build`, `target`) is build
    /// output only beside its tool's project file, because "a build tool makes it
    /// again" about the person's own `build` folder invites deleting it. Names
    /// compare ignoring ASCII case, as the Mac's disks do.
    pub fn of(name: &str, beside: &dyn Fn(&str) -> bool) -> FolderClass {
        if let Some((_, class)) = BY_NAME_ALONE.iter().find(|(known, _)| known.eq_ignore_ascii_case(name)) {
            return *class;
        }
        BESIDE_A_MARKER
            .iter()
            .find(|(known, _, markers)| known.eq_ignore_ascii_case(name) && markers.iter().any(|marker| beside(marker)))
            .map_or(FolderClass::Plain, |(_, class, _)| *class)
    }
}

/// The `~/Library` folders an app's data is looked for in, and the place each
/// is. Not `Preferences` (a settings file, not a folder of data) or `WebKit` (a
/// few web views' caches, beside `HTTPStorages`).
pub const APP_DATA_PLACES: [(&str, OwnerPlace); 7] = [
    ("Application Support", OwnerPlace::ApplicationSupport),
    ("Containers", OwnerPlace::Containers),
    ("Group Containers", OwnerPlace::GroupContainers),
    ("Caches", OwnerPlace::Caches),
    ("HTTPStorages", OwnerPlace::WebData),
    ("Saved Application State", OwnerPlace::SavedState),
    ("Logs", OwnerPlace::Logs),
];

/// Whether the Library child at `path` holds the data of the app whose id is
/// `bundle_id`, and in which place. The owner row's rule run the other way: the
/// child's NAME names the id (`Containers/<id>`, `Saved Application
/// State/<id>.savedState`, `Group Containers/<TEAM>.<id>`), and that id is the
/// app's own or one of its own (`<id>.<extension>`).
///
/// ❌ Never a folder named by name alone (`Application Support/Slack`): this row
/// counts what removing the app leaves behind, and a name match is a guess. A
/// group container named after a team's shared id (`<TEAM>.com.vendor`) isn't
/// counted for one app of that vendor either: other apps may live off it.
pub fn app_data_place(path: &str, home: &str, bundle_id: &str) -> Option<OwnerPlace> {
    match OwnerQuestion::of(path, home) {
        OwnerQuestion::BundleId { id, place }
            if APP_DATA_PLACES.iter().any(|(_, known)| *known == place) && belongs_to(&id, bundle_id) =>
        {
            Some(place)
        }
        _ => None,
    }
}

/// The Library folders named after an app's bundle id, which removing the app
/// leaves behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    /// Each folder found, biggest first. Empty is a real answer for the places
    /// the index answered: it holds none there.
    pub folders: Vec<LibraryFolder>,
    /// The places the index couldn't answer for (it hasn't listed them), so a
    /// folder there may be missing. Empty: every place was looked in.
    pub not_looked_in: Vec<OwnerPlace>,
    /// The folders' bytes together.
    pub total_bytes: u64,
    /// Whether the real total may be bigger: a place went unlooked, or a
    /// folder's size is a lower bound or unknown.
    pub total_is_lower_bound: bool,
}

impl AppData {
    /// The app's data from the folders the host found and the places it couldn't
    /// look in. With no place looked in at all, Cmdr can't tell:
    /// `Unknown(NotIndexed)`, ❌ never "no data".
    pub fn of(folders: Vec<LibraryFolder>, not_looked_in: Vec<OwnerPlace>) -> Fact<AppData> {
        if not_looked_in.len() >= APP_DATA_PLACES.len() {
            return Fact::unknown(UnknownWhy::NotIndexed);
        }
        let mut folders = folders;
        folders.sort_by(|a, b| by_size(&a.size, &b.size).then_with(|| a.path.cmp(&b.path)));
        let total_bytes = folders
            .iter()
            .filter_map(|folder| folder.size.known())
            .map(|size| size.logical)
            .sum();
        let total_is_lower_bound = !not_looked_in.is_empty()
            || folders.iter().any(|folder| {
                folder
                    .size
                    .known()
                    .is_none_or(|size| size.coverage == SizeCoverage::LowerBound)
            });
        Fact::Known(AppData {
            folders,
            not_looked_in,
            total_bytes,
            total_is_lower_bound,
        })
    }
}

/// One of an app's folders in the Library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFolder {
    /// Its name, for showing.
    pub name: ItemText,
    /// Its full path, exactly as the volume has it: for navigating there, ❌ never
    /// for showing.
    pub path: String,
    /// Which app-data folder it sits in.
    pub place: OwnerPlace,
    /// Its whole size.
    pub size: Fact<RecursiveSize>,
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
