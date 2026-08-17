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
}

#[test]
fn an_undeclared_backend_gets_the_conservative_answer_to_everything() {
    assert_eq!(
        BareVolume.capabilities(),
        VolumeCapabilities {
            backend_can_write: false,
            can_export: false,
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
        }
    );
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
