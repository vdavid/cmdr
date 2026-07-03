//! Behaviour tests for the archive reading core, driven against real fixture
//! zips (built in-test) and hostile byte-patched variants.

use std::sync::Arc;

use super::test_fixtures::{
    build_zip, deflated, dir, overstate_record_count, patch_equal_len, set_first_entry_encrypted, stored,
};
use super::*;

fn bytes_source(bytes: Vec<u8>) -> Arc<dyn ArchiveByteSource> {
    Arc::new(BytesSource::new(bytes))
}

fn parse(bytes: &[u8]) -> Result<ArchiveIndex, ArchiveError> {
    let src = BytesSource::new(bytes.to_vec());
    ArchiveIndex::parse(&src)
}

fn names(index: &ArchiveIndex, dir_path: &str) -> Vec<String> {
    index.list(dir_path).unwrap().into_iter().map(|n| n.name).collect()
}

async fn read_all(reader: &mut ArchiveEntryReader) -> Result<(Vec<u8>, Vec<usize>), ArchiveError> {
    let mut data = Vec::new();
    let mut chunk_sizes = Vec::new();
    while let Some(chunk) = reader.next_chunk().await {
        let chunk = chunk?;
        chunk_sizes.push(chunk.len());
        data.extend_from_slice(&chunk);
    }
    Ok((data, chunk_sizes))
}

// ---- Central-directory parse + listing ------------------------------------

#[test]
fn lists_top_level_and_nested_entries() {
    let zip = build_zip(&[
        stored("a.txt", "hello"),
        deflated("dir/b.txt", "world"),
        deflated("dir/sub/c.txt", "deep"),
    ]);
    let index = parse(&zip).unwrap();

    // Directories first, then files, at the root.
    assert_eq!(names(&index, ""), vec!["dir", "a.txt"]);
    assert_eq!(names(&index, "dir"), vec!["sub", "b.txt"]);
    assert_eq!(names(&index, "dir/sub"), vec!["c.txt"]);

    let a = index.get("a.txt").unwrap();
    assert_eq!(a.size, Some(5));
    assert!(!a.is_dir);
    assert!(index.is_directory("dir").unwrap());
    assert!(!index.is_directory("a.txt").unwrap());
    assert!(index.is_directory("nope").is_none());
    assert!(index.exists("dir/sub/c.txt"));
    assert!(!index.exists("dir/missing"));
    // Leading/trailing slashes normalize to the same node.
    assert!(index.get("/dir/b.txt/").is_some());
}

#[test]
fn synthesizes_directories_without_explicit_entries() {
    // Only file entries; the `x/` and `x/y/` directories are implied.
    let zip = build_zip(&[deflated("x/y/z.txt", "zed")]);
    let index = parse(&zip).unwrap();

    assert_eq!(names(&index, ""), vec!["x"]);
    assert_eq!(names(&index, "x"), vec!["y"]);
    assert_eq!(names(&index, "x/y"), vec!["z.txt"]);
    // A synthetic directory has no timestamp.
    assert_eq!(index.get("x").unwrap().modified, None);
    assert!(index.get("x").unwrap().is_dir);
}

#[test]
fn explicit_directory_entry_carries_a_timestamp() {
    let zip = build_zip(&[dir("docs/"), deflated("docs/readme.md", "hi")]);
    let index = parse(&zip).unwrap();
    let docs = index.get("docs").unwrap();
    assert!(docs.is_dir);
    assert!(docs.modified.is_some(), "explicit dir entry should carry an mtime");
    assert_eq!(names(&index, "docs"), vec!["readme.md"]);
}

// ---- Zip Slip (data-safety) -----------------------------------------------

#[test]
fn zip_slip_traversal_entry_is_quarantined_not_browsable() {
    // Build with a placeholder name of equal length, then patch it to a
    // traversal path (the `zip` writer won't emit `..` names directly).
    let mut zip = build_zip(&[stored("safe.txt", "ok"), stored("zz/evil.txt", "pwned")]);
    patch_equal_len(&mut zip, b"zz/evil.txt", b"../evil.txt");

    let index = parse(&zip).unwrap();

    // The traversal entry never enters the tree.
    assert_eq!(names(&index, ""), vec!["safe.txt"]);
    assert!(!index.exists("../evil.txt"));
    assert!(index.get("../evil.txt").is_none());
    // No node's path escapes the archive root.
    // (The only real node besides the root is `safe.txt`.)
    assert!(index.list("").unwrap().iter().all(|n| !n.path.contains("..")));

    // And it's recorded as quarantined for diagnostics.
    let quarantined = index.quarantined();
    assert_eq!(quarantined.len(), 1);
    assert_eq!(quarantined[0].0, "../evil.txt");
    assert_eq!(quarantined[0].1, QuarantineReason::ParentTraversal);
}

