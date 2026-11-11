//! What the local lister yields today, field for field, and that every listed
//! entry equals its own single-path re-stat.
//!
//! Two kinds of cell, on purpose. The parity cell compares a listed entry to
//! `get_single_entry` over every field (through `Debug`, so a new `FileEntry` field
//! joins automatically), which catches the two readers drifting apart but not both
//! going wrong together. So every field a write path acts on (`name`, `is_symlink`,
//! `is_directory`, `inode`, `size`) also gets a cell checking it against what
//! `lstat` itself says, and so do the display fields.
//!
//! Every cell runs once per [`Lister`], so a second lister is proven by adding it
//! to [`LISTERS`], with no cell edited.

use cmdr_fs::testing::symlink;
use std::collections::HashSet;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use cmdr_fs::entry::{FileEntry, get_group_name, get_owner_name};
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::host::VolumeHost;

use crate::reading::{Lister, get_single_entry, list_directory_with};

// ============================================================================
// The lister seam
// ============================================================================

/// Every lister a cell runs through. Linux has no `getattrlistbulk`.
#[cfg(target_os = "macos")]
const LISTERS: &[Lister] = &[Lister::Bulk, Lister::ReadDir];
#[cfg(not(target_os = "macos"))]
const LISTERS: &[Lister] = &[Lister::ReadDir];

/// Lists `dir` through `lister` the way `LocalPosixVolume::list_directory` does,
/// `is_volume_root` deciding whether `/.hidden` is read.
fn list(lister: Lister, dir: &Path, is_volume_root: bool) -> Vec<FileEntry> {
    list_directory_with(dir, is_volume_root, VolumeHost::detached().listings(), lister)
        .unwrap_or_else(|e| panic!("{lister:?} lists {}: {e}", dir.display()))
}

fn entry_named<'a>(entries: &'a [FileEntry], name: &str) -> &'a FileEntry {
    let mut matching = entries.iter().filter(|entry| entry.name == name);
    let entry = matching.next().unwrap_or_else(|| panic!("{name} is listed"));
    assert!(matching.next().is_none(), "{name} is listed once");
    entry
}

