//! Spotting Nextcloud's chunked upload, its URLs, and how a file splits.

use url::Url;

use super::{ChunkedUploads, chunk_name, chunks};

const MIB: u64 = 1024 * 1024;

fn detect(base: &str, user_id: Option<&str>) -> Option<ChunkedUploads> {
    ChunkedUploads::detect(&Url::parse(base).expect("a valid test URL"), user_id)
}

#[test]
fn the_legacy_endpoint_uploads_under_the_user_id_the_server_named() {
    let uploads = detect("https://cloud.example.com/remote.php/webdav/", Some("test_user_1")).expect("Nextcloud");
    assert_eq!(
        uploads.session("cmdr-1").as_str(),
        "https://cloud.example.com/remote.php/dav/uploads/test_user_1/cmdr-1/"
    );
    assert_eq!(
        uploads.chunk("cmdr-1", 7).as_str(),
        "https://cloud.example.com/remote.php/dav/uploads/test_user_1/cmdr-1/00007"
    );
    assert_eq!(
        uploads.assembled("cmdr-1").as_str(),
        "https://cloud.example.com/remote.php/dav/uploads/test_user_1/cmdr-1/.file"
    );
    // ❗ The legacy endpoint's paths re-spelled on the `dav/files` one: the
    // uploads collection lives under `remote.php/dav`, and sabre/dav refuses a
    // `Destination` outside its own base.
    assert_eq!(
        uploads.destination("/Movies/a b#1.mkv").as_str(),
        "https://cloud.example.com/remote.php/dav/files/test_user_1/Movies/a%20b%231.mkv"
    );
}

#[test]
fn the_files_endpoint_keeps_its_own_spelling_for_the_destination() {
    let uploads = detect(
        "http://127.0.0.1:13482/remote.php/dav/files/test_user_1/",
        Some("test_user_1"),
    )
    .expect("Nextcloud");
    assert_eq!(
        uploads.destination("/x.bin").as_str(),
        "http://127.0.0.1:13482/remote.php/dav/files/test_user_1/x.bin"
    );
    assert_eq!(
        uploads.session("s").as_str(),
        "http://127.0.0.1:13482/remote.php/dav/uploads/test_user_1/s/"
    );
}

#[test]
fn a_subfolder_install_and_a_deeper_base_keep_both_halves() {
    let uploads = detect(
        "https://example.com/nextcloud/remote.php/webdav/Photos/",
        Some("test_user_1"),
    )
    .expect("Nextcloud");
    assert_eq!(
        uploads.session("s").as_str(),
        "https://example.com/nextcloud/remote.php/dav/uploads/test_user_1/s/"
    );
    assert_eq!(
        uploads.destination("/2026/a.jpg").as_str(),
        "https://example.com/nextcloud/remote.php/dav/files/test_user_1/Photos/2026/a.jpg"
    );
}

#[test]
fn a_user_id_that_is_an_email_is_encoded_as_one_segment() {
    let uploads = detect(
        "https://c.example.com/remote.php/webdav/",
        Some("test_user_1@example.com"),
    )
    .expect("Nextcloud");
    assert_eq!(
        uploads.session("s").as_str(),
        "https://c.example.com/remote.php/dav/uploads/test_user_1%40example.com/s/"
    );
    assert_eq!(
        uploads.destination("/a").as_str(),
        "https://c.example.com/remote.php/dav/files/test_user_1%40example.com/a"
    );
}

#[test]
fn anything_else_uploads_in_one_piece() {
    // No `X-User-Id`: not Nextcloud, or nobody signed in.
    assert!(detect("https://cloud.example.com/remote.php/webdav/", None).is_none());
    assert!(detect("https://cloud.example.com/remote.php/webdav/", Some("")).is_none());
    // A header on a server whose URL has none of Nextcloud's endpoints.
    assert!(detect("http://testnas.local/dav/", Some("test_user_1")).is_none());
    // A public share has no uploads home.
    assert!(detect("https://cloud.example.com/public.php/webdav/", Some("test_user_1")).is_none());
    // The DAV root itself, and a `dav/files` with no user under it.
    assert!(detect("https://cloud.example.com/remote.php/dav/", Some("test_user_1")).is_none());
    assert!(detect("https://cloud.example.com/remote.php/dav/files/", Some("test_user_1")).is_none());
}

#[test]
fn chunk_names_sort_as_numbers_do() {
    assert_eq!(chunk_name(1), "00001");
    assert_eq!(chunk_name(10_000), "10000");
    assert!(chunk_name(9) < chunk_name(10));
}

#[test]
fn a_file_splits_into_numbered_chunks_with_a_short_last_one() {
    let planned: Vec<(u32, u64)> = chunks(150 * MIB, 64 * MIB).collect();
    assert_eq!(planned, vec![(1, 64 * MIB), (2, 64 * MIB), (3, 22 * MIB)]);
    let exact: Vec<(u32, u64)> = chunks(128 * MIB, 64 * MIB).collect();
    assert_eq!(exact, vec![(1, 64 * MIB), (2, 64 * MIB)]);
}

#[test]
fn a_file_too_big_for_ten_thousand_chunks_grows_its_chunks() {
    let size = 10_000 * 64 * MIB + 1;
    let planned: Vec<(u32, u64)> = chunks(size, 64 * MIB).collect();
    assert!(planned.len() <= 10_000, "{} chunks", planned.len());
    assert_eq!(planned.iter().map(|&(_, len)| len).sum::<u64>(), size);
    assert!(planned.windows(2).all(|pair| pair[1].0 == pair[0].0 + 1));
}
