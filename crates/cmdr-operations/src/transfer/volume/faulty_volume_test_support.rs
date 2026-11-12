//! One wrapper for the fault class this whole area is about: a volume that
//! answers WRONG, or refuses to answer, on the Nth call to a named operation.
//!
//! Before this existed, "wrong metadata" wasn't a fault you could inject the way
//! an I/O error is — every suite that needed one hand-rolled a forwarder, so the
//! test that would have caught a directory being streamed as a file was 40 lines
//! of `self.inner.…` away from being written, and nobody wrote it.
//!
//! Two pieces:
//!
//! - [`forward_volume_methods!`], the boilerplate remover. A `Volume` double
//!   names the methods it forwards and hand-writes only what it lies about, so
//!   the lie is the whole diff.
//! - [`FaultyVolume`], the general double: wrap any volume, arm a fault on an
//!   operation, and the Nth call to it fails with the error you chose.
//!
//! ❌ Not for partial-destination state. `InMemoryVolume` buffers a whole file
//! and creates it at the end, so a `FaultyVolume` over one can't show a
//! half-written destination; those cells use `LocalPosixVolume`
//! (`copy_wedge_test_support.rs::IncrementalDest` says the same thing).
//!
//! ❌ Not a stream double either. `GatedChunkStream` and `SlowChunkedStream`
//! control WHEN bytes arrive, which is a different axis; they stay where they
//! are.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{
    DirectoryCreation, ListingProgress, StreamLength, StreamWriteProgress, StreamWritten, Volume, VolumeError,
    VolumeReadStream,
};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use tokio_util::sync::CancellationToken;