/// Names `/.hidden` lists in `dir`, read here rather than through the reader, so
/// the expectation doesn't share the code under test.
fn dot_hidden_names(dir: &Path) -> HashSet<String> {
    fs::read_to_string(dir.join(".hidden"))
        .map(|contents| {
            contents
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Lists `dir` through `lister` and asserts the listing has exactly one entry per
/// name `read_dir` sees (nothing dropped, nothing invented), each equal in every
/// field to `get_single_entry` of its raw path.
///
/// The one known difference: at a volume root, a name `/.hidden` lists is hidden
/// in the listing and not in the re-stat, which has no idea it's at a root.
fn assert_listing_matches_restat(lister: Lister, dir: &Path, is_volume_root: bool) {
    let listed = list(lister, dir, is_volume_root);
    let dot_hidden = if is_volume_root {
        dot_hidden_names(dir)
    } else {
        HashSet::new()
    };
    let raw_paths: Vec<PathBuf> = fs::read_dir(dir)
        .expect("read the dir")
        .map(|entry| entry.expect("a dir entry").path())
        .collect();
    assert_eq!(
        listed.len(),
        raw_paths.len(),
        "{lister:?} lists one entry per name in {}",
        dir.display()
    );
    for raw_path in raw_paths {
        let name = raw_path.file_name().expect("a name").to_string_lossy().into_owned();
        let entry = entry_named(&listed, &name);
        let mut restat = get_single_entry(&raw_path).unwrap_or_else(|e| panic!("re-stat {}: {e}", raw_path.display()));
        if dot_hidden.contains(&name) {
            assert!(entry.is_hidden, "{name} is in /.hidden, so the root listing hides it");
            assert!(
                !restat.is_hidden || name.starts_with('.'),
                "the re-stat doesn't apply /.hidden to {name}"
            );
            restat.is_hidden = true;
        }
        assert_eq!(
            format!("{entry:#?}"),
            format!("{restat:#?}"),
            "{lister:?}: listing and re-stat agree on {name}"
        );
    }
}

// ============================================================================
// The fixture
// ============================================================================

const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFMT: u32 = 0o170_000;

const FILE_MTIME: i64 = 1_600_000_000;
const FOLDER_MTIME: i64 = 1_500_000_000;
const LINK_MTIME: i64 = 1_000_000_000;
/// 1960-01-01, before the epoch.
const PRE_EPOCH_MTIME: i64 = -315_619_200;

/// One of everything the lister has a rule for, built under `root`.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Every name the fixture creates.
    fn names() -> Vec<&'static str> {
        let mut names = vec![
            "plain.txt",
            "empty",
            "folder",
            "hard_a.txt",
            "hard_b.txt",
            "link_to_folder",
            "link_to_file",
            "broken_link",
            ".dotfile",
            "sparse.bin",
            "old.txt",
            "Bundle.app",
            "link_to_bundle.app",
            "archive.zip",
            "folder.zip",
        ];
        if cfg!(target_os = "macos") {
            names.extend(["flag_hidden.txt", "compressed.txt"]);
        }
        names
    }

    fn build(root: &Path) -> Self {
        fs::write(root.join("plain.txt"), b"hello").expect("plain.txt");
        fs::set_permissions(root.join("plain.txt"), fs::Permissions::from_mode(0o640)).expect("chmod plain.txt");
        fs::write(root.join("empty"), b"").expect("empty");
        fs::create_dir(root.join("folder")).expect("folder");
        fs::set_permissions(root.join("folder"), fs::Permissions::from_mode(0o750)).expect("chmod folder");
        fs::write(root.join("hard_a.txt"), b"shared").expect("hard_a.txt");
        fs::hard_link(root.join("hard_a.txt"), root.join("hard_b.txt")).expect("hard_b.txt");
        symlink("folder", root.join("link_to_folder")).expect("link_to_folder");
        symlink("plain.txt", root.join("link_to_file")).expect("link_to_file");
        symlink("nowhere", root.join("broken_link")).expect("broken_link");
        fs::write(root.join(".dotfile"), b"x").expect(".dotfile");
        let sparse = fs::File::create(root.join("sparse.bin")).expect("sparse.bin");
        sparse.set_len(8 * 1024 * 1024).expect("extend sparse.bin");
        drop(sparse);
        fs::write(root.join("old.txt"), b"old").expect("old.txt");
        fs::create_dir(root.join("Bundle.app")).expect("Bundle.app");
        symlink("Bundle.app", root.join("link_to_bundle.app")).expect("link_to_bundle.app");
        fs::write(root.join("archive.zip"), b"PK").expect("archive.zip");
        fs::create_dir(root.join("folder.zip")).expect("folder.zip");
        #[cfg(target_os = "macos")]
        {
            fs::write(root.join("flag_hidden.txt"), b"x").expect("flag_hidden.txt");
            chflags(&root.join("flag_hidden.txt"), libc::UF_HIDDEN);
            write_compressed(&root.join("compressed.txt"));
        }

        // Times last: creating an entry inside a folder would move its mtime.
        set_mtime(&root.join("plain.txt"), FILE_MTIME, true);
        set_mtime(&root.join("folder"), FOLDER_MTIME, true);
        set_mtime(&root.join("old.txt"), PRE_EPOCH_MTIME, true);
        for link in ["link_to_folder", "link_to_file", "broken_link"] {
            set_mtime(&root.join(link), LINK_MTIME, false);
        }
        Self {
            root: root.to_path_buf(),
        }
    }

    fn lstat(&self, name: &str) -> fs::Metadata {
        fs::symlink_metadata(self.root.join(name)).unwrap_or_else(|e| panic!("lstat {name}: {e}"))
    }

    fn list(&self, lister: Lister) -> Vec<FileEntry> {
        list(lister, &self.root, false)
    }
}

/// Sets `path`'s mtime (atime untouched), on the link itself when `follow` is false.
fn set_mtime(path: &Path, secs: i64, follow: bool) {
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("no NUL in a test path");
    let times = [
        libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
        libc::timespec {
            tv_sec: secs,
            tv_nsec: 0,
        },
    ];
    let flags = if follow { 0 } else { libc::AT_SYMLINK_NOFOLLOW };
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `times` is the
    // two-element array `utimensat` reads and nothing else.
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), flags) };
    assert_eq!(
        rc,
        0,
        "utimensat {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
}

