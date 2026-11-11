//! Files and folders that carry every metadata class a Mac can give them, for the
//! tests that pin what a copy or move keeps (macOS only).
//!
//! The items are the fixture of `docs/notes/transfer-metadata-experiments-2026-10-11.md`
//! § "The fixture", split so each cell can take the one item whose loss or failure
//! it pins: a tagged, hidden, `0750` file with a resource fork ([`MetadataFixture::rich_file`]),
//! a Locked one, a `deny delete` one, a tagged `0700` folder with a custom-icon
//! FinderInfo and the same ACL, a Locked `0500` folder with children, and a symlink
//! with its own dates.
//!
//! Every item is born 2015-03-01, modified 2020-06-15, and accessed 2021-01-01, so
//! a copy that sets only mtime shows birth == mtime (APFS and HFS+ drag birth down to
//! an earlier mtime) and a copy that stamps "now" shows neither.
//!
//! ❗ Each item is built in the order the quirks force: content, xattrs (a resource
//! fork write bumps mtime), mode, then ONE `setattrlist` for the three dates, then the
//! ACL and the flags (`uchg` refuses a later time-set).
//!
//! [`MetadataFixture`]'s `Drop` clears `uchg`/`uappnd` and strips every ACL in its
//! tree BEFORE its [`TestDir`] removes it, so a Locked `.cmdr-tmp-*` a copy left
//! behind can't fail the cleanup.

use std::ffi::CString;
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::TestDir;

/// 2015-03-01 00:00:00 UTC: every item's birth (creation) time.
pub const BIRTH_SECS: i64 = 1_425_168_000;
/// 2020-06-15 00:00:00 UTC: every item's modification time, later than its birth.
pub const MODIFIED_SECS: i64 = 1_592_179_200;
/// 2021-01-01 00:00:00 UTC: every item's access time.
pub const ACCESSED_SECS: i64 = 1_609_459_200;

/// An app's own xattr, the kind any program may set.
pub const APP_XATTR: &str = "user.app";
/// Finder tags (a binary plist of tag names).
pub const TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";
/// A Finder comment.
pub const COMMENT_XATTR: &str = "com.apple.metadata:kMDItemFinderComment";
/// FinderInfo, where a folder's "has custom icon" bit lives.
pub const FINDER_INFO_XATTR: &str = "com.apple.FinderInfo";
/// A classic resource fork. Writing one bumps mtime on APFS.
pub const RESOURCE_FORK_XATTR: &str = "com.apple.ResourceFork";
/// Gatekeeper's quarantine stamp.
pub const QUARANTINE_XATTR: &str = "com.apple.quarantine";
/// The xattr macOS puts on every file a process writes. Never part of a comparison.
pub const PROVENANCE_XATTR: &str = "com.apple.provenance";

/// A quarantine value shaped like Safari's.
pub const QUARANTINE_VALUE: &[u8] = b"0083;5f000000;Safari;00000000-0000-0000-0000-000000000000";

/// Stands in for a tag list. A copy treats the value as opaque bytes, so the cells
/// only compare it; nothing here parses it as the binary plist Finder writes.
const TAGS_VALUE: &[u8] = b"bplist00 Red\n6";

/// FinderInfo with the `kHasCustomIcon` bit (0x0400 in the Finder flags at byte 8) set.
const FOLDER_FINDER_INFO_VALUE: [u8; 32] = {
    let mut info = [0u8; 32];
    info[8] = 0x04;
    info
};

/// `UF_HIDDEN`: Finder's Hidden.
pub const HIDDEN_FLAG: u32 = libc::UF_HIDDEN;
/// `UF_IMMUTABLE` (`uchg`): Finder's Locked.
pub const LOCKED_FLAG: u32 = libc::UF_IMMUTABLE;

/// A scratch directory that holds metadata-rich items and can always be removed.
pub struct MetadataFixture {
    dir: TestDir,
}

impl MetadataFixture {
    /// An empty fixture directory; `label` names it the way [`TestDir::new`]'s does.
    pub fn new(label: &str) -> Self {
        Self {
            dir: TestDir::new(label),
        }
    }

