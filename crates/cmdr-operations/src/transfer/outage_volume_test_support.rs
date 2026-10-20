//! A volume whose session a test can take away and give back: the double for
//! `volume_outage.rs`, the way `liveness_test_support.rs` is for the watchdog.
//!
//! It behaves like the SMB share in `#398`. While its session is down, every
//! call fails at once with `DeviceDisconnected` and
//! [`Volume::connection_state`] reads `Disconnected` (what `cmdr-smb`'s
//! `check_connection` does). [`Volume::attempt_reconnect`] succeeds only once
//! the test has made the server reachable again, and a test can also arm the
//! session to drop at the start of the Nth write, which is the mid-copy outage,
//! at the Nth read-stream open (a copy FROM the share), or around the Nth rename: before the server sees it, or after the server
//! performed it but before its reply arrived (the landing a drop swallows). It
//! can also land every write in one shot (SMB's compound write for a small
//! file), replace a file in one forced rename (SMB's `ReplaceIfExists`), and
//! drop right after the Nth write completed, the same swallowed reply for a
//! write that had no temp, optionally with the write's date stamp cut off
//! too (SMB stamps a compound write by path, in a frame of its own). And its
//! reconnect can hang, as an SMB handshake a server accepted and never answered
//! does (`#406`).
//!
//! Like SMB it reports a session generation ([`Volume::connection_generation`]),
//! bumped by every reconnect that brings the session back. A test can make a
//! rename answer a LATE error: the session drops and someone else reconnects it
//! while the rename is in flight, so the rename fails with the old session's
//! error and the volume already reads live on the next generation.
//!
//! A swap's landing is armed by PATH rather than by count, since a concurrent
//! window's siblings rename and delete in no fixed order: the drop around the
//! next delete of a path or rename onto it, on either side, the late error of
//! a delete, the drop at the next stat of a path, and the drop as a path's
//! metadata goes on (a folder's dates, which a backend answers as a refused
//! class, never an error), or a refusal of it with the session up. It can also copy inside itself on the server
//! (`Volume::copy_within`), and it counts where every write went, so a cell can
//! tell a landing resumed from its temp from a file streamed again.
//!
//! Its copy scan is the shared walk (`scan_walk`) over its own stat and listing,
//! so the session can drop at a listing mid-walk.
//!
//! Wraps an `InMemoryVolume` (rather than owning one) so a test can read the
//! destination back through the same handle after the copy.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataReport};
use cmdr_fs::volume::{
    BackendKind, BatchScanResult, ConnectionState, CopyScanResult, DirectoryCreation, InMemoryVolume, LaneKey,
    ListingProgress, ScanBoundary, ScanConflict, ScanSource, SourceItemInfo, SpaceInfo, StreamLength,
    StreamWriteProgress, StreamWritten, Volume, VolumeError, VolumeReadStream, WriteMode, scan_walk,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(crate) struct OutageVolume {
    inner: Arc<InMemoryVolume>,
    /// Its own lane, so tests running in parallel never share an outage record.
    lane: String,
    /// Whether the session is up: what `connection_state` reports and what
    /// every call checks first.
    up: AtomicBool,
    /// Whether a reconnect would succeed right now: the server is back.
    reachable: AtomicBool,
    /// A reconnect is refused for its credentials, as after a password change.
    needs_sign_in: AtomicBool,
    reconnect_attempts: AtomicUsize,
    /// A reconnect attempt never returns while this is set.
    hang_reconnects: AtomicBool,
    /// Reconnect attempts whose future was dropped while it hung.
    abandoned_reconnects: AtomicUsize,
    opens: AtomicUsize,
    /// Drop the session at the start of this (0-based) read-stream open.
    drop_at_open: Mutex<Option<usize>>,
    writes: AtomicUsize,
    /// Where each write went, in order.
    written: Mutex<Vec<PathBuf>>,
    /// Drop the session at the start of this (0-based) write.
    drop_at_write: Mutex<Option<usize>>,
    /// Drop the session right after this (0-based) write completed.
    drop_after_write: Mutex<Option<usize>>,
    /// The write `drop_after_write` names lands its bytes, but not the source's
    /// date: the drop cut off the stamp that follows the bytes.
    drop_cuts_the_stamp: AtomicBool,
    /// Promise single-shot writes (`Volume::write_is_single_shot`).
    single_shot: AtomicBool,
    /// Promise a forced rename replaces a file in one step
    /// (`Volume::supports_atomic_replace_rename`), as SMB's `ReplaceIfExists`.
    replaces_atomically: AtomicBool,
    renames: AtomicUsize,
    /// Drop the session around this (0-based) rename, and on which side of it.
    drop_at_rename: Mutex<Option<(usize, RenameDrop)>>,
    /// Drop the session around this (0-based) rename and have someone else
    /// reconnect it before the rename's error arrives.
    stale_error_at_rename: Mutex<Option<(usize, RenameDrop)>>,
    /// Drop the session around the next rename onto this path, once.
    drop_at_rename_onto: Mutex<Option<(PathBuf, RenameDrop)>>,
    /// Drop the session around the next rename FROM this path, once.
    drop_at_rename_of: Mutex<Option<(PathBuf, RenameDrop)>>,
    /// The next rename from this path answers a dropped session's error after
    /// someone else reconnected it, once.
    stale_error_at_rename_of: Mutex<Option<(PathBuf, RenameDrop)>>,
    /// Drop the session around the next delete of this path, once.
    drop_at_delete_of: Mutex<Option<(PathBuf, RenameDrop)>>,
    /// The next delete of this path answers a dropped session's error after
    /// someone else reconnected it, once.
    stale_error_at_delete_of: Mutex<Option<(PathBuf, RenameDrop)>>,
    /// Drop the session as the next stat of this path starts, once.
    drop_at_stat_of: Mutex<Option<PathBuf>>,
    /// Drop the session as the next listing of this path starts, once.
    drop_at_listing_of: Mutex<Option<PathBuf>>,
    /// Refuse every listing of this path, with the session up.
    refuse_listing_of: Mutex<Option<PathBuf>>,
    /// Every listing sent: a walk that carried on from one that started over.
    listings: Mutex<Vec<PathBuf>>,
    /// Drop the session as the next metadata apply on this path starts, once.
    drop_at_apply_metadata_of: Mutex<Option<PathBuf>>,
    /// Refuse every metadata apply on this path, with the session up.
    refuse_metadata_of: Mutex<Option<PathBuf>>,
    /// Refuse the next deletes of this path that reach a live session, this
    /// many times.
    refuse_deletes_of: Mutex<Option<(PathBuf, usize)>>,
    /// Copy inside itself on the server (`Volume::copy_within`), counting each.
    copies_on_server: AtomicBool,
    server_copies: AtomicUsize,
    /// Which session is up: bumped each time a reconnect brings it back.
    generation: AtomicU64,
    /// Which backend it stands in for: an SMB share unless a test says
    /// otherwise ([`serve_as`](Self::serve_as)).
    backend: Mutex<BackendKind>,
}

/// Counts itself into its counter when dropped: held across an await that never
/// resolves, it counts the futures that were abandoned there.
struct CountOnDrop<'a>(&'a AtomicUsize);