#[cfg(target_os = "macos")]
fn chflags(path: &Path, flags: u32) {
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("no NUL in a test path");
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `chflags` only
    // writes the file's on-disk flags.
    let rc = unsafe { libc::chflags(c_path.as_ptr(), flags) };
    assert_eq!(rc, 0, "chflags {}: {}", path.display(), std::io::Error::last_os_error());
}

/// Writes a compressible file to `dest` through `ditto --hfsCompression`, which
/// stores it decmpfs-compressed (`UF_COMPRESSED`, few blocks for many bytes).
/// `--noclone` because on APFS ditto otherwise clones the source and compresses
/// nothing (verified on macOS 27.0.1, `ls -lO`, 2026-10-22).
#[cfg(target_os = "macos")]
fn write_compressed(dest: &Path) {
    let scratch = TestDir::new("compress_source");
    let source = scratch.join("source.txt");
    fs::write(&source, "all work and no play makes a dull listing\n".repeat(4096)).expect("the source");
    let status = std::process::Command::new("/usr/bin/ditto")
        .arg("--noclone")
        .arg("--hfsCompression")
        .arg(&source)
        .arg(dest)
        .status()
        .expect("run ditto");
    assert!(status.success(), "ditto --hfsCompression {}", dest.display());
}

fn secs_since_epoch(time: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    time.ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

// ============================================================================
// The categories, each against `lstat`
// ============================================================================

/// The names: exactly the fixture's, nothing dropped or invented.
fn check_names(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    let mut names: Vec<&str> = listed.iter().map(|entry| entry.name.as_str()).collect();
    names.sort_unstable();
    let mut expected = Fixture::names();
    expected.sort_unstable();
    assert_eq!(names, expected, "{lister:?} lists exactly the fixture");
    for entry in &listed {
        assert_eq!(
            Path::new(&entry.path),
            fx.root.join(&entry.name),
            "{lister:?}: {}'s path",
            entry.name
        );
    }
}

/// File, folder, and empty file: kind, sizes, icon.
fn check_kinds_and_sizes(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);

    let plain = entry_named(&listed, "plain.txt");
    assert!(
        !plain.is_directory && !plain.is_symlink,
        "{lister:?}: plain.txt is a file"
    );
    assert_eq!(plain.size, Some(5), "{lister:?}: plain.txt's size");
    assert_eq!(
        plain.physical_size,
        Some(fx.lstat("plain.txt").blocks() * 512),
        "{lister:?}: plain.txt's physical size is st_blocks * 512"
    );
    assert_eq!(plain.icon_id, "ext:txt", "{lister:?}: plain.txt's icon");

    let empty = entry_named(&listed, "empty");
    assert_eq!(empty.size, Some(0), "{lister:?}: an empty file has size 0, not None");
    assert_eq!(empty.icon_id, "file", "{lister:?}: an extensionless file's icon");

    let folder = entry_named(&listed, "folder");
    assert!(
        folder.is_directory && !folder.is_symlink,
        "{lister:?}: folder is a folder"
    );
    assert_eq!(folder.size, None, "{lister:?}: a folder has no size");
    assert_eq!(folder.physical_size, None, "{lister:?}: a folder has no physical size");
    assert_eq!(folder.icon_id, "dir", "{lister:?}: folder's icon");
}

/// `inode` only for a file with more than one link.
fn check_inode(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    let ino = fx.lstat("hard_a.txt").ino();
    assert_eq!(
        entry_named(&listed, "hard_a.txt").inode,
        Some(ino),
        "{lister:?}: hard_a.txt's inode"
    );
    assert_eq!(
        entry_named(&listed, "hard_b.txt").inode,
        Some(ino),
        "{lister:?}: hard_b.txt shares it"
    );
    assert_eq!(
        entry_named(&listed, "plain.txt").inode,
        None,
        "{lister:?}: a single-link file has no inode"
    );
    assert_eq!(
        entry_named(&listed, "folder").inode,
        None,
        "{lister:?}: a folder has no inode"
    );
}

