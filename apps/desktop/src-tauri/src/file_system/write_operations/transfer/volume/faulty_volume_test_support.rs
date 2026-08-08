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

use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{DirectoryCreation, ListingProgress, Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
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
/// ```ignore
/// impl Volume for LyingDest {
///     forward_volume_methods!(inner => name, root, list_directory, get_metadata, exists, delete);
///     fn as_any(&self) -> &dyn std::any::Any { self }
///     fn is_directory<'a>(&'a self, path: &'a Path) -> … { /* the lie */ }
/// }
/// ```
macro_rules! forward_volume_methods {
    ($inner:ident => $($method:ident),* $(,)?) => {
        $( forward_volume_methods!(@one $inner, $method); )*
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
        fn lane_key(&self) -> $crate::file_system::volume::LaneKey {
            self.$inner.lane_key()
        }
    };
    (@one $inner:ident, list_directory) => {
        fn list_directory<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            on_progress: Option<&'a (dyn Fn($crate::file_system::volume::ListingProgress) + Sync)>,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Vec<$crate::file_system::listing::FileEntry>, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.list_directory(path, on_progress)
        }
    };
    (@one $inner:ident, get_metadata) => {
        fn get_metadata<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<$crate::file_system::listing::FileEntry, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
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
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<bool, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.is_directory(path)
        }
    };
    (@one $inner:ident, create_file) => {
        fn create_file<'a>(
            &'a self,
            path: &'a ::std::path::Path,
            content: &'a [u8],
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_file(path, content)
        }
    };
    (@one $inner:ident, create_directory) => {
        fn create_directory<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_directory(path)
        }
    };
    (@one $inner:ident, create_directory_all) => {
        fn create_directory_all<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<$crate::file_system::volume::DirectoryCreation, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.create_directory_all(path)
        }
    };
    (@one $inner:ident, delete) => {
        fn delete<'a>(&'a self, path: &'a ::std::path::Path) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.delete(path)
        }
    };
    (@one $inner:ident, rename) => {
        fn rename<'a>(
            &'a self,
            from: &'a ::std::path::Path,
            to: &'a ::std::path::Path,
            force: bool,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<(), $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.rename(from, to, force)
        }
    };
    (@one $inner:ident, get_space_info) => {
        fn get_space_info<'a>(&'a self) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<$crate::file_system::volume::SpaceInfo, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
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
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<$crate::file_system::volume::CopyScanResult, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_copy(path)
        }
    };
    (@one $inner:ident, scan_for_copy_batch) => {
        fn scan_for_copy_batch<'a>(
            &'a self,
            paths: &'a [::std::path::PathBuf],
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<$crate::file_system::volume::BatchScanResult, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_copy_batch(paths)
        }
    };
    (@one $inner:ident, scan_for_conflicts) => {
        fn scan_for_conflicts<'a>(
            &'a self,
            source_items: &'a [$crate::file_system::volume::SourceItemInfo],
            dest_path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Vec<$crate::file_system::volume::ScanConflict>, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.scan_for_conflicts(source_items, dest_path)
        }
    };
    (@one $inner:ident, open_read_stream) => {
        fn open_read_stream<'a>(
            &'a self,
            path: &'a ::std::path::Path,
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<Box<dyn $crate::file_system::volume::VolumeReadStream>, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.open_read_stream(path)
        }
    };
    (@one $inner:ident, write_from_stream) => {
        fn write_from_stream<'a>(
            &'a self,
            dest: &'a ::std::path::Path,
            size: u64,
            stream: Box<dyn $crate::file_system::volume::VolumeReadStream>,
            on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
        ) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = Result<u64, $crate::file_system::volume::VolumeError>> + Send + 'a>> {
            self.$inner.write_from_stream(dest, size, stream, on_progress)
        }
    };
    (@one $inner:ident, write_is_single_shot) => {
        fn write_is_single_shot<'a>(&'a self, size: u64) -> ::std::pin::Pin<Box<dyn ::std::future::Future<Output = bool> + Send + 'a>> {
            self.$inner.write_is_single_shot(size)
        }
    };
}

pub(super) use forward_volume_methods;