impl Drop for CountOnDrop<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Which side of a rename the session drops on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenameDrop {
    /// The request never reaches the server: nothing moves.
    BeforeTheServerSeesIt,
    /// The server performs the rename, and the reply is lost with the session.
    AfterTheServerPerformsIt,
}

impl OutageVolume {
    pub(crate) fn wrap(inner: Arc<InMemoryVolume>, lane: &str) -> Arc<Self> {
        Arc::new(Self {
            inner,
            lane: lane.to_owned(),
            up: AtomicBool::new(true),
            reachable: AtomicBool::new(true),
            needs_sign_in: AtomicBool::new(false),
            reconnect_attempts: AtomicUsize::new(0),
            hang_reconnects: AtomicBool::new(false),
            abandoned_reconnects: AtomicUsize::new(0),
            opens: AtomicUsize::new(0),
            drop_at_open: Mutex::new(None),
            writes: AtomicUsize::new(0),
            written: Mutex::new(Vec::new()),
            drop_at_write: Mutex::new(None),
            drop_after_write: Mutex::new(None),
            drop_cuts_the_stamp: AtomicBool::new(false),
            single_shot: AtomicBool::new(false),
            replaces_atomically: AtomicBool::new(false),
            renames: AtomicUsize::new(0),
            drop_at_rename: Mutex::new(None),
            stale_error_at_rename: Mutex::new(None),
            drop_at_rename_onto: Mutex::new(None),
            drop_at_rename_of: Mutex::new(None),
            stale_error_at_rename_of: Mutex::new(None),
            drop_at_delete_of: Mutex::new(None),
            stale_error_at_delete_of: Mutex::new(None),
            drop_at_stat_of: Mutex::new(None),
            drop_at_listing_of: Mutex::new(None),
            refuse_listing_of: Mutex::new(None),
            listings: Mutex::new(Vec::new()),
            drop_at_apply_metadata_of: Mutex::new(None),
            refuse_metadata_of: Mutex::new(None),
            refuse_deletes_of: Mutex::new(None),
            copies_on_server: AtomicBool::new(false),
            server_copies: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
            backend: Mutex::new(BackendKind::Smb),
        })
    }

