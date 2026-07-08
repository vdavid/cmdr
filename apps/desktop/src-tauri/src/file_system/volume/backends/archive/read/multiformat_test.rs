//! Browse + extract tests for the non-zip archive formats (the tar family and
//! 7z), driven against fixtures built in memory (no checked-in blobs). Encoders
//! come from dev-dependencies; the shipped path stays decode-only.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// Wraps a byte source and counts reads that start at offset 0. A sequential
/// decoder (compressed tar, 7z) reads front-to-back, so each independent decode
/// pass begins with exactly one `read_at(0, …)`; counting those counts decode
/// passes. Used to prove the one-pass extractor decodes the stream once, not
/// once per file.
struct CountingSource {
    inner: Arc<dyn ArchiveByteSource>,
    zero_offset_reads: Arc<AtomicUsize>,
}

impl ArchiveByteSource for CountingSource {
    fn size(&self) -> u64 {
        self.inner.size()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        if offset == 0 && !buf.is_empty() {
            self.zero_offset_reads.fetch_add(1, Ordering::SeqCst);
        }
        self.inner.read_at(offset, buf)
    }
}

/// Drains a one-pass subtree extractor into a map of inner path → decoded bytes.
async fn drain_subtree(reader: &mut SubtreeExtractReader) -> HashMap<String, Vec<u8>> {
    let mut got: HashMap<String, Vec<u8>> = HashMap::new();
    while let Some(member) = reader.next_member().await.expect("next_member") {
        let mut data = Vec::new();
        while let Some(chunk) = reader.next_chunk().await.expect("next_chunk") {
            data.extend_from_slice(&chunk);
        }
        got.insert(member.inner_path, data);
    }
    got
}

// ---- Fixture builders ------------------------------------------------------

/// A tar entry to build into a fixture.
enum TarItem<'a> {
    File(&'a str, &'a [u8]),
    Dir(&'a str),
    Symlink(&'a str, &'a str),
}

