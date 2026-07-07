//! Tests for the one-pass sequential-archive extraction path in
//! `volume_strategy.rs` (`copy_single_path` → `extract_sequential_subtree`):
//! nested-subtree correctness through the plan + single-decode data pass, the
//! random-vs-sequential routing gate, and cancellation between members.
//!
//! The source is a real `ArchiveVolume` over a `.tar.gz` on disk (a compressed
//! tar is sequential-access, so it takes the one-pass path); the destination is
//! an `InMemoryVolume`, so the write lands through the normal `write_from_stream`.

use super::test_support::make_state;
use super::*;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume, TarCodec};
use crate::file_system::volume::{InMemoryVolume, Volume, VolumeError};

use super::super::super::state::OperationIntent;

/// A tar entry to build into a fixture.
enum Item<'a> {
    File(&'a str, &'a [u8]),
    Dir(&'a str),
    Symlink(&'a str, &'a str),
}

/// Builds a gzip-compressed tar from `(name, bytes)` files and writes it to a
/// unique temp path (the local-backed `ArchiveVolume` reads a real file). Cleaned
/// up by [`TarGzFixture`]'s `Drop`.
fn write_targz(files: &[(&str, &[u8])]) -> PathBuf {
    let items: Vec<Item> = files.iter().map(|(n, d)| Item::File(n, d)).collect();
    write_targz_items(&items)
}

/// Like [`write_targz`] but accepts dirs and symlinks too.
fn write_targz_items(items: &[Item]) -> PathBuf {
    let mut builder = ::tar::Builder::new(Vec::new());
    for item in items {
        let mut header = ::tar::Header::new_ustar();
        header.set_mtime(1_700_000_000);
        header.set_mode(0o644);
        match item {
            Item::File(name, data) => {
                header.set_size(data.len() as u64);
                header.set_entry_type(::tar::EntryType::Regular);
                header.set_cksum();
                builder.append_data(&mut header, name, *data).expect("append file");
            }
            Item::Dir(name) => {
                header.set_size(0);
                header.set_mode(0o755);
                header.set_entry_type(::tar::EntryType::Directory);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, std::io::empty())
                    .expect("append dir");
            }
            Item::Symlink(name, target) => {
                header.set_size(0);
                header.set_entry_type(::tar::EntryType::Symlink);
                header.set_link_name(target).expect("set link");
                header.set_cksum();
                builder
                    .append_data(&mut header, name, std::io::empty())
                    .expect("append symlink");
            }
        }
    }
    let plain = builder.into_inner().expect("finish tar");
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&plain).expect("gz write");
    let bytes = enc.finish().expect("gz finish");

    let path = std::env::temp_dir().join(format!("cmdr-seq-extract-{}.tar.gz", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).expect("write fixture");
    path
}

/// Owns a temp `.tar.gz` and hands out an `ArchiveVolume` over it; removes the
/// file on drop.
struct TarGzFixture {
    path: PathBuf,
}

