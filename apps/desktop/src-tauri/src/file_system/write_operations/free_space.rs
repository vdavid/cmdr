//! The pre-copy free-space check: how much room a copy can need, and how much
//! the destination has. Local copies only; the volume engine's twin is
//! `transfer/volume/copy.rs::room_to_check`.
//!
//! Both questions get the cheapest answer that stays honest:
//!
//! - **What the destination has** ([`available_space`]): `statvfs` first, which
//!   is instant. The "important usage" figure, which also counts space macOS
//!   frees on demand (APFS snapshots, iCloud caches), only when `statvfs` says
//!   the copy won't fit. That figure comes from NSURL capacity keys that walk
//!   the volume in the kernel and serialize across callers: 0.6 s for one copy,
//!   over 5 s for 40 copies starting at once (measured 2026-10-02, #350).
//! - **What the copy can need** ([`local_copy_need`]): the full source size,
//!   unless that doesn't fit and the destination is a local disk. Then each file
//!   that lands on an existing file counts only what it can add (#351).
//!
//! A shortfall is the person's call, ❌ never a verdict (`SpaceShortfall`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::cancellable::run_cancellable;
use super::scan_cache::{FileInfo, ScanResult};
use super::state::{WriteOperationState, is_cancelled};
use super::types::{ConflictResolution, SpaceShortfall, WriteOperationConfig, WriteOperationError};
use super::validation::is_same_file;

/// The local copy's free-space pre-flight, skipped when the person chose "Copy
/// anyway". The space query polls for cancel, so a cancel answers at once even
/// if `statvfs` blocks on a slow network drive. Only a shortfall onto a local
/// disk looks at what's already at the destination: a network one keeps the
/// full size, ❌ never a stat per file.
pub(super) fn check_local_copy_space(
    destination: &Path,
    sources: &[PathBuf],
    scan_result: &ScanResult,
    config: &WriteOperationConfig,
    state: &Arc<WriteOperationState>,
    operation_id: &str,
) -> Result<(), WriteOperationError> {
    if config.space_shortfall == SpaceShortfall::Proceed {
        log::info!("copy: free-space check skipped for op={operation_id}, the person chose to copy anyway");
        return Ok(());
    }
    let total_bytes = scan_result.total_bytes;
    let owned_destination = destination.to_path_buf();
    let available = run_cancellable(
        move || Ok(available_space(&owned_destination, total_bytes)),
        state,
        "disk_space_check",
        operation_id,
    )?;
    check_copy_space(destination, total_bytes, available, config.space_shortfall, || {
        if crate::file_system::index_provider::path_is_on_network_mount(destination) {
            return Ok(None);
        }
        let cancelled = || is_cancelled(&state.intent);
        match local_copy_need(
            destination,
            sources,
            &scan_result.files,
            config.conflict_resolution,
            cancelled,
        ) {
            Some(need) => Ok(Some(need.bytes())),
            None => Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            }),
        }
    })
}

/// The destination's free bytes for a copy of `required_bytes`, or `None` when
/// nothing answered. `None` lets the copy go ahead: a full disk then stops it
/// as `DestinationFull`.
#[cfg(unix)]
pub(super) fn available_space(destination: &Path, required_bytes: u64) -> Option<u64> {
    available_space_from(
        required_bytes,
        || statvfs_available(destination),
        || important_usage_available(destination),
    )
}

#[cfg(not(unix))]
pub(super) fn available_space(_destination: &Path, _required_bytes: u64) -> Option<u64> {
    None
}

/// The decision behind [`available_space`], with the platform calls passed in.
///
/// `fast` is `statvfs`: physically free blocks. `important` also counts what
/// the OS frees on demand, so it's the honest figure when `fast` falls short,
/// and the slow one, so it's asked only then. The larger of the two wins: free
/// blocks are free whatever the other figure says.
pub(super) fn available_space_from(
    required_bytes: u64,
    fast: impl FnOnce() -> Option<u64>,
    important: impl FnOnce() -> Option<u64>,
) -> Option<u64> {
    let fast = fast();
    if fast.is_some_and(|free| free >= required_bytes) {
        return fast;
    }
    match (fast, important()) {
        (Some(fast), Some(important)) => Some(fast.max(important)),
        (fast, important) => fast.or(important),
    }
}

/// Free blocks available to an unprivileged writer, from `statvfs`.
#[cfg(unix)]
fn statvfs_available(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();
    // SAFETY: c_path is a valid null-terminated C string, stat is a valid pointer
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: statvfs succeeded, stat is initialized
    let stat = unsafe { stat.assume_init() };
    #[allow(
        clippy::unnecessary_cast,
        reason = "Required for macOS where statvfs fields are not u64"
    )]
    Some(stat.f_bavail as u64 * stat.f_frsize as u64)
}

/// `NSURLVolumeAvailableCapacityForImportantUsageKey`: free space including
/// what macOS purges on demand, matching Finder and the status bar.
#[cfg(target_os = "macos")]
fn important_usage_available(destination: &Path) -> Option<u64> {
    crate::volumes::get_volume_space(&destination.to_string_lossy())?.available_bytes()
}