    /// `name` in the fixture's root: content, `user.app`, tags, a comment, quarantine, a
    /// resource fork, mode `0750`, the three dates, and Hidden. No ACL and no Locked, so
    /// it lands by every copy path today and a cell can read what each class became.
    pub fn rich_file(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, b"rich file").expect("fixture: write the rich file");
        set_xattr(&path, APP_XATTR, b"app value");
        set_xattr(&path, TAGS_XATTR, TAGS_VALUE);
        set_xattr(&path, COMMENT_XATTR, b"a Finder comment");
        set_xattr(&path, QUARANTINE_XATTR, QUARANTINE_VALUE);
        set_xattr(&path, RESOURCE_FORK_XATTR, b"resource fork bytes");
        set_mode(&path, 0o750);
        set_fixture_dates(&path);
        set_flags(&path, HIDDEN_FLAG);
        path
    }

    /// `name` in the fixture's root, dated and Locked (`uchg`).
    pub fn locked_file(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, b"locked file").expect("fixture: write the locked file");
        set_fixture_dates(&path);
        set_flags(&path, LOCKED_FLAG);
        path
    }

    /// `name` in the fixture's root, dated, with the ACL `everyone deny delete`.
    pub fn deny_delete_file(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, b"deny delete file").expect("fixture: write the ACL'd file");
        set_fixture_dates(&path);
        add_deny_delete_acl(&path);
        path
    }

    /// `name/` in the fixture's root, holding `child.txt`: tags, a comment, a custom-icon
    /// FinderInfo, mode `0700`, the three dates, `everyone deny delete`, and Hidden.
    pub fn rich_folder(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::create_dir(&path).expect("fixture: create the rich folder");
        std::fs::write(path.join("child.txt"), b"child").expect("fixture: write the folder's child");
        set_xattr(&path, TAGS_XATTR, TAGS_VALUE);
        set_xattr(&path, COMMENT_XATTR, b"a folder comment");
        set_xattr(&path, FINDER_INFO_XATTR, &FOLDER_FINDER_INFO_VALUE);
        set_mode(&path, 0o700);
        set_fixture_dates(&path);
        add_deny_delete_acl(&path);
        set_flags(&path, HIDDEN_FLAG);
        path
    }

    /// `name/` in the fixture's root, holding `a.txt` and `inner/b.txt`: tags, mode
    /// `0500` (no owner write, so nothing can be added to or removed from it), the
    /// three dates, and Locked. A copy can fill it only if both go on after its
    /// children land.
    pub fn locked_read_only_folder(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::create_dir_all(path.join("inner")).expect("fixture: create the read-only folder");
        std::fs::write(path.join("a.txt"), b"a").expect("fixture: write a.txt");
        std::fs::write(path.join("inner/b.txt"), b"b").expect("fixture: write inner/b.txt");
        set_xattr(&path, TAGS_XATTR, TAGS_VALUE);
        set_mode(&path, 0o500);
        set_fixture_dates(&path);
        set_flags(&path, LOCKED_FLAG);
        path
    }

    /// `name` in the fixture's root: a symlink to `target` wearing its OWN birth and
    /// modification dates (set with `FSOPT_NOFOLLOW`, so the target's stay as they are).
    pub fn symlink(&self, name: &str, target: &Path) -> PathBuf {
        let path = self.dir.join(name);
        crate::testing::symlink(target, &path).expect("fixture: create the symlink");
        set_times(&path, BIRTH_SECS, MODIFIED_SECS, ACCESSED_SECS, Follow::No);
        path
    }
}

impl std::ops::Deref for MetadataFixture {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.dir
    }
}

impl AsRef<Path> for MetadataFixture {
    fn as_ref(&self) -> &Path {
        &self.dir
    }
}

impl Drop for MetadataFixture {
    fn drop(&mut self) {
        // Best effort, and quiet: a failure here only means `TestDir`'s own removal
        // reports what's left, which names the culprit better than a panic in a drop.
        let _ = Command::new("chflags")
            .args(["-R", "nouchg,nouappnd"])
            .arg(&*self.dir)
            .status();
        let _ = Command::new("chmod").args(["-R", "-N"]).arg(&*self.dir).status();
        // A `0500` or `0555` folder a cell made would refuse its children's removal.
        let _ = Command::new("chmod").args(["-R", "u+rwx"]).arg(&*self.dir).status();
    }
}

