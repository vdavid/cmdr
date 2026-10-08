//! Hostile cells against each real provider, through the `Volume` API the app
//! drives: awkward names, folders, edge sizes, a 1,200-object folder, share
//! links, and the dates a stat and a listing show. Cancels, crashes, and
//! races: `live_hostile_failure_test.rs`. Each cell collects every miss per
//! provider and fails once at the end, so one run shows the whole picture.
//!
//! Skips without `CMDR_S3_LIVE=1` (`live_support.rs`); the runner is
//! `apps/desktop/test/s3-servers/live.sh all live_hostile`. Findings:
//! `docs/notes/s3/live-hostile-2026-10.md`.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cmdr_fs::volume::{RenameWork, ShareLinkExpiry, StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};
use unicode_normalization::UnicodeNormalization as _;

use super::S3Volume;
use super::live_hostile_support::*;
use super::live_support::*;
use super::testing::{BytesSource, distant_mtime, read_back};

fn names() -> Vec<String> {
    [
        "café nfc.txt",
        "cafe\u{301} nfd.txt",
        "🦀 crab 👩‍👩‍👧.txt",
        "שלום עולם.txt",
        "مرحبا.txt",
        "a b  c.txt",
        " leading space.txt",
        "trailing space ",
        "trailing dot.",
        "...",
        "100% sure.txt",
        "%2e%2e",
        "%20literal.txt",
        "plus+sign.txt",
        "hash#tag.txt",
        "question?.txt",
        "amp&er=sand;.txt",
        "back\\slash.txt",
        "quote\"'s.txt",
        "<angle>|pipe.txt",
        "~tilde!*()$,@:.txt",
        "[brackets]{curly}^`.txt",
        "tab\there.txt",
        "line\nbreak.txt",
        "日本語のファイル名.txt",
        "UPPER.TXT",
        "upper.txt",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// The names `live`'s provider refuses outright: GCS object names can't hold
/// CR or LF, and B2's can't hold any control character.
fn refused_names(live: &Live) -> &'static [&'static str] {
    match live.name {
        "gcs" => &["line\nbreak.txt"],
        "b2" => &["tab\there.txt", "line\nbreak.txt"],
        _ => &[],
    }
}

/// ❗ Every awkward name round-trips through list → stat → read → rename →
/// delete; a `..` segment resolves lexically and can't leave the bucket; a key
/// past 1,024 bytes is refused.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_names_round_trip() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("names");
        let folder = format!("{prefix}names");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");

        let mut names = names();
        for refused_name in refused_names(&live) {
            names.retain(|name| name != refused_name);
            let key = format!("{folder}/{refused_name}");
            let refused = write(&volume, &at(&volume, &key), WriteMode::CreateNew, b"no".to_vec()).await;
            let raw = verdict(&live.put(&client, &key, b"no", &[]).await);
            m.check(
                matches!(refused, Err(VolumeError::InvalidName(_)))
                    && live.keys_under(&client, &prefix).await.is_empty(),
                &format!("the provider refuses {refused_name:?}"),
                format!("{refused:?}; a raw PUT answers {raw}"),
            );
        }
        // Exactly 1,024 bytes of key, S3's ceiling.
        let room = 1024 - (folder.len() + 1);
        names.push("k".repeat(room - 4) + ".txt");
        for name in &names {
            let landed = write(
                &volume,
                &at(&volume, &format!("{folder}/{name}")),
                WriteMode::CreateNew,
                name.as_bytes().to_vec(),
            )
            .await;
            m.check(landed.is_ok(), &format!("write {name:?}"), format!("{landed:?}"));
        }

        let listed = volume.list_directory(&at(&volume, &folder), None).await;
        let listed = listed.unwrap_or_else(|e| {
            m.check(false, "list the names folder", format!("{e:?}"));
            Vec::new()
        });
        let composes = live.name == "r2";
        for name in &names {
            // R2 stores keys NFC, so the NFD name lists as its NFC twin.
            let wanted: String = if composes { name.nfc().collect() } else { name.clone() };
            let hits: Vec<_> = listed.iter().filter(|e| e.name == wanted).collect();
            m.check(
                hits.len() == 1 && !hits[0].is_directory && hits[0].size == Some(name.len() as u64),
                &format!("listed {name:?}"),
                format!(
                    "{} hit(s), {:?}",
                    hits.len(),
                    hits.first().map(|e| (e.is_directory, e.size, &e.path))
                ),
            );
        }
        m.check(
            listed.len() == names.len(),
            "the names folder's row count",
            format!("{} rows for {} names", listed.len(), names.len()),
        );

        for name in &names {
            let path = at(&volume, &format!("{folder}/{name}"));
            let stat = volume.get_metadata(&path).await;
            m.check(
                matches!(&stat, Ok(e) if e.size == Some(name.len() as u64) && !e.is_directory),
                &format!("stat {name:?}"),
                format!("{:?}", stat.as_ref().map(|e| e.size)),
            );
            let back = match volume.open_read_stream(&path).await {
                Ok(mut stream) => {
                    let mut bytes = Vec::new();
                    while let Some(Ok(piece)) = stream.next_chunk().await {
                        bytes.extend(piece);
                    }
                    bytes
                }
                Err(e) => format!("{e:?}").into_bytes(),
            };
            m.check(
                back == name.as_bytes(),
                &format!("read {name:?}"),
                format!("{} bytes", back.len()),
            );
            breathe().await;
            // The renamed name must fit the 1,024-byte ceiling too.
            let renamed_name = if name.len() > 900 {
                format!("r{}", &name[1..])
            } else {
                format!("{name}.r")
            };
            let renamed = at(&volume, &format!("{folder}/{renamed_name}"));
            let moved = volume.rename(&path, &renamed, false).await;
            let old_gone = matches!(volume.get_metadata(&path).await, Err(VolumeError::NotFound(_)));
            let new_back = if moved.is_ok() {
                read_back(&volume, &renamed).await
            } else {
                Vec::new()
            };
            m.check(
                moved.is_ok() && old_gone && new_back == name.as_bytes(),
                &format!("rename {name:?}"),
                format!(
                    "{moved:?}, old gone: {old_gone}, new intact: {}",
                    new_back == name.as_bytes()
                ),
            );
            let target = if moved.is_ok() { &renamed } else { &path };
            let deleted = volume.delete(target).await;
            let gone = matches!(volume.get_metadata(target).await, Err(VolumeError::NotFound(_)));
            m.check(
                deleted.is_ok() && gone,
                &format!("delete {name:?}"),
                format!("{deleted:?}, gone: {gone}"),
            );
        }

        // App paths resolve `.` and `..` lexically (`RemoteRoot`), so a `..`
        // lands where it points, and one climbing out of the bucket is
        // refused. A key one byte past the ceiling is the server's to refuse.
        let resolved = write(
            &volume,
            &at(&volume, &format!("{folder}/sub/../dots.txt")),
            WriteMode::CreateNew,
            b"dots".to_vec(),
        )
        .await;
        let landed = live.length(&client, &format!("{folder}/dots.txt")).await;
        m.check(
            resolved.is_ok() && landed == Some(4),
            "a `..` segment resolves lexically",
            format!("{resolved:?}, landed: {landed:?}"),
        );
        live.delete_each(&client, &[format!("{folder}/dots.txt")]).await;
        let escape = write(
            &volume,
            &at(&volume, "../escape.txt"),
            WriteMode::CreateNew,
            b"no".to_vec(),
        )
        .await;
        m.check(
            escape.is_err(),
            "a `..` out of the bucket is refused",
            format!("{escape:?}"),
        );
        let too_long = "k".repeat(room - 3) + ".txt";
        let refused = write(
            &volume,
            &at(&volume, &format!("{folder}/{too_long}")),
            WriteMode::CreateNew,
            b"no".to_vec(),
        )
        .await;
        m.check(refused.is_err(), "a 1,025-byte key is refused", format!("{refused:?}"));
        let leftover = live.keys_under(&client, &prefix).await;
        m.check(
            leftover.is_empty(),
            "nothing left under the prefix",
            format!("{leftover:?}"),
        );
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// ❗ A file beside a folder of its name, an empty folder's marker, and a
/// 20-level tree, through every row operation.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_folders() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("folders");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");

        // `twin` the file beside `twin/` the folder.
        live.put(&client, &format!("{prefix}twin"), b"the file", &[]).await;
        live.put(&client, &format!("{prefix}twin/child.txt"), b"the child", &[])
            .await;
        let top = at(&volume, prefix.trim_end_matches('/'));
        let rows = volume.list_directory(&top, None).await.unwrap_or_default();
        let shown: Vec<_> = rows.iter().map(|e| (e.name.clone(), e.is_directory)).collect();
        m.check(
            shown.contains(&("twin".to_string(), true))
                && shown.contains(&("twin (file)".to_string(), false))
                && rows.len() == 2,
            "a file beside its folder lists as `twin (file)`",
            format!("{shown:?}"),
        );
        let file_row = at(&volume, &format!("{prefix}twin (file)"));
        let folder_row = at(&volume, &format!("{prefix}twin"));
        let back = read_back(&volume, &file_row).await;
        m.check(
            back == b"the file",
            "read `twin (file)`",
            String::from_utf8_lossy(&back),
        );
        let folder_stat = volume.get_metadata(&folder_row).await;
        m.check(
            matches!(&folder_stat, Ok(e) if e.is_directory),
            "stat `twin` is the folder",
            format!("{:?}", folder_stat.map(|e| e.is_directory)),
        );
        let refused = volume.delete(&folder_row).await;
        let file_kept = live.length(&client, &format!("{prefix}twin")).await == Some(8);
        m.check(
            refused.is_err() && file_kept,
            "delete `twin` (a non-empty folder) refuses and keeps the file",
            format!("{refused:?}, file kept: {file_kept}"),
        );
        let moved = volume
            .rename(&file_row, &at(&volume, &format!("{prefix}twin.txt")), false)
            .await;
        let child_kept = live.length(&client, &format!("{prefix}twin/child.txt")).await == Some(9);
        let file_gone = live.length(&client, &format!("{prefix}twin")).await.is_none();
        m.check(
            moved.is_ok() && child_kept && file_gone,
            "rename `twin (file)` moves the file only",
            format!("{moved:?}, child kept: {child_kept}, file gone: {file_gone}"),
        );

        // An empty folder is its marker, and survives emptying.
        let empty = at(&volume, &format!("{prefix}empty"));
        let made = volume.create_directory(&empty).await;
        let again = volume.create_directory(&empty).await;
        let listed = volume.list_directory(&empty, None).await.map(|rows| rows.len());
        m.check(
            made.is_ok() && matches!(again, Err(VolumeError::AlreadyExists(_))) && listed.as_ref().ok() == Some(&0),
            "make an empty folder, twice",
            format!("{made:?}, again {again:?}, lists {listed:?}"),
        );
        let inner = at(&volume, &format!("{prefix}empty/a.txt"));
        write(&volume, &inner, WriteMode::CreateNew, b"a".to_vec())
            .await
            .expect("lands");
        let not_empty = volume.delete(&empty).await;
        let gone_inner = volume.delete(&inner).await;
        let still = volume.list_directory(&empty, None).await.map(|rows| rows.len());
        let removed = volume.delete(&empty).await;
        let after = volume.list_directory(&empty, None).await;
        m.check(
            not_empty.is_err()
                && gone_inner.is_ok()
                && still.as_ref().ok() == Some(&0)
                && removed.is_ok()
                && matches!(after, Err(VolumeError::NotFound(_))),
            "a marker folder survives emptying, then deletes",
            format!(
                "non-empty delete {not_empty:?}, emptied lists {still:?}, delete {removed:?}, then {:?}",
                after.map(|r| r.len())
            ),
        );
        let under_file = volume
            .create_directory(&at(&volume, &format!("{prefix}twin.txt/sub")))
            .await;
        m.check(
            matches!(under_file, Err(VolumeError::NotADirectory(_))),
            "a folder under a file is refused",
            format!("{under_file:?}"),
        );
        let over_folder = volume.create_file(&at(&volume, &format!("{prefix}twin")), b"x").await;
        m.check(
            matches!(over_folder, Err(VolumeError::AlreadyExists(_))),
            "New File on a folder's name is refused",
            format!("{over_folder:?}"),
        );

        // 20 levels, written as one key (no markers).
        let levels: Vec<String> = (1..=20).map(|n| format!("d{n:02}")).collect();
        let deep_key = format!("{prefix}deep/{}/leaf.txt", levels.join("/"));
        write(&volume, &at(&volume, &deep_key), WriteMode::CreateNew, b"deep".to_vec())
            .await
            .expect("lands");
        let deep_top = at(&volume, &format!("{prefix}deep"));
        let first = volume.list_directory(&deep_top, None).await.map(|rows| {
            rows.iter()
                .map(|e| (e.name.clone(), e.is_directory))
                .collect::<Vec<_>>()
        });
        let bottom = volume
            .list_directory(&at(&volume, &format!("{prefix}deep/{}", levels.join("/"))), None)
            .await
            .map(|rows| rows.iter().map(|e| e.name.clone()).collect::<Vec<_>>());
        m.check(
            first.as_ref().ok() == Some(&vec![("d01".to_string(), true)])
                && bottom.as_ref().ok() == Some(&vec!["leaf.txt".to_string()]),
            "a 20-level tree lists at both ends",
            format!("{first:?}, {bottom:?}"),
        );
        let tally = volume.tally_subtree(&deep_top, 10_000).await;
        m.check(
            matches!(&tally, Ok(t) if t.files == 1 && t.bytes == 4 && t.complete),
            "tally a 20-level tree",
            format!(
                "{:?}",
                tally.as_ref().map(|t| (t.files, t.bytes, t.folders, t.complete))
            ),
        );
        let work = volume.rename_work(&deep_top).await;
        let refused = volume
            .rename(&deep_top, &at(&volume, &format!("{prefix}deep2")), false)
            .await;
        m.check(
            matches!(work, Ok(RenameWork::CopyThenDelete)) && matches!(refused, Err(VolumeError::NotSupported)),
            "a folder renames by the engine, never by `rename`",
            format!("{work:?}, {refused:?}"),
        );
        volume.delete(&at(&volume, &deep_key)).await.expect("the leaf deletes");
        let vanished = volume.list_directory(&deep_top, None).await;
        m.check(
            matches!(vanished, Err(VolumeError::NotFound(_))),
            "a markerless tree vanishes with its last key",
            format!("{:?}", vanished.map(|r| r.len())),
        );
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// The size a big object gets on `live`: Wasabi bills 90 days of whatever
/// it stored, so it gets less. B2's free tier caps downloads at 1 GB a day,
/// and the read-back is a download, so it gets three production parts and a
/// short tail: 1 GiB alone would use the day's cap.
fn big_size(live: &Live) -> u64 {
    match live.name {
        "wasabi" => 300 * MIB as u64,
        "b2" => 200 * MIB as u64,
        _ => 1024 * MIB as u64,
    }
}

/// Everything a stream at `offset` yields, and the total it names.
async fn read_from(volume: &S3Volume, path: &Path, offset: u64) -> Result<(Vec<u8>, StreamLength), VolumeError> {
    let mut stream = volume.open_read_stream_at_offset(path, offset).await?;
    let mut bytes = Vec::new();
    while let Some(piece) = stream.next_chunk().await {
        bytes.extend(piece?);
    }
    Ok((bytes, stream.total_size()))
}

/// ❗ Sizes at every edge of the part plan (0, 1, the floor and ±1, a one-byte
/// last part), of known and unknown length, then ~1 GiB in production parts,
/// each read back byte for byte; ranged reads at the edges.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_sizes() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("sizes");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let floor = 5 * MIB as u64;
        volume.set_part_floor(floor);
        let sizes = [
            0,
            1,
            floor - 1,
            floor,
            floor + 1,
            2 * floor - 1,
            2 * floor,
            2 * floor + 1,
            3 * floor + 1,
        ];
        for (n, size) in sizes.into_iter().enumerate() {
            for known in [true, false] {
                if !known && ![0, floor + 1, 2 * floor].contains(&size) {
                    continue;
                }
                let tag = n as u8 * 7 + u8::from(known);
                let path = at(&volume, &format!("{prefix}{size}-{known}.bin"));
                let mut source = BytesSource::new(pattern(size as usize, tag)).modified_at(distant_mtime());
                if !known {
                    source = source.of_unknown_length();
                }
                let length = source.total_size();
                let wrote = volume
                    .write_from_stream(&path, WriteMode::CreateNew, length, Box::new(source), &|_| {
                        ControlFlow::Continue(())
                    })
                    .await;
                let mismatch = pattern_mismatch(&volume, &path, size, tag).await;
                let stat = volume.get_metadata(&path).await.map(|e| e.size);
                let what = format!("{size} bytes, {} length", if known { "known" } else { "unknown" }); // allowed-pluralize-noun: a test label naming a size in bytes
                m.check(
                    wrote.as_ref().ok() == Some(&size) && mismatch.is_none() && stat.as_ref().ok() == Some(&Some(size)),
                    &what,
                    format!("{wrote:?}, read back: {mismatch:?}, stat: {stat:?}"),
                );
            }
        }
        let leftovers = leftover_uploads(&live, &prefix).await;
        m.check(
            leftovers == Ok(Vec::new()),
            "no upload left after the size ladder",
            format!("{leftovers:?}"),
        );

        // Ranged reads on the two-parts-and-a-byte object.
        let size = 2 * floor + 1;
        let tag = 7 * 7 + 1;
        let path = at(&volume, &format!("{prefix}{size}-true.bin"));
        let window = volume.read_range(&path, floor - 2, 5).await;
        let expected: Vec<u8> = (floor - 2..floor + 3).map(|i| pattern_byte(i, tag)).collect();
        m.check(
            window.as_ref().ok() == Some(&expected),
            "read_range across a part edge",
            format!("{:?}", window.map(|w| w.len())),
        );
        let tail = read_from(&volume, &path, size - 1).await;
        m.check(
            matches!(&tail, Ok((bytes, StreamLength::Known(total))) if *bytes == vec![pattern_byte(size - 1, tag)] && *total == size),
            "a read from the last byte",
            format!("{:?}", tail.as_ref().map(|(b, t)| (b.len(), *t))),
        );
        let past = read_from(&volume, &path, size).await;
        m.check(
            matches!(&past, Ok((bytes, _)) if bytes.is_empty()),
            "a read from the end is empty",
            format!("{:?}", past.map(|(b, _)| b.len())),
        );

        // A download dropped mid-way, then read again whole.
        if let Ok(mut stream) = volume.open_read_stream(&path).await {
            let _ = stream.next_chunk().await;
            stream.cancel_and_release().await;
        }
        let again = pattern_mismatch(&volume, &path, size, tag).await;
        m.check(
            again.is_none(),
            "a download released mid-way, then read whole",
            format!("{again:?}"),
        );

        // ~1 GiB (300 MiB on Wasabi, 200 MiB on B2) in production parts, never held whole.
        volume.set_part_floor(crate::multipart::MIN_PART_SIZE);
        let big = big_size(&live);
        let key = format!("{prefix}big.bin");
        let path = at(&volume, &key);
        let started = std::time::Instant::now();
        let wrote = volume
            .write_from_stream(
                &path,
                WriteMode::CreateNew,
                StreamLength::Known(big),
                Box::new(PatternSource::new(big, 9)),
                &|_| ControlFlow::Continue(()),
            )
            .await;
        let up = started.elapsed().as_secs_f64();
        let started = std::time::Instant::now();
        let mismatch = pattern_mismatch(&volume, &path, big, 9).await;
        let down = started.elapsed().as_secs_f64();
        let mib = big as f64 / MIB as f64;
        m.check(
            wrote.as_ref().ok() == Some(&big) && mismatch.is_none(),
            &format!("{} MiB in 64 MiB parts", big / MIB as u64),
            format!(
                "{wrote:?} up at {:.1} MiB/s, read back {mismatch:?} at {:.1} MiB/s",
                mib / up,
                mib / down
            ),
        );
        live.delete_each(&client, &[key]).await;
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// ❗ A folder of 1,200 objects (1,150 files and five subfolders of ten):
/// listing pagination, `tally_subtree` with and past its cap, `rename_work`,
/// and `delete_files` across two `DeleteObjects` batches.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_scale() {
    use futures_util::StreamExt as _;
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("scale");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let mut keys: Vec<String> = (0..1150).map(|n| format!("{prefix}many/f{n:04}.txt")).collect();
        keys.extend((0..50).map(|n| format!("{prefix}many/sub{}/g{n:02}.txt", n / 10)));
        let started = std::time::Instant::now();
        let failed = futures_util::stream::iter(keys.iter().map(|key| {
            let client = &client;
            let live = &live;
            async move { live.put(client, key, key.as_bytes(), &[]).await.status.is_success() }
        }))
        .buffer_unordered(16)
        .filter(|ok| std::future::ready(!*ok))
        .count()
        .await;
        report(
            &live,
            "seed 1,200 objects",
            format!("{failed} failed, {}", seconds(started)),
        );

        let many = at(&volume, &format!("{prefix}many"));
        let pages = AtomicU32::new(0);
        let on_progress = |_: cmdr_fs::volume::ListingProgress| {
            pages.fetch_add(1, Ordering::Relaxed);
        };
        let started = std::time::Instant::now();
        let rows = volume.list_directory(&many, Some(&on_progress)).await;
        let files = rows.as_ref().map(|r| r.iter().filter(|e| !e.is_directory).count());
        let folders = rows.as_ref().map(|r| r.iter().filter(|e| e.is_directory).count());
        let sizes_right = rows.as_ref().is_ok_and(|r| {
            r.iter()
                .filter(|e| !e.is_directory)
                .all(|e| e.size == Some(e.path.len() as u64 - volume.root().as_os_str().len() as u64 - 1))
        });
        m.check(
            files.as_ref().ok() == Some(&1150)
                && folders.as_ref().ok() == Some(&5)
                && pages.load(Ordering::Relaxed) >= 2,
            "list 1,150 files and five folders",
            format!(
                "{files:?} files, {folders:?} folders, {} progress calls, sizes right: {sizes_right}, {}",
                pages.load(Ordering::Relaxed),
                seconds(started)
            ),
        );
        let tally = volume.tally_subtree(&many, 10_000).await;
        m.check(
            matches!(&tally, Ok(t) if t.files == 1200 && t.complete && t.folders >= 6),
            "tally 1,200 objects",
            format!(
                "{:?}",
                tally.as_ref().map(|t| (t.files, t.bytes, t.folders, t.complete))
            ),
        );
        let capped = volume.tally_subtree(&many, 1000).await;
        m.check(
            matches!(&capped, Ok(t) if !t.complete && t.files <= 1001),
            "tally capped at 1,000",
            format!("{:?}", capped.as_ref().map(|t| (t.files, t.complete))),
        );
        let work = volume.rename_work(&many).await;
        m.check(
            matches!(work, Ok(RenameWork::CopyThenDelete)),
            "rename_work on a big folder",
            format!("{work:?}"),
        );

        let paths: Vec<PathBuf> = keys.iter().map(|key| at(&volume, key)).collect();
        let started = std::time::Instant::now();
        let deleted = volume.delete_files(&paths).await;
        let failures = deleted.iter().filter(|r| r.is_err()).count();
        let left = live.keys_under(&client, &prefix).await;
        let after = volume.list_directory(&many, None).await;
        m.check(
            deleted.len() == 1200 && failures == 0 && left.is_empty() && matches!(after, Err(VolumeError::NotFound(_))),
            "delete 1,200 objects in batches",
            format!(
                "{} results, {failures} failed, {} left, then {:?}, {}",
                deleted.len(),
                left.len(),
                after.map(|r| r.len()),
                seconds(started)
            ),
        );
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

/// ❗ Share links: an awkward key's link fetches the bytes unsigned, a
/// three-second link stops working once expired, and a deleted object's link
/// serves nothing.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_share_links() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("share");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let http = cmdr_http::client_builder().build().expect("a plain client builds");

        for name in ["hash# & plus+ 100% é 🦀 ?.txt", "a b/c=d;e.txt", "שלום.txt"] {
            let path = at(&volume, &format!("{prefix}{name}"));
            let body = format!("shared {name}").into_bytes();
            write(&volume, &path, WriteMode::CreateNew, body.clone())
                .await
                .expect("lands");
            let link = volume
                .share_link(&path, ShareLinkExpiry::OneHour)
                .await
                .expect("a link");
            let answer = http.get(link.into_url()).send().await.expect("fetches");
            let status = answer.status();
            let fetched = answer.bytes().await.unwrap_or_default();
            m.check(
                status.is_success() && fetched.as_ref() == body.as_slice(),
                &format!("a link to {name:?}, fetched unsigned"),
                format!("{status}, body intact: {}", fetched.as_ref() == body.as_slice()),
            );
        }

        let key = format!("{prefix}short.txt");
        write(
            &volume,
            &at(&volume, &key),
            WriteMode::CreateNew,
            b"short-lived".to_vec(),
        )
        .await
        .expect("lands");
        let short = client
            .share_link(&live.bucket, &key, Duration::from_secs(3))
            .await
            .expect("a link");
        let fresh = http.get(short.as_str()).send().await.expect("fetches").status();
        // allowed-test-sleep: waits out the three-second link's expiry, the subject of the check
        tokio::time::sleep(Duration::from_secs(6)).await;
        let expired = http.get(short.as_str()).send().await.expect("fetches").status();
        m.check(
            fresh.is_success() && expired.is_client_error(),
            "a three-second link, fresh then expired",
            format!("{fresh} then {expired}"),
        );

        let doomed = at(&volume, &format!("{prefix}doomed.txt"));
        write(&volume, &doomed, WriteMode::CreateNew, b"doomed".to_vec())
            .await
            .expect("lands");
        let link = volume
            .share_link(&doomed, ShareLinkExpiry::OneHour)
            .await
            .expect("a link");
        volume.delete(&doomed).await.expect("deletes");
        let status = http.get(link.into_url()).send().await.expect("fetches").status();
        m.check(
            status == http::StatusCode::NOT_FOUND || status == http::StatusCode::FORBIDDEN,
            "a deleted object's link",
            format!("{status}"),
        );
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}

