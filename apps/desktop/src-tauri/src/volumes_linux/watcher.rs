//! Volume mount/unmount watcher for Linux.
//!
//! Two watchers run concurrently:
//! - The mount table: a dedicated thread `poll()`s `/proc/self/mounts` for the kernel's
//!   change signal and re-reads `/proc/mounts` only when it wakes.
//! - `/run/user/<uid>/gvfs/` (inotify): detects GVFS SMB share mount/unmount (these are
//!   subdirectories of a single gvfsd-fuse mount, so they don't appear in `/proc/mounts`)
//!
//! Both diff against known state and emit `volume-mounted` / `volume-unmounted`
//! Tauri events. Also registers/unregisters volumes with the global `VolumeManager`.

use crate::file_system::linux_mounts::{self, MountEntry};
use crate::ignore_poison::IgnorePoison;
use crate::volume_broadcast::{VolumeMounted, VolumeUnmounted};
use log::{debug, error, info, warn};
use notify::{Event, EventKind, RecommendedWatcher, Watcher};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use tauri::AppHandle;
use tauri_specta::Event as _;

/// The mount table the watcher diffs.
const PROC_MOUNTS: &str = "/proc/mounts";

/// The file whose `poll()` reports a mount-table change: `POLLPRI | POLLERR` once per
/// change in this process's mount namespace, and nothing in between.
const MOUNT_CHANGE_SIGNAL: &str = "/proc/self/mounts";

/// Global app handle for emitting events from the watcher.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// The watch on the mount table, kept until [`stop_volume_watcher`].
static WATCHER: Mutex<Option<MountTableWatch>> = Mutex::new(None);

/// The watcher instance for GVFS directory.
static GVFS_WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// Known mount points mapped to their filesystem type, for diffing.
static KNOWN_MOUNTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// Known GVFS SMB mount paths, for diffing.
static KNOWN_GVFS_MOUNTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

/// Start watching for volume mount/unmount events.
/// Call this once during app setup.
pub fn start_volume_watcher(app: &AppHandle) {
    if APP_HANDLE.set(app.clone()).is_err() {
        debug!("Linux volume watcher already initialized");
        return;
    }

    // Discovery read the same table at startup, so if it couldn't, the app knows no
    // attached volume either, and starting from nothing keeps the two in step: the
    // first readable check reports them as mounted.
    let initial = get_real_mounts().unwrap_or_default();
    let known = KNOWN_MOUNTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = known.lock_ignore_poison();
    *guard = initial;
    debug!("Initial Linux mounts: {} entries", guard.len());
    drop(guard);

    info!("Starting Linux volume watcher on {MOUNT_CHANGE_SIGNAL}");

    match start_mount_table_watch(check_for_mount_changes) {
        Ok(watch) => {
            *WATCHER.lock_ignore_poison() = Some(watch);
            info!("Linux volume watcher started successfully");
        }
        Err(e) => error!("{e}"),
    }

    start_gvfs_watcher();
}

/// Stops both mount watchers, waiting for the mount-table thread to finish. Idempotent.
pub fn stop_volume_watcher() {
    // Taken out of their locks before dropping: the mount-table watch joins its thread.
    let watch = WATCHER.lock_ignore_poison().take();
    let gvfs = GVFS_WATCHER
        .get()
        .and_then(|storage| storage.lock_ignore_poison().take());
    drop(watch);
    drop(gvfs);
}

/// A running watch on the mount table. Dropping it stops the thread and waits for it.
struct MountTableWatch {
    /// Closing this end hangs up the thread's end, which is the thread's stop signal.
    stop: Option<UnixStream>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for MountTableWatch {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            error!("The mount table watch thread panicked");
        }
    }
}

/// Starts watching the mount table on its own thread, calling `on_change` after each change.
///
/// ❌ Never inotify `/proc/mounts` instead: a mount raises no inotify event there, but every
/// open does, and the handler's own read is an open, so the watch feeds itself.
/// `DETAILS.md` § "Watching the mount table".
fn start_mount_table_watch(on_change: impl Fn() + Send + 'static) -> Result<MountTableWatch, String> {
    let table = File::open(MOUNT_CHANGE_SIGNAL).map_err(|e| format!("Failed to open {MOUNT_CHANGE_SIGNAL}: {e}"))?;
    let (stop, stop_signal) =
        UnixStream::pair().map_err(|e| format!("Failed to create the mount table watch's stop signal: {e}"))?;
    let thread = std::thread::Builder::new()
        .name("mount-table-watch".to_string())
        .spawn(move || watch_mount_table(&table, &stop_signal, &on_change))
        .map_err(|e| format!("Failed to start the mount table watch thread: {e}"))?;
    Ok(MountTableWatch {
        stop: Some(stop),
        thread: Some(thread),
    })
}

