//! Synthetic APFS and HFS+ disk images for tests that need a real removable volume
//! (macOS only).
//!
//! ❌ **Never a physical disk, and never a new FAT, exFAT, or MS-DOS image.** A
//! `diskutil unmount` of a physical FAT32 card wedged macOS 26's FSKit `msdos`
//! service and kernel-panicked a Mac (`crates/cmdr-index/src/indexing/tests/CLAUDE.md`).
//! The two legacy FAT variants exist only for the hand-run `external_drive_fixture`.
//!
//! What the harness guarantees, in code:
//!
//! - **One test at a time, machine-wide.** [`DiskImageSession::acquire`] takes an
//!   exclusive `flock` on `$TMPDIR/cmdr-disk-image-tests.lock`, held until the last
//!   handle drops. Every image needs a session, so no test can forget the lock, and
//!   two worktrees running the lane can't unmount under each other.
//! - **Every `hdiutil`, `diskutil`, and `umount` call goes through the runner**, a
//!   closed list of verbs, each SIGKILLed past [`runner::TOOL_DEADLINE`].
//! - **Before every call that changes a disk, the target is proven ours**: `diskutil`
//!   resolves it to its physical whole disk and `hdiutil info` must list both under
//!   this image's backing file. DiskArbitration reuses a freed BSD unit at once, so
//!   a stored node is never trusted. `detach -force` also refuses nested and outer
//!   images.
//! - **Unique volume names** (`CMDR<pid><n>`), `-nobrowse` everywhere, and a
//!   [`DiskImage`] detaches and deletes itself on drop, unwind included.
//! - **Leftovers are reclaimed at [`DiskImageSession::acquire`]**, because a drop can't
//!   run when the test process is SIGKILLed. See `reclaim_orphans` for why that's safe
//!   only there.

mod create;
mod error;
mod facts;
mod file_holder;
mod runner;

#[cfg(test)]
mod real_images;

pub use error::HarnessError;
pub use facts::Refusal;
pub use file_holder::FileHolder;
pub use runner::{CreateFs, CreateLayout, Finished};

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use serde::de::DeserializeOwned;

use super::{TestDir, holds_within};
use crate::ignore_poison::IgnorePoison;
use crate::log_private::LogPrivate;
use facts::{AttachedImages, DiskFacts};
use runner::{Call, TOOL_DEADLINE};

const LOCK_FILE_NAME: &str = "cmdr-disk-image-tests.lock";

/// Threads that hold a session right now. A second `acquire` on one of them would
/// wait on its own lock forever, so it panics instead.
static HOLDERS: Mutex<Vec<ThreadId>> = Mutex::new(Vec::new());

/// Feeds [`DiskImageSession::unique_volume_name`].
static NEXT_NAME: AtomicU32 = AtomicU32::new(1);

/// The machine-wide right to create, change, and detach disk images, held for a
/// test's whole body.
///
/// ```ignore
/// let session = DiskImageSession::acquire();
/// let image = DiskImage::attach(&session, ImageSpec::Apfs).expect("attach");
/// ```
///
/// The lock is an `flock` on its own open file description, so it serializes
/// threads of one process as well as processes. Acquire it once per test and pass
/// the `Arc` down.
#[derive(Debug)]
pub struct DiskImageSession {
    _lock: File,
    owner: ThreadId,
    /// Backing files this session attached, for the outer-image check.
    images: Mutex<Vec<PathBuf>>,
}

impl DiskImageSession {
    /// Waits for the machine-wide lock and returns the session holding it.
    ///
    /// # Panics
    ///
    /// When this thread already holds a session, or the lock file can't be opened
    /// or locked.
    pub fn acquire() -> Arc<Self> {
        let owner = std::thread::current().id();
        assert!(
            !HOLDERS.lock_ignore_poison().contains(&owner),
            "this thread already holds a DiskImageSession; pass that one down, since a second would wait on the first forever"
        );
        let path = std::env::temp_dir().join(LOCK_FILE_NAME);
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .unwrap_or_else(|error| panic!("couldn't open the disk-image lock at {}: {error}", path.display()));
        // allowed-lock-poison: `File::lock` is an `flock` on the lock file, not a `Mutex`, so there's no poison to recover; a lock that can't be taken must stop the test
        lock.lock()
            .unwrap_or_else(|error| panic!("couldn't take the disk-image lock at {}: {error}", path.display()));
        HOLDERS.lock_ignore_poison().push(owner);
        let session = Arc::new(Self {
            _lock: lock,
            owner,
            images: Mutex::new(Vec::new()),
        });
        session.reclaim_orphans();
        session
    }