fn secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).expect("after the epoch").as_secs()
}

/// ❗ The date: a stat shows the source's own mtime (`x-amz-meta-mtime`), a
/// listing the upload time; a rename, a one-request copy, and a copy in parts
/// keep it; a copy of a foreign object takes its `Last-Modified`; an
/// overwrite from a dateless source drops the old one.
#[tokio::test(flavor = "multi_thread")]
async fn live_hostile_metadata() {
    let mut all = Vec::new();
    for live in live_targets() {
        let mut m = Misses::new(&live);
        let client = live.client();
        let prefix = live_prefix("metadata");
        let volume = live.connect(Some(&live.bucket)).await.expect("connects");
        let floor = 5 * MIB as u64;
        volume.set_part_floor(floor);
        let distant = secs(distant_mtime());
        let now = secs(SystemTime::now());
        let near_now = |t: Option<u64>| t.is_some_and(|t| t.abs_diff(now) < 3600);
        let mtime_of = |path: PathBuf| {
            let volume = &volume;
            async move { volume.get_metadata(&path).await.ok().and_then(|e| e.modified_at) }
        };

        let small = format!("{prefix}small.txt");
        let big = format!("{prefix}big.bin");
        let empty = format!("{prefix}empty.txt");
        write(&volume, &at(&volume, &small), WriteMode::CreateNew, b"dated".to_vec())
            .await
            .expect("lands");
        write(
            &volume,
            &at(&volume, &big),
            WriteMode::CreateNew,
            pattern(3 * floor as usize + 1, 4),
        )
        .await
        .expect("lands");
        write(&volume, &at(&volume, &empty), WriteMode::CreateNew, Vec::new())
            .await
            .expect("lands");
        for key in [&small, &big, &empty] {
            let stat = mtime_of(at(&volume, key)).await;
            m.check(
                stat == Some(distant),
                &format!(
                    "stat shows the source's mtime ({})",
                    key.rsplit('/').next().unwrap_or_default()
                ),
                format!("{stat:?}"),
            );
        }
        let rows = volume
            .list_directory(&at(&volume, prefix.trim_end_matches('/')), None)
            .await
            .unwrap_or_default();
        let listed = rows.iter().find(|e| e.name == "small.txt").and_then(|e| e.modified_at);
        m.check(
            near_now(listed),
            "a listing shows the upload time",
            format!("{listed:?} (now {now})"),
        );

        let renamed = format!("{prefix}renamed.txt");
        let moved = volume.rename(&at(&volume, &small), &at(&volume, &renamed), false).await;
        let stat = mtime_of(at(&volume, &renamed)).await;
        m.check(
            moved.is_ok() && stat == Some(distant),
            "a rename keeps the mtime",
            format!("{moved:?}, {stat:?}"),
        );
        for (from, to) in [
            (&renamed, format!("{prefix}copied.txt")),
            (&big, format!("{prefix}copied.bin")),
        ] {
            let copied = volume
                .copy_on_server(
                    &volume,
                    &at(&volume, from),
                    &at(&volume, &to),
                    WriteMode::CreateNew,
                    &CopyHook::new(None, None),
                )
                .await;
            let stat = mtime_of(at(&volume, &to)).await;
            m.check(
                copied.is_ok() && stat == Some(distant),
                &format!(
                    "a server copy keeps the mtime ({})",
                    to.rsplit('/').next().unwrap_or_default()
                ),
                format!("{copied:?}, {stat:?}"),
            );
        }

        // A foreign object (no mtime) copied: the copy keeps its upload time.
        let foreign = format!("{prefix}foreign.txt");
        live.put(&client, &foreign, b"foreign", &[]).await;
        let foreign_time = mtime_of(at(&volume, &foreign)).await;
        // allowed-test-sleep: a copy made in the same second would hide a copy that took its own upload time
        tokio::time::sleep(Duration::from_secs(2)).await;
        let to = format!("{prefix}foreign-copy.txt");
        let copied = volume
            .copy_on_server(
                &volume,
                &at(&volume, &foreign),
                &at(&volume, &to),
                WriteMode::CreateNew,
                &CopyHook::new(None, None),
            )
            .await;
        let stat = mtime_of(at(&volume, &to)).await;
        m.check(
            copied.is_ok() && stat.is_some() && stat == foreign_time,
            "a copy of a dateless object keeps the source's upload time",
            format!("{copied:?}, copy {stat:?}, source {foreign_time:?}"),
        );

        // An overwrite from a dateless source must not keep the old mtime.
        for key in [&renamed, &big] {
            breathe().await;
            let path = at(&volume, key);
            let source = BytesSource::new(b"no date".to_vec());
            let length = source.total_size();
            let replaced = volume
                .write_from_stream(&path, WriteMode::CreateOrReplace, length, Box::new(source), &|_| {
                    ControlFlow::Continue(())
                })
                .await;
            let stat = mtime_of(path).await;
            m.check(
                replaced.is_ok() && near_now(stat),
                &format!(
                    "a dateless overwrite drops the old mtime ({})",
                    key.rsplit('/').next().unwrap_or_default()
                ),
                format!("{replaced:?}, {stat:?}"),
            );
        }
        live.clean(&client, &prefix).await;
        all.push((live.name.to_string(), m.misses));
    }
    verdict_of(all);
}