    /// Stands in for `kind` instead of an SMB share: a phone over ADB holds a
    /// session the same way, and is still a device rather than a server.
    pub(crate) fn serve_as(&self, kind: BackendKind) {
        *self.backend.lock_ignore_poison() = kind;
    }

    /// The session drops and the server becomes unreachable.
    pub(crate) fn go_away(&self) {
        self.reachable.store(false, Ordering::SeqCst);
        self.up.store(false, Ordering::SeqCst);
    }

    /// The server is reachable again; the session stays down until someone
    /// reconnects it.
    pub(crate) fn become_reachable(&self) {
        self.reachable.store(true, Ordering::SeqCst);
    }

    /// Somebody else (a pane's own backoff, a sibling task's wait) reconnected
    /// the session.
    pub(crate) fn reconnected_elsewhere(&self) {
        self.reachable.store(true, Ordering::SeqCst);
        self.bring_the_session_back();
    }

    /// The session comes up, on the next generation if it was down.
    fn bring_the_session_back(&self) {
        if !self.up.swap(true, Ordering::SeqCst) {
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// The session generation [`Volume::connection_generation`] reports.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub(crate) fn require_sign_in(&self) {
        self.needs_sign_in.store(true, Ordering::SeqCst);
    }

    /// The session drops (and the server goes away) the moment read-stream open
    /// number `n` (0-based) starts: a copy FROM the share meets the outage.
    pub(crate) fn drop_at_open(&self, n: usize) {
        *self.drop_at_open.lock_ignore_poison() = Some(n);
    }

    /// The session drops (and the server goes away) the moment write number
    /// `n` (0-based) starts.
    pub(crate) fn drop_at_write(&self, n: usize) {
        *self.drop_at_write.lock_ignore_poison() = Some(n);
    }

    /// Write number `n` (0-based) completes on the server, and then the session
    /// drops (and the server goes away) before its reply arrives.
    pub(crate) fn drop_after_write(&self, n: usize) {
        *self.drop_after_write.lock_ignore_poison() = Some(n);
    }

    /// Like [`drop_after_write`](Self::drop_after_write), but the drop also cuts
    /// off the source's date: the file keeps the date the server gave it, as an
    /// SMB compound write does when its path stamp never went out.
    pub(crate) fn drop_after_write_before_its_stamp(&self, n: usize) {
        self.drop_after_write(n);
        self.drop_cuts_the_stamp.store(true, Ordering::SeqCst);
    }

    /// Lands every write in one indivisible shot, as SMB's compound write does
    /// for a small file, so the transfer writes straight to the final name.
    pub(crate) fn write_single_shot(&self) {
        self.single_shot.store(true, Ordering::SeqCst);
    }

    /// Replaces a file in one forced rename, as SMB's `ReplaceIfExists` does,
    /// so a swap lands in that one step rather than delete-then-rename.
    pub(crate) fn replace_atomically(&self) {
        self.replaces_atomically.store(true, Ordering::SeqCst);
    }

    /// The session drops (and the server goes away) around rename number `n`
    /// (0-based), on the given side of it.
    pub(crate) fn drop_at_rename(&self, n: usize, side: RenameDrop) {
        *self.drop_at_rename.lock_ignore_poison() = Some((n, side));
    }

    /// Rename number `n` (0-based) answers the error of a session that dropped
    /// under it (on the given side), arriving only after someone else had
    /// already reconnected: the volume reads live on the next generation.
    pub(crate) fn stale_error_at_rename(&self, n: usize, side: RenameDrop) {
        *self.stale_error_at_rename.lock_ignore_poison() = Some((n, side));
    }

    /// The session drops (and the server goes away) around the next rename
    /// onto `to`, on the given side of it: a swap's rename after its delete,
    /// whichever rename number that is in a concurrent window.
    pub(crate) fn drop_at_rename_onto(&self, to: &Path, side: RenameDrop) {
        *self.drop_at_rename_onto.lock_ignore_poison() = Some((to.to_path_buf(), side));
    }

    /// The session drops (and the server goes away) around the next rename
    /// FROM `from`, wherever to, on the given side of it: a swap's move of the
    /// original aside, onto a fresh scratch name a test can't name ahead.
    pub(crate) fn drop_at_rename_of(&self, from: &Path, side: RenameDrop) {
        *self.drop_at_rename_of.lock_ignore_poison() = Some((from.to_path_buf(), side));
    }

    /// The next rename from `from` answers the error of a session that dropped
    /// under it (on the given side), arriving only after someone else had
    /// already reconnected: the volume reads live on the next generation.
    pub(crate) fn stale_error_at_rename_of(&self, from: &Path, side: RenameDrop) {
        *self.stale_error_at_rename_of.lock_ignore_poison() = Some((from.to_path_buf(), side));
    }

    /// The session drops (and the server goes away) around the next delete of
    /// `path`, on the given side of it: before the server sees it, or after it
    /// deleted the file and before its reply arrived.
    pub(crate) fn drop_at_delete_of(&self, path: &Path, side: RenameDrop) {
        *self.drop_at_delete_of.lock_ignore_poison() = Some((path.to_path_buf(), side));
    }

    /// The next delete of `path` answers the error of a session that dropped
    /// under it (on the given side), arriving only after someone else had
    /// already reconnected: the volume reads live on the next generation.
    pub(crate) fn stale_error_at_delete_of(&self, path: &Path, side: RenameDrop) {
        *self.stale_error_at_delete_of.lock_ignore_poison() = Some((path.to_path_buf(), side));
    }

    /// The session drops (and the server goes away) as the next listing of
    /// `path` starts: a walk over the share (a copy's scan, a compress's plan)
    /// cut off mid-tree.
    pub(crate) fn drop_at_listing_of(&self, path: &Path) {
        *self.drop_at_listing_of.lock_ignore_poison() = Some(path.to_path_buf());
    }

    /// Every listing of `path` is refused with the session up: a server's
    /// answer, which no wait changes.
    pub(crate) fn refuse_listing_of(&self, path: &Path) {
        *self.refuse_listing_of.lock_ignore_poison() = Some(path.to_path_buf());
    }

    /// How many listings of `path` were sent, the one the drop cut off included.
    pub(crate) fn listings_of(&self, path: &Path) -> usize {
        self.listings.lock_ignore_poison().iter().filter(|p| *p == path).count()
    }

    /// The session drops (and the server goes away) as the next stat of `path`
    /// starts (`get_metadata`, and every probe built on it, `entry_kind` too).
    pub(crate) fn drop_at_stat_of(&self, path: &Path) {
        *self.drop_at_stat_of.lock_ignore_poison() = Some(path.to_path_buf());
    }

    /// The session drops (and the server goes away) as the next metadata apply
    /// on `path` starts: the apply answers every class refused, as a share
    /// does when the drop cuts off a folder's dates.
    pub(crate) fn drop_at_apply_metadata_of(&self, path: &Path) {
        *self.drop_at_apply_metadata_of.lock_ignore_poison() = Some(path.to_path_buf());
    }

    /// Every metadata apply on `path` is refused while the session stays up: a
    /// server that won't take a folder's dates.
    pub(crate) fn refuse_metadata_of(&self, path: &Path) {
        *self.refuse_metadata_of.lock_ignore_poison() = Some(path.to_path_buf());
    }

    /// The next `times` deletes of `path` that reach a live session are
    /// refused: the session that dropped still holds the file open, because
    /// the server hasn't noticed yet that it's gone (`STATUS_SHARING_VIOLATION`
    /// on a share that tears the old session down some time after the new one
    /// is up).
    pub(crate) fn refuse_deletes_of(&self, path: &Path, times: usize) {
        *self.refuse_deletes_of.lock_ignore_poison() = Some((path.to_path_buf(), times));
    }

    /// Copies inside itself on the server from now on, the way SFTP's
    /// `copy-data` does.
    pub(crate) fn copy_on_the_server(&self) {
        self.copies_on_server.store(true, Ordering::SeqCst);
    }

    /// How many copies the server made for us.
    pub(crate) fn server_copies(&self) -> usize {
        self.server_copies.load(Ordering::SeqCst)
    }

    /// How many writes reached the volume: a file streamed again shows here.
    pub(crate) fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    /// How many writes went to `final_path` or to a staged temp of it.
    pub(crate) fn writes_for(&self, final_path: &Path) -> usize {
        let temp_prefix = format!("{}.cmdr-tmp-", final_path.display());
        self.written
            .lock_ignore_poison()
            .iter()
            .filter(|target| *target == final_path || target.to_string_lossy().starts_with(&temp_prefix))
            .count()
    }

    pub(crate) fn reconnect_attempts(&self) -> usize {
        self.reconnect_attempts.load(Ordering::SeqCst)
    }

    /// Reconnect attempts started from now on never return, until
    /// [`answer_reconnects`](Self::answer_reconnects). One already hanging
    /// stays hung: only dropping its future ends it.
    pub(crate) fn hang_reconnects(&self) {
        self.hang_reconnects.store(true, Ordering::SeqCst);
    }

    /// Reconnect attempts started from now on answer again.
    pub(crate) fn answer_reconnects(&self) {
        self.hang_reconnects.store(false, Ordering::SeqCst);
    }

    /// How many hung reconnect attempts had their future dropped.
    pub(crate) fn abandoned_reconnects(&self) -> usize {
        self.abandoned_reconnects.load(Ordering::SeqCst)
    }

    fn gate(&self) -> Result<(), VolumeError> {
        if self.up.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(VolumeError::DeviceDisconnected("the session is down".to_owned()))
        }
    }