    /// Detach whatever a previous run left attached.
    ///
    /// ❗ **Why this is safe here and nowhere else**: the machine-wide `flock` is held by
    /// the time this runs, and the kernel releases a dead process's `flock`. So no LIVE
    /// harness session can own a matching attachment — anything still attached that passes
    /// every check in [`facts::orphan_verdict`] was left by a run that is already gone.
    ///
    /// **Why it's needed**: [`DiskImage`]'s `Drop` covers a panic and an early return, but
    /// nothing covers a SIGKILL, which is what nextest sends a test that overruns its cap.
    /// Six images were found attached this way (macOS 27.0, 2026-09-16) with their backing
    /// files still on disk, which is the signature of a process that died without running a
    /// destructor: the attach-window path below deletes its file on the way out.
    ///
    /// ❌ Never `-force`, and ❌ never anything that fails a check: those are left alone.
    fn reclaim_orphans(&self) {
        let attached = match self.attached_images() {
            Ok(attached) => attached,
            Err(error) => {
                log::warn!(
                    target: "testing::disk_images",
                    "couldn't read the attached images, so leftovers stay attached: {error}"
                );
                return;
            }
        };
        let temp_dir = std::env::temp_dir();
        for image in &attached.images {
            match facts::orphan_verdict(image, &temp_dir) {
                Ok(whole) => self.detach_orphan(&image.image_path, whole),
                // Only what LOOKS like ours earns a line: this Mac may have any number of
                // unrelated images attached, and naming each one every run would bury the
                // ones that matter.
                Err(facts::NotAnOrphan::NotInTempDir | facts::NotAnOrphan::DirNotOurs) => {}
                Err(reason) => log::warn!(
                    target: "testing::disk_images",
                    "leaving {} attached, since it isn't provably ours: {reason:?}",
                    LogPrivate::file_path(&image.image_path)
                ),
            }
        }
    }

    /// Detach one leftover, through the same ownership check every other change makes, so
    /// `hdiutil` has to agree the node and its whole disk belong to that backing file.
    fn detach_orphan(&self, image_path: &Path, whole: &str) {
        let node = whole.strip_prefix("/dev/").unwrap_or(whole);
        match self.run(
            image_path,
            Call::Detach {
                whole: node,
                force: false,
            },
        ) {
            Ok(_) => log::info!(
                target: "testing::disk_images",
                "detached {}, left attached by a run that couldn't clean up after itself",
                LogPrivate::file_path(&image_path)
            ),
            Err(error) => log::warn!(
                target: "testing::disk_images",
                "couldn't detach the leftover image {}: {error}",
                LogPrivate::file_path(&image_path)
            ),
        }
    }

    /// Detach `image_path` if `hdiutil` still lists it, for the window between an attach
    /// that landed and the guard that would have owned it. Same evidence as the sweep.
    fn detach_if_attached(&self, image_path: &Path) {
        let Ok(attached) = self.attached_images() else {
            return;
        };
        let Some(image) = attached.images.iter().find(|image| image.image_path == image_path) else {
            return;
        };
        if let Ok(whole) = facts::orphan_verdict(image, &std::env::temp_dir()) {
            self.detach_orphan(image_path, whole);
        }
    }

