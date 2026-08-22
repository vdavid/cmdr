//! Letting the server do a copy that never leaves it.
//!
//! Duplicating a file inside one remote volume otherwise pulls every byte down
//! the link and pushes it straight back up. A backend whose protocol can copy
//! for itself (SFTP's `copy-data@openssh.com`) answers `Volume::copy_within`,
//! and `stream_pipe_file` asks it before reaching for a stream.
//!
//! Three things the cells hold it to, and each one is a way to lose data or time
//! if it drifts:
//!
//! - It is asked **only when both sides are the same volume instance**. A
//!   `copy_within` across two servers would copy inside the wrong one.
//! - A backend that answers `NotSupported` — including one whose SERVER simply
//!   lacks the extension — falls back to streaming, with the same bytes landing.
//! - The bytes are staged exactly as a streamed write is, so a destination that
//!   already existed is never left holding a partial.

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::faulty_volume::forward_volume_methods;
use super::test_support::make_state;
use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, Volume, VolumeError, VolumeReadStream};
use crate::file_system::write_operations::state::OperationIntent;

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
        write_is_single_shot,
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
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.streamed.fetch_add(1, Ordering::SeqCst);
        self.inner.write_from_stream(dest, size, stream, on_progress)
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
            let size = stream.total_size();
            // A conforming backend reports its own stop as `Cancelled`, the way
            // the chunk loop of a real `copy-data` does.
            if on_progress(0, size).is_break() {
                return Err(VolumeError::Cancelled(to.display().to_string()));
            }
            self.inner.write_from_stream(to, size, stream, on_progress).await
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
) -> Result<u64, VolumeError> {
    stream_pipe_file(
        source_volume,
        Path::new(from),
        None,
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
    let volume = ServerCopyVolume::new("nas", true);
    volume
        .inner
        .create_file(Path::new("holiday.mov"), b"a whole lot of bytes")
        .await
        .unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state();

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
    let source = ServerCopyVolume::new("nas-a", true);
    let dest = ServerCopyVolume::new("nas-b", true);
    source
        .inner
        .create_file(Path::new("report.pdf"), b"pages")
        .await
        .unwrap();
    let source_volume: Arc<dyn Volume> = source.clone();
    let dest_volume: Arc<dyn Volume> = dest.clone();
    let state = make_state();

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
    let volume = ServerCopyVolume::new("old-nas", false);
    volume.inner.create_file(Path::new("notes.txt"), b"kept").await.unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state();

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
    let volume = ServerCopyVolume::new("nas", true);
    volume.inner.create_file(Path::new("a.txt"), b"fresh").await.unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state();

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
    let volume = ServerCopyVolume::new("nas", true);
    volume
        .inner
        .create_file(Path::new("big.bin"), b"0123456789")
        .await
        .unwrap();
    let as_volume: Arc<dyn Volume> = volume.clone();
    let state = make_state();
    state.intent.store(OperationIntent::Stopped as u8, Ordering::SeqCst);

    let outcome = stream_pipe_file(
        &as_volume,
        Path::new("big.bin"),
        None,
        &as_volume,
        Path::new("big copy.bin"),
        &state,
        &|_, _| ControlFlow::Break(()),
        WriteStaging::Stage,
    )
    .await;

    assert!(
        matches!(outcome, Err(VolumeError::Cancelled(_))),
        "a cancel is a cancel, not a fallback to the slow path: {outcome:?}"
    );
    assert!(
        !volume.inner.exists(Path::new("big copy.bin")).await,
        "and nothing wearing the user's chosen name survives it"
    );
}

/// A `ListingProgress` import keeps the forwarding macro's signatures nameable.
const _: Option<ListingProgress> = None;