/// Linux has no purgeable space, so `statvfs` is the whole answer.
#[cfg(all(unix, not(target_os = "macos")))]
fn important_usage_available(_destination: &Path) -> Option<u64> {
    None
}

/// What a copy can add to its destination, as an upper bound.
///
/// A file landing on an existing file under an overwrite policy can add at most
/// `size - existing`. While it lands, the new bytes sit beside the old ones
/// (temp, then rename), so the bound also keeps `headroom`: the largest
/// `min(size, existing)` of those files. For each file,
/// `size = max(0, size - existing) + min(size, existing)`, so the total stays
/// above the real peak at every point of the copy, whatever the order.
// DEFAULT-OK: the zero value is the true need of a copy with no files counted yet.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct SpaceNeed {
    growth: u64,
    headroom: u64,
}

impl SpaceNeed {
    /// The bytes the destination must have free.
    pub(super) fn bytes(self) -> u64 {
        self.growth.saturating_add(self.headroom)
    }

    /// A file with nothing at its destination name.
    pub(super) fn add_new(&mut self, size: u64) {
        self.growth = self.growth.saturating_add(size);
    }

    /// A file whose destination name holds a file of `existing` bytes.
    ///
    /// `Skip` adds nothing. The overwrite policies add what replacing can add,
    /// and skipping adds less. `Rename` keeps both, and so can a `Stop` answer,
    /// so those count the whole file.
    pub(super) fn add_clash(&mut self, size: u64, existing: u64, policy: ConflictResolution) {
        match policy {
            ConflictResolution::Skip => {}
            ConflictResolution::Overwrite
            | ConflictResolution::OverwriteSmaller
            | ConflictResolution::OverwriteOlder => {
                self.growth = self.growth.saturating_add(size.saturating_sub(existing));
                self.headroom = self.headroom.max(size.min(existing));
            }
            ConflictResolution::Rename | ConflictResolution::Stop => self.add_new(size),
        }
    }
}

/// What copying `files` into `destination` can need, given what's already
/// there, or `None` when `is_cancelled` turned true partway.
///
/// One `lstat` per file at its destination name, so ❌ never call it for a
/// network destination: that's a round trip per file.
///
/// A source that already lives at the destination is duplicated to a free
/// ` (N)` name (`transfer/DETAILS.md` § "Self-collision"), so its files count
/// in full rather than as clashes with themselves.
pub(super) fn local_copy_need(
    destination: &Path,
    sources: &[PathBuf],
    files: &[FileInfo],
    policy: ConflictResolution,
    is_cancelled: impl Fn() -> bool,
) -> Option<SpaceNeed> {
    let duplicated: Vec<&PathBuf> = sources
        .iter()
        .filter(|source| {
            source
                .file_name()
                .is_some_and(|name| is_same_file(source, &destination.join(name)))
        })
        .collect();

    let mut need = SpaceNeed::default();
    for (index, file) in files.iter().enumerate() {
        if index % 1024 == 0 && is_cancelled() {
            return None;
        }
        let duplicate = duplicated.iter().any(|root| file.path.starts_with(root));
        let existing = match file.path.strip_prefix(&file.source_root) {
            Ok(relative) if !duplicate => fs::symlink_metadata(destination.join(relative))
                .ok()
                .filter(|metadata| metadata.file_type().is_file())
                .map(|metadata| metadata.len()),
            _ => None,
        };
        match existing {
            Some(existing) => need.add_clash(file.size, existing, policy),
            None => need.add_new(file.size),
        }
    }
    Some(need)
}

/// The free-space verdict for a copy of `total_bytes` into a destination with
/// `available` bytes free.
///
/// `refine` answers what the copy can really need when the full size doesn't
/// fit, or `Ok(None)` when it can't tell cheaply; it runs only on a shortfall.
pub(super) fn check_copy_space(
    destination: &Path,
    total_bytes: u64,
    available: Option<u64>,
    shortfall: SpaceShortfall,
    refine: impl FnOnce() -> Result<Option<u64>, WriteOperationError>,
) -> Result<(), WriteOperationError> {
    if shortfall == SpaceShortfall::Proceed {
        return Ok(());
    }
    let Some(available) = available else {
        return Ok(());
    };
    if total_bytes <= available {
        return Ok(());
    }
    let required = refine()?.unwrap_or(total_bytes).min(total_bytes);
    if required <= available {
        log::info!(
            "free space: {} of {} would fit in {} free at {}, once files already there are counted",
            required,
            total_bytes,
            available,
            destination.display()
        );
        return Ok(());
    }
    Err(insufficient_space(destination, required, available))
}

/// The `InsufficientSpace` refusal, naming the `/Volumes/<name>` the
/// destination sits on when there is one.
fn insufficient_space(destination: &Path, required: u64, available: u64) -> WriteOperationError {
    let volume_name = destination
        .ancestors()
        .find(|p| p.parent().is_some_and(|pp| pp == Path::new("/Volumes")))
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    WriteOperationError::InsufficientSpace {
        required,
        available,
        volume_name,
    }
}
