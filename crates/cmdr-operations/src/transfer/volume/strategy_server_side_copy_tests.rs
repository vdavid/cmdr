//! Letting the server do a copy that never leaves it.
//!
//! Duplicating a file inside one remote volume otherwise pulls every byte down
//! the link and pushes it straight back up. A backend whose protocol can copy
//! for itself (SFTP's `copy-data@openssh.com`, S3's `CopyObject`) answers
//! `Volume::copy_on_server`, and `stream_pipe_file` asks it before reaching for
//! a stream.
//!
//! What the cells hold it to, and each one is a way to lose data or time if it
//! drifts:
//!
//! - **Which sources it copies from is the backend's call**: the default (and
//!   SFTP, through `copy_within`) answers only for the same volume instance,
//!   while an object store copies across the places of one account. A copy
//!   across two servers would copy inside the wrong one.
//! - A backend that answers `NotSupported` — including one whose SERVER simply
//!   lacks the extension — falls back to streaming, with the same bytes landing.
//! - The bytes are staged exactly as a streamed write is, so a destination that
//!   already existed is never left holding a partial, except on a
//!   whole-publishing destination, which publishes the copy whole and takes it
//!   at the final name.
//! - A pause lands at the backend's checkpoint.

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::super::faulty_volume::forward_volume_methods;
use super::super::transfer_error::PathedVolumeError;
use super::test_support::make_state;
use super::*;
use crate::state::OperationIntent;
use crate::testing::TestOperations;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::WriteMode;
use cmdr_fs::volume::{
    InMemoryVolume, ListingProgress, ServerCopyProgress, StreamLength, StreamWriteProgress, StreamWritten, Volume,
    VolumeError, VolumeReadStream,
};

/// A volume that can copy inside itself, and counts who asked.
///
/// The copy is done by piping the INNER volume's own stream into its own write,
/// which is what a real server-side copy looks like from out here: the bytes land
/// and this layer never sees them. The counters are what tell the two paths
/// apart — `copies` says the fast path was offered the work, and `streamed` says
/// the ordinary path did it.
struct ServerCopyVolume {
    inner: Arc<InMemoryVolume>,
    /// Whether the "server" has the extension. `false` reproduces a NAS whose
    /// firmware never learned `copy-data`.
    can_copy_itself: bool,
    copies: AtomicUsize,
    streamed: AtomicUsize,
}

impl ServerCopyVolume {
    fn new(name: &str, can_copy_itself: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new(name)),
            can_copy_itself,
            copies: AtomicUsize::new(0),
            streamed: AtomicUsize::new(0),
        })
    }

    fn copies(&self) -> usize {
        self.copies.load(Ordering::SeqCst)
    }

    fn streamed(&self) -> usize {
        self.streamed.load(Ordering::SeqCst)
    }
}

impl Volume for ServerCopyVolume {
    forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        exists,
        get_space_info,
        local_path,
        supports_streaming,
        supports_export,
        supports_local_fs_access,
        operations_are_local,
        max_concurrent_ops,
        create_directory_errors_on_existing_dir,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        supports_unknown_length_writes, write_is_single_shot,
        list_directory,
        get_metadata,
        is_directory,
        create_file,
        create_directory,
        delete,
        rename,
        open_read_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.streamed.fetch_add(1, Ordering::SeqCst);
        self.inner.write_from_stream(dest, mode, length, stream, on_progress)
    }

    fn supports_copy_within(&self) -> bool {
        true
    }

    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.copies.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if !self.can_copy_itself {
                return Err(VolumeError::NotSupported);
            }
            let stream = self.inner.open_read_stream(from).await?;
            let size = stream.total_size().known().expect("stored files have a known length");
            // A conforming backend reports its own stop as `Cancelled`, the way
            // the chunk loop of a real `copy-data` does.
            if on_progress(0, size).is_break() {
                return Err(VolumeError::Cancelled(to.display().to_string().into()));
            }
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
}

