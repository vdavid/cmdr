//! Volumes whose answers a test scripts: a pinned watch coverage, a foreign
//! spelling table, and a read script that makes an early read finish late.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream, WatchCoverage,
};

/// Writes the `Volume` methods a fake passes straight to its `inner`
/// `InMemoryVolume` (and `as_any`), so each fake spells out only the answers it
/// scripts. A method a fake doesn't name keeps the trait's default answer.
macro_rules! forward_to_inner {
    ($($method:ident),+ $(,)?) => {
        $(forward_to_inner!(@ $method);)+
    };
    (@ name) => {
        fn name(&self) -> &str {
            self.inner.name()
        }
    };
    (@ root) => {
        fn root(&self) -> &Path {
            self.inner.root()
        }
    };
    (@ as_any) => {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    };
    (@ get_metadata) => {
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
            self.inner.get_metadata(path)
        }
    };
    (@ exists) => {
        fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            self.inner.exists(path)
        }
    };
    (@ is_directory) => {
        fn is_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
            self.inner.is_directory(path)
        }
    };
    (@ create_directory) => {
        fn create_directory<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
            self.inner.create_directory(path)
        }
    };
    (@ get_space_info) => {
        fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
            self.inner.get_space_info()
        }
    };
    (@ scan_for_copy) => {
        fn scan_for_copy<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_copy(path)
        }
    };
    (@ scan_for_copy_batch) => {
        fn scan_for_copy_batch<'a>(
            &'a self,
            paths: &'a [PathBuf],
        ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_copy_batch(paths)
        }
    };
    (@ scan_for_conflicts) => {
        fn scan_for_conflicts<'a>(
            &'a self,
            source_items: &'a [SourceItemInfo],
            dest_path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
            self.inner.scan_for_conflicts(source_items, dest_path)
        }
    };
    (@ open_read_stream) => {
        fn open_read_stream<'a>(
            &'a self,
            path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
            self.inner.open_read_stream(path)
        }
    };
}

/// A `Volume` whose `listing_watch_coverage` answer a test pins, delegating every
/// other method to an `InMemoryVolume`.
///
/// The fresh-listing oracle only answers on `EveryWriter`, and `InMemoryVolume`
/// reports `None` (the trait's default). Pinning the answer is what lets a test
/// drive all three states without a real change watch
/// entry — including `ThisMachineOnly`, which has no other reachable fixture.
pub struct WatchCoverageVolume {
    inner: InMemoryVolume,
    /// The pinned answer, as the `WatchCoverage` discriminant so the field stays
    /// lock-free (`set_coverage` races with the oracle reading it).
    coverage: AtomicU8,
    /// How many `get_metadata` calls reached the volume, for a test pinning that
    /// a cached answer spared the round trip.
    metadata_calls: std::sync::atomic::AtomicUsize,
    /// How many `list_directory` calls reached the volume, the same pin for a
    /// cached listing.
    list_calls: std::sync::atomic::AtomicUsize,
}

impl WatchCoverageVolume {
    /// An empty volume named `name` that answers `coverage`.
    pub fn new(name: &str, coverage: WatchCoverage) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            coverage: AtomicU8::new(encode_coverage(coverage)),
            metadata_calls: std::sync::atomic::AtomicUsize::new(0),
            list_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// How many `get_metadata` calls reached this volume so far.
    pub fn metadata_calls(&self) -> usize {
        self.metadata_calls.load(Ordering::Relaxed)
    }

    /// How many `list_directory` calls reached this volume so far.
    pub fn list_calls(&self) -> usize {
        self.list_calls.load(Ordering::Relaxed)
    }

    /// Answers `coverage` from now on.
    pub fn set_coverage(&self, coverage: WatchCoverage) {
        self.coverage.store(encode_coverage(coverage), Ordering::Relaxed);
    }
}

fn encode_coverage(coverage: WatchCoverage) -> u8 {
    match coverage {
        WatchCoverage::None => 0,
        WatchCoverage::ThisMachineOnly => 1,
        WatchCoverage::EveryWriter => 2,
    }
}

fn decode_coverage(raw: u8) -> WatchCoverage {
    match raw {
        1 => WatchCoverage::ThisMachineOnly,
        2 => WatchCoverage::EveryWriter,
        _ => WatchCoverage::None,
    }
}

impl Volume for WatchCoverageVolume {
    forward_to_inner!(
        name,
        root,
        as_any,
        exists,
        is_directory,
        create_directory,
        get_space_info,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        open_read_stream,
    );

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.metadata_calls.fetch_add(1, Ordering::Relaxed);
        self.inner.get_metadata(path)
    }

    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        decode_coverage(self.coverage.load(Ordering::Relaxed))
    }
}