/// Generates plain forwarding bodies for the named `Volume` methods, so a double
/// spends its lines on the ONE method it lies about instead of on 30 it doesn't.
///
/// `$inner` is the field holding the wrapped volume. ❌ `as_any` is never
/// forwardable (it must return the wrapper itself), and a method left off the
/// list silently takes the TRAIT DEFAULT rather than the inner volume's
/// override — which is the right behavior to want sometimes and a trap the rest
/// of the time, so list every method whose answer the code under test reads.
///
/// ```text
/// impl Volume for LyingDest {
///     forward_volume_methods!(inner => name, root, list_directory, get_metadata, exists, delete);
///     fn as_any(&self) -> &dyn std::any::Any { self }
///     fn is_directory<'a>(&'a self, path: &'a Path) -> … { /* the lie */ }
/// }
/// ```
#[macro_export]
macro_rules! forward_volume_methods {
    ($inner:ident => $($method:ident),* $(,)?) => {
        $( $crate::forward_volume_methods!(@one $inner, $method); )*
    };

    (@one $inner:ident, name) => {
        fn name(&self) -> &str {
            self.$inner.name()
        }
    };
    (@one $inner:ident, root) => {
        fn root(&self) -> &::std::path::Path {
            self.$inner.root()
        }
    };
    (@one $inner:ident, lane_key) => {
        fn lane_key(&self) -> ::cmdr_fs::volume::LaneKey {
            self.$inner.lane_key()
        }
    };
    (@one $inner:ident, connection_state) => {
        fn connection_state(&self) -> Option<::cmdr_fs::volume::ConnectionState> {
            self.$inner.connection_state()
        }
    };
    (@one $inner:ident, connection_generation) => {
        fn connection_generation(&self) -> Option<u64> {
            self.$inner.connection_generation()
        }
    };
    (@one $inner:ident, attempt_reconnect) => {
        fn attempt_reconnect<'a>(&'a self) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.attempt_reconnect()
        }
    };
    (@one $inner:ident, list_directory) => {
        fn list_directory<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            on_progress: Option<&'a (dyn Fn(::cmdr_fs::volume::ListingProgress) + Sync)>,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Vec<::cmdr_fs::entry::FileEntry>, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.list_directory(path, on_progress)
        }
    };
    (@one $inner:ident, get_metadata) => {
        fn get_metadata<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::entry::FileEntry, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.get_metadata(path)
        }
    };
    (@one $inner:ident, exists) => {
        fn exists<'a>(&'a self, path: &'a ::std::path::Path) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = bool> + Send + 'a>> {
            self.$inner.exists(path)
        }
    };
    (@one $inner:ident, is_directory) => {
        fn is_directory<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<bool, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.is_directory(path)
        }
    };
    (@one $inner:ident, create_file) => {
        fn create_file<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            content: &'a [u8],
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_file(path, content)
        }
    };
    (@one $inner:ident, create_directory) => {
        fn create_directory<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_directory(path)
        }
    };
    (@one $inner:ident, create_directory_all) => {
        fn create_directory_all<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::volume::DirectoryCreation, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_directory_all(path)
        }
    };
    (@one $inner:ident, apply_metadata) => {
        fn apply_metadata<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            carried: &'a ::cmdr_fs::metadata::CarriedMetadata,
            modified: ::std::option::Option<::std::time::SystemTime>,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = ::cmdr_fs::metadata::MetadataReport> + Send + 'a>> {
            self.$inner.apply_metadata(path, carried, modified)
        }
    };
    (@one $inner:ident, metadata_caps) => {
        fn metadata_caps(&self) -> ::cmdr_fs::metadata::MetadataCaps {
            self.$inner.metadata_caps()
        }
    };
    (@one $inner:ident, folder_metadata) => {
        fn folder_metadata<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            wanted: ::cmdr_fs::metadata::MetadataClasses,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = ::cmdr_fs::metadata::CarriedMetadata> + Send + 'a>> {
            self.$inner.folder_metadata(path, wanted)
        }
    };
    (@one $inner:ident, clear_blockers) => {
        fn clear_blockers<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = ::cmdr_fs::metadata::Unblock> + Send + 'a>> {
            self.$inner.clear_blockers(path)
        }
    };
    (@one $inner:ident, delete) => {
        fn delete<'a>(&'a self, path: &'a ::std::path::Path) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.delete(path)
        }
    };
    (@one $inner:ident, rename) => {
        fn rename<'a>(
            &'a self,
            from: &'a ::std::path::Path,
            to: &'a ::std::path::Path,
            force: bool,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.rename(from, to, force)
        }
    };
    (@one $inner:ident, get_space_info) => {
        fn get_space_info<'a>(&'a self) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::volume::SpaceInfo, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.get_space_info()
        }
    };
    (@one $inner:ident, local_path) => {
        fn local_path(&self) -> Option<::std::path::PathBuf> {
            self.$inner.local_path()
        }
    };
    (@one $inner:ident, supports_streaming) => {
        fn supports_streaming(&self) -> bool {
            self.$inner.supports_streaming()
        }
    };
    (@one $inner:ident, supports_unknown_length_writes) => {
        fn supports_unknown_length_writes(&self) -> bool {
            self.$inner.supports_unknown_length_writes()
        }
    };
    (@one $inner:ident, supports_export) => {
        fn supports_export(&self) -> bool {
            self.$inner.supports_export()
        }
    };
    (@one $inner:ident, supports_local_fs_access) => {
        fn supports_local_fs_access(&self) -> bool {
            self.$inner.supports_local_fs_access()
        }
    };
    (@one $inner:ident, operations_are_local) => {
        fn operations_are_local(&self) -> bool {
            self.$inner.operations_are_local()
        }
    };
    (@one $inner:ident, max_concurrent_ops) => {
        fn max_concurrent_ops(&self) -> usize {
            self.$inner.max_concurrent_ops()
        }
    };
    (@one $inner:ident, create_directory_errors_on_existing_dir) => {
        fn create_directory_errors_on_existing_dir(&self) -> bool {
            self.$inner.create_directory_errors_on_existing_dir()
        }
    };
    (@one $inner:ident, scan_for_copy) => {
        fn scan_for_copy<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::volume::CopyScanResult, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_copy(path)
        }
    };
    (@one $inner:ident, scan_for_copy_batch) => {
        fn scan_for_copy_batch<'a>(
            &'a self,
            paths: &'a [::std::path::PathBuf],
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::volume::BatchScanResult, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_copy_batch(paths)
        }
    };
    (@one $inner:ident, scan_for_conflicts) => {
        fn scan_for_conflicts<'a>(
            &'a self,
            source_items: &'a [::cmdr_fs::volume::SourceItemInfo],
            dest_path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Vec<::cmdr_fs::volume::ScanConflict>, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_conflicts(source_items, dest_path)
        }
    };
    (@one $inner:ident, open_read_stream) => {
        fn open_read_stream<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Box<dyn ::cmdr_fs::volume::VolumeReadStream>, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.open_read_stream(path)
        }
    };
    (@one $inner:ident, write_from_stream) => {
        fn write_from_stream<'a>(
            &'a self,
            dest: &'a ::std::path::Path,
            mode: ::cmdr_fs::volume::WriteMode,
            length: ::cmdr_fs::volume::StreamLength,
            stream: Box<dyn ::cmdr_fs::volume::VolumeReadStream>,
            on_progress: &'a (dyn Fn(::cmdr_fs::volume::StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<::cmdr_fs::volume::StreamWritten, ::cmdr_fs::volume::VolumeError>> + Send + 'a>> {
            self.$inner.write_from_stream(dest, mode, length, stream, on_progress)
        }
    };
    (@one $inner:ident, write_is_single_shot) => {
        fn write_is_single_shot<'a>(&'a self, length: ::cmdr_fs::volume::StreamLength) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = bool> + Send + 'a>> {
            self.$inner.write_is_single_shot(length)
        }
    };
    (@one $inner:ident, publishes_writes_whole) => {
        fn publishes_writes_whole(&self) -> bool {
            self.$inner.publishes_writes_whole()
        }
    };
}