/// Blocks in `poll()` until the mount table changes or `stop_signal` hangs up, and calls
/// `on_change` after each change.
///
/// Returns, logged, on any `poll` failure but `EINTR` and on a hung-up or invalid table
/// descriptor. Both descriptors are owned for the thread's whole life, so neither is a
/// transient state, and retrying either in a loop would spin.
fn watch_mount_table(table: &File, stop_signal: &UnixStream, on_change: &dyn Fn()) {
    loop {
        let mut fds = [
            libc::pollfd {
                fd: table.as_raw_fd(),
                events: libc::POLLPRI,
                revents: 0,
            },
            libc::pollfd {
                fd: stop_signal.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `fds` is a live, correctly-typed `pollfd` array whose length is passed as
        // `nfds`, and both descriptors are borrowed from `table` and `stop_signal`, which
        // outlive the call.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            error!("The mount table watch stopped: poll failed: {err}");
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        let table_events = fds[0].revents;
        if table_events & (libc::POLLHUP | libc::POLLNVAL) != 0 {
            error!("The mount table watch stopped: {MOUNT_CHANGE_SIGNAL} reported poll events {table_events:#x}");
            return;
        }
        if table_events & (libc::POLLPRI | libc::POLLERR) != 0 {
            on_change();
        }
    }
}

/// What one read of the mount table changed against `known`, and the state to keep.
#[derive(Debug, PartialEq)]
struct MountChanges {
    mounted: Vec<String>,
    unmounted: Vec<String>,
    current: HashMap<String, String>,
}

/// Reads the mount table at `mounts_file` and diffs its real mounts against `known`.
///
/// `None` when the table can't be read: unknown, ❌ never "every volume unmounted".
fn check_mount_table(known: &HashMap<String, String>, mounts_file: &Path) -> Option<MountChanges> {
    let current = real_mounts(linux_mounts::read_mount_table(mounts_file)?);
    let mounted = current
        .keys()
        .filter(|path| !known.contains_key(*path))
        .cloned()
        .collect();
    let unmounted = known
        .keys()
        .filter(|path| !current.contains_key(*path))
        .cloned()
        .collect();
    Some(MountChanges {
        mounted,
        unmounted,
        current,
    })
}

/// Diff current mounts against known state and emit events.
fn check_for_mount_changes() {
    let Some(known) = KNOWN_MOUNTS.get() else {
        return;
    };
    let mut known_guard = known.lock_ignore_poison();
    let Some(changes) = check_mount_table(&known_guard, Path::new(PROC_MOUNTS)) else {
        return;
    };

    for path in &changes.mounted {
        debug!("Volume mounted: {}", path);
        emit_volume_mounted(path);
    }
    for path in &changes.unmounted {
        debug!("Volume unmounted: {}", path);
        emit_volume_unmounted(path);
    }
    let changed = !changes.mounted.is_empty() || !changes.unmounted.is_empty();
    *known_guard = changes.current;

    // Broadcast updated volume list to frontend
    if changed {
        crate::volume_broadcast::emit_volumes_changed();
    }
}

/// Build a map of real (non-virtual) mount points from /proc/mounts, or `None` when it
/// can't be read.
fn get_real_mounts() -> Option<HashMap<String, String>> {
    linux_mounts::parse_proc_mounts().map(real_mounts)
}

/// The real (non-virtual) mount points in `entries`, mapped to their filesystem type.
fn real_mounts(entries: Vec<MountEntry>) -> HashMap<String, String> {
    let virtual_types: &[&str] = &[
        "proc",
        "sysfs",
        "devpts",
        "tmpfs",
        "cgroup",
        "cgroup2",
        "devtmpfs",
        "hugetlbfs",
        "mqueue",
        "debugfs",
        "tracefs",
        "securityfs",
        "pstore",
        "configfs",
        "fusectl",
        "binfmt_misc",
        "autofs",
        "efivarfs",
        "ramfs",
        "rpc_pipefs",
        "nfsd",
        "nsfs",
        "bpf",
    ];

    // Another account's own FUSE mount is never news here: discovery and the
    // registry sweep skip the same rows (`mounts::is_private_to_another_user`).
    let this_user = super::mounts::this_user();

    entries
        .into_iter()
        .filter(|e| !virtual_types.contains(&e.fstype.as_str()))
        .filter(|e| !super::mounts::is_private_to_another_user(e, this_user))
        .map(|e| (e.mountpoint, e.fstype))
        .collect()
}

/// Start watching the GVFS directory for SMB share mount/unmount.
/// Skips silently if `/run/user/<uid>/gvfs/` doesn't exist (non-GNOME systems).
fn start_gvfs_watcher() {
    // SAFETY: `getuid` reads the process's real UID; always safe, no args or pointers.
    let uid = unsafe { libc::getuid() };
    let gvfs_dir = format!("/run/user/{}/gvfs", uid);
    let gvfs_path = Path::new(&gvfs_dir);

    if !gvfs_path.is_dir() {
        debug!("GVFS directory {} not found, skipping GVFS watcher", gvfs_dir);
        return;
    }

    // Snapshot current GVFS SMB mounts
    let initial = get_current_gvfs_smb_paths(&gvfs_dir);
    let known = KNOWN_GVFS_MOUNTS.get_or_init(|| Mutex::new(HashSet::new()));
    debug!("Initial GVFS SMB mounts: {} entries", initial.len());
    *known.lock_ignore_poison() = initial;

    let gvfs_dir_owned = gvfs_dir.clone();
    let watcher_result = notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
        Ok(event) => handle_gvfs_event(event, &gvfs_dir_owned),
        Err(e) => error!("GVFS watcher error: {}", e),
    });

    match watcher_result {
        Ok(mut watcher) => {
            if let Err(e) = watcher.watch(gvfs_path, notify::RecursiveMode::NonRecursive) {
                warn!("Failed to watch GVFS directory {}: {}", gvfs_dir, e);
                return;
            }

            let storage = GVFS_WATCHER.get_or_init(|| Mutex::new(None));
            *storage.lock_ignore_poison() = Some(watcher);

            info!("GVFS watcher started on {}", gvfs_dir);
        }
        Err(e) => {
            warn!("Failed to create GVFS watcher: {}", e);
        }
    }
}