/// Links take `is_directory` from their target and everything else from the link
/// itself: its mtime, its mode, no size.
fn check_symlinks(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);

    let to_folder = entry_named(&listed, "link_to_folder");
    assert!(to_folder.is_symlink, "{lister:?}: link_to_folder is a link");
    assert!(
        to_folder.is_directory,
        "{lister:?}: link_to_folder is a folder, from its target"
    );
    assert_eq!(to_folder.icon_id, "symlink-dir", "{lister:?}: link_to_folder's icon");

    let to_file = entry_named(&listed, "link_to_file");
    assert!(
        to_file.is_symlink && !to_file.is_directory,
        "{lister:?}: link_to_file is a link to a file"
    );
    assert_eq!(to_file.icon_id, "symlink-file", "{lister:?}: link_to_file's icon");

    // A broken link is a whole link entry, not the minimal row: its `lstat` works.
    let broken = entry_named(&listed, "broken_link");
    assert!(
        broken.is_symlink && !broken.is_directory,
        "{lister:?}: broken_link is a link to nothing"
    );
    assert_eq!(broken.icon_id, "symlink-file", "{lister:?}: broken_link's icon");

    for link in [to_folder, to_file, broken] {
        let name = &link.name;
        let meta = fx.lstat(name);
        assert_eq!(
            link.size, None,
            "{lister:?}: {name} has no size, even when its target does"
        );
        assert_eq!(link.physical_size, None, "{lister:?}: {name} has no physical size");
        assert_eq!(
            link.modified_at,
            Some(LINK_MTIME as u64),
            "{lister:?}: {name}'s mtime is the link's own"
        );
        assert_eq!(
            link.permissions & S_IFMT,
            S_IFLNK,
            "{lister:?}: {name}'s mode is the link's"
        );
        assert_eq!(link.permissions, meta.mode(), "{lister:?}: {name}'s mode bits");
        assert_eq!(link.owner, get_owner_name(meta.uid()), "{lister:?}: {name}'s owner");
        assert!(link.inode.is_none(), "{lister:?}: {name} has no inode");
    }
}

/// Dot names and (macOS) `UF_HIDDEN` are hidden; an ordinary name isn't.
fn check_hidden(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    assert!(
        entry_named(&listed, ".dotfile").is_hidden,
        "{lister:?}: a dot name is hidden"
    );
    assert!(
        !entry_named(&listed, "plain.txt").is_hidden,
        "{lister:?}: an ordinary name isn't"
    );
    #[cfg(target_os = "macos")]
    assert!(
        entry_named(&listed, "flag_hidden.txt").is_hidden,
        "{lister:?}: UF_HIDDEN hides a name with no dot"
    );
}

/// `physical_size` is `st_blocks * 512`, which a sparse or compressed file puts
/// below its length.
fn check_physical_size(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    let mut names = vec!["sparse.bin"];
    if cfg!(target_os = "macos") {
        names.push("compressed.txt");
    }
    for name in names {
        let meta = fx.lstat(name);
        let entry = entry_named(&listed, name);
        assert_eq!(entry.size, Some(meta.len()), "{lister:?}: {name}'s size is its length");
        assert_eq!(
            entry.physical_size,
            Some(meta.blocks() * 512),
            "{lister:?}: {name}'s physical size is st_blocks * 512"
        );
    }
}

/// Dates in Unix seconds; one before 1970 reads `None`.
fn check_dates(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    let plain = entry_named(&listed, "plain.txt");
    assert_eq!(
        plain.modified_at,
        Some(FILE_MTIME as u64),
        "{lister:?}: plain.txt's mtime"
    );
    assert_eq!(
        plain.created_at,
        secs_since_epoch(fx.lstat("plain.txt").created()),
        "{lister:?}: plain.txt's birth time"
    );
    #[cfg(target_os = "macos")]
    assert!(plain.created_at.is_some(), "{lister:?}: macOS always has a birth time");
    assert_eq!(
        entry_named(&listed, "folder").modified_at,
        Some(FOLDER_MTIME as u64),
        "{lister:?}: folder's mtime"
    );

    let old = entry_named(&listed, "old.txt");
    assert_eq!(old.modified_at, None, "{lister:?}: a pre-1970 mtime reads None");
    assert_eq!(
        old.created_at,
        secs_since_epoch(fx.lstat("old.txt").created()),
        "{lister:?}: old.txt's birth time"
    );
    for entry in &listed {
        check_added_at_is_fresh(entry, lister);
    }
}