/// Whether a time-set follows a symlink to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    /// Set the target's times (what a time-set on a file means).
    Yes,
    /// Set the link's own times (`FSOPT_NOFOLLOW`).
    No,
}

/// Birth, modification, and access time of `path` in ONE `setattrlist`, so neither
/// date can drag the other (APFS pulls birth down to an earlier mtime).
pub fn set_times(path: &Path, birth: i64, modified: i64, accessed: i64, follow: Follow) {
    // The attribute buffer holds the requested times in bit order: CRTIME (0x200),
    // MODTIME (0x400), ACCTIME (0x1000).
    #[repr(C)]
    struct Times {
        birth: libc::timespec,
        modified: libc::timespec,
        accessed: libc::timespec,
    }
    let at = |secs: i64| libc::timespec {
        tv_sec: secs,
        tv_nsec: 0,
    };
    let mut times = Times {
        birth: at(birth),
        modified: at(modified),
        accessed: at(accessed),
    };
    let mut attrs = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_CRTIME | libc::ATTR_CMN_MODTIME | libc::ATTR_CMN_ACCTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let options = match follow {
        Follow::Yes => 0,
        Follow::No => libc::FSOPT_NOFOLLOW,
    };
    let c_path = c_path(path);
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call, `attrs` names
    // exactly the three timespecs `times` holds, in bit order, and the size passed is
    // that buffer's.
    let rc = unsafe {
        libc::setattrlist(
            c_path.as_ptr(),
            (&raw mut attrs).cast(),
            (&raw mut times).cast(),
            size_of::<Times>(),
            options,
        )
    };
    assert_eq!(
        rc,
        0,
        "fixture: setattrlist on {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
}

fn set_fixture_dates(path: &Path) {
    set_times(path, BIRTH_SECS, MODIFIED_SECS, ACCESSED_SECS, Follow::Yes);
}

/// Sets `path`'s BSD flags to exactly `flags` (following a symlink).
pub fn set_flags(path: &Path, flags: u32) {
    let c_path = c_path(path);
    // SAFETY: `c_path` is a NUL-terminated path that outlives the call.
    let rc = unsafe { libc::chflags(c_path.as_ptr(), flags) };
    assert_eq!(
        rc,
        0,
        "fixture: chflags on {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
}

/// Adds `everyone deny delete` to `path`'s ACL, the way Finder's sharing panel and
/// `chmod +a` do.
pub fn add_deny_delete_acl(path: &Path) {
    add_acl_entry(path, "everyone deny delete");
}

/// Adds one `chmod +a` entry (like `"everyone deny add_file"`) to `path`'s ACL.
pub fn add_acl_entry(path: &Path, entry: &str) {
    let status = Command::new("chmod")
        .args(["+a", entry])
        .arg(path)
        .status()
        .expect("fixture: run chmod +a");
    assert!(status.success(), "fixture: chmod +a {entry:?} on {}", path.display());
}

/// Whether `path` itself (never a symlink's target) carries an extended ACL.
pub fn has_acl(path: &Path) -> bool {
    !exacl::getfacl(path, exacl::AclOption::SYMLINK_ACL)
        .unwrap_or_else(|e| panic!("read the ACL of {}: {e}", path.display()))
        .is_empty()
}

/// Sets the xattr `name` on `path` (following a symlink), panicking on refusal.
pub fn set_xattr(path: &Path, name: &str, value: &[u8]) {
    xattr::set(path, name, value).unwrap_or_else(|e| panic!("fixture: set {name} on {}: {e}", path.display()));
}

/// Sets `path`'s permission bits to exactly `mode` (following a symlink).
pub fn set_mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .unwrap_or_else(|e| panic!("fixture: chmod {mode:o} {}: {e}", path.display()));
}

fn c_path(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).expect("fixture: a path without NUL")
}