    /// A volume name no other run on this machine is using: `CMDR<pid><n>`.
    pub fn unique_volume_name(&self) -> String {
        format!(
            "CMDR{}{}",
            std::process::id(),
            NEXT_NAME.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// Runs `call`. A call that changes a disk runs only after its target is proven
    /// to belong to the image backed by `owner` (and, for `detach -force`, that the
    /// image isn't nested in or holding another).
    fn run(&self, owner: &Path, call: Call<'_>) -> Result<Finished, HarnessError> {
        runner::gated(
            &call,
            |target| {
                let attached = self.attached_images().map_err(|error| Refusal::Unverifiable {
                    detail: error.to_string(),
                })?;
                facts::check_ownership(target, owner, &attached, &mut |node| self.disk_facts(node))?;
                if let Call::Detach { force: true, .. } = call {
                    let session_images = self.images.lock_ignore_poison().clone();
                    facts::check_force_detach(owner, &attached, &session_images, &mut host_node)?;
                }
                Ok(())
            },
            || runner::run(&call, TOOL_DEADLINE),
        )
    }

    fn attached_images(&self) -> Result<AttachedImages, HarnessError> {
        let call = Call::Info;
        let finished = runner::run(&call, TOOL_DEADLINE)?;
        parse(&call, &finished.stdout)
    }

    /// The physical whole disk of the image backed by `image_path`, resolved fresh from
    /// `hdiutil info` and `diskutil`.
    fn whole_of(&self, image_path: &Path) -> Result<String, HarnessError> {
        let refused = |refusal| HarnessError::Refused {
            call: format!("resolving the whole disk of {}", image_path.display()),
            refusal,
        };
        let attached = self.attached_images()?;
        let entity = attached
            .images
            .iter()
            .find(|image| image.image_path == image_path)
            .ok_or_else(|| refused(Refusal::ImageNotAttached))?
            .entities
            .first()
            .ok_or_else(|| refused(Refusal::ImageNotAttached))?;
        let facts = self.disk_facts(&entity.dev_entry).ok_or_else(|| {
            refused(Refusal::UnknownTarget {
                target: entity.dev_entry.clone(),
            })
        })?;
        facts::physical_whole(&facts, &mut |node| self.disk_facts(node)).map_err(|stuck_at| {
            refused(Refusal::UnresolvedWhole {
                target: entity.dev_entry.clone(),
                stuck_at,
            })
        })
    }

    /// Waits for `hdiutil info` to stop listing `image_path`, after a detach answered
    /// success. Answers an error rather than panicking: `Drop` may already be unwinding.
    fn until_unlisted(&self, image_path: &Path) -> Result<(), HarnessError> {
        let mut unreadable = None;
        let unlisted = holds_within(TOOL_DEADLINE, || match self.attached_images() {
            Ok(attached) => {
                unreadable = None;
                !attached.images.iter().any(|image| image.image_path == image_path)
            }
            Err(error) => {
                unreadable = Some(error);
                false
            }
        });
        match (unlisted, unreadable) {
            (true, _) => Ok(()),
            (false, Some(error)) => Err(error),
            (false, None) => Err(HarnessError::StillListed {
                image: image_path.to_path_buf(),
            }),
        }
    }

    /// `diskutil info -plist` for a node or mount point; `None` when `diskutil`
    /// doesn't know it or couldn't answer, which every caller treats as "not
    /// proven ours".
    fn disk_facts(&self, target: &str) -> Option<DiskFacts> {
        let call = Call::DiskInfo { target };
        let finished = runner::run(&call, TOOL_DEADLINE).ok()?;
        plist::from_bytes(&finished.stdout).ok()
    }
}

impl Drop for DiskImageSession {
    fn drop(&mut self) {
        HOLDERS.lock_ignore_poison().retain(|holder| *holder != self.owner);
    }
}

fn parse<T: DeserializeOwned>(call: &Call<'_>, bytes: &[u8]) -> Result<T, HarnessError> {
    plist::from_bytes(bytes).map_err(|error| HarnessError::Unreadable {
        call: call.describe(),
        detail: error.to_string(),
    })
}

/// The `/dev/` node of the volume holding `path`, from `statfs`. Only ever asked
/// about this session's own backing files.
fn host_node(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statfs` is a plain C struct of integers and fixed char arrays, for
    // which all-zero bytes are a valid value; the call below overwrites it.
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c_path` is NUL-terminated and outlives the call, and `stat` is a
    // live, correctly typed out-pointer.
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    // SAFETY: on success the kernel fills `f_mntfromname` with a NUL-terminated
    // string inside its fixed array, and `stat` outlives the borrow.
    let node = unsafe { std::ffi::CStr::from_ptr(stat.f_mntfromname.as_ptr()) };
    Some(node.to_string_lossy().into_owned())
}

/// Which image [`DiskImage::attach`] builds. Every volume gets a unique name and is
/// mounted `-nobrowse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSpec {
    /// One APFS volume.
    Apfs,
    /// Two APFS volumes in one container. A 1.1 GB sparse image: `addVolume` answers
    /// -69493 on a small container (APFS scales its volume cap with the container).
    ApfsTwoVolumes,
    /// One HFS+ volume, roomy enough for a tree of tens of thousands of files.
    Hfs,
    /// Two HFS+ partitions on one GPT disk: 60 MB, then the rest.
    HfsTwoPartitions,
}

/// One mounted volume of a [`DiskImage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedVolume {
    /// The unique volume name.
    pub name: String,
    /// Its BSD node, `diskNsM`.
    pub node: String,
    /// Where it's mounted.
    pub mount_point: PathBuf,
}