/// Copies `source` to `dest` on the given volumes, the way the merge walker and
/// the top-level driver both do.
async fn pipe(
    source_volume: &Arc<dyn Volume>,
    dest_volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    from: &str,
    to: &str,
) -> Result<u64, PathedVolumeError> {
    stream_pipe_file(
        source_volume,
        Path::new(from),
        SourceFileFacts::default(),
        dest_volume,
        Path::new(to),
        state,
        &|_, _| ControlFlow::Continue(()),
        WriteStaging::Stage,
    )
    .await
}

/// Two paths on one server: the server copies, and no byte reaches Cmdr.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_inside_one_volume_lets_the_server_do_it() {
    let engine = TestOperations::new();
    let volume = ServerCopyVolume::new("nas", true);
    volume
        .inner
        .create_file(Path::new("holiday.mov"), b"a whole lot of bytes")
        .await
        .unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state(&engine);

    let bytes = pipe(&as_volume, &as_volume, &state, "holiday.mov", "holiday copy.mov")
        .await
        .unwrap();

    assert_eq!(bytes, 20);
    assert_eq!(volume.copies(), 1, "the fast path was taken");
    assert_eq!(
        volume.streamed(),
        0,
        "❗ and nothing was streamed through Cmdr: that IS the whole win"
    );
    assert!(volume.inner.exists(Path::new("holiday copy.mov")).await);
}

/// ❗ Two different volumes are never eligible, whatever either can do alone.
///
/// A `copy_within` here would ask the destination to copy a path that only exists
/// on the source, which on a server holding a same-named file is not a failure —
/// it is the wrong file, copied silently.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_between_two_volumes_never_asks_for_a_server_side_one() {
    let engine = TestOperations::new();
    let source = ServerCopyVolume::new("nas-a", true);
    let dest = ServerCopyVolume::new("nas-b", true);
    source
        .inner
        .create_file(Path::new("report.pdf"), b"pages")
        .await
        .unwrap();
    let source_volume: Arc<dyn Volume> = source.clone();
    let dest_volume: Arc<dyn Volume> = dest.clone();
    let state = make_state(&engine);

    let bytes = pipe(&source_volume, &dest_volume, &state, "report.pdf", "report.pdf")
        .await
        .unwrap();

    assert_eq!(bytes, 5);
    assert_eq!(source.copies() + dest.copies(), 0, "neither side was asked");
    assert_eq!(dest.streamed(), 1, "the bytes went the ordinary way");
    assert!(dest.inner.exists(Path::new("report.pdf")).await);
}

/// A server that lacks the extension still copies, the ordinary way.
///
/// ⚠️ This is the common case on the NAS firmware people actually run, so the
/// fallback is the path that has to be boring: same bytes, same destination, one
/// wasted question.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_cannot_copy_for_itself_falls_back_to_streaming() {
    let engine = TestOperations::new();
    let volume = ServerCopyVolume::new("old-nas", false);
    volume.inner.create_file(Path::new("notes.txt"), b"kept").await.unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state(&engine);

    let bytes = pipe(&as_volume, &as_volume, &state, "notes.txt", "notes copy.txt")
        .await
        .unwrap();

    assert_eq!(bytes, 4);
    assert_eq!(volume.copies(), 1, "it was asked");
    assert_eq!(volume.streamed(), 1, "said no, and the bytes went the ordinary way");
    assert!(volume.inner.exists(Path::new("notes copy.txt")).await);
}