/// Handle inotify events on the GVFS directory.
fn handle_gvfs_event(event: Event, gvfs_dir: &str) {
    match event.kind {
        EventKind::Create(_) | EventKind::Remove(_) => {
            check_for_gvfs_changes(gvfs_dir);
        }
        _ => {}
    }
}

/// Diff current GVFS SMB directories against known state and emit events.
fn check_for_gvfs_changes(gvfs_dir: &str) {
    let current = get_current_gvfs_smb_paths(gvfs_dir);

    let known = match KNOWN_GVFS_MOUNTS.get() {
        Some(k) => k,
        None => return,
    };

    let mut known_guard = known.lock_ignore_poison();

    let mut changed = false;

    // Newly mounted shares
    for path in &current {
        if !known_guard.contains(path) {
            debug!("GVFS SMB share mounted: {}", path);
            emit_volume_mounted(path);
            changed = true;
        }
    }

    // Unmounted shares
    for path in known_guard.iter() {
        if !current.contains(path) {
            debug!("GVFS SMB share unmounted: {}", path);
            emit_volume_unmounted(path);
            changed = true;
        }
    }

    *known_guard = current;

    // Broadcast updated volume list to frontend
    if changed {
        crate::volume_broadcast::emit_volumes_changed();
    }
}

/// Scan the GVFS directory for current SMB share mount paths.
fn get_current_gvfs_smb_paths(gvfs_dir: &str) -> HashSet<String> {
    let mut paths = HashSet::new();
    let Ok(entries) = std::fs::read_dir(gvfs_dir) else {
        return paths;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let dirname = name.to_string_lossy();
        if super::parse_gvfs_smb_dirname(&dirname).is_some() {
            paths.insert(entry.path().to_string_lossy().to_string());
        }
    }
    paths
}