    /// Runs `call` (a delete or a rename) under whichever path-armed drop it
    /// meets, each firing once: the plain drop on its side, or the late error
    /// that arrives after someone else reconnected.
    async fn under_path_drops<'a>(
        &'a self,
        path: &Path,
        drop: &Mutex<Option<(PathBuf, RenameDrop)>>,
        stale: Option<&Mutex<Option<(PathBuf, RenameDrop)>>>,
        call: impl Fn() -> Fut<'a, Result<(), VolumeError>>,
    ) -> Result<(), VolumeError> {
        match armed_for(drop, path) {
            Some(RenameDrop::BeforeTheServerSeesIt) => self.go_away(),
            Some(RenameDrop::AfterTheServerPerformsIt) => {
                self.gate()?;
                call().await?;
                self.go_away();
            }
            None => {}
        }
        if let Some(side) = stale.and_then(|stale| armed_for(stale, path)) {
            self.gate()?;
            if side == RenameDrop::AfterTheServerPerformsIt {
                call().await?;
            }
            self.go_away();
            self.reconnected_elsewhere();
            return Err(VolumeError::DeviceDisconnected("the session is down".to_owned()));
        }
        self.gate()?;
        call().await
    }
}

/// Takes the arm in `slot` when it names `path`.
fn armed_for(slot: &Mutex<Option<(PathBuf, RenameDrop)>>, path: &Path) -> Option<RenameDrop> {
    let mut armed = slot.lock_ignore_poison();
    match &*armed {
        Some((at, _)) if at == path => armed.take().map(|(_, side)| side),
        _ => None,
    }
}

