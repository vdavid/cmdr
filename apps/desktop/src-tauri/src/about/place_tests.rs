use std::cell::RefCell;
use std::path::PathBuf;

use super::*;
use crate::test_support::TestDir;

/// A local disk at `/`, a folder the mount table calls an NFS share, and every path
/// the walk touched.
struct World {
    _dir: TestDir,
    root: PathBuf,
    share: PathBuf,
    mounts: Mounts,
    touched: RefCell<Vec<PathBuf>>,
}

impl LocalFs for World {
    fn is_symlink(&self, path: &Path) -> std::io::Result<bool> {
        self.touched.borrow_mut().push(path.to_path_buf());
        Os.is_symlink(path)
    }

    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
        self.touched.borrow_mut().push(path.to_path_buf());
        Os.read_link(path)
    }
}

impl World {
    fn new() -> Self {
        let dir = TestDir::new("about-place");
        // Canonical, so the test's own temp-folder link (`/var` → `/private/var`)
        // isn't what the walk follows.
        let root = std::fs::canonicalize(dir.as_ref()).expect("canonical");
        let share = root.join("share");
        std::fs::create_dir(&share).expect("make the share");
        std::fs::write(share.join("report.pdf"), "x").expect("write into it");
        let mounts = Mounts::from_rows(vec![
            ("/".into(), "/dev/sda1".into(), "ext4".into()),
            (share.to_string_lossy().into_owned(), "nas:/export".into(), "nfs".into()),
        ]);
        Self {
            _dir: dir,
            root,
            share,
            mounts,
            touched: RefCell::new(Vec::new()),
        }
    }

    fn link(&self, name: &str, target: impl AsRef<Path>) -> PathBuf {
        let link = self.root.join(name);
        std::os::unix::fs::symlink(target, &link).expect("link");
        link
    }

    fn local(&self, path: &Path) -> bool {
        on_a_local_disk(path, &self.mounts, self)
    }

    fn reaches(&self, path: &Path) -> bool {
        reaches_off_the_disk(path, &self.mounts, self)
    }

    fn resolved(&self, path: &Path) -> Option<PathBuf> {
        resolved_within(path, &self.mounts, self)
    }

    fn touched_the_share(&self) -> bool {
        self.touched.borrow().iter().any(|path| path.starts_with(&self.share))
    }
}

/// ❗ A path that reaches a share through a linked folder is found out without one
/// call into the share, absolute link or relative.
#[test]
fn a_parent_linking_into_a_share_is_found_without_a_call_into_it() {
    let world = World::new();
    let absolute = world.link("nas", &world.share);
    assert!(world.reaches(&absolute.join("report.pdf")));
    assert!(!world.local(&absolute.join("report.pdf")));

    std::fs::create_dir(world.root.join("deeper")).expect("make a folder");
    let relative = world.root.join("deeper").join("nas");
    std::os::unix::fs::symlink("../share", &relative).expect("link to it");
    assert!(world.reaches(&relative.join("report.pdf")));
    assert!(!world.local(&relative.join("report.pdf")));
    assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());

    assert!(world.reaches(&world.share.join("report.pdf")), "the share itself");
    assert!(!world.local(&world.share.join("report.pdf")));
    assert!(!world.local(&world.share), "the share's own root");
}

/// ❗ On Linux the mount table is matched by text, so `~/nas/report.pdf` (with `~/nas`
/// → an NFS mount) used to read as the home folder's disk and get every local job.
/// The walk places it by where its parent's link leads, the same on both OSes.
#[test]
fn a_symlinked_parent_into_an_nfs_mount_isnt_a_local_disk() {
    let world = World::new();
    let home = world.root.join("home");
    std::fs::create_dir(&home).expect("make a home");
    let nas = home.join("nas");
    std::os::unix::fs::symlink(&world.share, &nas).expect("link into the mount");
    assert!(!world.local(&nas.join("report.pdf")));
    assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());
}