/// The added date of an entry the fixture just made: on macOS (APFS or HFS+, both
/// record it) a moment ago, whichever lister ran; on Linux, none. "A moment" is
/// generous: an hour, so a slow CI box can't trip it, which still catches a zero,
/// a misread slot, or a pre-epoch value.
///
/// HFS+ records none for a symlink (verified on macOS 27.0.1, this cell on a
/// `testing::disk_images` HFS+ image, 2026-10-22), so a link may have none;
/// whether the two listers agree on it is the re-stat parity cell's job.
fn check_added_at_is_fresh(entry: &FileEntry, lister: Lister) {
    let name = &entry.name;
    if cfg!(target_os = "macos") {
        let now = secs_since_epoch(Ok(std::time::SystemTime::now())).expect("after 1970");
        let Some(added) = entry.added_at else {
            assert!(entry.is_symlink, "{lister:?}: an added date for {name}");
            return;
        };
        assert!(
            now.saturating_sub(3600) <= added && added <= now + 5,
            "{lister:?}: {name} was added a moment ago ({added}, now {now})"
        );
    } else {
        assert_eq!(entry.added_at, None, "{lister:?}: {name} has no added date on Linux");
    }
}

/// `permissions` is the whole `st_mode`, file-type bits included.
fn check_permissions(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    assert_eq!(
        entry_named(&listed, "plain.txt").permissions,
        S_IFREG | 0o640,
        "{lister:?}: plain.txt's mode"
    );
    assert_eq!(
        entry_named(&listed, "folder").permissions,
        S_IFDIR | 0o750,
        "{lister:?}: folder's mode"
    );
}

/// Owner and group are names resolved from the entry's own uid and gid.
fn check_owner_and_group(fx: &Fixture, lister: Lister) {
    for entry in fx.list(lister) {
        let meta = fx.lstat(&entry.name);
        assert_eq!(
            entry.owner,
            get_owner_name(meta.uid()),
            "{lister:?}: {}'s owner",
            entry.name
        );
        assert_eq!(
            entry.group,
            get_group_name(meta.gid()),
            "{lister:?}: {}'s group",
            entry.name
        );
    }
}

/// `is_package` for a bundle-named folder, `is_archive` for a zip-named file only.
fn check_package_and_archive(fx: &Fixture, lister: Lister) {
    let listed = fx.list(lister);
    let bundle = entry_named(&listed, "Bundle.app");
    assert!(bundle.is_package, "{lister:?}: Bundle.app is a package");
    assert!(
        bundle.icon_id.starts_with("pkg:"),
        "{lister:?}: Bundle.app's icon is {}",
        bundle.icon_id
    );
    assert!(
        !entry_named(&listed, "folder").is_package,
        "{lister:?}: folder isn't a package"
    );
    let bundle_link = entry_named(&listed, "link_to_bundle.app");
    assert!(
        bundle_link.is_symlink && bundle_link.is_directory && !bundle_link.is_package,
        "{lister:?}: a link to a bundle is a folder link, never a package"
    );
    assert!(
        entry_named(&listed, "archive.zip").is_archive,
        "{lister:?}: archive.zip is an archive"
    );
    assert!(
        !entry_named(&listed, "folder.zip").is_archive,
        "{lister:?}: a folder named .zip isn't an archive"
    );
    assert!(
        !entry_named(&listed, "plain.txt").is_archive,
        "{lister:?}: plain.txt isn't an archive"
    );
}