pub(crate) use crate::forward_volume_methods;
use cmdr_fs::volume::WriteMode;

/// The operations [`FaultyVolume`] can be armed to fail on. One variant per
/// method whose failure a transfer has to survive; the enum keeps a test from
/// naming an operation by string and getting silence when it typos one. No
/// `Default`, deliberately: "which operation?" has no zero value, and a wrong
/// one arms the fault somewhere the test never looks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FaultyOp {
    /// `list_directory`, `list_directory_with_cancel`, `list_directory_for_scan`.
    ListDirectory,
    /// `get_metadata`.
    GetMetadata,
    /// `is_directory`.
    IsDirectory,
    /// `create_directory` and `create_directory_all`.
    CreateDirectory,
    /// `create_file`.
    CreateFile,
    /// `delete` and `delete_with_cancel`.
    Delete,
    /// `rename`, matched by its DESTINATION path (`failing_call_at`).
    Rename,
    /// `rename`, matched by its SOURCE path: the call that moves a file away
    /// from `at`, wherever to (a swap's move aside, whose destination is a
    /// fresh scratch name).
    RenameFrom,
    /// `open_read_stream` and `open_read_stream_with_hint`.
    OpenReadStream,
    /// The Nth stream this volume opens fails its FIRST read: the open
    /// answered, so the failure reaches the engine through the destination's
    /// `write_from_stream`, the way a source that drops mid-file does.
    ReadStream,
    /// `write_from_stream`.
    WriteFromStream,
    /// `flush_writes`, the closing device barrier: a drive that took every
    /// write and then wouldn't confirm them. Never matched by path.
    FlushWrites,
}

/// One armed fault: the Nth call to `op` fails with `error`, every other call
/// forwards.
struct ArmedFault {
    /// Which call fails, 1-based. `1` is the first call, so a test says "the
    /// first write fails" rather than counting from zero and getting it wrong.
    nth: usize,
    error: VolumeError,
    /// Only calls at this path count (and fail); `None` counts every call. A
    /// `rename` is matched by its destination (`to`), a `write_from_stream` by
    /// the path it writes.
    at: Option<PathBuf>,
}

/// Any volume, plus a fault you armed on one of its operations.
///
/// Counting is per-operation and process-lifetime (the wrapper's, not the
/// process's), so `nth: 2` means the second call to THAT operation on THIS
/// wrapper — a shape that survives a driver reordering its other calls.
pub struct FaultyVolume<V: Volume> {
    inner: Arc<V>,
    faults: HashMap<FaultyOp, ArmedFault>,
    calls: StdMutex<HashMap<FaultyOp, usize>>,
    /// `Some(n)`: every `rename` onto, and `create_directory` of, a name longer
    /// than `n` UTF-8 bytes is refused with `InvalidName`, the way an
    /// eCryptfs-backed share answers one (cmdr-reports#83).
    name_cap: Option<usize>,
    /// How many names the cap refused.
    names_refused: std::sync::atomic::AtomicUsize,
}