/// Emit a volume-mounted event and register with VolumeManager.
fn emit_volume_mounted(volume_path: &str) {
    register_volume_with_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeMounted {
            volume_path: volume_path.to_string(),
        };
        if let Err(e) = payload.emit(app) {
            error!("Failed to emit volume-mounted event: {}", e);
        } else {
            debug!("Emitted volume-mounted for {}", volume_path);
        }
    }
}

/// Emit a volume-unmounted event and unregister from VolumeManager.
fn emit_volume_unmounted(volume_path: &str) {
    unregister_volume_from_manager(volume_path);

    if let Some(app) = APP_HANDLE.get() {
        let payload = VolumeUnmounted {
            volume_path: volume_path.to_string(),
            // A mount watcher speaks in paths: the id it resolved doesn't always
            // mean "gone" (a promoted volume keeps serving from another mount),
            // so the consumer looks the row up as it always has.
            volume_id: None,
        };
        if let Err(e) = payload.emit(app) {
            error!("Failed to emit volume-unmounted event: {}", e);
        } else {
            debug!("Emitted volume-unmounted for {}", volume_path);
        }
    }
}

/// Register a volume with the global VolumeManager.
fn register_volume_with_manager(volume_path: &str) {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    use std::sync::Arc;

    let volume_id = super::volume_id_for_mount(volume_path);

    // For GVFS SMB shares, extract the share name instead of the raw dirname
    let name = if let Some(dirname) = Path::new(volume_path).file_name().and_then(|n| n.to_str()) {
        if let Some(gvfs) = super::parse_gvfs_smb_dirname(dirname) {
            gvfs.share
        } else {
            dirname.to_string()
        }
    } else {
        "Unknown".to_string()
    };

    let mount = crate::file_system::index_provider::mount_class_at(Path::new(volume_path));
    let volume = Arc::new(LocalPosixVolume::on_mount(&name, volume_path, mount));
    get_volume_manager().register(&volume_id, volume);
    debug!("Registered mounted volume: {} -> {}", volume_id, volume_path);
}