/// ❗ A copy onto a name that already holds a file stages, exactly as a streamed
/// write does.
///
/// The destination genuinely holds a byte-incomplete file while a server-side
/// copy runs, so nothing here may take the single-shot exemption: the bytes land
/// on a temp and take the user's filename at the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_side_copy_lands_on_a_temp_and_leaves_none_behind() {
    let engine = TestOperations::new();
    let volume = ServerCopyVolume::new("nas", true);
    volume.inner.create_file(Path::new("a.txt"), b"fresh").await.unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state(&engine);

    pipe(&as_volume, &as_volume, &state, "a.txt", "b.txt").await.unwrap();

    let entries = volume.inner.list_directory(Path::new(""), None).await.unwrap();
    let leftovers: Vec<&FileEntry> = entries.iter().filter(|e| e.name.contains(".cmdr-tmp-")).collect();
    assert!(
        leftovers.is_empty(),
        "the staging temp took the final name and left nothing: {leftovers:?}"
    );
    assert!(entries.iter().any(|e| e.name == "b.txt"));
}

/// Cancel reaches a server-side copy through the same callback a streamed one
/// uses, and the partial goes with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_server_side_copy_leaves_no_destination_behind() {
    let engine = TestOperations::new();
    let volume = ServerCopyVolume::new("nas", true);
    volume
        .inner
        .create_file(Path::new("big.bin"), b"0123456789")
        .await
        .unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state(&engine);
    state.intent().set_for_test(OperationIntent::Stopped);

    let outcome = stream_pipe_file(
        &as_volume,
        Path::new("big.bin"),
        SourceFileFacts::default(),
        &as_volume,
        Path::new("big copy.bin"),
        &state,
        &|_, _| ControlFlow::Break(()),
        WriteStaging::Stage,
    )
    .await;

    assert!(
        matches!(
            outcome,
            Err(PathedVolumeError {
                error: VolumeError::Cancelled(_),
                ..
            })
        ),
        "a cancel is a cancel, not a fallback to the slow path: {outcome:?}"
    );
    assert!(
        !volume.inner.exists(Path::new("big copy.bin")).await,
        "and nothing wearing the user's chosen name survives it"
    );
}

/// An object store's place: it publishes every write whole, and it copies on
/// the server from any place of its own account, not only from itself (two S3
/// buckets under one key). Records each copy it was handed, and parks at the
/// progress hook's checkpoint the way a multipart copy does between parts.
struct StorePlace {
    inner: Arc<InMemoryVolume>,
    account: &'static str,
    /// `(to, mode)` for every `copy_on_server` that reached this place.
    copies: std::sync::Mutex<Vec<(std::path::PathBuf, WriteMode)>>,
    streamed: AtomicUsize,
    /// Set once the copy has passed its first checkpoint.
    checkpointed: std::sync::atomic::AtomicBool,
}

impl StorePlace {
    fn new(name: &str, account: &'static str) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new(name).with_whole_publish()),
            account,
            copies: std::sync::Mutex::new(Vec::new()),
            streamed: AtomicUsize::new(0),
            checkpointed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn copies(&self) -> Vec<(std::path::PathBuf, WriteMode)> {
        self.copies.lock().expect("test lock").clone()
    }

    /// `source` as a place of this account, the way `S3Volume` matches one.
    fn same_account_peer<'s>(&self, source: &'s dyn Volume) -> Option<&'s StorePlace> {
        source
            .as_any()
            .downcast_ref::<StorePlace>()
            .filter(|peer| peer.account == self.account)
    }
}