fn lstat(path: &Path) -> std::fs::Metadata {
    std::fs::symlink_metadata(path).unwrap_or_else(|e| panic!("lstat {}: {e}", path.display()))
}

/// `path`'s own birth time in Unix seconds (never its target's).
pub fn birth_secs(path: &Path) -> i64 {
    lstat(path).st_birthtime()
}

/// `path`'s own modification time in Unix seconds (never its target's).
pub fn modified_secs(path: &Path) -> i64 {
    lstat(path).st_mtime()
}

/// `path`'s own access time in Unix seconds (never its target's).
pub fn accessed_secs(path: &Path) -> i64 {
    lstat(path).st_atime()
}

/// `path`'s own BSD flags.
pub fn flags(path: &Path) -> u32 {
    lstat(path).st_flags()
}

/// `path`'s permission bits, setuid/setgid/sticky included.
pub fn mode(path: &Path) -> u32 {
    lstat(path).permissions().mode() & 0o7777
}

/// Every xattr on `path` with its value, sorted by name, minus
/// [`PROVENANCE_XATTR`] (macOS adds it to whatever a process writes).
pub fn xattrs(path: &Path) -> Vec<(String, Vec<u8>)> {
    let names = xattr::list(path).unwrap_or_else(|e| panic!("list xattrs of {}: {e}", path.display()));
    let mut found: Vec<(String, Vec<u8>)> = names
        .filter_map(|name| {
            let name = name.to_string_lossy().into_owned();
            if name == PROVENANCE_XATTR {
                return None;
            }
            let value = xattr::get(path, &name)
                .unwrap_or_else(|e| panic!("read {name} of {}: {e}", path.display()))
                .unwrap_or_default();
            Some((name, value))
        })
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each item wears what it promises, so a cell reading a loss reads a real loss.
    #[test]
    fn every_item_carries_the_metadata_it_promises() {
        let fixture = MetadataFixture::new("metadata_fixture_self_test");

        let rich = fixture.rich_file("f.txt");
        assert_eq!(birth_secs(&rich), BIRTH_SECS);
        assert_eq!(modified_secs(&rich), MODIFIED_SECS);
        assert_eq!(accessed_secs(&rich), ACCESSED_SECS);
        assert_eq!(mode(&rich), 0o750);
        assert_eq!(flags(&rich) & HIDDEN_FLAG, HIDDEN_FLAG);
        let names: Vec<String> = xattrs(&rich).into_iter().map(|(name, _)| name).collect();
        for expected in [
            APP_XATTR,
            TAGS_XATTR,
            COMMENT_XATTR,
            QUARANTINE_XATTR,
            RESOURCE_FORK_XATTR,
        ] {
            assert!(names.iter().any(|n| n == expected), "{expected} missing from {names:?}");
        }

        let locked = fixture.locked_file("locked.txt");
        assert_eq!(flags(&locked) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&locked), BIRTH_SECS);

        let acl = fixture.deny_delete_file("acl.txt");
        assert!(
            std::fs::remove_file(&acl).is_err(),
            "`deny delete` refuses even the owner's own unlink"
        );

        let folder = fixture.rich_folder("dir");
        assert_eq!(birth_secs(&folder), BIRTH_SECS);
        assert_eq!(modified_secs(&folder), MODIFIED_SECS);
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(flags(&folder) & HIDDEN_FLAG, HIDDEN_FLAG);
        assert_eq!(
            xattrs(&folder).len(),
            3,
            "tags, comment, FinderInfo: {:?}",
            xattrs(&folder)
        );

        let read_only = fixture.locked_read_only_folder("read-only");
        assert_eq!(mode(&read_only), 0o500);
        assert_eq!(flags(&read_only) & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(birth_secs(&read_only), BIRTH_SECS);
        assert!(read_only.join("inner/b.txt").is_file());

        let link = fixture.symlink("link", &rich);
        assert_eq!(birth_secs(&link), BIRTH_SECS);
        assert_eq!(modified_secs(&link), MODIFIED_SECS);
        // The fixture's `Drop` unlocks and strips the ACLs, so `TestDir` can remove it all.
    }
}