/// An attached synthetic disk image. Dropping it detaches the image (ownership
/// checked; `-force` only as the fallback, and only for a non-nested image), then
/// deletes its backing file.
///
/// A detach that fails panics the test, unless the test is already panicking, so an
/// image left attached never passes silently.
#[derive(Debug)]
pub struct DiskImage {
    session: Arc<DiskImageSession>,
    /// The backing file, spelled exactly as it was passed to `hdiutil attach`, which is
    /// how `hdiutil info` spells it back.
    image_path: PathBuf,
    volumes: Vec<MountedVolume>,
    /// Dropped after `Drop::drop` runs, so the file outlives the detach. `None` for an
    /// image stored on another image's volume, whose teardown takes the file with it.
    _dir: Option<TestDir>,
}

impl DiskImage {
    /// Creates and attaches a fresh image shaped like `spec`.
    pub fn attach(session: &Arc<DiskImageSession>, spec: ImageSpec) -> Result<Self, HarnessError> {
        match spec {
            ImageSpec::Apfs => {
                let name = session.unique_volume_name();
                Self::create_and_attach(session, CreateFs::Apfs, CreateLayout::Gpt, "64m", false, &name)
            }
            ImageSpec::Hfs => {
                let name = session.unique_volume_name();
                Self::create_and_attach(session, CreateFs::HfsPlus, CreateLayout::Gpt, "256m", false, &name)
            }
            ImageSpec::ApfsTwoVolumes => {
                let first = session.unique_volume_name();
                let mut image =
                    Self::create_and_attach(session, CreateFs::Apfs, CreateLayout::Gpt, "1100m", true, &first)?;
                let first_node = image.volume(0)?.node.clone();
                let container = session
                    .disk_facts(&first_node)
                    .and_then(|facts| facts.apfs_container_reference)
                    .ok_or_else(|| HarnessError::Unreadable {
                        call: format!("diskutil info -plist {first_node}"),
                        detail: "no APFSContainerReference".to_string(),
                    })?;
                let second = session.unique_volume_name();
                image.run(Call::AddApfsVolume {
                    container: &container,
                    name: &second,
                })?;
                let second_node = image.node_named(&second)?;
                image.run(Call::MountNobrowse { node: &second_node })?;
                image.volumes = image.mounted_volumes(&[&first, &second])?;
                Ok(image)
            }
            ImageSpec::HfsTwoPartitions => {
                let placeholder = session.unique_volume_name();
                let mut image = Self::create_and_attach(
                    session,
                    CreateFs::HfsPlus,
                    CreateLayout::Gpt,
                    "256m",
                    false,
                    &placeholder,
                )?;
                let whole = image.whole()?;
                let (first, second) = (session.unique_volume_name(), session.unique_volume_name());
                image.run(Call::PartitionTwoJhfs {
                    whole: &whole,
                    first: &first,
                    second: &second,
                })?;
                // `partitionDisk` mounts both partitions browsable; remount them hidden.
                for name in [&first, &second] {
                    let node = image.node_named(name)?;
                    image.run(Call::Unmount { node: &node })?;
                    image.run(Call::MountNobrowse { node: &node })?;
                }
                image.volumes = image.mounted_volumes(&[&first, &second])?;
                Ok(image)
            }
        }
    }

    /// Creates and attaches a FAT32 or exFAT image on an MBR disk, for the hand-run
    /// `external_drive_fixture` alone.
    ///
    /// ❌ Never call this from a new test, and never from a lane: the FSKit `msdos`
    /// unmount is the kernel-panic surface. `fs` must be one of the two `Legacy`
    /// variants.
    pub fn attach_legacy_fat_fixture(
        session: &Arc<DiskImageSession>,
        fs: CreateFs,
        volume_name: &str,
    ) -> Result<Self, HarnessError> {
        assert!(
            matches!(fs, CreateFs::LegacyFat32 | CreateFs::LegacyExFat),
            "attach_legacy_fat_fixture is for the FAT fixture only; use DiskImage::attach"
        );
        Self::create_and_attach(session, fs, CreateLayout::Mbr, "64m", false, volume_name)
    }