// ---- Encrypted / corrupt / not-an-archive → typed errors -------------------

#[tokio::test]
async fn encrypted_entry_is_detected_and_rejected() {
    let mut zip = build_zip(&[deflated("secret.txt", "classified")]);
    set_first_entry_encrypted(&mut zip);

    let index = parse(&zip).unwrap();
    assert!(index.has_encrypted_entries());
    assert!(index.get("secret.txt").unwrap().encrypted);

    // Browsing still worked (name is listed); extraction is rejected.
    let src = bytes_source(zip);
    let err = index.open_read("secret.txt", src).unwrap_err();
    assert!(matches!(err, ArchiveError::Encrypted), "got {err:?}");
}

#[test]
fn corrupt_central_directory_is_a_typed_error() {
    let mut zip = build_zip(&[stored("a.txt", "hi")]);
    overstate_record_count(&mut zip);
    let err = parse(&zip).unwrap_err();
    assert!(matches!(err, ArchiveError::Corrupt(_)), "got {err:?}");
}

#[test]
fn non_zip_bytes_are_not_an_archive() {
    let err = parse(b"this is definitely not a zip file, there is no EOCD anywhere").unwrap_err();
    assert!(matches!(err, ArchiveError::NotAnArchive), "got {err:?}");
    // Empty input too.
    assert!(matches!(parse(b"").unwrap_err(), ArchiveError::NotAnArchive));
}

#[tokio::test]
async fn opening_a_directory_or_missing_path_is_a_typed_error() {
    let zip = build_zip(&[deflated("dir/f.txt", "x")]);
    let index = parse(&zip).unwrap();
    let src = bytes_source(zip);

    let dir_err = index.open_read("dir", Arc::clone(&src)).unwrap_err();
    assert!(matches!(dir_err, ArchiveError::IsADirectory(_)), "got {dir_err:?}");
    let missing_err = index.open_read("nope.txt", src).unwrap_err();
    assert!(matches!(missing_err, ArchiveError::NotFound(_)), "got {missing_err:?}");
}

// ---- Streaming reads -------------------------------------------------------

#[tokio::test]
async fn streams_large_entry_in_bounded_chunks() {
    // ~300 KiB of compressible data: exercises multi-chunk decompression and
    // proves we never buffer the whole entry (each chunk is bounded).
    let content: Vec<u8> = (0..300_000).map(|i| (i % 251) as u8).collect();
    let zip = build_zip(&[deflated("big.bin", content.clone())]);
    let index = parse(&zip).unwrap();
    let src = bytes_source(zip);

    let mut reader = index.open_read("big.bin", src).unwrap();
    assert_eq!(reader.total_size(), content.len() as u64);

    let (data, chunk_sizes) = read_all(&mut reader).await.unwrap();
    assert_eq!(data, content, "decompressed content must match");
    assert_eq!(reader.bytes_read(), content.len() as u64);
    assert!(chunk_sizes.len() > 1, "a 300 KiB entry must arrive in several chunks");
    assert!(
        chunk_sizes.iter().all(|&n| n <= 128 * 1024),
        "no chunk may exceed the 128 KiB bound (proves no whole-entry buffer): {chunk_sizes:?}"
    );
}

#[tokio::test]
async fn reads_stored_entry_bytes_exactly() {
    let content = b"stored, not compressed".to_vec();
    let zip = build_zip(&[stored("plain.txt", content.clone())]);
    let index = parse(&zip).unwrap();
    let src = bytes_source(zip);

    let mut reader = index.open_read("plain.txt", src).unwrap();
    let (data, _) = read_all(&mut reader).await.unwrap();
    assert_eq!(data, content);
}