impl TarGzFixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        Self {
            path: write_targz(files),
        }
    }

    fn from_items(items: &[Item]) -> Self {
        Self {
            path: write_targz_items(items),
        }
    }

    fn volume(&self) -> Arc<dyn Volume> {
        // A local-backed parent, so the archive reads its real temp file via the
        // `LocalFileSource` fast path.
        let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
        Arc::new(ArchiveVolume::new(
            parent,
            self.path.clone(),
            ArchiveFormat::Tar(TarCodec::Gzip),
        ))
    }

    fn inner(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

impl Drop for TarGzFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn read_dest(dest: &Arc<dyn Volume>, path: &str) -> Result<Vec<u8>, VolumeError> {
    let mut stream = dest.open_read_stream(Path::new(path)).await?;
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_materializes_a_nested_subtree() {
    // A later, chunk-spanning member proves the single decode reaches deep files.
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let fixture = TarGzFixture::new(&[
        ("docs/a.txt", b"alpha"),
        ("docs/sub/b.txt", b"bravo"),
        ("docs/sub/deep/c.bin", &big),
        // Outside the extracted subtree: must not be materialized.
        ("other/x.txt", b"nope"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();

    let bytes = copy_single_path(
        &source,
        &fixture.inner("docs"),
        true, // source is a directory
        None,
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
    )
    .await
    .expect("sequential extract");

    assert_eq!(bytes, 5 + 5 + big.len() as u64, "total bytes = the three files");
    assert_eq!(read_dest(&dest, "/out/a.txt").await.unwrap(), b"alpha");
    assert_eq!(read_dest(&dest, "/out/sub/b.txt").await.unwrap(), b"bravo");
    assert_eq!(read_dest(&dest, "/out/sub/deep/c.bin").await.unwrap(), big);
    assert!(
        !dest.exists(Path::new("/out/../other/x.txt")).await,
        "files outside the subtree are never extracted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compressed_tar_is_sequential_but_zip_is_random() {
    // The routing gate: `copy_single_path` sends a directory source to the
    // one-pass extractor iff `extraction_is_sequential` is true. A compressed tar
    // is; a zip (and a plain tar) is not, so those keep the per-entry walk.
    let targz = TarGzFixture::new(&[("d/a.txt", b"x")]);
    assert!(
        targz.volume().extraction_is_sequential(&targz.inner("d")),
        "a .tar.gz must take the one-pass path"
    );

    // A zip archive volume over the same-shaped path reports random-access.
    let zip_parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("p").with_local_fs_access());
    let zip_vol = ArchiveVolume::new(zip_parent, PathBuf::from("/tmp/whatever.zip"), ArchiveFormat::Zip);
    assert!(
        !zip_vol.extraction_is_sequential(Path::new("/tmp/whatever.zip/d")),
        "a zip must keep the random-access per-entry path"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_lands_empty_dirs_and_symlinks() {
    // The directory structure (including an EMPTY explicit dir with no file
    // members, and a dir entry appearing AFTER a file already inside it) comes
    // from the parsed tree in the plan pass, not the byte stream. A symlink entry
    // carries no data, so it extracts to a benign empty file — the same as the
    // per-entry path.
    let fixture = TarGzFixture::from_items(&[
        Item::File("docs/sub/b.txt", b"bravo"),
        // The explicit `docs/sub/` dir entry arrives AFTER its child above.
        Item::Dir("docs/sub/"),
        // An empty explicit directory with nothing inside it.
        Item::Dir("docs/empty/"),
        Item::Symlink("docs/link", "../../etc/passwd"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();

    copy_single_path(
        &source,
        &fixture.inner("docs"),
        true,
        None,
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &|_| {},
        None,
    )
    .await
    .expect("extract");

    assert_eq!(read_dest(&dest, "/out/sub/b.txt").await.unwrap(), b"bravo");
    assert!(
        dest.is_directory(Path::new("/out/empty")).await.unwrap_or(false),
        "the empty explicit directory lands even with no file members"
    );
    // A symlink entry extracts to an empty file, never a symlink out of the root.
    assert_eq!(
        read_dest(&dest, "/out/link").await.unwrap(),
        Vec::<u8>::new(),
        "a symlink member extracts to a benign empty file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sequential_extract_cancels_between_members() {
    let fixture = TarGzFixture::new(&[
        ("docs/a.txt", b"alpha"),
        ("docs/b.txt", b"bravo"),
        ("docs/c.txt", b"charlie"),
    ]);
    let source = fixture.volume();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));
    let state = make_state();

    // Cancel right after the FIRST file completes: the data pass's between-member
    // `is_cancelled` check must then stop before writing the second file.
    let completed = Arc::new(AtomicUsize::new(0));
    let on_complete = {
        let completed = Arc::clone(&completed);
        let state = Arc::clone(&state);
        move |_bytes: u64| {
            if completed.fetch_add(1, Ordering::SeqCst) == 0 {
                state.intent.store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            }
        }
    };

    let result = copy_single_path(
        &source,
        &fixture.inner("docs"),
        true,
        None,
        &dest,
        Path::new("/out"),
        &state,
        &CreatedPaths::default(),
        &|_, _| ControlFlow::Continue(()),
        &on_complete,
        None,
    )
    .await;

    assert!(
        matches!(result, Err(VolumeError::Cancelled(_))),
        "cancel mid-extract surfaces as Cancelled, got {result:?}"
    );
    assert_eq!(completed.load(Ordering::SeqCst), 1, "exactly one member was written");
    assert_eq!(
        read_dest(&dest, "/out/a.txt").await.unwrap(),
        b"alpha",
        "first file landed"
    );
    assert!(!dest.exists(Path::new("/out/b.txt")).await, "second file never started");
    assert!(!dest.exists(Path::new("/out/c.txt")).await, "third file never started");
}