/// A byte-exact `InMemoryVolume` whose `find_stored_spelling` answers from a
/// table, the way an SMB share answers a foreign path: a lookup of the foreign
/// spelling misses, and the resolve names where it's really stored.
pub struct SpelledVolume {
    inner: InMemoryVolume,
    /// Foreign path → what the resolve answers for it.
    answers: std::collections::HashMap<PathBuf, Result<Option<PathBuf>, VolumeError>>,
    /// When set, every listing is refused with this instead of reaching `inner`.
    listing_refusal: Option<VolumeError>,
    resolves: std::sync::atomic::AtomicUsize,
}

impl SpelledVolume {
    /// `inner`, with no foreign spelling resolving yet.
    pub fn new(inner: InMemoryVolume) -> Self {
        Self {
            inner,
            answers: std::collections::HashMap::new(),
            listing_refusal: None,
            resolves: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Refuses every listing with `error`, the way a share refuses a folder it
    /// won't show.
    pub fn listings_refused_with(mut self, error: VolumeError) -> Self {
        self.listing_refusal = Some(error);
        self
    }

    /// Makes the resolve answer `answer` for `foreign`.
    pub fn resolving(mut self, foreign: &str, answer: Result<Option<&str>, VolumeError>) -> Self {
        self.answers
            .insert(PathBuf::from(foreign), answer.map(|p| p.map(PathBuf::from)));
        self
    }

    /// How many resolves reached this volume.
    pub fn resolves(&self) -> usize {
        self.resolves.load(Ordering::Relaxed)
    }
}

impl Volume for SpelledVolume {
    forward_to_inner!(name, root, as_any, get_metadata, exists, is_directory);

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        match &self.listing_refusal {
            Some(error) => {
                let error = error.clone();
                Box::pin(async move { Err(error) })
            }
            // `InMemoryVolume` lists a directory it doesn't hold as empty; a share
            // answers a name it doesn't store with a miss, which is the whole point.
            None => Box::pin(async move {
                if path != self.inner.root() && !self.inner.exists(path).await {
                    return Err(VolumeError::NotFound(path.display().to_string()));
                }
                self.inner.list_directory(path, on_progress).await
            }),
        }
    }

    fn find_stored_spelling<'a>(
        &'a self,
        path: &'a Path,
        _cancel: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, VolumeError>> + Send + 'a>> {
        self.resolves.fetch_add(1, Ordering::Relaxed);
        let answer = self.answers.get(path).cloned().unwrap_or(Ok(None));
        Box::pin(async move { answer })
    }
}

/// A `Volume` whose `list_directory` answers from a SCRIPT: each call pops the next
/// `(delay, entries)` pair, so a test can make an early read finish late.
///
/// This is what makes the full-refresh ordering hazard reproducible. Two refreshes
/// of one directory each do read-then-write-the-cache; if the read that started
/// FIRST also finishes LAST, it writes a snapshot the newer one has already
/// superseded, and the cache keeps the older truth until some later event happens
/// to correct it. Without a scripted delay the interleaving is timing-dependent and
/// a test for it would itself be a flake.
pub struct ScriptedListVolume {
    inner: InMemoryVolume,
    script: std::sync::Mutex<std::collections::VecDeque<(std::time::Duration, Vec<FileEntry>)>>,
}

impl ScriptedListVolume {
    /// A volume named `name` whose reads answer `script` in order: each one
    /// waits its duration, then returns its entries.
    pub fn new(name: &str, script: Vec<(std::time::Duration, Vec<FileEntry>)>) -> Self {
        Self {
            inner: InMemoryVolume::new(name),
            script: std::sync::Mutex::new(script.into_iter().collect()),
        }
    }

    /// Scripted reads not yet consumed. A test asserts on this to count how many
    /// times the directory was actually read.
    pub fn unread(&self) -> usize {
        self.script.lock_ignore_poison().len()
    }
}

impl Volume for ScriptedListVolume {
    forward_to_inner!(
        name,
        root,
        as_any,
        get_metadata,
        exists,
        is_directory,
        get_space_info,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        open_read_stream,
    );

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let next = self.script.lock_ignore_poison().pop_front();
        Box::pin(async move {
            match next {
                Some((delay, entries)) => {
                    // allowed-test-sleep: the scripted latency IS the subject — it is what makes the
                    // read that started first finish last, deterministically.
                    crate::rt::time::sleep(delay).await;
                    Ok(entries)
                }
                None => Ok(Vec::new()),
            }
        })
    }
}