#[tokio::test]
async fn truncated_entry_data_errors_instead_of_hanging() {
    // Index from the full archive, then read through a source truncated inside
    // the entry's data (the central directory is gone too). The reader must
    // surface a typed error, not spin forever on repeated EOF reads.
    let content: Vec<u8> = (0..200_000).map(|i| (i % 251) as u8).collect();
    let full = build_zip(&[stored("big.bin", content)]);
    let index = parse(&full).unwrap();

    let truncated = full[..full.len() / 2].to_vec();
    let src = bytes_source(truncated);
    let mut reader = index.open_read("big.bin", src).unwrap();

    let result = read_all(&mut reader).await;
    assert!(matches!(result, Err(ArchiveError::Corrupt(_))), "got {result:?}");
}

#[tokio::test]
async fn concurrent_reads_are_independent() {
    let a_content: Vec<u8> = (0..200_000).map(|i| (i % 7) as u8).collect();
    let b_content: Vec<u8> = (0..200_000).map(|i| (i % 13) as u8).collect();
    let zip = build_zip(&[
        deflated("a.bin", a_content.clone()),
        deflated("b.bin", b_content.clone()),
    ]);
    let index = parse(&zip).unwrap();
    let src = bytes_source(zip);

    // Two readers over one shared source, driven concurrently.
    let mut ra = index.open_read("a.bin", Arc::clone(&src)).unwrap();
    let mut rb = index.open_read("b.bin", Arc::clone(&src)).unwrap();
    let (a_res, b_res) = tokio::join!(read_all(&mut ra), read_all(&mut rb));

    assert_eq!(a_res.unwrap().0, a_content);
    assert_eq!(b_res.unwrap().0, b_content);
}

// ---- Filename encoding -----------------------------------------------------

#[test]
fn non_utf8_name_is_decoded_best_effort() {
    // Patch an ASCII name to contain a high byte with no UTF-8 flag set, so
    // rc-zip falls back to its heuristic (CP437/Windows-1252) decoder. We don't
    // assert the exact glyph (the heuristic owns that) — only that it decodes to
    // a valid, non-empty name without erroring, and the ASCII parts survive.
    let mut zip = build_zip(&[stored("file_x.txt", "data")]);
    patch_equal_len(&mut zip, b"file_x.txt", b"file_\xe9.txt");

    let index = parse(&zip).unwrap();
    let top = index.list("").unwrap();
    assert_eq!(top.len(), 1);
    let name = &top[0].name;
    assert!(name.starts_with("file_") && name.ends_with(".txt"), "name was {name:?}");
    assert_ne!(
        name, "file_x.txt",
        "the high byte should have decoded to a non-ASCII glyph"
    );
}

// ---- Index cache -----------------------------------------------------------

#[test]
fn cache_reuses_index_then_invalidates_on_change() {
    let path = std::env::temp_dir().join(format!("cmdr-archive-cache-{}.zip", uuid::Uuid::new_v4()));
    std::fs::write(&path, build_zip(&[stored("one.txt", "1")])).unwrap();

    let cache = ArchiveIndexCache::new();
    let first = cache.index_for_local(&path).unwrap();
    let second = cache.index_for_local(&path).unwrap();
    assert!(Arc::ptr_eq(&first, &second), "same file must hit the cache");
    assert_eq!(cache.len(), 1);
    assert_eq!(names(&first, ""), vec!["one.txt"]);

    // Rewrite the archive with a different entry set (different size ⇒ new key).
    std::fs::write(&path, build_zip(&[stored("two.txt", "22"), stored("three.txt", "333")])).unwrap();
    let third = cache.index_for_local(&path).unwrap();
    assert!(!Arc::ptr_eq(&first, &third), "an external edit must miss the cache");
    assert_eq!(names(&third, ""), vec!["three.txt", "two.txt"]);

    cache.clear();
    assert!(cache.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cache_reports_a_typed_error_for_a_non_archive_file() {
    let path = std::env::temp_dir().join(format!("cmdr-archive-bad-{}.bin", uuid::Uuid::new_v4()));
    std::fs::write(&path, b"not a zip").unwrap();
    let cache = ArchiveIndexCache::new();
    let err = cache.index_for_local(&path).unwrap_err();
    assert!(matches!(err, ArchiveError::NotAnArchive), "got {err:?}");
    let _ = std::fs::remove_file(&path);
}