impl Volume for StorePlace {
    forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        exists,
        get_space_info,
        local_path,
        supports_streaming,
        supports_export,
        supports_local_fs_access,
        operations_are_local,
        max_concurrent_ops,
        create_directory_errors_on_existing_dir,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        supports_unknown_length_writes, write_is_single_shot, publishes_writes_whole,
        list_directory,
        get_metadata,
        is_directory,
        create_file,
        create_directory,
        delete,
        rename,
        open_read_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        self.streamed.fetch_add(1, Ordering::SeqCst);
        self.inner.write_from_stream(dest, mode, length, stream, on_progress)
    }

    fn may_copy_on_server_from(&self, source: &dyn Volume) -> bool {
        self.same_account_peer(source).is_some()
    }

    fn copy_on_server<'a>(
        &'a self,
        source: &'a dyn Volume,
        from: &'a Path,
        to: &'a Path,
        mode: WriteMode,
        progress: &'a dyn ServerCopyProgress,
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(peer) = self.same_account_peer(source) else {
                return Err(VolumeError::NotSupported);
            };
            self.copies.lock().expect("test lock").push((to.to_path_buf(), mode));
            if progress.checkpoint().await.is_break() {
                return Err(VolumeError::Cancelled(to.display().to_string().into()));
            }
            self.checkpointed.store(true, Ordering::SeqCst);
            let stream = peer.inner.open_read_stream(from).await?;
            let size = stream.total_size().known().expect("stored files have a known length");
            self.inner
                .write_from_stream(to, mode, StreamLength::Known(size), stream, &|written| {
                    progress.advanced(written.bytes_written, size)
                })
                .await
                .map(|written| written.bytes)
        })
    }
}

/// ❗ Two places of one account are two volume instances, and the server can
/// still copy between them: the destination decides from the source's
/// identity, so no byte crosses the Mac.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_between_two_places_of_one_account_lets_the_server_do_it() {
    let engine = TestOperations::new();
    let source = StorePlace::new("photos", "acct-1");
    let dest = StorePlace::new("archive", "acct-1");
    source
        .inner
        .create_file(Path::new("a.jpg"), b"jpeg bytes")
        .await
        .unwrap();
    let source_volume: Arc<dyn Volume> = source.clone();
    let dest_volume: Arc<dyn Volume> = dest.clone();
    let state = make_state(&engine);

    let bytes = pipe(&source_volume, &dest_volume, &state, "a.jpg", "a.jpg")
        .await
        .unwrap();

    assert_eq!(bytes, 10);
    assert_eq!(dest.copies().len(), 1, "the server was asked");
    assert_eq!(dest.streamed.load(Ordering::SeqCst), 0, "and nothing streamed");
    assert!(dest.inner.exists(Path::new("a.jpg")).await);
}

/// Another account's place is another server's namespace: streamed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_from_another_account_streams() {
    let engine = TestOperations::new();
    let source = StorePlace::new("theirs", "acct-1");
    let dest = StorePlace::new("ours", "acct-2");
    source.inner.create_file(Path::new("a.jpg"), b"jpeg").await.unwrap();
    let source_volume: Arc<dyn Volume> = source.clone();
    let dest_volume: Arc<dyn Volume> = dest.clone();
    let state = make_state(&engine);

    pipe(&source_volume, &dest_volume, &state, "a.jpg", "a.jpg")
        .await
        .unwrap();

    assert!(dest.copies().is_empty());
    assert_eq!(dest.streamed.load(Ordering::SeqCst), 1);
}

/// ❗ A whole-publishing destination's server-side copy goes straight to the
/// final name, refusing a taken one (`CreateNew`): a temp would cost a landing
/// rename, which on an object store is a second full copy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_whole_publishing_destination_copies_straight_to_the_final_name() {
    let engine = TestOperations::new();
    let place = StorePlace::new("bucket", "acct-1");
    place.inner.create_file(Path::new("a.txt"), b"fresh").await.unwrap();
    let as_volume: Arc<dyn Volume> = place.clone();
    let state = make_state(&engine);

    pipe(&as_volume, &as_volume, &state, "a.txt", "b.txt").await.unwrap();

    assert_eq!(
        place.copies(),
        vec![(std::path::PathBuf::from("b.txt"), WriteMode::CreateNew)],
        "the final name, never a .cmdr-tmp, and no-overwrite"
    );
}