    fn create_and_attach(
        session: &Arc<DiskImageSession>,
        fs: CreateFs,
        layout: CreateLayout,
        size: &str,
        sparse: bool,
        volume_name: &str,
    ) -> Result<Self, HarnessError> {
        let dir = TestDir::new("disk_image");
        let image_path = dir.join(if sparse { "image.sparseimage" } else { "image.dmg" });
        session.create_image(
            &image_path,
            Call::Create {
                image: &image_path,
                size,
                fs,
                layout,
                volume_name,
                sparse,
            },
        )?;
        if let Err(error) = session.run(&image_path, Call::Attach { image: &image_path }) {
            // ❗ The attach may have LANDED before the runner killed it at its deadline, and
            // the guard that would own it doesn't exist yet. `dir` drops on the way out and
            // takes the backing file with it, so an attachment left here would outlive its
            // own file and the sweep above would be its only hope.
            session.detach_if_attached(&image_path);
            return Err(error);
        }
        session.images.lock_ignore_poison().push(image_path.clone());
        // Attached from here on: the guard exists before anything else can fail, so
        // an early return still detaches. The mount points come from `hdiutil info`,
        // the same answer every later ownership check reads.
        let mut image = Self {
            session: Arc::clone(session),
            image_path,
            volumes: Vec::new(),
            _dir: Some(dir),
        };
        image.volumes = image.mounted_volumes(&[volume_name])?;
        Ok(image)
    }

    /// Attaches a fresh image whose backing file sits on this image's first volume, the
    /// way Finder attaches one: through DiskImages2, so a `diskimagesiod` the same-uid
    /// holder walk can't see serves it. That's the shape of a `.dmg` opened from a USB
    /// stick.
    ///
    /// ❗ Drop it BEFORE this image: while it's attached, this one can't detach.
    pub fn attach_stored_here(&self, session: &Arc<DiskImageSession>) -> Result<Self, HarnessError> {
        let volume_name = session.unique_volume_name();
        let image_path = self.volume(0)?.mount_point.join("image.dmg");
        session.create_image(
            &image_path,
            Call::Create {
                image: &image_path,
                size: "16m",
                fs: CreateFs::HfsPlus,
                layout: CreateLayout::Gpt,
                volume_name: &volume_name,
                sparse: false,
            },
        )?;
        session.run(&image_path, Call::AttachThroughDaemon { image: &image_path })?;
        session.images.lock_ignore_poison().push(image_path.clone());
        // Attached from here on: the guard exists before anything else can fail.
        let mut image = Self {
            session: Arc::clone(session),
            image_path,
            volumes: Vec::new(),
            _dir: None,
        };
        image.volumes = image.mounted_volumes(&[&volume_name])?;
        Ok(image)
    }

    /// Whether DiskImages2 serves this image, so its serving process is a `diskimagesiod`.
    pub fn served_by_diskimages2(&self) -> Result<bool, HarnessError> {
        let attached = self.session.attached_images()?;
        Ok(attached
            .images
            .iter()
            .any(|image| image.image_path == self.image_path && image.diskimages2))
    }

    /// The mounted volumes, in the order the spec names them.
    pub fn volumes(&self) -> &[MountedVolume] {
        &self.volumes
    }

    /// The backing file.
    pub fn image_path(&self) -> &Path {
        &self.image_path
    }

    /// The process serving this image, which is the one holding its backing FILE open.
    ///
    /// What a test needs to ask "who holds the volume the `.dmg` sits on": that process
    /// is a real holder of it, for as long as the image stays attached.
    pub fn serving_pid(&self) -> Result<u32, HarnessError> {
        let attached = self.session.attached_images()?;
        attached
            .images
            .iter()
            .find(|image| image.image_path == self.image_path)
            .and_then(|image| image.hdid_pid)
            .ok_or_else(|| HarnessError::Unreadable {
                call: "hdiutil info -plist".to_string(),
                detail: format!("no process serves {}", self.image_path.display()),
            })
    }

    /// Whether `hdiutil info` still lists this image.
    pub fn is_attached(&self) -> Result<bool, HarnessError> {
        let attached = self.session.attached_images()?;
        Ok(attached.images.iter().any(|image| image.image_path == self.image_path))
    }