/// Builds a plain tar byte stream from `items` using the `tar` crate.
fn build_tar(items: &[TarItem]) -> Vec<u8> {
    let mut builder = ::tar::Builder::new(Vec::new());
    for item in items {
        let mut header = ::tar::Header::new_ustar();
        header.set_mtime(1_700_000_000);
        header.set_mode(0o644);
        match item {
            TarItem::File(name, data) => {
                header.set_size(data.len() as u64);
                header.set_entry_type(::tar::EntryType::Regular);
                header.set_cksum();
                builder.append_data(&mut header, name, *data).expect("append file");
            }
            TarItem::Dir(name) => {
                header.set_size(0);
                header.set_mode(0o755);
                header.set_entry_type(::tar::EntryType::Directory);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, std::io::empty())
                    .expect("append dir");
            }
            TarItem::Symlink(name, target) => {
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
    builder.into_inner().expect("finish tar")
}

/// Compresses a plain tar with the given codec's ENCODER (dev-only), producing
/// the bytes the matching decoder in [`super::format`] must round-trip.
fn compress(codec: TarCodec, plain: &[u8]) -> Vec<u8> {
    match codec {
        TarCodec::Plain => plain.to_vec(),
        TarCodec::Gzip => {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(plain).expect("gz write");
            enc.finish().expect("gz finish")
        }
        TarCodec::Bzip2 => {
            let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
            enc.write_all(plain).expect("bz write");
            enc.finish().expect("bz finish")
        }
        TarCodec::Xz => {
            let mut out = Vec::new();
            let mut enc = lzma_rust2::XzWriter::new(&mut out, lzma_rust2::XzOptions::with_preset(6)).expect("xz new");
            enc.write_all(plain).expect("xz write");
            enc.finish().expect("xz finish");
            out
        }
        TarCodec::Zstd => zstd::encode_all(plain, 3).expect("zstd encode"),
    }
}

/// Parses a tar fixture (in the given codec), returning the index and the byte
/// source the extraction reads from.
fn parse_tar(codec: TarCodec, items: &[TarItem]) -> (ArchiveIndex, Arc<dyn ArchiveByteSource>) {
    let bytes = compress(codec, &build_tar(items));
    let src: Arc<dyn ArchiveByteSource> = Arc::new(BytesSource::new(bytes));
    let index = ArchiveIndex::parse(Arc::clone(&src), ArchiveFormat::Tar(codec)).expect("parse tar");
    (index, src)
}

/// A minimal raw ustar entry (header + padded data), used to inject a hostile
/// name the `tar` crate's `append_data` would refuse — the tar analog of the zip
/// byte-patch fixtures.
fn raw_ustar_entry(name: &str, data: &[u8]) -> Vec<u8> {
    let mut header = [0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    set_octal(&mut header[100..108], 0o644); // mode
    set_octal(&mut header[124..136], data.len() as u64); // size
    set_octal(&mut header[136..148], 1_700_000_000); // mtime
    header[156] = b'0'; // typeflag: regular file
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    // Checksum over the header with the checksum field as spaces.
    header[148..156].copy_from_slice(b"        ");
    let sum: u32 = header.iter().map(|&b| b as u32).sum();
    let chk = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(chk.as_bytes());

    let mut out = header.to_vec();
    out.extend_from_slice(data);
    // Pad data to a 512-byte boundary.
    let pad = (512 - data.len() % 512) % 512;
    out.extend(std::iter::repeat_n(0u8, pad));
    out
}

/// Writes `val` as zero-padded octal into `field`, NUL-terminated (the ustar
/// numeric-field convention every tar reader accepts).
fn set_octal(field: &mut [u8], val: u64) {
    let width = field.len() - 1;
    let text = format!("{val:0width$o}");
    field[..width].copy_from_slice(&text.as_bytes()[..width]);
    field[width] = 0;
}

/// Drains an entry reader to completion.
async fn read_all(index: &ArchiveIndex, src: Arc<dyn ArchiveByteSource>, inner: &str) -> Result<Vec<u8>, ArchiveError> {
    let mut reader = index.open_read(inner, src)?;
    let mut out = Vec::new();
    while let Some(chunk) = reader.next_chunk().await {
        out.extend_from_slice(&chunk?);
    }
    Ok(out)
}

// ---- tar: browse -----------------------------------------------------------

#[test]
fn tar_browses_a_synthetic_tree() {
    let (index, _src) = parse_tar(
        TarCodec::Plain,
        &[
            TarItem::File("top.txt", b"hi"),
            TarItem::Dir("dir/"),
            TarItem::File("dir/inner.bin", b"deep"),
            // No explicit `implied/` entry: the directory must be synthesized.
            TarItem::File("implied/sub/leaf.txt", b"leaf"),
        ],
    );

    let names = |p: &str| -> Vec<String> { index.list(p).unwrap().into_iter().map(|n| n.name).collect() };
    assert_eq!(names(""), vec!["dir", "implied", "top.txt"]);
    assert_eq!(names("dir"), vec!["inner.bin"]);
    assert_eq!(names("implied"), vec!["sub"]);
    assert_eq!(names("implied/sub"), vec!["leaf.txt"]);
    assert_eq!(index.get("top.txt").unwrap().size, Some(2));
    assert!(index.is_directory("implied").unwrap(), "synthesized dir");
}

// ---- tar: extract (random-access plain vs prefix-decode compressed) --------

#[tokio::test]
async fn tar_each_codec_round_trips_a_file() {
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    for codec in [
        TarCodec::Plain,
        TarCodec::Gzip,
        TarCodec::Bzip2,
        TarCodec::Xz,
        TarCodec::Zstd,
    ] {
        let (index, src) = parse_tar(
            codec,
            &[TarItem::File("a.txt", b"first"), TarItem::File("big.bin", &big)],
        );
        // A member after a prior one: for a compressed tar this exercises the
        // prefix-decode reaching a later member.
        let got = read_all(&index, src, "big.bin").await.expect("extract");
        assert_eq!(got, big, "codec {codec:?} must round-trip the bytes");
    }
}

#[tokio::test]
async fn tar_extract_streams_in_bounded_chunks() {
    // A large member must arrive in several ≤128 KiB chunks (never whole-buffered).
    let content: Vec<u8> = (0..400_000u32).map(|i| (i % 253) as u8).collect();
    let (index, src) = parse_tar(TarCodec::Gzip, &[TarItem::File("big.bin", &content)]);
    let mut reader = index.open_read("big.bin", src).unwrap();
    let mut chunk_sizes = Vec::new();
    let mut data = Vec::new();
    while let Some(chunk) = reader.next_chunk().await {
        let chunk = chunk.unwrap();
        chunk_sizes.push(chunk.len());
        data.extend_from_slice(&chunk);
    }
    assert_eq!(data, content);
    assert!(chunk_sizes.len() > 1, "must arrive in multiple chunks");
    assert!(
        chunk_sizes.iter().all(|&n| n <= 128 * 1024),
        "chunk bound: {chunk_sizes:?}"
    );
}

// ---- tar: Zip Slip (data-safety) -------------------------------------------

#[test]
fn tar_traversal_entry_is_quarantined_not_browsable() {
    // A safe entry plus a hostile `../evil.txt` injected as a raw ustar header.
    let mut bytes = build_tar(&[TarItem::File("safe.txt", b"ok")]);
    // Splice the hostile entry in BEFORE the two-block terminator the builder
    // appended (1024 trailing zero bytes).
    bytes.truncate(bytes.len() - 1024);
    bytes.extend(raw_ustar_entry("../evil.txt", b"pwned"));
    bytes.extend(std::iter::repeat_n(0u8, 1024));

    let src: Arc<dyn ArchiveByteSource> = Arc::new(BytesSource::new(bytes));
    let index = ArchiveIndex::parse(src, ArchiveFormat::Tar(TarCodec::Plain)).expect("parse");

    assert_eq!(
        index.list("").unwrap().into_iter().map(|n| n.name).collect::<Vec<_>>(),
        vec!["safe.txt"],
        "the traversal entry never enters the tree"
    );
    assert!(!index.exists("../evil.txt"));
    assert_eq!(index.quarantined().len(), 1, "hostile name recorded as quarantined");
}

#[tokio::test]
async fn tar_symlink_entry_is_marked_and_creates_no_symlink() {
    // A symlink entry's data is empty (its target lives in the header), so
    // extraction yields a benign empty file — never a symlink pointing out of the
    // extraction root.
    let (index, src) = parse_tar(
        TarCodec::Plain,
        &[
            TarItem::Symlink("link", "../../etc/passwd"),
            TarItem::File("real.txt", b"data"),
        ],
    );
    let node = index.get("link").expect("symlink node");
    assert!(node.is_symlink, "symlink is flagged for the UI");
    assert!(!node.is_dir);
    let bytes = read_all(&index, src, "link").await.expect("extract symlink");
    assert!(
        bytes.is_empty(),
        "a symlink entry extracts to no content, never a symlink"
    );
}

// ---- one-pass subtree extract (the O(n²) → one-pass fix) -------------------

#[tokio::test]
async fn one_pass_subtree_extract_decodes_a_compressed_tar_once() {
    // A compressed tar has no random access: a per-entry read re-decodes the
    // prefix, so extracting N files is N decode passes (O(n²)). The one-pass
    // extractor must decode the stream ONCE for the whole subtree.
    let big: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let plain = build_tar(&[
        TarItem::File("docs/a.txt", b"alpha"),
        TarItem::File("docs/sub/b.txt", b"bravo"),
        TarItem::File("docs/c.bin", &big),
        // Outside the extracted subtree: must NOT be delivered.
        TarItem::File("other/x.txt", b"nope"),
    ]);
    let bytes = compress(TarCodec::Gzip, &plain);
    let zero_reads = Arc::new(AtomicUsize::new(0));
    let src: Arc<dyn ArchiveByteSource> = Arc::new(CountingSource {
        inner: Arc::new(BytesSource::new(bytes)),
        zero_offset_reads: Arc::clone(&zero_reads),
    });
    let index = ArchiveIndex::parse(Arc::clone(&src), ArchiveFormat::Tar(TarCodec::Gzip)).expect("parse");

    // The parse itself scans the whole stream once; measure only the extract.
    let baseline = zero_reads.load(Ordering::SeqCst);
    let mut reader = index.open_subtree_extract("docs", Arc::clone(&src));
    let got = drain_subtree(&mut reader).await;
    let extract_passes = zero_reads.load(Ordering::SeqCst) - baseline;

    assert_eq!(
        extract_passes, 1,
        "a 3-file subtree must decode the stream exactly once; decode-pass count was {extract_passes}"
    );
    assert_eq!(
        got.keys().cloned().collect::<std::collections::BTreeSet<_>>(),
        ["docs/a.txt", "docs/c.bin", "docs/sub/b.txt"]
            .iter()
            .map(|s| s.to_string())
            .collect::<std::collections::BTreeSet<_>>(),
        "only the subtree's files, never other/x.txt"
    );
    assert_eq!(got["docs/a.txt"], b"alpha");
    assert_eq!(got["docs/sub/b.txt"], b"bravo");
    assert_eq!(got["docs/c.bin"], big, "a later, chunk-spanning member round-trips");
}

#[tokio::test]
async fn one_pass_subtree_extract_skipping_a_member_still_reads_the_rest() {
    // The copy engine skips a subtree file (conflict resolution said skip) by
    // advancing to the next member WITHOUT reading its bytes. `next_member` must
    // drain the skipped member's unread chunks so the stream stays aligned and the
    // following member decodes correctly.
    let big: Vec<u8> = (0..200_000u32).map(|i| (i % 241) as u8).collect();
    let (index, src) = parse_tar(
        TarCodec::Gzip,
        &[
            TarItem::File("docs/skip.bin", &big),
            TarItem::File("docs/keep.txt", b"kept"),
        ],
    );
    let mut reader = index.open_subtree_extract("docs", src);

    // First member: take the header but DON'T drain its chunks (a skip).
    let first = reader.next_member().await.expect("next_member").expect("a member");
    assert_eq!(first.inner_path, "docs/skip.bin");

    // Advancing must drain the skipped member's data, then hand us the next one.
    let second = reader.next_member().await.expect("next_member").expect("a member");
    assert_eq!(second.inner_path, "docs/keep.txt");
    let mut data = Vec::new();
    while let Some(chunk) = reader.next_chunk().await.expect("next_chunk") {
        data.extend_from_slice(&chunk);
    }
    assert_eq!(data, b"kept", "the member after a skipped one decodes correctly");
    assert!(
        reader.next_member().await.expect("next_member").is_none(),
        "subtree is exhausted"
    );
}

#[tokio::test]
async fn one_pass_subtree_extract_decodes_a_solid_7z_once() {
    let big: Vec<u8> = (0..80_000u32).map(|i| (i % 247) as u8).collect();
    let bytes = build_7z_solid(&[
        ("docs/a.txt", b"alpha"),
        ("docs/deep/b.bin", &big),
        ("other/x.txt", b"nope"),
    ]);
    let zero_reads = Arc::new(AtomicUsize::new(0));
    let src: Arc<dyn ArchiveByteSource> = Arc::new(CountingSource {
        inner: Arc::new(BytesSource::new(bytes)),
        zero_offset_reads: Arc::clone(&zero_reads),
    });
    let index = ArchiveIndex::parse(Arc::clone(&src), ArchiveFormat::SevenZ).expect("parse 7z");

    let baseline = zero_reads.load(Ordering::SeqCst);
    let mut reader = index.open_subtree_extract("docs", Arc::clone(&src));
    let got = drain_subtree(&mut reader).await;
    let extract_passes = zero_reads.load(Ordering::SeqCst) - baseline;

    assert_eq!(
        extract_passes, 1,
        "a solid 7z subtree extract decodes once, got {extract_passes}"
    );
    assert_eq!(got.len(), 2, "only the subtree's two files");
    assert_eq!(got["docs/a.txt"], b"alpha");
    assert_eq!(got["docs/deep/b.bin"], big);
}

// ---- 7z --------------------------------------------------------------------

/// Builds a 7z archive from `(name, data)` files, each pushed separately. The
/// writer needs `Write + Seek`, so it builds into a `Cursor`.
fn build_7z(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).expect("7z writer");
    for (name, data) in files {
        let entry = sevenz_rust2::ArchiveEntry::new_file(name);
        writer.push_archive_entry(entry, Some(*data)).expect("push entry");
    }
    writer.finish().expect("finish 7z").into_inner()
}

/// Builds a SOLID 7z: all files in one compression block (`push_archive_entries`).
fn build_7z_solid(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(std::io::Cursor::new(Vec::new())).expect("7z writer");
    let entries: Vec<_> = files
        .iter()
        .map(|(n, _)| sevenz_rust2::ArchiveEntry::new_file(n))
        .collect();
    let readers: Vec<sevenz_rust2::SourceReader<&[u8]>> = files.iter().map(|(_, d)| (*d).into()).collect();
    writer.push_archive_entries(entries, readers).expect("push solid");
    writer.finish().expect("finish 7z").into_inner()
}

fn parse_7z(bytes: Vec<u8>) -> (ArchiveIndex, Arc<dyn ArchiveByteSource>) {
    let src: Arc<dyn ArchiveByteSource> = Arc::new(BytesSource::new(bytes));
    let index = ArchiveIndex::parse(Arc::clone(&src), ArchiveFormat::SevenZ).expect("parse 7z");
    (index, src)
}

#[tokio::test]
async fn sevenz_browses_and_extracts() {
    let (index, src) = parse_7z(build_7z(&[("readme.txt", b"hello 7z"), ("dir/inner.bin", b"nested")]));

    let names = |p: &str| -> Vec<String> { index.list(p).unwrap().into_iter().map(|n| n.name).collect() };
    assert_eq!(names(""), vec!["dir", "readme.txt"]);
    assert_eq!(names("dir"), vec!["inner.bin"]);
    assert_eq!(index.get("readme.txt").unwrap().size, Some(8));

    let got = read_all(&index, src, "readme.txt").await.expect("extract");
    assert_eq!(got, b"hello 7z");
}

#[tokio::test]
async fn sevenz_solid_block_extracts_a_later_member() {
    // A solid block concatenates entries into one stream; reaching the LAST one
    // decodes the block prefix in front of it. Proves the block-prefix decode.
    let last: Vec<u8> = (0..50_000u32).map(|i| (i % 249) as u8).collect();
    let (index, src) = parse_7z(build_7z_solid(&[
        ("a.txt", b"alpha"),
        ("b.txt", b"bravo"),
        ("c.bin", &last),
    ]));

    assert_eq!(read_all(&index, Arc::clone(&src), "a.txt").await.unwrap(), b"alpha");
    assert_eq!(read_all(&index, src, "c.bin").await.unwrap(), last);
}