/// ❗ An item that is itself a link into a share lives on the local disk (its card
/// describes the link), but reading through it reaches the share: a card Cmdr
/// started asks it nothing. Neither question calls into the share.
#[test]
fn an_item_that_is_a_link_into_a_share_lives_local_and_reaches_the_share() {
    let world = World::new();
    let nas = world.link("nas", &world.share);
    assert!(world.local(&nas), "a link lives where its parent lives");
    assert!(world.reaches(&nas), "reading through it would reach the share");
    let into_a_file = world.link("report", world.share.join("report.pdf"));
    assert!(world.reaches(&into_a_file));
    assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());
}

/// A path that stays on the local disk, links and all, reads as local; a gone one
/// is the probe's to say.
#[test]
fn a_local_path_reads_local_links_included() {
    let world = World::new();
    let docs = world.root.join("docs");
    std::fs::create_dir(&docs).expect("make docs");
    std::fs::write(docs.join("notes.txt"), "x").expect("write");
    let alias = world.link("alias", &docs);
    assert!(world.local(&alias.join("notes.txt")));
    assert!(!world.reaches(&alias.join("notes.txt")));
    let to_a_local_file = world.link("notes", docs.join("notes.txt"));
    assert!(!world.reaches(&to_a_local_file));
    assert!(world.local(&world.root.join("gone").join("x")));
    assert!(!world.reaches(&world.root.join("gone").join("x")));
}

/// A link loop proves nothing: a background card asks nothing, and the gate fails
/// closed.
#[test]
fn a_link_loop_is_unproven() {
    let world = World::new();
    let a = world.root.join("a");
    let b = world.root.join("b");
    std::os::unix::fs::symlink(&b, &a).expect("a → b");
    std::os::unix::fs::symlink(&a, &b).expect("b → a");
    assert!(world.reaches(&a.join("x")));
    assert!(!world.local(&a.join("x")));
    assert_eq!(world.resolved(&a.join("x")), None);
}

/// The gate's resolve follows every local link, the item's own included, and stops
/// at a share's edge with the rest as written.
#[test]
fn the_gates_resolve_follows_local_links_and_stops_at_a_share() {
    let world = World::new();
    let keys = world.root.join(".ssh");
    std::fs::create_dir(&keys).expect("make the folder");
    std::fs::write(keys.join("id_ed25519"), "x").expect("write the key");
    let folder_link = world.link("keys", &keys);
    assert_eq!(
        world.resolved(&folder_link.join("id_ed25519")),
        Some(keys.join("id_ed25519"))
    );
    let item_link = world.link("key", keys.join("id_ed25519"));
    assert_eq!(world.resolved(&item_link), Some(keys.join("id_ed25519")));
    assert_eq!(
        world.resolved(&folder_link.join("gone.txt")),
        Some(keys.join("gone.txt")),
        "a part that's gone stays as written"
    );

    let nas = world.link("nas", &world.share);
    assert_eq!(
        world.resolved(&nas.join("report.pdf")),
        Some(world.share.join("report.pdf"))
    );
    assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());
}

/// The data volume's mounts are listed under `/System/Volumes/Data` and spelled from
/// `/`; a path no mount covers isn't a local disk.
#[test]
fn a_data_volume_mount_covers_its_short_spelling() {
    let mounts = Mounts::from_rows(vec![
        ("/".into(), "/dev/disk3s1s1".into(), "apfs".into()),
        ("/System/Volumes/Data".into(), "/dev/disk3s5".into(), "apfs".into()),
        (
            "/System/Volumes/Data/home".into(),
            "map auto_home".into(),
            "autofs".into(),
        ),
    ]);
    assert!(mounts.is_local_disk(Path::new("/Users/jane")));
    assert!(!mounts.is_local_disk(Path::new("/home/jane")));
    assert!(!mounts.is_local_disk(Path::new("/System/Volumes/Data/home/jane")));
    assert!(!Mounts::from_rows(Vec::new()).is_local_disk(Path::new("/Users")));
}