/// Every category, for a cell that builds the fixture somewhere else (a real image).
#[cfg(target_os = "macos")]
fn check_every_category(fx: &Fixture, lister: Lister) {
    check_names(fx, lister);
    check_kinds_and_sizes(fx, lister);
    check_inode(fx, lister);
    check_symlinks(fx, lister);
    check_hidden(fx, lister);
    check_physical_size(fx, lister);
    check_dates(fx, lister);
    check_permissions(fx, lister);
    check_owner_and_group(fx, lister);
    check_package_and_archive(fx, lister);
    assert_listing_matches_restat(lister, &fx.root, false);
}

// ============================================================================
// The cells
// ============================================================================

/// Runs `check` over a fresh fixture for every lister.
fn on_fixture(label: &str, check: fn(&Fixture, Lister)) {
    let dir = TestDir::new(label);
    let fx = Fixture::build(&dir);
    for &lister in LISTERS {
        check(&fx, lister);
    }
}

#[test]
fn every_listed_entry_equals_its_single_entry_restat() {
    on_fixture("parity_restat", |fx, lister| {
        assert_listing_matches_restat(lister, &fx.root, false);
    });
}

#[test]
fn the_listing_has_exactly_the_fixture_names() {
    on_fixture("parity_names", check_names);
}

#[test]
fn files_folders_and_empty_files_carry_their_kind_size_and_icon() {
    on_fixture("parity_kinds", check_kinds_and_sizes);
}

#[test]
fn only_a_hardlinked_file_carries_its_inode() {
    on_fixture("parity_inode", check_inode);
}

#[test]
fn a_link_takes_its_folderness_from_the_target_and_the_rest_from_itself() {
    on_fixture("parity_symlinks", check_symlinks);
}

#[test]
fn dot_names_and_the_hidden_flag_hide_an_entry() {
    on_fixture("parity_hidden", check_hidden);
}

#[test]
fn physical_size_is_the_allocated_blocks() {
    on_fixture("parity_physical", |fx, lister| {
        let sparse = fx.lstat("sparse.bin");
        assert!(
            sparse.blocks() * 512 < sparse.len(),
            "precondition: sparse.bin is sparse ({} blocks for {} bytes)",
            sparse.blocks(),
            sparse.len()
        );
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::MetadataExt as _;
            let compressed = fx.lstat("compressed.txt");
            assert_ne!(
                compressed.st_flags() & libc::UF_COMPRESSED,
                0,
                "precondition: compressed.txt is compressed"
            );
            assert!(
                compressed.blocks() * 512 < compressed.len(),
                "precondition: compressed.txt takes fewer bytes than it holds"
            );
        }
        check_physical_size(fx, lister);
    });
}

#[test]
fn dates_are_unix_seconds_and_a_pre_epoch_one_is_none() {
    on_fixture("parity_dates", check_dates);
}

#[test]
fn permissions_carry_the_file_type_bits() {
    on_fixture("parity_permissions", check_permissions);
}

#[test]
fn owner_and_group_are_resolved_names() {
    on_fixture("parity_owner", check_owner_and_group);
}

#[test]
fn packages_and_archives_are_named_by_kind_and_suffix() {
    on_fixture("parity_package", check_package_and_archive);
}

/// `/.hidden` hides a name only in a listing of the volume root; the single-path
/// re-stat never applies it (it has no notion of a root), the one field where the
/// two readers knowingly differ.
#[test]
fn dot_hidden_applies_only_to_a_volume_root_listing() {
    let dir = TestDir::new("parity_dot_hidden");
    fs::write(dir.join("listed_in_dot_hidden.txt"), b"x").expect("the hidden-by-list file");
    fs::write(dir.join("ordinary.txt"), b"x").expect("an ordinary file");
    fs::write(dir.join(".hidden"), "listed_in_dot_hidden.txt\n").expect("/.hidden");
    let hidden_path = dir.join("listed_in_dot_hidden.txt");

    for &lister in LISTERS {
        let at_root = list(lister, &dir, true);
        assert!(
            entry_named(&at_root, "listed_in_dot_hidden.txt").is_hidden,
            "{lister:?}: a root listing hides a /.hidden name"
        );
        assert!(
            !entry_named(&at_root, "ordinary.txt").is_hidden,
            "{lister:?}: a root listing leaves other names alone"
        );
        let below_root = list(lister, &dir, false);
        assert!(
            !entry_named(&below_root, "listed_in_dot_hidden.txt").is_hidden,
            "{lister:?}: /.hidden means nothing below a root"
        );
        assert_listing_matches_restat(lister, &dir, true);
        assert_listing_matches_restat(lister, &dir, false);
    }
    assert!(
        !get_single_entry(&hidden_path).expect("re-stat").is_hidden,
        "the re-stat never applies /.hidden"
    );
}

