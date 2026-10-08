//! `Volume::capabilities()` folds the trait's own predicates and nothing else.
//!
//! The point of the fold is that a capability has ONE answer: flip a predicate
//! and the published surface follows, with no second table to keep in step.

use super::{InMemoryVolume, Volume, VolumeCapabilities};
use crate::entry::FileEntry;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

/// The most conservative backend there is: it lists and stats, nothing more.
/// Every capability default has to be the safe answer for it.
struct BareVolume;

impl Volume for BareVolume {
    fn name(&self) -> &str {
        "Bare"
    }

    fn root(&self) -> &Path {
        Path::new("/")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(super::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, super::VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn get_metadata<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, super::VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(super::VolumeError::NotSupported) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    fn is_directory<'a>(
        &'a self,
        _path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, super::VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(super::VolumeError::NotSupported) })
    }
}

/// A backend that declares both capabilities, to pin that the fold reads the
/// predicates rather than hardcoding either answer.
struct WritableExportingVolume(BareVolume);

impl Volume for WritableExportingVolume {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn root(&self) -> &Path {
        self.0.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(super::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, super::VolumeError>> + Send + 'a>> {
        self.0.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, super::VolumeError>> + Send + 'a>> {
        self.0.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.0.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, super::VolumeError>> + Send + 'a>> {
        self.0.is_directory(path)
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn supports_share_links(&self) -> bool {
        true
    }

    fn renames_can_copy(&self) -> bool {
        true
    }
}

#[test]
fn an_undeclared_backend_gets_the_conservative_answer_to_everything() {
    assert_eq!(
        BareVolume.capabilities(),
        VolumeCapabilities {
            backend_can_write: false,
            can_export: false,
            can_share_links: false,
            // Follows `backend_kind`, whose default is `Local`.
            can_be_indexed: true,
            renames_can_copy: false,
            has_os_mount_fallback: false,
        }
    );
}

#[test]
fn declaring_a_predicate_moves_the_published_surface() {
    assert_eq!(
        WritableExportingVolume(BareVolume).capabilities(),
        VolumeCapabilities {
            backend_can_write: true,
            can_export: true,
            can_share_links: true,
            can_be_indexed: true,
            renames_can_copy: true,
            has_os_mount_fallback: false,
        }
    );
}

#[test]
fn the_in_memory_double_publishes_the_read_write_surface_a_test_expects() {
    let volume = InMemoryVolume::new("Test");
    assert_eq!(
        volume.capabilities(),
        VolumeCapabilities {
            backend_can_write: true,
            can_export: true,
            can_share_links: false,
            can_be_indexed: true,
            renames_can_copy: false,
            has_os_mount_fallback: false,
        }
    );
}

/// The double that renames by copying says so, so a same-volume move on it
/// scans like the S3 one it stands in for.
#[test]
fn the_in_memory_double_publishes_renames_that_copy() {
    assert!(
        InMemoryVolume::new("Store")
            .with_renames_by_copy()
            .capabilities()
            .renames_can_copy
    );
}

/// A server and a view inside a drive are never offered a drive index; a disk, a
/// share, and a phone (over MTP or ADB) are. The published answer follows the
/// backend kind, so it can't disagree with the index's own routing.
#[test]
fn indexability_follows_the_backend_kind() {
    use super::BackendKind;
    for (kind, indexable) in [
        (BackendKind::Local, true),
        (BackendKind::Smb, true),
        (BackendKind::Mtp, true),
        (BackendKind::Adb, true),
        (BackendKind::Sftp, false),
        (BackendKind::Webdav, false),
        (BackendKind::Archive, false),
        (BackendKind::GitPortal, false),
    ] {
        let volume = InMemoryVolume::new("Test").with_backend_kind(kind);
        assert_eq!(volume.capabilities().can_be_indexed, indexable, "{kind:?}");
    }
}

/// Only an SMB share can also be reached through the OS's own mount, so only its
/// green dot may say "connected directly": on any other server "direct" is the
/// only way there is.
#[test]
fn only_smb_has_an_os_mount_fallback() {
    use super::BackendKind;
    for (kind, has_fallback) in [
        (BackendKind::Smb, true),
        (BackendKind::Local, false),
        (BackendKind::Sftp, false),
        (BackendKind::Webdav, false),
        (BackendKind::S3, false),
        (BackendKind::Mtp, false),
        (BackendKind::Adb, false),
        (BackendKind::Archive, false),
        (BackendKind::GitPortal, false),
    ] {
        let volume = InMemoryVolume::new("Test").with_backend_kind(kind);
        assert_eq!(volume.capabilities().has_os_mount_fallback, has_fallback, "{kind:?}");
    }
}

#[tokio::test]
async fn a_backend_claiming_writability_has_to_back_it_up() {
    let volume = InMemoryVolume::new("Test");
    super::conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new("/scratch-dir")).await;
}

#[tokio::test]
async fn a_backend_declining_writability_has_to_refuse_mutations() {
    super::conformance::assert_writability_matches_the_mutations_offered(&BareVolume, Path::new("/scratch-dir")).await;
}

#[test]
fn an_undeclared_backend_mints_no_share_link() {
    let outcome = block_on(BareVolume.share_link(Path::new("/a.txt"), super::ShareLinkExpiry::SevenDays));
    assert!(matches!(outcome, Err(super::VolumeError::NotSupported)), "{outcome:?}");
}

/// A tiny executor for the one future above, which never awaits anything.
fn block_on<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime builds");
    runtime.block_on(future)
}

#[test]
fn a_share_link_never_prints_its_signature() {
    let link = super::ShareLink::new("https://b.s3.amazonaws.com/k?X-Amz-Signature=deadbeef".to_string());
    assert!(
        !format!("{link:?}").contains("deadbeef"),
        "Debug must not print the URL"
    );
    assert_eq!(link.into_url(), "https://b.s3.amazonaws.com/k?X-Amz-Signature=deadbeef");
}

#[test]
fn every_expiry_fits_inside_s3s_seven_day_ceiling() {
    use super::ShareLinkExpiry;
    assert_eq!(ShareLinkExpiry::OneHour.duration().as_secs(), 3_600);
    assert_eq!(ShareLinkExpiry::OneDay.duration().as_secs(), 86_400);
    assert_eq!(ShareLinkExpiry::SevenDays.duration().as_secs(), 604_800);
}
