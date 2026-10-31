//! The harness against real images: each spec attaches, mounts every volume
//! `-nobrowse`, and leaves nothing behind when it drops.
//!
//! `#[ignore]`d because they attach real disk images; hand-run with
//! `cargo nextest run -p cmdr-fs --run-ignored only -E 'test(testing::disk_images::real_images::)'`.
//! Serialized in the `disk-image` nextest group, and machine-wide by the session lock.

use super::*;
use crate::testing::wait_until;
use std::time::Duration;

/// Whether the mount at `mount_point` carries `MNT_DONTBROWSE`, read from the mount
/// table's own flags.
fn is_mounted_nobrowse(mount_point: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(mount_point.as_os_str().as_bytes()).expect("no NUL in a mount point");
    // SAFETY: `statfs` is a plain C struct for which all-zero bytes are valid; the
    // call overwrites it.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is NUL-terminated and outlives the call; `stat` is a live out-pointer.
    let answered = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } == 0;
    answered && stat.f_flags & libc::MNT_DONTBROWSE as u32 != 0
}

/// Attaches `spec`, checks every volume is a writable nobrowse mount, drops the
/// image, and checks it's detached, unmounted, and deleted. Returns the facts of
/// each volume, read while attached.
fn attach_check_and_drop(spec: ImageSpec, volume_count: usize) -> Vec<DiskFacts> {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, spec).expect("attach the image");
    let image_path = image.image_path().to_path_buf();
    assert_eq!(image.volumes().len(), volume_count, "volumes: {:?}", image.volumes());

    let mut facts = Vec::new();
    for volume in image.volumes() {
        assert!(
            volume.mount_point.is_dir(),
            "{} is mounted",
            volume.mount_point.display()
        );
        assert!(
            is_mounted_nobrowse(&volume.mount_point),
            "{} is mounted nobrowse",
            volume.mount_point.display()
        );
        std::fs::write(volume.mount_point.join("probe.txt"), b"x").expect("the volume is writable");
        facts.push(session.disk_facts(&volume.node).expect("diskutil knows the volume"));
    }
    let mount_points: Vec<PathBuf> = image.volumes().iter().map(|v| v.mount_point.clone()).collect();

    drop(image);

    let still_attached = session
        .attached_images()
        .expect("hdiutil info")
        .images
        .iter()
        .any(|attached| attached.image_path == image_path);
    assert!(!still_attached, "the image detaches on drop");
    for mount_point in mount_points {
        let description = format!("{} to leave /Volumes", mount_point.display());
        wait_until(Duration::from_secs(5), &description, || !mount_point.exists());
    }
    assert!(!image_path.exists(), "the backing file is deleted");
    facts
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn an_apfs_image_mounts_one_nobrowse_volume_and_leaves_nothing_behind() {
    attach_check_and_drop(ImageSpec::Apfs, 1);
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn a_two_volume_apfs_image_mounts_both_volumes_of_one_container() {
    let facts = attach_check_and_drop(ImageSpec::ApfsTwoVolumes, 2);
    assert_ne!(facts[0].volume_name, facts[1].volume_name);
    assert!(facts[0].apfs_container_reference.is_some());
    assert_eq!(facts[0].apfs_container_reference, facts[1].apfs_container_reference);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn an_hfs_image_mounts_one_nobrowse_volume_and_leaves_nothing_behind() {
    attach_check_and_drop(ImageSpec::Hfs, 1);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_two_partition_hfs_image_mounts_both_partitions_of_one_disk() {
    let facts = attach_check_and_drop(ImageSpec::HfsTwoPartitions, 2);
    assert_ne!(facts[0].volume_name, facts[1].volume_name);
    assert!(facts[0].apfs_container_reference.is_none());
    assert_eq!(facts[0].parent_whole_disk, facts[1].parent_whole_disk);
}

/// Every mount in the kernel's table with its filesystem identity (`f_fsid`), read with
/// `getfsstat(MNT_NOWAIT)`, the same non-blocking read the app's presence answer uses.
fn mount_table() -> Vec<(PathBuf, [i32; 2])> {
    // SAFETY: the documented count query: a null buffer and zero size write nothing.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    assert!(count > 0, "getfsstat lists at least `/`");
    let capacity = count as usize + 4;
    let mut table: Vec<libc::statfs> = Vec::with_capacity(capacity);
    let bytes = (capacity * size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: `table` has room for `capacity` records and `bytes` is exactly their length.
    let filled = unsafe { libc::getfsstat(table.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    assert!(filled > 0, "getfsstat fills the table");
    // SAFETY: the kernel initialized `filled` records, clamped to the capacity.
    unsafe { table.set_len((filled as usize).min(capacity)) };
    table
        .iter()
        .map(|stat| {
            // SAFETY: `f_mntonname` is NUL-terminated inside its fixed array, and `stat`
            // outlives the borrow.
            let on = unsafe { std::ffi::CStr::from_ptr(stat.f_mntonname.as_ptr()) };
            // SAFETY: libc declares `fsid_t` `#[repr(C)]` around exactly one `[i32; 2]`;
            // it only keeps the field private.
            let fsid: [i32; 2] = unsafe { std::mem::transmute(stat.f_fsid) };
            (PathBuf::from(on.to_string_lossy().into_owned()), fsid)
        })
        .collect()
}

/// Renames the image's volume with a file open for writing on it, and checks the rename
/// moved the mount point live: the old path left the mount table, the same filesystem
/// (same `f_fsid`) is listed at the new one, and the handle opened before the rename
/// still writes the file, now read back under the new path.
fn a_rename_moves_the_mount_point_of_a_filesystem_that_stays_mounted(spec: ImageSpec) {
    use std::io::Write;

    let session = DiskImageSession::acquire();
    let mut image = DiskImage::attach(&session, spec).expect("attach the image");
    let before = image.volumes()[0].mount_point.clone();
    let fsid = mount_table()
        .into_iter()
        .find(|(on, _)| *on == before)
        .map(|(_, fsid)| fsid)
        .expect("the volume is in the mount table");
    let mut open = File::create(before.join("open-across-the-rename.txt")).expect("create a file");
    open.write_all(b"before ").expect("write before the rename");

    let after = image.rename_volume(0).expect("rename the volume").mount_point.clone();

    assert_ne!(after, before, "the mount point follows the new name");
    let table = mount_table();
    assert!(
        !table.iter().any(|(on, _)| *on == before),
        "{} left the mount table",
        before.display()
    );
    assert_eq!(
        table.iter().find(|(on, _)| *on == after).map(|(_, fsid)| *fsid),
        Some(fsid),
        "the same filesystem is listed at {}",
        after.display()
    );
    open.write_all(b"after")
        .expect("the handle opened before the rename still writes");
    open.sync_all().expect("sync the file");
    drop(open);
    assert_eq!(
        std::fs::read(after.join("open-across-the-rename.txt")).expect("read under the new name"),
        b"before after",
        "nothing unmounted under the open handle"
    );
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn renaming_a_mounted_apfs_volume_moves_its_mount_point_and_keeps_its_filesystem_mounted() {
    a_rename_moves_the_mount_point_of_a_filesystem_that_stays_mounted(ImageSpec::Apfs);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn renaming_a_mounted_hfs_volume_moves_its_mount_point_and_keeps_its_filesystem_mounted() {
    a_rename_moves_the_mount_point_of_a_filesystem_that_stays_mounted(ImageSpec::Hfs);
}

/// Holds the first new whole-disk `/dev/rdiskN` read-only for [`DeviceHolder::HOLD`], the
/// shape of whatever opens an image's device while `hdiutil create` is still building it.
/// Dropping it waits out the hold.
struct DeviceHolder {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DeviceHolder {
    /// Long enough to outlast the create's own detach, which is what it has to overlap.
    const HOLD: Duration = Duration::from_secs(3);

    fn whole_disk_nodes() -> Vec<std::ffi::OsString> {
        std::fs::read_dir("/dev")
            .expect("list /dev")
            .flatten()
            .map(|entry| entry.file_name())
            .filter(|name| {
                name.to_str()
                    .and_then(|name| name.strip_prefix("rdisk"))
                    .is_some_and(|unit| !unit.contains('s'))
            })
            .collect()
    }

    /// ❗ Start it with the session held: the lock is what makes a new node this test's.
    fn start(_session: &DiskImageSession) -> Self {
        use std::os::unix::fs::OpenOptionsExt;

        let before = Self::whole_disk_nodes();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread = std::thread::spawn({
            let stop = Arc::clone(&stop);
            move || {
                while !stop.load(Ordering::Relaxed) {
                    let new_node = Self::whole_disk_nodes().into_iter().find(|node| !before.contains(node));
                    let held = new_node.and_then(|node| {
                        File::options()
                            .read(true)
                            .custom_flags(libc::O_NONBLOCK)
                            .open(Path::new("/dev").join(node))
                            .ok()
                    });
                    if held.is_some() {
                        // allowed-test-sleep: the hold IS the fixture; `held` drops when it ends
                        std::thread::sleep(Self::HOLD);
                        return;
                    }
                    // allowed-test-sleep: polling /dev tightly enough to land inside the create's one-second window
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for DeviceHolder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The snapshot `HarnessError::says_resource_busy` relies on: the tool's real words, and
/// the attachment a busy create leaves under the path it was given. Rounds, since the
/// holder can lose the race on a loaded Mac, and a create it missed simply works.
#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_held_device_makes_hdiutil_create_answer_busy_and_leave_its_image_attached() {
    let session = DiskImageSession::acquire();
    for _ in 0..5 {
        let dir = TestDir::new("disk_image");
        let image_path = dir.join("image.dmg");
        let name = session.unique_volume_name();
        let holder = DeviceHolder::start(&session);
        let answer = session.run(
            &image_path,
            Call::Create {
                image: &image_path,
                size: "64m",
                fs: CreateFs::HfsPlus,
                layout: CreateLayout::Gpt,
                volume_name: &name,
                sparse: false,
            },
        );
        let Err(error) = answer else {
            continue;
        };
        assert!(error.says_resource_busy(), "got {error}");
        let listed = |session: &DiskImageSession| {
            let attached = session.attached_images().expect("hdiutil info");
            attached.images.iter().any(|image| image.image_path == image_path)
        };
        assert!(listed(&session), "a busy create leaves its image attached");

        drop(holder);
        session.settle_failed_create(&image_path).expect("the leftover goes");
        assert!(!listed(&session), "the settle detaches the leftover");
        assert!(!image_path.exists(), "and removes its file, so a retry can create it");
        return;
    }
    panic!("the holder never got in during five creates, so this pinned nothing");
}

/// The whole fix: the first create loses to a holder, and the image still attaches.
#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn an_image_attaches_even_when_something_holds_its_device_during_the_create() {
    let session = DiskImageSession::acquire();
    let holder = DeviceHolder::start(&session);
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    assert!(image.is_attached().expect("hdiutil info"));
    drop(holder);
}

#[test]
#[ignore = "attaches a real APFS disk image via hdiutil; run with --run-ignored"]
fn an_eject_aimed_at_a_volume_that_isnt_this_images_is_refused_before_diskutil_runs() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Apfs).expect("attach the image");
    // The Mac's own Data volume: `diskutil info` answers for it, and `hdiutil` lists
    // it under no image of ours, so the eject never runs.
    let result = image.eject(Path::new("/System/Volumes/Data"));
    assert!(
        matches!(
            result.as_ref().map_err(HarnessError::refusal),
            Err(Some(Refusal::NotOurs { .. }))
        ),
        "got {result:?}"
    );
}
