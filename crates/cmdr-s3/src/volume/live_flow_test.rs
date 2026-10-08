//! The volume's own flows against each real provider, the way the app drives
//! them: connect, list, upload (one PUT and multipart, a folded tail
//! included), refuse an occupied key, overwrite, read back, keep the date,
//! rename, move a folder by server-side copy, copy in parts, share a link that
//! fetches unsigned, and delete. Then throughput: uploads and server-side
//! copies at several part widths, timed.
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh`.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Instant;

use cmdr_fs::volume::{ServerCopyProgress, ShareLinkExpiry, Volume, VolumeError, WriteMode};

use super::S3Volume;
use super::live_support::*;
use super::testing::{BytesSource, distant_mtime, read_back};

/// A copy's progress hook that never pauses or cancels.
struct Silent;

impl ServerCopyProgress for Silent {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

fn at(volume: &S3Volume, key: &str) -> PathBuf {
    volume.root().join(key)
}

async fn write(volume: &S3Volume, path: &Path, mode: WriteMode, bytes: Vec<u8>) -> Result<u64, VolumeError> {
    let source = BytesSource::new(bytes).modified_at(distant_mtime());
    let length = cmdr_fs::volume::VolumeReadStream::total_size(&source);
    volume
        .write_from_stream(path, mode, length, Box::new(source), &|_| ControlFlow::Continue(()))
        .await
}

async fn copy(volume: &S3Volume, from: &str, to: &str) -> Result<u64, VolumeError> {
    volume
        .copy_on_server(
            volume,
            &at(volume, from),
            &at(volume, to),
            WriteMode::CreateNew,
            &Silent,
        )
        .await
}

/// Writes, then reads back and compares: the finding for one upload.
async fn round_trip(volume: &S3Volume, key: &str, bytes: Vec<u8>) -> String {
    let started = Instant::now();
    let path = at(volume, key);
    match write(volume, &path, WriteMode::CreateNew, bytes.clone()).await {
        Ok(_) => {
            let back = read_back(volume, &path).await;
            format!("ok in {}, read back intact: {}", seconds(started), back == bytes)
        }
        Err(e) => format!("FAILED: {e:?}"),
    }
}

/// ❗ The app's flows on each provider, end to end through the volume.
#[tokio::test(flavor = "multi_thread")]
async fn live_volume_flows_end_to_end() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("flows");
        let volume = live
            .connect(Some(&live.bucket))
            .await
            .unwrap_or_else(|e| panic!("[{}] the bucket didn't connect: {e:?}", live.name));

        // One PUT, refused when occupied, then overwritten.
        let small = format!("{prefix}small.txt");
        report(
            &live,
            "upload one PUT",
            round_trip(&volume, &small, b"hello, bucket".to_vec()).await,
        );
        let taken = write(&volume, &at(&volume, &small), WriteMode::CreateNew, b"again".to_vec()).await;
        report(&live, "CreateNew over an occupied key", format!("{taken:?}"));
        assert!(
            matches!(taken, Err(VolumeError::AlreadyExists(_))),
            "[{}] {taken:?}",
            live.name
        );
        breathe().await;
        let replaced = write(
            &volume,
            &at(&volume, &small),
            WriteMode::CreateOrReplace,
            b"replaced!".to_vec(),
        )
        .await;
        let back = read_back(&volume, &at(&volume, &small)).await;
        report(
            &live,
            "CreateOrReplace over an existing object",
            format!("{replaced:?}, reads back {:?}", String::from_utf8_lossy(&back)),
        );
        assert_eq!(back, b"replaced!", "[{}]", live.name);
        let stat = volume.get_metadata(&at(&volume, &small)).await.expect("a stat");
        let kept = stat.modified_at == Some(distant_mtime().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
        report(
            &live,
            "the date kept on upload",
            format!("{kept} ({:?})", stat.modified_at),
        );

        // Multipart: a tail under 5 MiB (R2 refuses it folded into a larger
        // last part), then the production 64 MiB part floor.
        volume.set_part_floor((5 * MIB) as u64);
        report(
            &live,
            "upload 11 MiB in 5 MiB parts (a 1 MiB tail)",
            round_trip(&volume, &format!("{prefix}folded.bin"), pattern(11 * MIB, 1)).await,
        );
        report(
            &live,
            "upload 12 MiB in 5 MiB parts (a 2 MiB tail)",
            round_trip(&volume, &format!("{prefix}twelve.bin"), pattern(12 * MIB, 2)).await,
        );
        volume.set_part_floor(crate::multipart::MIN_PART_SIZE);
        let big = format!("{prefix}big.bin");
        report(
            &live,
            "upload 70 MiB in 64 MiB parts",
            round_trip(&volume, &big, pattern(70 * MIB, 3)).await,
        );

        // Server-side copies: one request, then in parts.
        let started = Instant::now();
        let small_copy = copy(&volume, &small, &format!("{prefix}small-copy.txt")).await;
        report(
            &live,
            "server-side copy, one request",
            format!("{small_copy:?} in {}", seconds(started)),
        );
        let started = Instant::now();
        let big_copy = copy(&volume, &big, &format!("{prefix}big-copy.bin")).await;
        let intact = big_copy.is_ok()
            && read_back(&volume, &at(&volume, &format!("{prefix}big-copy.bin"))).await == pattern(70 * MIB, 3);
        report(
            &live,
            "server-side copy of 70 MiB in parts",
            format!("{big_copy:?} in {}, intact: {intact}", seconds(started)),
        );
        volume.set_part_floor((5 * MIB) as u64);
        let folded_copy = copy(
            &volume,
            &format!("{prefix}folded.bin"),
            &format!("{prefix}folded-copy.bin"),
        )
        .await;
        report(
            &live,
            "server-side copy of 11 MiB in 5 MiB parts (a 1 MiB tail)",
            format!("{folded_copy:?}"),
        );
        volume.set_part_floor(crate::multipart::MIN_PART_SIZE);

        // Rename one small file, then move a folder the way the engine does:
        // copy each file on the server, then batch-delete the sources.
        let renamed = volume
            .rename(
                &at(&volume, &small),
                &at(&volume, &format!("{prefix}renamed.txt")),
                false,
            )
            .await;
        report(&live, "rename one small file", format!("{renamed:?}"));
        for n in 0..3 {
            write(
                &volume,
                &at(&volume, &format!("{prefix}folder/f{n}.txt")),
                WriteMode::CreateNew,
                vec![b'a'; 10 + n],
            )
            .await
            .expect("a folder file lands");
        }
        let mut sources = Vec::new();
        for n in 0..3 {
            let from = format!("{prefix}folder/f{n}.txt");
            copy(&volume, &from, &format!("{prefix}moved/f{n}.txt"))
                .await
                .expect("a folder file copies");
            sources.push(at(&volume, &from));
        }
        let deleted = volume.delete_files(&sources).await;
        let moved = volume
            .list_directory(&at(&volume, &format!("{prefix}moved")), None)
            .await
            .map(|entries| entries.len());
        let left = volume
            .list_directory(&at(&volume, &format!("{prefix}folder")), None)
            .await
            .map(|entries| entries.len());
        report(
            &live,
            "move a folder (copy each, batch delete)",
            format!(
                "deletes ok: {}, moved: {moved:?}, left behind: {left:?}",
                deleted.iter().all(Result::is_ok)
            ),
        );

        // A share link fetched with no credentials, like a browser.
        let link = volume
            .share_link(
                &at(&volume, &format!("{prefix}renamed.txt")),
                ShareLinkExpiry::SevenDays,
            )
            .await
            .expect("a link mints");
        let fetched = cmdr_http::client_builder()
            .build()
            .expect("a plain client builds")
            .get(link.into_url())
            .send()
            .await
            .expect("the link fetches");
        let status = fetched.status();
        let body = fetched.bytes().await.unwrap_or_default();
        report(
            &live,
            "a seven-day share link, fetched unsigned",
            format!("{status}, body intact: {}", body.as_ref() == b"replaced!"),
        );

        // A folder made and deleted, and the listing of what's left.
        let folder = at(&volume, &format!("{prefix}empty"));
        let made = volume.create_directory(&folder).await;
        let removed = volume.delete(&folder).await;
        report(
            &live,
            "make and delete an empty folder",
            format!("{made:?}, {removed:?}"),
        );
        let listed = volume
            .list_directory(&at(&volume, prefix.trim_end_matches('/')), None)
            .await;
        report(
            &live,
            "list the scratch folder",
            format!("{:?}", listed.map(|entries| entries.len())),
        );
        live.clean(&client, &prefix).await;
    }
}

/// ❗ A `CreateNew` write's metadata comes back under the names the readers
/// use, `x-amz-meta-mtime` (the stat's date) and `x-amz-meta-cmdr-write` (the
/// landing check and the cut-off cleanup), on every provider. On GCS the
/// write goes out signed GCS's way with `x-goog-meta-*` headers
/// (`NoOverwrite::GoogGenerationMatch`), and an S3-signed HEAD must still
/// answer `x-amz-meta-*`, or Cmdr would misjudge its own write.
#[tokio::test(flavor = "multi_thread")]
async fn live_create_new_metadata_reads_back_under_the_names_cmdr_reads() {
    let mut misses = Vec::new();
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("metadata-names");
        let volume = live
            .connect(Some(&live.bucket))
            .await
            .unwrap_or_else(|e| panic!("[{}] the bucket didn't connect: {e:?}", live.name));
        let key = format!("{prefix}dated.txt");
        let written = write(&volume, &at(&volume, &key), WriteMode::CreateNew, b"dated".to_vec()).await;
        let head = live.head(&client, &key).await;
        let names: Vec<&str> = head
            .headers
            .keys()
            .map(http::HeaderName::as_str)
            .filter(|name| name.contains("-meta-"))
            .collect();
        let stat = volume.get_metadata(&at(&volume, &key)).await;
        let distant = distant_mtime()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_secs();
        let dated = stat.as_ref().is_ok_and(|s| s.modified_at == Some(distant));
        report(
            &live,
            "CreateNew's metadata",
            format!("{written:?}; HEAD meta headers {names:?}; stat keeps the date: {dated}"),
        );
        let token = head.header(crate::metadata::WRITE_TOKEN_HEADER);
        if written.is_err()
            || head.header(crate::metadata::MTIME_HEADER).is_none()
            || token.is_none_or(str::is_empty)
            || !dated
        {
            misses.push(format!("[{}] {written:?}, names {names:?}, dated {dated}", live.name));
        }
        live.clean(&client, &prefix).await;
    }
    assert!(misses.is_empty(), "{misses:#?}");
}

/// `cmdr stores these bytes verbatim\n`, compressed by the `gzip`, `brotli`,
/// `zstd`, and Perl `Compress::Zlib` command-line tools.
const GZIP: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x4b, 0xce, 0x4d, 0x29, 0x52, 0x28, 0x2e, 0xc9, 0x2f,
    0x4a, 0x2d, 0x56, 0x28, 0xc9, 0x48, 0x2d, 0x4e, 0x55, 0x48, 0xaa, 0x2c, 0x01, 0xb2, 0xcb, 0x52, 0x8b, 0x92, 0x12,
    0x4b, 0x32, 0x73, 0xb9, 0x00, 0xce, 0xed, 0x88, 0x3e, 0x21, 0x00, 0x00, 0x00,
];
const BROTLI: &[u8] = &[
    0x1f, 0x20, 0x00, 0xf8, 0xc5, 0x1d, 0xec, 0x37, 0xce, 0x13, 0x86, 0xd1, 0xa1, 0xa3, 0x07, 0x21, 0xe5, 0x15, 0x59,
    0x64, 0x37, 0xf8, 0xcc, 0x64, 0xaa, 0x36, 0x1d, 0x70, 0xf1, 0xdd, 0x77, 0xf4, 0xc7, 0xf0, 0x08,
];
const ZSTD: &[u8] = &[
    0x28, 0xb5, 0x2f, 0xfd, 0x04, 0x58, 0x09, 0x01, 0x00, 0x63, 0x6d, 0x64, 0x72, 0x20, 0x73, 0x74, 0x6f, 0x72, 0x65,
    0x73, 0x20, 0x74, 0x68, 0x65, 0x73, 0x65, 0x20, 0x62, 0x79, 0x74, 0x65, 0x73, 0x20, 0x76, 0x65, 0x72, 0x62, 0x61,
    0x74, 0x69, 0x6d, 0x0a, 0xc5, 0x69, 0xed, 0x25,
];
const DEFLATE: &[u8] = &[
    0x78, 0x9c, 0x4b, 0xce, 0x4d, 0x29, 0x52, 0x28, 0x2e, 0xc9, 0x2f, 0x4a, 0x2d, 0x56, 0x28, 0xc9, 0x48, 0x2d, 0x4e,
    0x55, 0x48, 0xaa, 0x2c, 0x01, 0xb2, 0xcb, 0x52, 0x8b, 0x92, 0x12, 0x4b, 0x32, 0x73, 0xb9, 0x00, 0xd7, 0x08, 0x0c,
    0x6b,
];

/// ❗ An object stored with a `Content-Encoding` (web assets on S3 often are)
/// reads back as its STORED bytes, the length its `Content-Length` and ETag
/// describe: a file manager copies files, it doesn't decode them.
#[tokio::test(flavor = "multi_thread")]
async fn live_encoded_objects_read_back_verbatim() {
    let mut misses = Vec::new();
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("encoded");
        let volume = live
            .connect(Some(&live.bucket))
            .await
            .unwrap_or_else(|e| panic!("[{}] the bucket didn't connect: {e:?}", live.name));
        for (encoding, stored) in [("gzip", GZIP), ("br", BROTLI), ("zstd", ZSTD), ("deflate", DEFLATE)] {
            let key = format!("{prefix}page.{encoding}");
            let put = live.put(&client, &key, stored, &[("content-encoding", encoding)]).await;
            if !put.status.is_success() {
                report(
                    &live,
                    &format!("{encoding} object"),
                    format!("PUT refused: {}", verdict(&put)),
                );
                continue;
            }
            let head = live.head(&client, &key).await;
            let back = read_back(&volume, &at(&volume, &key)).await;
            let verbatim = back == stored;
            report(
                &live,
                &format!("{encoding} object"),
                format!(
                    "stored {} B as {:?} (HEAD: {} {:?} B); read back {} B, verbatim: {verbatim}",
                    stored.len(),
                    head.header("content-encoding"),
                    head.status.as_u16(),
                    head.header("content-length"),
                    back.len()
                ),
            );
            let size = volume.get_metadata(&at(&volume, &key)).await.ok().and_then(|m| m.size);
            if !verbatim || size != Some(stored.len() as u64) {
                misses.push(format!(
                    "[{}] {encoding}: {} B back, stat says {size:?}, for {} B stored",
                    live.name,
                    back.len(),
                    stored.len()
                ));
            }
        }
        live.clean(&client, &prefix).await;
    }
    assert!(misses.is_empty(), "{misses:#?}");
}

/// Uploads at two, four, and eight parts in flight, and server-side copies at
/// four, eight, and 16, timed, each deleted right after.
/// ❗ Off the short-body allowlist ("Other"), an overwrite goes as a one-part
/// multipart upload: a Cancel at its last moment (right before the
/// completion) keeps the original byte for byte and leaves no upload, and a
/// finished one lands whole.
#[tokio::test(flavor = "multi_thread")]
async fn live_an_overwrite_off_the_allowlist_goes_in_one_part() {
    for live in live_targets().iter().filter_map(Live::as_other) {
        let client = live.client();
        let prefix = live_prefix("overwrite-in-parts");
        let volume = live
            .connect(Some(&live.bucket))
            .await
            .unwrap_or_else(|e| panic!("[{}] the bucket didn't connect: {e:?}", live.name));
        let key = format!("{prefix}kept.txt");
        let path = at(&volume, &key);
        let original = b"the original, which must survive".to_vec();
        write(&volume, &path, WriteMode::CreateNew, original.clone())
            .await
            .unwrap_or_else(|e| panic!("[{}] seeding the original: {e:?}", live.name));

        let fresh = pattern(3 * 1024 * 1024 + 7, b'o');
        let total = fresh.len() as u64;
        let source = BytesSource::new(fresh.clone());
        let length = cmdr_fs::volume::VolumeReadStream::total_size(&source);
        let cancelled = volume
            .write_from_stream(
                &path,
                WriteMode::CreateOrReplace,
                length,
                Box::new(source),
                &|progress| {
                    if progress.bytes_written >= total {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                },
            )
            .await;
        let back = read_back(&volume, &path).await;
        let uploads = live.uploads_under(&client, &prefix).await;
        report(
            &live,
            "an overwrite cancelled right before its completion",
            format!(
                "{cancelled:?}, original intact: {}, uploads left: {uploads:?}",
                back == original
            ),
        );
        assert!(
            matches!(cancelled, Err(VolumeError::Cancelled(_))),
            "[{}] {cancelled:?}",
            live.name
        );
        assert_eq!(back, original, "[{}] the original must survive", live.name);
        assert_eq!(uploads, Ok(Vec::new()), "[{}]", live.name);

        breathe().await;
        let replaced = write(&volume, &path, WriteMode::CreateOrReplace, fresh.clone()).await;
        let etag = live
            .head(&client, &key)
            .await
            .header("etag")
            .unwrap_or_default()
            .to_string();
        let back = read_back(&volume, &path).await;
        report(
            &live,
            "a finished overwrite",
            format!("{replaced:?}, ETag {etag}, reads back intact: {}", back == fresh),
        );
        assert!(back == fresh, "[{}] the overwrite must land whole", live.name);
        assert!(
            etag.trim_matches('"').ends_with("-1"),
            "[{}] one part: {etag}",
            live.name
        );
        assert_eq!(
            live.uploads_under(&client, &prefix).await,
            Ok(Vec::new()),
            "[{}]",
            live.name
        );
        live.clean(&client, &prefix).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn live_throughput_by_part_width() {
    for live in live_targets() {
        let client = live.client();
        let prefix = live_prefix("throughput");
        let volume = live
            .connect(Some(&live.bucket))
            .await
            .unwrap_or_else(|e| panic!("[{}] the bucket didn't connect: {e:?}", live.name));

        // 64 MiB in 8 MiB parts, so eight parts can be in flight.
        volume.set_part_floor((8 * MIB) as u64);
        let bytes = pattern(64 * MIB, 4);
        for width in [2, 4, 8] {
            volume.set_concurrency(16, width).await;
            let key = format!("{prefix}upload-{width}.bin");
            let started = Instant::now();
            let outcome = write(&volume, &at(&volume, &key), WriteMode::CreateNew, bytes.clone()).await;
            let secs = started.elapsed().as_secs_f64();
            report(
                &live,
                // allowed-pluralize-noun: `width` is 2, 4, or 8
                &format!("upload 64 MiB, {width} parts in flight"),
                format!("{:?} in {secs:.2} s ({:.1} MiB/s)", outcome.map(|_| "ok"), 64.0 / secs),
            );
            live.delete_each(&client, &[key]).await;
        }

        // A 140 MiB source, then copies of it in 8 MiB parts (18 of them).
        let source = format!("{prefix}source.bin");
        volume.set_concurrency(16, 8).await;
        write(
            &volume,
            &at(&volume, &source),
            WriteMode::CreateNew,
            pattern(140 * MIB, 5),
        )
        .await
        .expect("the 140 MiB source lands");
        for width in [4, 8, 16] {
            volume.set_concurrency(width, 4).await;
            let key = format!("{prefix}copy-{width}.bin");
            let started = Instant::now();
            let outcome = copy(&volume, &source, &key).await;
            report(
                &live,
                &format!("server-side copy of 140 MiB in 8 MiB parts, {width} in flight"),
                format!("{:?} in {}", outcome.map(|_| "ok"), seconds(started)),
            );
            live.delete_each(&client, &[key]).await;
        }
        volume.set_part_floor(crate::multipart::MIN_PART_SIZE);
        volume.set_concurrency(16, 4).await;
        let key = format!("{prefix}copy-production.bin");
        let started = Instant::now();
        let outcome = copy(&volume, &source, &key).await;
        report(
            &live,
            "server-side copy of 140 MiB in 64 MiB parts",
            format!("{:?} in {}", outcome.map(|_| "ok"), seconds(started)),
        );
        live.clean(&client, &prefix).await;
    }
}
