//! Gatekeeper's quarantine on everything an extract creates.
//!
//! **Why this exists.** A browser marks what it downloads with
//! `com.apple.quarantine`, and Gatekeeper checks a quarantined app before its
//! first launch. `ditto -x`, bsdtar, and Archive Utility copy an archive's
//! quarantine onto every file AND folder they extract (verified for the first two,
//! `docs/notes/transfer-metadata-experiments-2026-10-11.md` § "The quirks"), so an
//! app that came out of a downloaded zip is still checked. An extract that drops
//! it hands the user an app Gatekeeper never looks at: a security gap, not only
//! lost metadata.
//!
//! **What travels.** The archive file's own value, byte for byte, onto each item
//! the operation CREATED: every file it writes (an Overwrite replaces the file, so
//! the new one wears it too) and every folder its walk made, ❌ never a folder it
//! merged into. Only from an archive on a LOCAL parent (a remote archive has no
//! xattr to read, and Finder doesn't propagate one either), and only onto a
//! destination that stores xattrs. macOS only: Linux has no quarantine.
//!
//! **Where it happens.** A file's goes in with the rest of what its source
//! carries: [`QuarantinedStream`] adds it to the stream's xattrs, and the
//! destination puts it on the staged temp with the others, before the mode and
//! the landing rename (`cmdr_fs::metadata::apply_staged` owns that order). A
//! folder gets it through `Volume::apply_metadata` the moment the walk creates
//! it, ❌ never in `folder_metadata.rs`'s pass, which skips a folder with no
//! source date (a zip's implicit folders) and every folder of a cancelled or
//! failed subtree: exactly the shape that leaves an extracted `.app` bundle
//! unquarantined around quarantined files. An xattr on a fresh, empty folder
//! blocks nothing and bumps no date.
//!
//! **Cost.** One `getxattr` on the archive per operation (cached on
//! `WriteOperationState`), then nothing per file beyond the xattr write the
//! destination already makes, and one `apply_metadata` per created folder, only
//! when the archive had a quarantine. Every non-archive source answers `None`
//! from `Volume::local_backing_file` with no syscall.

use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses, Xattr};
use cmdr_fs::volume::ScanStop;

use super::super::super::state::WriteOperationState;
use cmdr_fs::volume::{StreamLength, Volume, VolumeError, VolumeReadStream};

/// Wraps `stream` so its xattrs carry the archive's quarantine, when
/// `source_volume` is a quarantined archive on a local parent. Any other source
/// gets its stream back untouched.
pub(super) fn quarantine_stream(
    state: &WriteOperationState,
    source_volume: &Arc<dyn Volume>,
    stream: Box<dyn VolumeReadStream>,
) -> Box<dyn VolumeReadStream> {
    match archive_quarantine(state, source_volume) {
        Some(value) => Box::new(QuarantinedStream {
            inner: stream,
            quarantine: Xattr {
                name: QUARANTINE_XATTR.into(),
                value,
            },
        }),
        None => stream,
    }
}

/// Puts the archive's quarantine on `created`, a folder this operation just made
/// on `dest_volume`. A no-op unless the source is a quarantined archive on a
/// local parent; a destination that stores no xattrs answers `NotApplicable`.
pub(super) async fn quarantine_created_folder(
    state: &WriteOperationState,
    source_volume: &Arc<dyn Volume>,
    dest_volume: &Arc<dyn Volume>,
    created: &Path,
) {
    let Some(value) = archive_quarantine(state, source_volume) else {
        return;
    };
    let carried = CarriedMetadata {
        xattrs: vec![Xattr {
            name: QUARANTINE_XATTR.into(),
            value,
        }],
        ..CarriedMetadata::default()
    };
    let report = dest_volume.apply_metadata(created, &carried, None).await;
    if report.any_refused() {
        log::debug!(
            target: "copy",
            "extract quarantine: couldn't set it on {}; the folder is there either way",
            LogPrivate::dir_path(created),
        );
    }
}

/// Puts the archive's quarantine in place of any quarantine `xattrs` carry, when
/// `source_volume` is a quarantined archive on a local parent: a folder's set
/// from its AppleDouble sidecar is the archive AUTHOR's word, and a forged value
/// could claim the user already approved what's inside. A set with no
/// quarantine is left alone (the folder got the archive's at creation).
pub(super) fn keep_archive_quarantine(
    state: &WriteOperationState,
    source_volume: &Arc<dyn Volume>,
    xattrs: &mut [Xattr],
) {
    if !xattrs.iter().any(|xattr| xattr.name == QUARANTINE_XATTR) {
        return;
    }
    if let Some(value) = archive_quarantine(state, source_volume) {
        for xattr in xattrs.iter_mut().filter(|xattr| xattr.name == QUARANTINE_XATTR) {
            xattr.value.clone_from(&value);
        }
    }
}

/// A source stream whose xattrs gain the archive's quarantine. Forwards
/// everything else, `modified_at` and `stop_signal` included.
struct QuarantinedStream {
    inner: Box<dyn VolumeReadStream>,
    quarantine: Xattr,
}

impl QuarantinedStream {
    /// `carried` with the archive's quarantine in place of any the source had,
    /// when the caller asked for xattrs.
    fn with_quarantine(&self, mut carried: CarriedMetadata, wanted: MetadataClasses) -> CarriedMetadata {
        if wanted.contains(MetadataClasses::XATTRS) {
            carried.xattrs.retain(|xattr| xattr.name != self.quarantine.name);
            carried.xattrs.push(self.quarantine.clone());
        }
        carried
    }
}

impl VolumeReadStream for QuarantinedStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        self.inner.next_chunk()
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

    fn metadata(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.with_quarantine(self.inner.metadata(wanted), wanted)
    }

    fn metadata_outline(&self, wanted: MetadataClasses) -> CarriedMetadata {
        self.with_quarantine(self.inner.metadata_outline(wanted), wanted)
    }

    fn stop_signal(&self) -> ScanStop {
        self.inner.stop_signal()
    }

    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.inner.cancel_and_release()
    }
}

const QUARANTINE_XATTR: &str = "com.apple.quarantine";

/// The quarantine of the local archive `source_volume` reads from, or `None` when
/// it isn't an archive, sits on a remote parent, or carries none.
///
/// Read once per operation and cached on `state`, keyed by the archive's path, so
/// an operation that somehow read from a second archive asks that one itself
/// instead of inheriting the first one's answer.
#[cfg(target_os = "macos")]
fn archive_quarantine(state: &WriteOperationState, source_volume: &Arc<dyn Volume>) -> Option<Vec<u8>> {
    let archive = source_volume.local_backing_file()?;
    let (read_for, value) = state
        .extract_quarantine
        .get_or_init(|| (archive.to_path_buf(), read_quarantine(archive)));
    if read_for == archive {
        value.clone()
    } else {
        read_quarantine(archive)
    }
}

/// Linux has no quarantine to carry.
#[cfg(not(target_os = "macos"))]
fn archive_quarantine(_state: &WriteOperationState, _source_volume: &Arc<dyn Volume>) -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "macos")]
fn read_quarantine(archive: &Path) -> Option<Vec<u8>> {
    match crate::local_disk::read_xattr(archive, QUARANTINE_XATTR) {
        Ok(value) => value,
        Err(e) => {
            log::debug!(
                target: "copy",
                "extract quarantine: couldn't read it from {} ({e}); extracting without one",
                LogPrivate::file_path(archive),
            );
            None
        }
    }
}
