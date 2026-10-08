//! `list_as_stored` against a byte-exact volume that knows a foreign spelling.

use std::path::{Path, PathBuf};

use super::*;
use crate::file_system::listing::caching_test_support::SpelledVolume;
use crate::file_system::volume::InMemoryVolume;

/// `fotók` composed (how the server stores it) and decomposed (how the macOS
/// kernel mount spelled it for the pane).
const STORED: &str = "/fot\u{f3}k";
const FOREIGN: &str = "/foto\u{301}k";

async fn album() -> InMemoryVolume {
    let volume = InMemoryVolume::new("Share");
    volume
        .create_directory(Path::new(STORED))
        .await
        .expect("seed the album");
    volume
        .create_file(&Path::new(STORED).join("photo.jpg"), b"jpeg")
        .await
        .expect("seed a photo");
    volume
}

#[tokio::test]
async fn a_path_that_lists_as_given_never_asks_for_another_spelling() {
    let volume = SpelledVolume::new(album().await);

    let listed = list_as_stored(&volume, Path::new(STORED), None, None)
        .await
        .expect("the stored spelling lists");

    assert_eq!(listed.path, Path::new(STORED));
    assert_eq!(listed.stored_spelling_of(Path::new(STORED)), None);
    assert_eq!(volume.resolves(), 0, "the happy path costs nothing extra");
}

/// The pane asked for the kernel mount's spelling: the listing comes back under
/// the server's, and so does every child, so the next click inside is exact.
#[tokio::test]
async fn a_foreign_spelling_lists_the_stored_directory_under_its_stored_path() {
    let volume = SpelledVolume::new(album().await).resolving(FOREIGN, Ok(Some(STORED)));

    let listed = list_as_stored(&volume, Path::new(FOREIGN), None, None)
        .await
        .expect("the foreign spelling resolves and lists");

    assert_eq!(listed.path, Path::new(STORED));
    assert_eq!(listed.stored_spelling_of(Path::new(FOREIGN)).as_deref(), Some(STORED));
    let child = &listed.entries.first().expect("the photo is listed").path;
    assert!(
        child.starts_with(STORED),
        "{child} must carry the stored spelling of its directory"
    );
}

/// No other spelling found: the pane gets the listing's own `NotFound`, which is
/// what drives its "this folder is gone" walk-up.
#[tokio::test]
async fn a_miss_with_no_other_spelling_keeps_its_not_found() {
    let volume = SpelledVolume::new(album().await);

    let result = list_as_stored(&volume, Path::new("/gone"), None, None).await;

    assert!(
        matches!(result, Err(VolumeError::NotFound(_))),
        "got {:?}",
        result.err()
    );
    assert_eq!(volume.resolves(), 1);
}

/// Two look-alikes: the refusal reaches the pane as itself, never a guess.
#[tokio::test]
async fn a_look_alike_refusal_reaches_the_pane() {
    let volume =
        SpelledVolume::new(album().await).resolving(FOREIGN, Err(VolumeError::AmbiguousName(FOREIGN.to_string())));

    let result = list_as_stored(&volume, Path::new(FOREIGN), None, None).await;

    assert!(
        matches!(result, Err(VolumeError::AmbiguousName(_))),
        "got {:?}",
        result.err()
    );
}

/// A pane open in an accented folder when the share moves from the kernel mount
/// to a direct connection holds the kernel's spelling of the folder and of every
/// entry. The respell re-reads it through the new backend: the listing moves to
/// the stored spelling, so the watcher's reports (in server bytes) find it, and
/// its entries are replaced by ones the new backend can open.
#[tokio::test]
async fn a_backend_swap_respells_an_open_listing_and_its_entries() {
    use crate::file_system::listing::caching::find_listings_for_path_on_volume;
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::volume::manager::get_volume_manager;

    let volume_id = format!("respell-{}", uuid::Uuid::new_v4());
    let volume = SpelledVolume::new(album().await).resolving(FOREIGN, Ok(Some(STORED)));
    get_volume_manager().register(&volume_id, std::sync::Arc::new(volume));
    let kernel_entry = FileEntry::new("photo.jpg".to_string(), format!("{FOREIGN}/photo.jpg"), false, false);
    let listing = TestListing::new()
        .volume(&volume_id)
        .path(FOREIGN)
        .entries(vec![kernel_entry])
        .insert("respell");

    respell_listing(volume_id.clone(), listing.id().to_string(), FOREIGN.into()).await;
    let found_by_server_bytes = find_listings_for_path_on_volume(Some(&volume_id), Path::new(STORED));
    get_volume_manager().unregister(&volume_id);

    listing.with_listing(|cached| {
        assert_eq!(cached.path.as_path(), Path::new(STORED));
        let paths: Vec<&str> = cached.entries().iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![format!("{STORED}/photo.jpg")],
            "entries carry the stored bytes"
        );
    });
    assert_eq!(
        found_by_server_bytes.len(),
        1,
        "a change reported in the server's bytes must reach the open pane"
    );
}

/// The kernel mount, reading a folder while the direct connection replaces it:
/// the swap lands after this listing started and before it's cached, which is
/// exactly when a pane landing on the share triggers the upgrade
/// (`network::smb_pane_upgrade`).
struct SwappedMidListing {
    kernel: InMemoryVolume,
    volume_id: String,
    successor: std::sync::Mutex<Option<std::sync::Arc<dyn Volume>>>,
}

impl Volume for SwappedMidListing {
    fn name(&self) -> &str {
        self.kernel.name()
    }