impl ScanSource for OutageVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> scan_walk::Walking<'a, FileEntry> {
        self.get_metadata(path)
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> scan_walk::Walking<'a, Vec<FileEntry>> {
        self.list_directory(path, None)
    }
}

impl Volume for OutageVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.lane.clone())
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn max_concurrent_ops(&self) -> usize {
        self.inner.max_concurrent_ops()
    }
    fn connection_generation(&self) -> Option<u64> {
        Some(self.generation())
    }
    fn backend_kind(&self) -> BackendKind {
        *self.backend.lock_ignore_poison()
    }
    fn connection_state(&self) -> Option<ConnectionState> {
        Some(
            if self.needs_sign_in.load(Ordering::SeqCst) && !self.up.load(Ordering::SeqCst) {
                ConnectionState::NeedsSignIn
            } else if self.up.load(Ordering::SeqCst) {
                ConnectionState::Direct
            } else {
                ConnectionState::Disconnected
            },
        )
    }
    fn attempt_reconnect<'a>(&'a self) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.reconnect_attempts.fetch_add(1, Ordering::SeqCst);
            if self.hang_reconnects.load(Ordering::SeqCst) {
                let _abandoned = CountOnDrop(&self.abandoned_reconnects);
                std::future::pending::<()>().await;
            }
            if self.needs_sign_in.load(Ordering::SeqCst) {
                return Err(VolumeError::PermissionDenied {
                    path: "/".to_owned(),
                    raw_os_error: None,
                });
            }
            if self.reachable.load(Ordering::SeqCst) {
                self.bring_the_session_back();
                Ok(())
            } else {
                Err(VolumeError::ConnectionTimeout("the server didn't answer".to_owned()))
            }
        })
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Fut<'a, Result<Vec<FileEntry>, VolumeError>> {
        Box::pin(async move {
            self.listings.lock_ignore_poison().push(path.to_path_buf());
            let armed_here = {
                let mut armed = self.drop_at_listing_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            self.gate()?;
            if self.refuse_listing_of.lock_ignore_poison().as_deref() == Some(path) {
                let path = path.display().to_string();
                return Err(VolumeError::PermissionDenied {
                    path,
                    raw_os_error: None,
                });
            }
            self.inner.list_directory(path, on_progress).await
        })
    }
    fn get_metadata<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<FileEntry, VolumeError>> {
        Box::pin(async move {
            let armed_here = {
                let mut armed = self.drop_at_stat_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            self.gate()?;
            self.inner.get_metadata(path).await
        })
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Fut<'a, bool> {
        Box::pin(async move { self.gate().is_ok() && self.inner.exists(path).await })
    }
    fn is_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<bool, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.is_directory(path).await
        })
    }
    fn create_directory<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory(path).await
        })
    }
    fn create_directory_all<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<DirectoryCreation, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_directory_all(path).await
        })
    }
    fn create_file<'a>(&'a self, path: &'a Path, content: &'a [u8]) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.create_file(path, content).await
        })
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            let refused_here = self.up.load(Ordering::SeqCst) && {
                let mut armed = self.refuse_deletes_of.lock_ignore_poison();
                match armed.as_mut() {
                    Some((held, left)) if held == path && *left > 0 => {
                        *left -= 1;
                        true
                    }
                    _ => false,
                }
            };
            if refused_here {
                return Err(VolumeError::IoError {
                    message: "sharing violation".to_string(),
                    raw_os_error: None,
                });
            }
            self.under_path_drops(
                path,
                &self.drop_at_delete_of,
                Some(&self.stale_error_at_delete_of),
                || self.inner.delete(path),
            )
            .await
        })
    }
    fn rename<'a>(&'a self, from: &'a Path, to: &'a Path, force: bool) -> Fut<'a, Result<(), VolumeError>> {
        Box::pin(async move {
            let n = self.renames.fetch_add(1, Ordering::SeqCst);
            let armed = *self.drop_at_rename.lock_ignore_poison();
            match armed {
                Some((at, RenameDrop::BeforeTheServerSeesIt)) if at == n => self.go_away(),
                Some((at, RenameDrop::AfterTheServerPerformsIt)) if at == n => {
                    self.gate()?;
                    self.inner.rename(from, to, force).await?;
                    self.go_away();
                }
                _ => {}
            }
            let stale = *self.stale_error_at_rename.lock_ignore_poison();
            if let Some((at, side)) = stale
                && at == n
            {
                self.gate()?;
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.reconnected_elsewhere();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned()));
            }
            if let Some(side) = armed_for(&self.drop_at_rename_of, from) {
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.gate()?;
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.gate()?;
            }
            if let Some(side) = armed_for(&self.stale_error_at_rename_of, from) {
                self.gate()?;
                if side == RenameDrop::AfterTheServerPerformsIt {
                    self.inner.rename(from, to, force).await?;
                }
                self.go_away();
                self.reconnected_elsewhere();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned()));
            }
            self.under_path_drops(to, &self.drop_at_rename_onto, None, || {
                self.inner.rename(from, to, force)
            })
            .await
        })
    }
    fn supports_atomic_replace_rename(&self) -> bool {
        self.replaces_atomically.load(Ordering::SeqCst)
    }
    fn supports_copy_within(&self) -> bool {
        self.copies_on_server.load(Ordering::SeqCst)
    }
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<u64, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.server_copies.fetch_add(1, Ordering::SeqCst);
            let stream = self.inner.open_read_stream(from).await?;
            let size = stream.total_size().known().expect("stored files have a known length");
            self.inner
                .write_from_stream(
                    to,
                    WriteMode::CreateOrReplace,
                    StreamLength::Known(size),
                    stream,
                    &|progress| on_progress(progress.bytes_written, size),
                )
                .await
                .map(|written| written.bytes)
        })
    }
    fn get_space_info<'a>(&'a self) -> Fut<'a, Result<SpaceInfo, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.get_space_info().await
        })
    }
    fn open_read_stream<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<Box<dyn VolumeReadStream>, VolumeError>> {
        Box::pin(async move {
            let n = self.opens.fetch_add(1, Ordering::SeqCst);
            if *self.drop_at_open.lock_ignore_poison() == Some(n) {
                self.go_away();
            }
            self.gate()?;
            self.inner.open_read_stream(path).await
        })
    }
    fn scan_for_copy<'a>(&'a self, path: &'a Path) -> Fut<'a, Result<CopyScanResult, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.scan_for_copy(path).await
        })
    }
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Fut<'a, Result<BatchScanResult, VolumeError>> {
        // The shared walk over this volume's own stat and listing, so a drop
        // armed at a listing lands inside the walk, as on a real share.
        scan_walk::scan_trees(self, paths, boundary)
    }
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        self.inner.create_directory_errors_on_existing_dir()
    }
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Fut<'a, Result<Vec<ScanConflict>, VolumeError>> {
        Box::pin(async move {
            self.gate()?;
            self.inner.scan_for_conflicts(source_items, dest_path).await
        })
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Fut<'a, Result<StreamWritten, VolumeError>> {
        Box::pin(async move {
            let n = self.writes.fetch_add(1, Ordering::SeqCst);
            self.written.lock_ignore_poison().push(dest.to_path_buf());
            if *self.drop_at_write.lock_ignore_poison() == Some(n) {
                self.go_away();
            }
            self.gate()?;
            let written = self
                .inner
                .write_from_stream(dest, mode, length, stream, on_progress)
                .await?;
            if *self.drop_after_write.lock_ignore_poison() == Some(n) {
                if self.drop_cuts_the_stamp.load(Ordering::SeqCst) {
                    let servers_own = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.as_secs());
                    self.inner.set_modified_at(dest, Some(servers_own));
                }
                self.go_away();
                return Err(VolumeError::DeviceDisconnected("the session is down".to_owned()));
            }
            Ok(written)
        })
    }
    fn write_is_single_shot<'a>(&'a self, _length: StreamLength) -> Fut<'a, bool> {
        Box::pin(async move { self.single_shot.load(Ordering::SeqCst) })
    }
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Fut<'a, MetadataReport> {
        Box::pin(async move {
            let armed_here = {
                let mut armed = self.drop_at_apply_metadata_of.lock_ignore_poison();
                armed.as_deref() == Some(path) && armed.take().is_some()
            };
            if armed_here {
                self.go_away();
            }
            let refused_here = self.refuse_metadata_of.lock_ignore_poison().as_deref() == Some(path);
            if self.gate().is_err() || refused_here {
                return MetadataReport::uniform(carried, modified, ClassOutcome::Refused);
            }
            self.inner.apply_metadata(path, carried, modified).await
        })
    }
}