impl<V: Volume + 'static> FaultyVolume<V> {
    /// Wraps `inner` with no faults armed: a pure forwarder until a
    /// [`failing_call`](Self::failing_call) says otherwise.
    pub fn wrapping(inner: Arc<V>) -> Self {
        Self {
            inner,
            faults: HashMap::new(),
            calls: StdMutex::new(HashMap::new()),
            name_cap: None,
            names_refused: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Refuses every name longer than `max_bytes` UTF-8 bytes at a `rename`'s
    /// destination and at a `create_directory`, with `InvalidName`: a share
    /// with a short name limit, where the staged temp (at most 143 bytes) fits
    /// and the final name doesn't.
    pub fn refusing_names_longer_than(mut self, max_bytes: usize) -> Self {
        self.name_cap = Some(max_bytes);
        self
    }

    /// How many names [`refusing_names_longer_than`](Self::refusing_names_longer_than)
    /// refused, so a cell can prove its refusals fired.
    pub fn names_refused(&self) -> usize {
        self.names_refused.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The refusal for `path` when its name is over the cap.
    fn over_name_cap(&self, path: &Path) -> Option<VolumeError> {
        let cap = self.name_cap?;
        let leaf = path.file_name()?.as_encoded_bytes().len();
        (leaf > cap).then(|| {
            self.names_refused.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            VolumeError::InvalidName("STATUS_OBJECT_NAME_INVALID".into())
        })
    }

    /// Arms `op` to fail on its `nth` call (1-based) with `error`. One fault per
    /// operation; arming the same operation twice replaces the first.
    pub fn failing_call(mut self, op: FaultyOp, nth: usize, error: VolumeError) -> Self {
        assert!(nth >= 1, "failing_call: `nth` is 1-based, so 0 arms nothing");
        self.faults.insert(op, ArmedFault { nth, error, at: None });
        self
    }

    /// Like [`failing_call`](Self::failing_call), counting only the calls at
    /// `at`: the Nth call to `op` AT THAT PATH fails. For a driver that runs
    /// several files at once, where "the first delete" names whichever task got
    /// there first and only the path names the file the cell is about.
    pub fn failing_call_at(mut self, op: FaultyOp, at: impl Into<PathBuf>, nth: usize, error: VolumeError) -> Self {
        assert!(nth >= 1, "failing_call_at: `nth` is 1-based, so 0 arms nothing");
        self.faults.insert(
            op,
            ArmedFault {
                nth,
                error,
                at: Some(at.into()),
            },
        );
        self
    }

    /// Seals the wrapper into the `Arc` every driver takes.
    pub fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    #[cfg(test)]
    /// The wrapped volume, so a test can still reach its own knobs
    /// (`set_stat_failing`, `set_reported_type`, …) after wrapping.
    pub(crate) fn inner(&self) -> &Arc<V> {
        &self.inner
    }

    /// Whether the fault armed on `op` actually fired, so a test can prove its
    /// injection reached the code under test. A fault that never fires makes the
    /// test assert the UNFAULTED behavior while reading as though it covered the
    /// faulted one, which is a green test covering nothing.
    pub fn fault_fired(&self, op: FaultyOp) -> bool {
        let Some(armed) = self.faults.get(&op) else {
            return false;
        };
        self.calls.lock_ignore_poison().get(&op).copied().unwrap_or(0) >= armed.nth
    }

    /// How many calls to `op` this wrapper saw: every call when nothing is
    /// armed on `op`, only the ones at its path when a fault is armed
    /// [`at`](Self::failing_call_at) one. For a cell that pins a call COUNT,
    /// like "N loose files cost far fewer than N barriers" (#1148).
    pub fn calls_to(&self, op: FaultyOp) -> usize {
        self.calls.lock_ignore_poison().get(&op).copied().unwrap_or(0)
    }

    /// Hands back the inner volume's opened stream, broken on its first read
    /// when this open is the one [`FaultyOp::ReadStream`] is armed on.
    fn breaking_its_first_read<'a>(&'a self, path: &Path, opened: OpeningStream<'a>) -> OpeningStream<'a> {
        let fault = self.fault_for(FaultyOp::ReadStream, path);
        Box::pin(async move {
            let inner = opened.await?;
            Ok(match fault {
                Some(error) => Box::new(FirstReadFails {
                    inner,
                    error: Some(error),
                }) as Box<dyn VolumeReadStream>,
                None => inner,
            })
        })
    }

    /// Counts this call to `op` at `path` and returns the error if it's the
    /// armed one. A fault armed at another path neither counts nor fires here.
    fn fault_for(&self, op: FaultyOp, path: &Path) -> Option<VolumeError> {
        let Some(armed) = self.faults.get(&op) else {
            *self.calls.lock_ignore_poison().entry(op).or_insert(0) += 1;
            return None;
        };
        if armed.at.as_deref().is_some_and(|at| at != path) {
            return None;
        }
        let mut calls = self.calls.lock_ignore_poison();
        let seen = calls.entry(op).or_insert(0);
        *seen += 1;
        (*seen == armed.nth).then(|| armed.error.clone())
    }
}

impl<V: Volume + 'static> Volume for FaultyVolume<V> {
    forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        // A session's answers, so a wrapped `OutageVolume` is a LIVE session
        // that refuses: the cell for "a refusal fails at once, no wait".
        connection_state,
        connection_generation,
        attempt_reconnect,
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
        apply_metadata,
        metadata_caps,
        folder_metadata,
        clear_blockers,
        supports_unknown_length_writes, write_is_single_shot, publishes_writes_whole,
    );

    fn read_link_target<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, VolumeError>> + Send + 'a>> {
        self.inner.read_link_target(path)
    }

    fn can_hold_links(&self) -> bool {
        self.inner.can_hold_links()
    }

    fn create_symlink<'a>(
        &'a self,
        path: &'a Path,
        target: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_symlink(path, target)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::ListDirectory, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.list_directory(path, on_progress),
        }
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::ListDirectory, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.list_directory_with_cancel(path, on_progress, cancel),
        }
    }

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
        clock: &'a cmdr_fs::volume::ScanListingClock,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::ListDirectory, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.list_directory_for_scan(path, cancel, clock),
        }
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::GetMetadata, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.get_metadata(path),
        }
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::IsDirectory, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.is_directory(path),
        }
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self
            .fault_for(FaultyOp::CreateDirectory, path)
            .or_else(|| self.over_name_cap(path))
        {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_directory(path),
        }
    }

    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::CreateDirectory, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_directory_all(path),
        }
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::CreateFile, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_file(path, content),
        }
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::Delete, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.delete(path),
        }
    }

    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::Delete, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.delete_with_cancel(path, cancel),
        }
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self
            .fault_for(FaultyOp::RenameFrom, from)
            .or_else(|| self.fault_for(FaultyOp::Rename, to))
            .or_else(|| self.over_name_cap(to))
        {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.rename(from, to, force),
        }
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::OpenReadStream, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.breaking_its_first_read(path, self.inner.open_read_stream(path)),
        }
    }

    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::OpenReadStream, path) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.breaking_its_first_read(path, self.inner.open_read_stream_with_hint(path, size_hint)),
        }
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::WriteFromStream, dest) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.write_from_stream(dest, mode, length, stream, on_progress),
        }
    }

    fn flush_writes(&self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + '_>> {
        match self.fault_for(FaultyOp::FlushWrites, Path::new("")) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.flush_writes(),
        }
    }
}

/// What `open_read_stream` hands back.
type OpeningStream<'a> = Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>>;

/// A source stream whose first read fails, for [`FaultyOp::ReadStream`].
/// Everything else (size, date, metadata) is the inner stream's, so the
/// destination sets up its write exactly as it would for the real file.
struct FirstReadFails {
    inner: Box<dyn VolumeReadStream>,
    /// Taken by the first read; later reads forward.
    error: Option<VolumeError>,
}

impl VolumeReadStream for FirstReadFails {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        match self.error.take() {
            Some(error) => Box::pin(async move { Some(Err(error)) }),
            None => self.inner.next_chunk(),
        }
    }

    fn total_size(&self) -> StreamLength {
        self.inner.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.inner.bytes_read()
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        self.inner.modified_at()
    }

    fn metadata(&self, wanted: cmdr_fs::metadata::MetadataClasses) -> cmdr_fs::metadata::CarriedMetadata {
        self.inner.metadata(wanted)
    }

    fn metadata_outline(&self, wanted: cmdr_fs::metadata::MetadataClasses) -> cmdr_fs::metadata::CarriedMetadata {
        self.inner.metadata_outline(wanted)
    }
}

#[cfg(test)]
#[path = "faulty_volume_tests.rs"]
mod tests;