    fn root(&self) -> &Path {
        self.kernel.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let successor = self.successor.lock().expect("test lock").take();
            if let Some(successor) = successor {
                crate::network::smb_upgrade::register_replacing_predecessor(&self.volume_id, successor).await;
            }
            self.kernel.list_directory(path, on_progress).await
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.kernel.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> std::pin::Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.kernel.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.kernel.is_directory(path)
    }
}

/// The upgrade's own re-read runs before this listing is cached, so it can't
/// see it: the listing has to notice on its own that the backend that read it
/// is gone, or the pane keeps the kernel's spelling on a byte-exact share.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_read_while_the_backend_is_replaced_lands_on_the_stored_spelling() {
    use crate::file_system::listing::caching_test_support::{TestListingGuard, unique_test_id};
    use crate::file_system::listing::sorting::{DirectorySortMode, SortColumn, SortOrder};
    use crate::file_system::listing::streaming::{
        CollectorListingEventSink, ListingEventSink, StreamingListingState, read_directory_with_progress,
    };
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::test_support::wait_until_async;

    let volume_id = format!("respell-mid-listing-{}", uuid::Uuid::new_v4());
    let kernel = InMemoryVolume::new("Share");
    kernel.create_directory(Path::new(FOREIGN)).await.expect("seed");
    kernel
        .create_file(&Path::new(FOREIGN).join("photo.jpg"), b"jpeg")
        .await
        .expect("seed");
    let direct = SpelledVolume::new(album().await).resolving(FOREIGN, Ok(Some(STORED)));
    let swapped = SwappedMidListing {
        kernel,
        volume_id: volume_id.clone(),
        successor: std::sync::Mutex::new(Some(std::sync::Arc::new(direct))),
    };
    get_volume_manager().register(&volume_id, std::sync::Arc::new(swapped));
    let listing = TestListingGuard::adopt(unique_test_id("respell-mid-listing"));

    let events: std::sync::Arc<dyn ListingEventSink> = std::sync::Arc::new(CollectorListingEventSink::new());
    let state = std::sync::Arc::new(StreamingListingState::new());
    read_directory_with_progress(
        &events,
        listing.id(),
        &state,
        &volume_id,
        Path::new(FOREIGN),
        true,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    )
    .await
    .expect("the kernel mount lists the folder");
    let respelled = || listing.with_listing(|cached| cached.path.as_path() == Path::new(STORED));
    wait_until_async(
        std::time::Duration::from_secs(5),
        "the listing to be respelled",
        respelled,
    )
    .await;
    get_volume_manager().unregister(&volume_id);

    listing.with_listing(|cached| {
        let paths: Vec<&str> = cached.entries().iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![format!("{STORED}/photo.jpg")],
            "entries carry the stored bytes"
        );
    });
}

/// Only a miss is a spelling question: any other refusal is what it is.
#[tokio::test]
async fn a_refusal_other_than_a_miss_is_not_resolved() {
    let volume = SpelledVolume::new(album().await)
        .resolving(FOREIGN, Ok(Some(STORED)))
        .listings_refused_with(VolumeError::PermissionDenied {
            path: FOREIGN.to_string(),
            raw_os_error: None,
        });

    let result = list_as_stored(&volume, Path::new(FOREIGN), None, None).await;

    assert!(
        matches!(result, Err(VolumeError::PermissionDenied { .. })),
        "got {:?}",
        result.err()
    );
    assert_eq!(volume.resolves(), 0, "a refusal is not a spelling question");
}

/// A file dragged in from Finder, or pasted after a copy there: the kernel
/// mount spelled it decomposed, the server stores it composed. The source lands
/// on the server's own spelling before any operation sees it.
#[tokio::test]
async fn a_leaf_from_outside_cmdr_takes_the_stored_spelling() {
    let foreign_leaf = format!("{FOREIGN}/re\u{301}sz.jpg");
    let stored_leaf = format!("{STORED}/r\u{e9}sz.jpg");
    let volume = SpelledVolume::new(album().await).resolving(&foreign_leaf, Ok(Some(&stored_leaf)));

    let paths = stored_spellings(&volume, vec![PathBuf::from(&foreign_leaf)]).await;

    assert_eq!(paths, vec![PathBuf::from(stored_leaf)]);
}

/// An all-ASCII path can't differ in Unicode form, and the kernel mount keeps
/// case, so it's never a round trip. A path whose spelling is already the
/// stored one keeps it.
#[tokio::test]
async fn an_exact_or_ascii_leaf_stays_as_given() {
    let volume = SpelledVolume::new(album().await);
    let exact = format!("{STORED}/photo.jpg");

    let paths = stored_spellings(&volume, vec![PathBuf::from("/plain/photo.jpg"), PathBuf::from(&exact)]).await;

    assert_eq!(paths, vec![PathBuf::from("/plain/photo.jpg"), PathBuf::from(exact)]);
    assert_eq!(volume.resolves(), 1, "only the accented path asks the volume");
}

/// Two look-alikes and no exact match: the path stays as given, so whatever
/// the operation does next means those exact bytes (on a byte-exact share, a
/// plain "couldn't find"), and never lands on a twin it guessed.
#[tokio::test]
async fn a_look_alike_leaf_is_never_guessed() {
    let foreign_leaf = format!("{FOREIGN}/photo.jpg");
    let volume = SpelledVolume::new(album().await)
        .resolving(&foreign_leaf, Err(VolumeError::AmbiguousName(foreign_leaf.clone())));

    let paths = stored_spellings(&volume, vec![PathBuf::from(&foreign_leaf)]).await;

    assert_eq!(paths, vec![PathBuf::from(foreign_leaf)]);
}