/// ❗ A pause lands inside a server-side copy, at the checkpoint the backend
/// asks before each piece, and the copy goes on once resumed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_paused_server_side_copy_parks_at_its_checkpoint() {
    let engine = TestOperations::new();
    let place = StorePlace::new("bucket", "acct-1");
    place.inner.create_file(Path::new("a.bin"), b"0123").await.unwrap();
    let as_volume: Arc<dyn Volume> = place.clone();
    let state = make_state(&engine);
    state.pause_gate.pause();

    let task = {
        let as_volume = Arc::clone(&as_volume);
        let state = Arc::clone(&state);
        tokio::spawn(async move { pipe(&as_volume, &as_volume, &state, "a.bin", "b.bin").await })
    };
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the copy to reach its checkpoint", || {
        !place.copies().is_empty()
    })
    .await;
    assert!(
        !place.checkpointed.load(Ordering::SeqCst),
        "a paused copy waits at its checkpoint"
    );

    state.pause_gate.resume();
    task.await.unwrap().unwrap();
    assert!(place.inner.exists(Path::new("b.bin")).await);
}

/// An ordinary destination (a local disk, an SMB share) that counts every
/// server-side copy it's offered and every delete it's asked for.
///
/// The deletes are the tell: a server-copy attempt that stages and then hears
/// `NotSupported` deletes the temp it never wrote, once per file.
struct PlainDest {
    inner: Arc<InMemoryVolume>,
    offered: AtomicUsize,
    deletes: AtomicUsize,
}

impl PlainDest {
    fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new(name)),
            offered: AtomicUsize::new(0),
            deletes: AtomicUsize::new(0),
        })
    }
}

impl Volume for PlainDest {
    forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        exists,
        get_space_info,
        local_path,
        supports_streaming,
        supports_export,
        supports_local_fs_access,
        operations_are_local,
        max_concurrent_ops,
        create_directory_errors_on_existing_dir,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        supports_unknown_length_writes, write_is_single_shot,
        list_directory,
        get_metadata,
        is_directory,
        create_file,
        create_directory,
        rename,
        open_read_stream,
        write_from_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.deletes.fetch_add(1, Ordering::SeqCst);
        self.inner.delete(path)
    }

    fn copy_on_server<'a>(
        &'a self,
        source: &'a dyn Volume,
        from: &'a Path,
        to: &'a Path,
        mode: WriteMode,
        progress: &'a dyn ServerCopyProgress,
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.offered.fetch_add(1, Ordering::SeqCst);
        self.inner.copy_on_server(source, from, to, mode, progress)
    }
}

/// ❗ ERR-PTRGY: a destination that can't copy from this source is never
/// offered the copy, so nothing is staged for it and nothing is deleted after.
///
/// An SMB-to-local move used to stage a temp, record it in the persisted
/// ledger, hear `NotSupported`, and delete the temp it never wrote, for every
/// file. One of those per-file deletes is where a user's move sat for 100 s.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_cannot_copy_from_the_source_stages_nothing_for_it() {
    let engine = TestOperations::new();
    let source = Arc::new(InMemoryVolume::new("nas"));
    source.create_file(Path::new("scan.pdf"), b"pages").await.unwrap();
    let dest = PlainDest::new("mac");
    let source_volume: Arc<dyn Volume> = source;
    let dest_volume: Arc<dyn Volume> = dest.clone();
    let state = make_state(&engine);

    let bytes = pipe(&source_volume, &dest_volume, &state, "scan.pdf", "scan.pdf")
        .await
        .unwrap();

    assert_eq!(bytes, 5);
    assert_eq!(dest.offered.load(Ordering::SeqCst), 0, "the copy was never offered");
    assert_eq!(
        dest.deletes.load(Ordering::SeqCst),
        0,
        "and no temp was staged for it, so none was deleted"
    );
    assert!(dest.inner.exists(Path::new("scan.pdf")).await);
}