    /// `diskutil eject <mount_point>`, once `mount_point` is proven to be one of this
    /// image's volumes. The production eject runs exactly this verb.
    pub fn eject(&self, mount_point: &Path) -> Result<Finished, HarnessError> {
        self.run(Call::Eject { mount_point })
    }

    /// `diskutil unmount` of volume `index`, once its node is proven this image's. The unmount
    /// DiskArbitration mediates, so an approval session hears about it.
    pub fn unmount_volume(&self, index: usize) -> Result<Finished, HarnessError> {
        let node = self.volume(index)?.node.clone();
        self.run(Call::Unmount { node: &node })
    }

    /// `diskutil unmountDisk` of this image's whole disk: every volume on it, as its own DA request.
    pub fn unmount_disk(&self) -> Result<Finished, HarnessError> {
        let whole = self.whole()?;
        self.run(Call::UnmountDisk { whole: &whole })
    }

    /// `/sbin/umount` of volume `index`, once its mount point is proven this image's.
    ///
    /// ❗ The raw syscall wrapper BYPASSES DiskArbitration: no approval is asked and no
    /// `WillUnmount` is posted, so an approval session learns of it only from the
    /// description change afterwards. That's the shape of a drive that vanished, and the
    /// only way to drive it without pulling a real cable.
    pub fn raw_umount(&self, index: usize) -> Result<Finished, HarnessError> {
        let mount_point = self.volume(index)?.mount_point.clone();
        self.run(Call::RawUmount {
            mount_point: &mount_point,
        })
    }

    /// `diskutil renameVolume` of volume `index` to a fresh unique name, once its node
    /// is proven this image's. The volume list is read back from `hdiutil info`
    /// afterwards, so [`Self::volumes`] says where the volume is mounted now.
    pub fn rename_volume(&mut self, index: usize) -> Result<&MountedVolume, HarnessError> {
        let node = self.volume(index)?.node.clone();
        let name = self.session.unique_volume_name();
        self.run(Call::RenameVolume {
            node: &node,
            name: &name,
        })?;
        let mut names: Vec<String> = self.volumes.iter().map(|volume| volume.name.clone()).collect();
        names[index] = name;
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.volumes = self.mounted_volumes(&names)?;
        self.volume(index)
    }