/// The added date is when an entry landed in its folder: a move into another
/// folder renews it (and leaves the mtime alone), and the re-stat the watcher
/// takes of the moved entry carries the same date the listing does.
#[cfg(target_os = "macos")]
#[test]
fn a_move_into_another_folder_renews_the_added_date() {
    let dir = TestDir::new("parity_added_move");
    let (from, to) = (dir.join("from"), dir.join("to"));
    fs::create_dir(&from).expect("from");
    fs::create_dir(&to).expect("to");
    fs::write(from.join("moved.txt"), b"x").expect("moved.txt");
    fs::create_dir(from.join("moved_folder")).expect("moved_folder");
    let names = ["moved.txt", "moved_folder"];

    let before: Vec<(Lister, Vec<FileEntry>)> = LISTERS.iter().map(|&l| (l, list(l, &from, false))).collect();
    // allowed-test-sleep: added dates are whole seconds, so the move must land in a later one.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    for name in names {
        fs::rename(from.join(name), to.join(name)).expect("move it");
    }

    for (lister, before) in before {
        let after = list(lister, &to, false);
        for name in names {
            let old = entry_named(&before, name);
            let new = entry_named(&after, name);
            let (old_added, new_added) = (
                old.added_at.expect("an added date before"),
                new.added_at.expect("an added date after"),
            );
            assert!(
                new_added > old_added,
                "{lister:?}: a move renews the added date of {name} ({old_added} -> {new_added})"
            );
            if name == "moved.txt" {
                assert_eq!(new.modified_at, old.modified_at, "{lister:?}: a move leaves the mtime");
            }
            assert_eq!(
                get_single_entry(&to.join(name)).expect("re-stat").added_at,
                Some(new_added),
                "{lister:?}: the re-stat carries the listing's added date for {name}"
            );
        }
    }
}

/// APFS refuses a non-UTF-8 name (`EILSEQ`), so this runs on Linux only. The name
/// is lossy, and the path is the lossy spelling too.
#[cfg(target_os = "linux")]
#[test]
fn a_non_utf8_name_lists_lossily() {
    let dir = TestDir::new("parity_non_utf8");
    let raw = std::ffi::OsStr::from_bytes(b"caf\xe9.txt");
    fs::write(dir.join(raw), b"x").expect("a non-UTF-8 name");

    for &lister in LISTERS {
        let listed = list(lister, &dir, false);
        let entry = entry_named(&listed, "caf\u{FFFD}.txt");
        assert_eq!(
            entry.size,
            Some(1),
            "{lister:?}: the entry is stat'ed through its raw name"
        );
        assert_eq!(
            entry.path,
            dir.join(raw).to_string_lossy(),
            "{lister:?}: the path is the lossy spelling"
        );
        assert_listing_matches_restat(lister, &dir, false);
    }
}

/// Puts a folder back to a mode its `TestDir` can delete, even when the test
/// fails first. Declared after the `TestDir`, so it drops before it.
struct RestoreMode<'a>(&'a Path);

impl Drop for RestoreMode<'_> {
    fn drop(&mut self) {
        let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
    }
}

