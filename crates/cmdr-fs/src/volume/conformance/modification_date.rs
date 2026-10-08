//! What a copy promises about dates: the destination keeps the source's
//! modification date, and the source reports the date it lists.
//!
//! Two assertions, because a copy loses the date when EITHER end drops it: a
//! source stream that reports `None`, or a destination that ignores what the
//! stream reports. A backend runs both, since every mutable backend is a source
//! and a destination. The contract: `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md`
//! § "Copies keep the source's date".

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::volume::{StreamLength, Volume, VolumeError, VolumeReadStream, WriteMode};

/// The date [`assert_write_from_stream_keeps_the_source_date`] hands its
/// destination, in Unix seconds: 2021-01-29 08:30:15 UTC.
///
/// Years in the past, so a destination that stamps "now" can't pass by being
/// fast. The stream reports it a quarter past the second
/// ([`SOURCE_DATE_NANOS`]), so a destination that rounds instead of truncating
/// still lands on this second.
pub const SOURCE_DATE_SECS: u64 = 1_611_909_015;

/// The sub-second part of the date the dated source reports.
pub const SOURCE_DATE_NANOS: u32 = 250_000_000;

/// How old a file's listed date must be before
/// [`assert_read_stream_reports_the_listed_date`] trusts it to tell a real date
/// from a stream that reports the moment it opened.
const OLD_ENOUGH: Duration = Duration::from_secs(24 * 60 * 60);

/// A copy onto `volume` keeps the source's modification date.
///
/// Streams a few bytes into `dest` through [`Volume::write_from_stream`] from a
/// source whose [`VolumeReadStream::modified_at`] reports
/// [`SOURCE_DATE_SECS`], then insists the destination lists that date.
///
/// `dest` must not exist yet. `tolerance` is the destination's clock
/// granularity: `Duration::ZERO` for a store that keeps whole seconds or finer
/// (the same second must come back), 2 s for one that keeps even seconds only
/// (FAT). The listing is whole seconds, so nothing finer is comparable here.
///
/// **Why this one is worth a shared assertion.** Every backend's copy suite
/// checksums the bytes, and a destination that stamps its own date passes all of
/// them. On 2026-10-07 a 562-photo copy from a phone onto a NAS landed every file
/// dated to the copy, and only S3 had ever been asked about dates.
pub async fn assert_write_from_stream_keeps_the_source_date(volume: &dyn Volume, dest: &Path, tolerance: Duration) {
    assert!(
        !volume.exists(dest).await,
        "fixture precondition: {} must not exist yet",
        dest.display()
    );

    let content = b"bytes from a file last changed in 2021\n".to_vec();
    let length = content.len() as u64;
    let written = volume
        .write_from_stream(
            dest,
            WriteMode::CreateNew,
            StreamLength::Known(length),
            Box::new(DatedSource::new(content)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await;
    assert_eq!(
        written.as_ref().ok(),
        Some(&length),
        "the dated write onto {} must land every byte; got {written:?}",
        dest.display()
    );

    let listed = volume
        .get_metadata(dest)
        .await
        .unwrap_or_else(|e| panic!("{} must be stattable after the write, got {e:?}", dest.display()))
        .modified_at;
    let Some(listed) = listed else {
        panic!(
            "{} lists no modification date at all after the write, so the source's date ({SOURCE_DATE_SECS}) can't have been kept",
            dest.display()
        );
    };
    let off_by = listed.abs_diff(SOURCE_DATE_SECS);
    assert!(
        off_by <= tolerance.as_secs(),
        "a copy onto {} must keep the source's date (Unix {SOURCE_DATE_SECS}, 2021-01-29 08:30:15 UTC); it lists {listed}, {off_by} s off, so the destination stamped its own",
        dest.display()
    );
}

/// The read stream of `path` reports the date `volume` lists for it.
///
/// `path` must be a file whose listed date is at least a day old, so a stream
/// that reports the moment it opened can't pass. The easy way to get one is the
/// file [`assert_write_from_stream_keeps_the_source_date`] just wrote; a backend
/// that can't store dates ages one by its fixture's own means.
///
/// The two may differ by one second: the listing truncates to whole seconds,
/// and a stream fed by a different call may have rounded a sub-second date.
/// Anything further apart is two different dates.
///
/// **Why this one is worth a shared assertion.** The stream's date is the only
/// thing a destination can keep. A source that knows the date (it listed it a
/// moment ago) but reports `None` on its stream silently makes every copy off
/// it carry the copy's own date, on every destination.
pub async fn assert_read_stream_reports_the_listed_date(volume: &dyn Volume, path: &Path) {
    let listed = volume
        .get_metadata(path)
        .await
        .unwrap_or_else(|e| panic!("fixture precondition: {} must be stattable, got {e:?}", path.display()))
        .modified_at
        .unwrap_or_else(|| panic!("fixture precondition: {} must list a modification date", path.display()));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_secs();
    assert!(
        listed + OLD_ENOUGH.as_secs() <= now,
        "fixture precondition: {} must list a date at least a day old, so a stream reporting \"now\" can't pass; it lists {listed} and now is {now}",
        path.display()
    );

    let stream = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("{} must be readable, got {e:?}", path.display()));
    let Some(reported) = stream.modified_at() else {
        panic!(
            "the read stream of {} reports no modification date, though the volume lists {listed}; every copy off this backend would carry the copy's own date",
            path.display()
        );
    };
    let reported = reported
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|e| {
            panic!(
                "the read stream of {} reports a date before 1970: {e:?}",
                path.display()
            )
        })
        .as_secs();
    let off_by = reported.abs_diff(listed);
    assert!(
        off_by <= 1,
        "the read stream of {} must report the date the volume lists ({listed}); it reports {reported}, {off_by} s off",
        path.display()
    );
}