    /// Moves volume `index`'s mount into `dir`, an empty folder the test owns:
    /// `diskutil unmount`, then `diskutil mount -mountPoint`, once its node is proven
    /// this image's. A drive mounted inside an ordinary folder, which a `/Volumes`
    /// mount never is. The detach on drop unmounts it from there like anywhere else,
    /// so `dir` must outlive this image.
    pub fn remount_volume_at(&mut self, index: usize, dir: &Path) -> Result<&MountedVolume, HarnessError> {
        let node = self.volume(index)?.node.clone();
        self.run(Call::Unmount { node: &node })?;
        self.run(Call::MountNobrowseAt {
            node: &node,
            mount_point: dir,
        })?;
        let names: Vec<String> = self.volumes.iter().map(|volume| volume.name.clone()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.volumes = self.mounted_volumes(&names)?;
        self.volume(index)
    }

    /// `hdiutil detach -force` of the whole image, once it's proven ours and not
    /// nested. A detached image answers [`Refusal::ImageNotAttached`].
    pub fn force_detach(&self) -> Result<(), HarnessError> {
        let whole = self.whole()?;
        self.run(Call::Detach {
            whole: &whole,
            force: true,
        })
        .map(|_| ())
    }

    /// Attaches this image's backing file again, after a [`force_detach`], and
    /// reads its mount points fresh.
    ///
    /// [`force_detach`]: Self::force_detach
    ///
    /// What plugging a drive back in looks like. The volume NAMES are unchanged
    /// (they live in the filesystem, not in the attachment), but the mount point
    /// can come back somewhere else, which is the whole reason a leftover record
    /// stores its path relative to the root.
    ///
    /// ❗ Goes through `session.run` like every other call, but the identity
    /// check can't run first: a detached image has no node to map back to its
    /// path. That's the same position `create_and_attach`'s attach is in, and
    /// safe for the same reason — the argument is OUR backing file, named from
    /// this guard, so there is no stored node to have gone stale.
    pub fn reattach(&mut self) -> Result<(), HarnessError> {
        let names: Vec<String> = self.volumes.iter().map(|volume| volume.name.clone()).collect();
        self.session.run(
            &self.image_path,
            Call::Attach {
                image: &self.image_path,
            },
        )?;
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        self.volumes = self.mounted_volumes(&names)?;
        Ok(())
    }

    fn run(&self, call: Call<'_>) -> Result<Finished, HarnessError> {
        self.session.run(&self.image_path, call)
    }

    fn volume(&self, index: usize) -> Result<&MountedVolume, HarnessError> {
        self.volumes.get(index).ok_or_else(|| HarnessError::Unreadable {
            call: "hdiutil info -plist".to_string(),
            detail: format!("the image has no mounted volume #{index}"),
        })
    }

    /// This image's physical whole disk, resolved fresh from `hdiutil info` and
    /// `diskutil`.
    fn whole(&self) -> Result<String, HarnessError> {
        self.session.whole_of(&self.image_path)
    }

    /// The node of this image's volume named `name`, mounted or not.
    fn node_named(&self, name: &str) -> Result<String, HarnessError> {
        let attached = self.session.attached_images()?;
        attached
            .images
            .iter()
            .filter(|image| image.image_path == self.image_path)
            .flat_map(|image| &image.entities)
            .filter_map(|entity| self.session.disk_facts(&entity.dev_entry))
            .find(|facts| facts.volume_name == name)
            .map(|facts| facts.device_identifier)
            .ok_or_else(|| HarnessError::Unreadable {
                call: "hdiutil info -plist".to_string(),
                detail: format!("no volume named {name} on {}", self.image_path.display()),
            })
    }

    /// This image's mounted volumes named `names`, in that order.
    fn mounted_volumes(&self, names: &[&str]) -> Result<Vec<MountedVolume>, HarnessError> {
        let attached = self.session.attached_images()?;
        let mounted: Vec<(DiskFacts, PathBuf)> = attached
            .images
            .iter()
            .filter(|image| image.image_path == self.image_path)
            .flat_map(|image| &image.entities)
            .filter_map(|entity| {
                let mount_point = entity.mount_point.clone()?;
                Some((self.session.disk_facts(&entity.dev_entry)?, mount_point))
            })
            .collect();
        names
            .iter()
            .map(|name| {
                mounted
                    .iter()
                    .find(|(facts, _)| facts.volume_name == *name)
                    .map(|(facts, mount_point)| MountedVolume {
                        name: (*name).to_string(),
                        node: facts.device_identifier.clone(),
                        mount_point: mount_point.clone(),
                    })
                    .ok_or_else(|| HarnessError::Unreadable {
                        call: "hdiutil info -plist".to_string(),
                        detail: format!("volume {name} isn't mounted"),
                    })
            })
            .collect()
    }

    /// Detaches the image and returns once `hdiutil info` no longer lists it.
    ///
    /// ❗ A detach answering success doesn't mean the image is gone from `hdiutil info`
    /// yet: the image's teardown can trail the tool's exit under load, which is how a
    /// test asserting "detached" right after the drop flaked (#830, on the sparse
    /// two-volume image). Waiting here makes "dropped" mean "detached" for every
    /// caller, and the next image's attach starts on a settled `hdiutil`.
    fn detach_for_good(&self) -> Result<(), HarnessError> {
        let whole = match self.whole() {
            Ok(whole) => whole,
            Err(error) if error.refusal() == Some(&Refusal::ImageNotAttached) => return Ok(()),
            Err(error) => return Err(error),
        };
        match self.run(Call::Detach {
            whole: &whole,
            force: false,
        }) {
            Ok(_) => self.session.until_unlisted(&self.image_path),
            Err(error) if error.refusal() == Some(&Refusal::ImageNotAttached) => Ok(()),
            Err(_) => match self.force_detach() {
                Ok(()) => self.session.until_unlisted(&self.image_path),
                Err(error) if error.refusal() == Some(&Refusal::ImageNotAttached) => Ok(()),
                Err(error) => Err(error),
            },
        }
    }
}

impl Drop for DiskImage {
    fn drop(&mut self) {
        if let Err(error) = self.detach_for_good() {
            if std::thread::panicking() {
                log::warn!(
                    target: "testing::disk_images",
                    "couldn't detach the disk image at {}: {error}",
                    LogPrivate::file_path(&self.image_path)
                );
            } else {
                panic!(
                    "couldn't detach the disk image at {}: {error}",
                    self.image_path.display()
                );
            }
        }
    }
}