/// A readable but unsearchable folder: `readdir` names every child, but no child
/// can be `lstat`ed, so each lists as the minimal row (no size, no dates, no
/// owner, `file` or `symlink-broken` by `d_type`), and a re-stat of one fails.
#[test]
fn an_unsearchable_folder_lists_every_name_as_the_minimal_row() {
    // Root searches a 0444 folder anyway (the Linux Docker lane runs as root).
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = TestDir::new("parity_unsearchable");
    let locked = dir.join("locked");
    fs::create_dir(&locked).expect("the folder");
    fs::write(locked.join("child.txt"), b"x").expect("a file");
    fs::create_dir(locked.join("subfolder")).expect("a folder");
    symlink("child.txt", locked.join("child_link")).expect("a link");
    fs::write(locked.join(".dot_child"), b"x").expect("a dot file");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o444)).expect("chmod 0444");
    let _restore = RestoreMode(&locked);

    for &lister in LISTERS {
        let listed = list(lister, &locked, false);
        let mut names: Vec<&str> = listed.iter().map(|entry| entry.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [".dot_child", "child.txt", "child_link", "subfolder"],
            "{lister:?}: every name"
        );
        for entry in &listed {
            let name = &entry.name;
            let is_link = name == "child_link";
            let expected = FileEntry {
                icon_id: if is_link { "symlink-broken" } else { "file" }.to_string(),
                ..FileEntry::new(
                    name.clone(),
                    locked.join(name).to_string_lossy().into_owned(),
                    false,
                    is_link,
                )
            };
            assert_eq!(
                format!("{entry:#?}"),
                format!("{expected:#?}"),
                "{lister:?}: {name} is the minimal row"
            );
        }
        assert!(
            entry_named(&listed, ".dot_child").is_hidden,
            "{lister:?}: the minimal row still hides a dot name"
        );
    }
    assert!(
        get_single_entry(&locked.join("child.txt")).is_err(),
        "a child of an unsearchable folder can't be re-stat'ed"
    );
}

/// `/`'s entries include firmlinks (`Users`, `Applications`, `Library`, …) and mount
/// points (`/dev`, `/System/Volumes/Data` underneath), each the same listed as
/// re-stat'ed. `LocalPosixVolume` lists `/` as its root, so `/.hidden` applies.
#[cfg(target_os = "macos")]
#[test]
fn the_root_folder_equals_its_restats() {
    for &lister in LISTERS {
        let listed = list(lister, Path::new("/"), true);
        for name in ["Users", "Applications", "Library", "System"] {
            assert!(
                entry_named(&listed, name).is_directory,
                "{lister:?}: /{name} is a folder"
            );
        }
        assert_listing_matches_restat(lister, Path::new("/"), true);
    }
}

/// Real APFS and HFS+ images: a listing of a mount point's parent, and the whole
/// fixture on HFS+. `#[ignore]`d because they attach images; they run in
/// `pnpm check disk-images`, serialized in the `disk-image` nextest group.
#[cfg(target_os = "macos")]
mod real_image {
    use super::*;
    use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};

    /// A folder holding a mount point lists the mounted volume's root for it, as
    /// its re-stat does.
    #[test]
    #[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
    fn a_mount_points_parent_equals_its_restats() {
        let dir = TestDir::new("parity_mount_parent");
        let parent = dir.join("parent");
        fs::create_dir_all(parent.join("mnt")).expect("the mount point");
        fs::write(parent.join("beside.txt"), b"x").expect("a file beside the mount");

        let session = DiskImageSession::acquire();
        let mut image = DiskImage::attach(&session, ImageSpec::Apfs).expect("attach the image");
        let mount_point = image
            .remount_volume_at(0, &parent.join("mnt"))
            .expect("mount it inside the folder")
            .mount_point
            .clone();
        fs::write(mount_point.join("on_the_image.txt"), b"x").expect("a file on the image");

        for &lister in LISTERS {
            let listed = list(lister, &parent, false);
            assert!(
                entry_named(&listed, "mnt").is_directory,
                "{lister:?}: the mount point is a folder"
            );
            assert_listing_matches_restat(lister, &parent, false);
        }
        drop(image);
    }

    /// Every category on a real HFS+ volume, which isn't APFS underneath: no
    /// sparse files, its own birth-time and compression storage.
    #[test]
    #[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
    fn every_category_holds_on_hfs_plus() {
        let session = DiskImageSession::acquire();
        let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
        let root = image.volumes()[0].mount_point.join("fixture");
        fs::create_dir(&root).expect("the fixture folder");
        let fx = Fixture::build(&root);
        for &lister in LISTERS {
            check_every_category(&fx, lister);
        }
        drop(image);
    }
}