/// Drop a gone mount root from the volume that owned it.
///
/// Keyed by root (which works even after the mount is gone, when
/// `volume_id_for_mount`'s SMB branch can no longer recover the right ID), and
/// it unregisters only when that was the volume's LAST mount: one CIFS share can
/// be mounted at two paths and both derive one ID. Falls back to deriving the ID
/// from the path if no entry claims the root.
fn unregister_volume_from_manager(volume_path: &str) {
    use crate::file_system::volume::manager::{RootRemoval, get_volume_manager};

    let manager = get_volume_manager();
    match manager.remove_root(Path::new(volume_path)) {
        RootRemoval::Unregistered { id, volume } => {
            volume.on_unmount();
            // The frontend retires a slow-connection notice about this share now, so
            // its server's next genuine fallback has to be news again.
            crate::network::os_mount_notice::forget_unmounted_volume(&id);
            debug!("Unregistered volume: {} ({})", id, volume_path);
        }
        RootRemoval::Promoted { id, new_root } => {
            info!(
                "{volume_path} unmounted, but volume {id} is still mounted at {}; promoted it to that root.",
                new_root.display()
            );
        }
        RootRemoval::ActiveRootStranded { id } => {
            warn!("{volume_path} unmounted and volume {id} can't move to one of its other mounts, so it stays there.");
        }
        RootRemoval::SiblingDropped { id } => {
            debug!("{volume_path} unmounted; volume {id} still serves from its active root");
        }
        RootRemoval::Unknown => {
            let volume_id = super::volume_id_for_mount(volume_path);
            manager.unregister(&volume_id);
            debug!("Unregistered volume: {} ({})", volume_id, volume_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestDir, wait_until};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn test_volume_event_payload_serialization() {
        let payload = VolumeMounted {
            volume_path: "/mnt/usb".to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("volumePath"));
        assert!(json.contains("/mnt/usb"));
    }

    #[test]
    fn test_get_real_mounts_filters_virtual() {
        let mounts = get_real_mounts().expect("the test process can read its own mount table");
        // Should not contain virtual fs mount points
        for (path, fstype) in &mounts {
            assert_ne!(fstype, "proc", "Should filter proc at {}", path);
            assert_ne!(fstype, "sysfs", "Should filter sysfs at {}", path);
            assert_ne!(fstype, "tmpfs", "Should filter tmpfs at {}", path);
        }
    }

    /// Another account's FUSE mount coming or going changes nothing this account
    /// can use, so the diff never sees it and no event fires.
    #[test]
    fn another_accounts_fuse_mount_is_no_mount_change() {
        let me = super::super::mounts::this_user();
        let someone_else = me.wrapping_add(1);
        let table = linux_mounts::parse_proc_mounts_from_content(&format!(
            "\
/dev/sda1 / ext4 rw,relatime 0 0
pcloud /home/sven/pCloudDrive fuse.pcloud rw,relatime,user_id={me},group_id={me} 0 0
pcloud /home/rin/pCloudDrive fuse.pcloud rw,relatime,user_id={someone_else},group_id={someone_else} 0 0
"
        ));
        let mut watched: Vec<String> = real_mounts(table).into_keys().collect();
        watched.sort();
        assert_eq!(watched, ["/", "/home/sven/pCloudDrive"]);
    }

    /// Regression anchor for the self-feeding watch: an inotify watch on
    /// `/proc/mounts` wakes on every OPEN of the file, and the handler's own read
    /// is an open, so it re-checked about a thousand times a second forever.
    #[test]
    fn watching_the_mount_table_doesnt_wake_itself() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let watch = start_mount_table_watch({
            let wakes = Arc::clone(&wakes);
            move || {
                wakes.fetch_add(1, Ordering::SeqCst);
                // What the real handler does on every wake: read the table.
                let _ = linux_mounts::parse_proc_mounts();
            }
        })
        .expect("the mount table is watchable on Linux");

        // Somebody else reading the table isn't a mount change either.
        let _ = linux_mounts::parse_proc_mounts();
        // allowed-test-sleep: a negative assertion over a window; nothing should wake the watch in it
        std::thread::sleep(Duration::from_millis(500));
        let seen = wakes.load(Ordering::SeqCst);
        drop(watch);

        // allowed-pluralize-noun: the message only prints when seen > 1, so "times" is always plural
        assert!(seen <= 1, "the watch woke {seen} times in 500 ms with no mount change");
    }

    #[test]
    fn an_unreadable_mount_table_reports_no_changes() {
        let known = HashMap::from([("/".to_string(), "ext4".to_string())]);
        let changes = check_mount_table(&known, Path::new("/proc/self/no-such-mount-table"));
        assert_eq!(
            changes, None,
            "an unreadable table must never read as every volume unmounted"
        );
    }

    #[test]
    fn dropping_the_watch_stops_its_thread() {
        let watch = start_mount_table_watch(|| {}).expect("the mount table is watchable on Linux");
        let stopped = Arc::new(AtomicBool::new(false));
        std::thread::spawn({
            let stopped = Arc::clone(&stopped);
            move || {
                drop(watch);
                stopped.store(true, Ordering::SeqCst);
            }
        });
        wait_until(Duration::from_secs(2), "the mount table watch thread to stop", || {
            stopped.load(Ordering::SeqCst)
        });
    }

    #[test]
    #[ignore = "mounts a tmpfs, so it needs CAP_SYS_ADMIN: run it in a privileged Linux container with --run-ignored=ignored-only"]
    fn a_real_mount_wakes_the_watch() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let dir = TestDir::new("mount-table-watch");
        let target = CString::new(dir.as_os_str().as_bytes()).expect("a temp dir path has no NUL byte");
        let wakes = Arc::new(AtomicUsize::new(0));
        let watch = start_mount_table_watch({
            let wakes = Arc::clone(&wakes);
            move || {
                wakes.fetch_add(1, Ordering::SeqCst);
            }
        })
        .expect("the mount table is watchable on Linux");

        // SAFETY: every pointer is a NUL-terminated C string that lives for the call, and
        // tmpfs takes no mount data, so `data` is null.
        let mounted = unsafe {
            libc::mount(
                c"none".as_ptr(),
                target.as_ptr(),
                c"tmpfs".as_ptr(),
                0,
                std::ptr::null(),
            )
        };
        assert_eq!(mounted, 0, "mounting a tmpfs: {}", io::Error::last_os_error());
        wait_until(Duration::from_secs(2), "the watch to see the new mount", || {
            wakes.load(Ordering::SeqCst) >= 1
        });

        // SAFETY: `target` is the NUL-terminated path mounted above.
        unsafe { libc::umount(target.as_ptr()) };
        drop(watch);
    }
}