/// A server-side copy reads as itself in the transfer probe, never as
/// `streaming 0/0`: a dump of a copy stuck there has to say where it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_side_copy_names_its_own_phase_in_the_probe() {
    use super::super::super::transfer_probe::{self, TaskRole, TaskRow};

    let engine = TestOperations::new();

    let place = StorePlace::new("bucket", "acct-1");
    place.inner.create_file(Path::new("a.bin"), b"0123").await.unwrap();
    let as_volume: Arc<dyn Volume> = place.clone();
    let guard = engine.register_state("server-copy-phase", make_state(&engine));
    let state = Arc::clone(guard.state());
    let op_probe = transfer_probe::register_operation(
        guard.id().as_str(),
        1,
        1,
        transfer_probe::TransferEnds::none(),
        Arc::clone(&state),
        Arc::new(crate::event_sinks::CollectorEventSink::new()),
    );
    let task_probe = op_probe
        .probe()
        .begin_task(TaskRow::source(0), TaskRole::File, "a.bin", "b.bin");
    // Parked at the copy's checkpoint, so the dump is read mid-copy.
    state.pause_gate.pause();

    let task = {
        let as_volume = Arc::clone(&as_volume);
        let state = Arc::clone(&state);
        let handle = task_probe.probe();
        tokio::spawn(transfer_probe::CURRENT_TASK_PROBE.scope(handle, async move {
            pipe(&as_volume, &as_volume, &state, "a.bin", "b.bin").await
        }))
    };
    cmdr_fs::testing::wait_until_async(Duration::from_secs(5), "the copy to reach its checkpoint", || {
        !place.copies().is_empty()
    })
    .await;

    let dump = op_probe.probe().render_dump("test");
    assert!(dump.contains("copying-on-server"), "{dump}");
    state.pause_gate.resume();
    task.await.unwrap().unwrap();
}

/// The server copied the file, performed the landing rename, and the drop
/// swallowed its reply: once the share is back, the landing is settled like a
/// streamed one's, and the file lands ONCE, with no second copy and no
/// stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_side_copy_whose_landing_reply_was_lost_lands_once() {
    use crate::outage_slot::OutagePolicy;
    use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
    const CADENCE: &[Duration] = &[Duration::from_millis(10)];

    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("nas"));
    inner
        .create_file(Path::new("/holiday.mov"), b"a whole lot of bytes")
        .await
        .unwrap();
    let nas = OutageVolume::wrap(Arc::clone(&inner), "server-copy-landing-lost");
    nas.copy_on_the_server();
    nas.drop_at_rename_onto(Path::new("/holiday copy.mov"), RenameDrop::AfterTheServerPerformsIt);
    let volume: Arc<dyn Volume> = Arc::clone(&nas) as Arc<dyn Volume>;
    let state = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(30),
                cadence: CADENCE,
                poll_every: Duration::from_millis(5),
                attempt_timeout: Duration::from_secs(1),
            }),
    );
    let watched = Arc::clone(&state);
    let share = Arc::clone(&nas);
    tokio::spawn(async move {
        cmdr_fs::testing::wait_until_async(Duration::from_secs(10), "the copy to wait for the share", || {
            watched.outage.waiting_for().is_some()
        })
        .await;
        share.become_reachable();
    });

    let bytes = pipe(&volume, &volume, &state, "/holiday.mov", "/holiday copy.mov")
        .await
        .expect("a landing the server performed is the copy's own file");

    assert_eq!(bytes, 20);
    assert!(nas.reconnect_attempts() > 0, "the share dropped at the landing");
    assert_eq!(nas.server_copies(), 1, "the server copied it once");
    assert_eq!(nas.writes(), 0, "and nothing streamed it");
    let names: Vec<String> = inner
        .list_directory(Path::new("/"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(
        !names.iter().any(|name| name.contains(".cmdr-tmp-")),
        "no temp left: {names:?}"
    );
    assert_eq!(
        inner.get_metadata(Path::new("/holiday copy.mov")).await.unwrap().size,
        Some(20)
    );
}

/// A `ListingProgress` import keeps the forwarding macro's signatures nameable.
const _: Option<ListingProgress> = None;