/// The operations [`FaultyVolume`] can be armed to fail on. One variant per
/// method whose failure a transfer has to survive; the enum keeps a test from
/// naming an operation by string and getting silence when it typos one. No
/// `Default`, deliberately: "which operation?" has no zero value, and a wrong
/// one arms the fault somewhere the test never looks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum FaultyOp {
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
    /// `rename`.
    Rename,
    /// `open_read_stream` and `open_read_stream_with_hint`.
    OpenReadStream,
    /// `write_from_stream`.
    WriteFromStream,
}

/// One armed fault: the Nth call to `op` fails with `error`, every other call
/// forwards.
struct ArmedFault {
    /// Which call fails, 1-based. `1` is the first call, so a test says "the
    /// first write fails" rather than counting from zero and getting it wrong.
    nth: usize,
    error: VolumeError,
}

/// Any volume, plus a fault you armed on one of its operations.
///
/// Counting is per-operation and process-lifetime (the wrapper's, not the
/// process's), so `nth: 2` means the second call to THAT operation on THIS
/// wrapper — a shape that survives a driver reordering its other calls.
pub(crate) struct FaultyVolume<V: Volume> {
    inner: Arc<V>,
    faults: HashMap<FaultyOp, ArmedFault>,
    calls: StdMutex<HashMap<FaultyOp, usize>>,
}

impl<V: Volume + 'static> FaultyVolume<V> {
    /// Wraps `inner` with no faults armed: a pure forwarder until a
    /// [`failing_call`](Self::failing_call) says otherwise.
    pub(crate) fn wrapping(inner: Arc<V>) -> Self {
        Self {
            inner,
            faults: HashMap::new(),
            calls: StdMutex::new(HashMap::new()),
        }
    }

    /// Arms `op` to fail on its `nth` call (1-based) with `error`. One fault per
    /// operation; arming the same operation twice replaces the first.
    pub(crate) fn failing_call(mut self, op: FaultyOp, nth: usize, error: VolumeError) -> Self {
        assert!(nth >= 1, "failing_call: `nth` is 1-based, so 0 arms nothing");
        self.faults.insert(op, ArmedFault { nth, error });
        self
    }

    /// Seals the wrapper into the `Arc` every driver takes.
    pub(crate) fn arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// The wrapped volume, so a test can still reach its own knobs
    /// (`set_stat_failing`, `set_reported_type`, …) after wrapping.
    pub(crate) fn inner(&self) -> &Arc<V> {
        &self.inner
    }

    /// Whether the fault armed on `op` actually fired, so a test can prove its
    /// injection reached the code under test. A fault that never fires makes the
    /// test assert the UNFAULTED behavior while reading as though it covered the
    /// faulted one, which is a green test covering nothing.
    pub(crate) fn fault_fired(&self, op: FaultyOp) -> bool {
        let Some(armed) = self.faults.get(&op) else {
            return false;
        };
        self.calls.lock_ignore_poison().get(&op).copied().unwrap_or(0) >= armed.nth
    }

    /// Counts this call to `op` and returns the error if it's the armed one.
    fn fault_for(&self, op: FaultyOp) -> Option<VolumeError> {
        let armed = self.faults.get(&op)?;
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
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::ListDirectory) {
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
        match self.fault_for(FaultyOp::ListDirectory) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.list_directory_with_cancel(path, on_progress, cancel),
        }
    }

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::ListDirectory) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.list_directory_for_scan(path, cancel),
        }
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::GetMetadata) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.get_metadata(path),
        }
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::IsDirectory) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.is_directory(path),
        }
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::CreateDirectory) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_directory(path),
        }
    }

    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::CreateDirectory) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_directory_all(path),
        }
    }

    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::CreateFile) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.create_file(path, content),
        }
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::Delete) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.delete(path),
        }
    }

    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::Delete) {
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
        match self.fault_for(FaultyOp::Rename) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.rename(from, to, force),
        }
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::OpenReadStream) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.open_read_stream(path),
        }
    }

    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::OpenReadStream) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.open_read_stream_with_hint(path, size_hint),
        }
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        match self.fault_for(FaultyOp::WriteFromStream) {
            Some(e) => Box::pin(async move { Err(e) }),
            None => self.inner.write_from_stream(dest, size, stream, on_progress),
        }
    }
}

#[cfg(test)]
#[path = "faulty_volume_tests.rs"]
mod tests;
