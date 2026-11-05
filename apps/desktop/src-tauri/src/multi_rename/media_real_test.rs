//! A session's media read end to end: real files on the local disk, read through
//! the volume manager's `root` volume, `VolumeByteSource`, and `read_media_from`,
//! then rendered by the preview. `media_test.rs` pins the loader with a stand-in
//! reader; this proves the real path behind it fits together.

use std::sync::Arc;
use std::time::Duration;

use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::LocalPosixVolume;
use cmdr_local::reading::list_directory_core;
use cmdr_media_meta::MediaField;
use cmdr_media_meta::test_fixtures::{apple_meta, atom, concat, exif_jpeg, ftyp, id3_mp3_with_cover, mvhd_v0};
use exif::{Field, In, Tag, Value};

use super::app_host::app_multi_rename;
use super::plan::{MultiRenameSpec, RowStatus};
use super::run_test::strip_diacritics;
use super::session::MultiRenamePreview;
use crate::file_system::volume::manager::get_volume_manager;
use crate::test_support::{TestDir, wait_until_async};

/// Registers the real local disk as `root`, the volume a listing of a scratch dir names.
fn ensure_root_volume() {
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

/// A scratch folder holding `files`, and a cached listing of it.
fn folder(tag: &str, files: &[(&str, Vec<u8>)]) -> (TestDir, TestListingGuard) {
    let dir = TestDir::new(tag);
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists"))
        .insert(crate::listing_host::listings(), tag);
    (dir, listing)
}

fn photo(taken: &str) -> Vec<u8> {
    exif_jpeg(&[Field {
        tag: Tag::DateTimeOriginal,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![taken.as_bytes().to_vec()]),
    }])
}

/// A QuickTime movie whose Apple creation date says `created`, its `moov` after the frames.
fn movie(created: &str) -> Vec<u8> {
    let moov = atom(
        b"moov",
        &concat(&[
            &mvhd_v0(0),
            &apple_meta(&[("com.apple.quicktime.creationdate", created)]),
        ]),
    );
    concat(&[&ftyp(b"qt  "), &atom(b"mdat", &[0; 4096]), &moov])
}

fn masked(name_mask: &str) -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: name_mask.to_string(),
        remove_diacritics: false,
        ..strip_diacritics()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn photos_movies_and_songs_on_the_local_disk_name_themselves() {
    ensure_root_volume();
    let (_dir, listing) = folder(
        "media-real",
        &[
            ("IMG_0001.jpg", photo("2026:07:14 09:05:30")),
            ("clip.mov", movie("2025-12-24T18:00:00+0100")),
            // A 3 MB cover the read skips.
            ("song.mp3", id3_mp3_with_cover(3 * 1024 * 1024)),
            ("notes.txt", b"not media".to_vec()),
        ],
    );
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    app_multi_rename()
        .start_media(&session.session_id, Arc::new(|_| {}))
        .expect("the read starts");
    // Under nextest's 8 s limit, so a stuck read fails with this wait's message.
    wait_until_async(Duration::from_secs(5), "the media read to finish", || {
        app_multi_rename()
            .media_progress(&session.session_id)
            .is_ok_and(|p| p.done)
    })
    .await;
    let progress = app_multi_rename()
        .media_progress(&session.session_id)
        .expect("progress");
    assert_eq!((progress.read, progress.total), (3, 3));

    let dated = app_multi_rename()
        .preview(&session.session_id, &masked("[=exif.taken:d] [N]"))
        .expect("a preview");
    assert_eq!(
        row(&dated, "IMG_0001.jpg"),
        ("2026-07-14 IMG_0001.jpg", &RowStatus::Ready)
    );
    assert_eq!(row(&dated, "clip.mov"), ("2025-12-24 clip.mov", &RowStatus::Ready));
    let no_date = RowStatus::NoValue {
        field: MediaField::ExifTaken,
    };
    assert_eq!(row(&dated, "notes.txt"), ("notes.txt", &no_date));

    let songs = app_multi_rename()
        .preview(&session.session_id, &masked("[=tag.artist] - [=tag.title]"))
        .expect("a preview");
    assert_eq!(
        row(&songs, "song.mp3"),
        ("Daft Punk - Digital Love.mp3", &RowStatus::Ready)
    );
    let no_artist = RowStatus::NoValue {
        field: MediaField::TagArtist,
    };
    assert_eq!(row(&songs, "notes.txt"), ("notes.txt", &no_artist));
    app_multi_rename().close(&session.session_id);
}

/// The new name and status of the row whose old name is `old_name`.
fn row<'a>(preview: &'a MultiRenamePreview, old_name: &str) -> (&'a str, &'a RowStatus) {
    let row = preview
        .rows
        .iter()
        .find(|row| row.old_name == old_name)
        .expect("the preview lists the file");
    (row.new_name.as_str(), &row.status)
}