/// A folder [`Volume::set_modified`] dated lists that date, though writing its
/// contents bumped it to "now" first.
///
/// Creates `dir`, writes a file into it (which is what bumps a folder's date on
/// every real store), sets the folder's date to [`SOURCE_DATE_SECS`], and
/// insists the folder lists it. `dir` must not exist yet; `tolerance` as for
/// [`assert_write_from_stream_keeps_the_source_date`].
///
/// **Why this one is worth a shared assertion.** A copied folder's date is set
/// AFTER its contents land, by a call nothing else exercises, and a backend
/// answering `Ok` without storing anything passes every copy suite: the bytes
/// are all there.
pub async fn assert_set_modified_dates_a_folder(volume: &dyn Volume, dir: &Path, tolerance: Duration) {
    assert!(
        !volume.exists(dir).await,
        "fixture precondition: {} must not exist yet",
        dir.display()
    );
    volume
        .create_directory(dir)
        .await
        .unwrap_or_else(|e| panic!("{} must be creatable, got {e:?}", dir.display()));
    let content = b"a file that lands before its folder is dated\n".to_vec();
    let length = content.len() as u64;
    volume
        .write_from_stream(
            &dir.join("inside.txt"),
            WriteMode::CreateNew,
            StreamLength::Known(length),
            Box::new(DatedSource::new(content)),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("a file must land in {}, got {e:?}", dir.display()));

    volume
        .set_modified(dir, UNIX_EPOCH + Duration::new(SOURCE_DATE_SECS, SOURCE_DATE_NANOS))
        .await
        .unwrap_or_else(|e| panic!("dating the folder {} must succeed, got {e:?}", dir.display()));

    let listed = volume
        .get_metadata(dir)
        .await
        .unwrap_or_else(|e| panic!("{} must be stattable after dating it, got {e:?}", dir.display()))
        .modified_at
        .unwrap_or_else(|| panic!("the folder {} lists no modification date at all", dir.display()));
    let off_by = listed.abs_diff(SOURCE_DATE_SECS);
    assert!(
        off_by <= tolerance.as_secs(),
        "the folder {} must list the date it was given (Unix {SOURCE_DATE_SECS}); it lists {listed}, {off_by} s off",
        dir.display()
    );
}

/// A source stream over bytes in hand that reports [`SOURCE_DATE_SECS`] as its
/// file's date, the way a real backend's stream reports what its open learned.
struct DatedSource {
    bytes: Option<Vec<u8>>,
    total: u64,
    read: u64,
}

impl DatedSource {
    fn new(bytes: Vec<u8>) -> Self {
        let total = bytes.len() as u64;
        Self {
            bytes: Some(bytes),
            total,
            read: 0,
        }
    }
}

impl VolumeReadStream for DatedSource {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let bytes = self.bytes.take()?;
            self.read += bytes.len() as u64;
            Some(Ok(bytes))
        })
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.total)
    }

    fn bytes_read(&self) -> u64 {
        self.read
    }

    fn modified_at(&self) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::new(SOURCE_DATE_SECS, SOURCE_DATE_NANOS))
    }
}
