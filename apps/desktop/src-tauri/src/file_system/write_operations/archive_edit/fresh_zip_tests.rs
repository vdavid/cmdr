//! Producer, bridge, and ZIP64 decision tests for `fresh_zip.rs`.

use super::*;

async fn collect(mut output: FreshZipOutput) -> Result<Vec<u8>, FreshZipError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = output.next_chunk().await {
        bytes.extend(chunk);
    }
    output.finish().await?;
    Ok(bytes)
}

#[test]
fn zip64_is_selected_at_the_per_entry_boundary() {
    assert!(needs_zip64(zip::ZIP64_BYTES_THR));
    assert!(needs_zip64(zip::ZIP64_BYTES_THR + 1));
    assert!(
        !needs_zip64(3 * 1024 * 1024 * 1024),
        "a 3 GiB entry stays a plain entry"
    );
    // The exact switch point: the last size whose worst case still fits.
    let (mut plain, mut large) = (0, zip::ZIP64_BYTES_THR);
    while large - plain > 1 {
        let mid = plain + (large - plain) / 2;
        if needs_zip64(mid) {
            large = mid;
        } else {
            plain = mid;
        }
    }
    assert!(deflate_bound(plain) < zip::ZIP64_BYTES_THR);
    assert!(deflate_bound(plain + 1) >= zip::ZIP64_BYTES_THR);
}

#[test]
fn an_entry_just_under_4_gib_gets_zip64_because_deflate_can_grow_it() {
    // Pre-fix, only the UNCOMPRESSED size was compared, so these entries
    // failed at their data descriptor once incompressible bytes grew past
    // 4 GiB (a ~4 GiB video).
    assert!(needs_zip64(zip::ZIP64_BYTES_THR - 1));
    assert!(needs_zip64(zip::ZIP64_BYTES_THR - zip::ZIP64_BYTES_THR / 10));
}

#[tokio::test]
async fn producer_preserves_names_empty_entries_metadata_and_level() {
    let entries = vec![
        FreshZipEntry {
            name: "folder/".into(),
            source: FreshZipSource::Bytes(vec![]),
            size: 0,
            is_directory: true,
            modified: None,
            unix_mode: Some(0o750),
        },
        FreshZipEntry {
            name: "folder/empty.txt".into(),
            source: FreshZipSource::Bytes(vec![]),
            size: 0,
            is_directory: false,
            modified: None,
            unix_mode: Some(0o640),
        },
    ];
    let bytes = collect(spawn_fresh_zip(entries, Some(1)).expect("spawn producer"))
        .await
        .expect("produce");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
    assert_eq!(archive.len(), 2);
    assert_eq!(archive.by_index(0).expect("dir").name(), "folder/");
    assert_eq!(archive.by_index(0).expect("dir").unix_mode(), Some(0o40750));
    assert_eq!(archive.by_index(1).expect("file").unix_mode(), Some(0o100640));
    assert_eq!(archive.by_index(1).expect("file").size(), 0);
}

/// The extended-timestamp (UT) mtime `entry` carries in the central directory.
fn extended_mtime(archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, index: usize) -> Option<u32> {
    let entry = archive.by_index(index).expect("entry");
    entry.extra_data_fields().find_map(|field| match field {
        zip::ExtraField::ExtendedTimestamp(times) => times.mod_time(),
        _ => None,
    })
}

#[tokio::test]
async fn producer_dates_entries_in_local_time_with_the_exact_utc_second_beside_it() {
    use chrono::{Datelike, Timelike};
    // An odd second: the DOS field alone can't carry it.
    let mtime_secs: u32 = 1_600_000_001;
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_secs(u64::from(mtime_secs));
    let entries = vec![
        FreshZipEntry {
            name: "report.txt".into(),
            source: FreshZipSource::Bytes(b"hello".to_vec()),
            size: 5,
            is_directory: false,
            modified: Some(modified),
            unix_mode: None,
        },
        FreshZipEntry {
            name: "undated/".into(),
            source: FreshZipSource::Bytes(vec![]),
            size: 0,
            is_directory: true,
            modified: None,
            unix_mode: None,
        },
    ];
    let bytes = collect(spawn_fresh_zip(entries, None).expect("spawn producer"))
        .await
        .expect("produce");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");

    // Pre-fix there was no extended field at all, and the DOS field was UTC.
    assert_eq!(extended_mtime(&mut archive, 0), Some(mtime_secs));
    let local = chrono::DateTime::<chrono::Local>::from(modified).naive_local();
    let dos = archive.by_index(0).expect("file").last_modified().expect("a DOS time");
    assert_eq!(
        (dos.year(), dos.month(), dos.day(), dos.hour(), dos.minute()),
        (
            local.year() as u16,
            local.month() as u8,
            local.day() as u8,
            local.hour() as u8,
            local.minute() as u8
        ),
        "the DOS field is this machine's wall-clock time"
    );

    // A source with no mtime is dated now, never the format's 1980 zero.
    let undated = extended_mtime(&mut archive, 1).expect("an undated entry still carries a time");
    assert!(undated > mtime_secs);
}

#[tokio::test]
async fn output_larger_than_capacity_waits_for_a_gated_consumer() {
    let payload = (0..CHUNK_BYTES * (CHANNEL_CHUNKS + 3))
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "large.bin".into(),
            size: payload.len() as u64,
            source: FreshZipSource::Bytes(payload.clone()),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    tokio::task::yield_now().await;
    let bytes = collect(output)
        .await
        .expect("producer resumes after consumer opens gate");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip");
    let mut actual = Vec::new();
    archive
        .by_name("large.bin")
        .expect("entry")
        .read_to_end(&mut actual)
        .expect("read");
    assert_eq!(actual, payload);
}

/// One remote entry of `size` bytes, its feeder, and the cancellation
/// source the producer and feeder share.
fn remote_pipeline(size: u64) -> (RemoteZipFeeder, FreshZipOutput, FreshZipCancellation) {
    let (feeder, source) = remote_source_bridge();
    let cancellation = FreshZipCancellation::new(None);
    let output = spawn_fresh_zip_with_progress(
        vec![FreshZipEntry {
            name: "remote.bin".into(),
            size,
            source: FreshZipSource::Remote(source),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
        None,
        cancellation.clone(),
    )
    .expect("spawn producer");
    (feeder, output, cancellation)
}

#[tokio::test]
async fn late_remote_source_error_is_not_eof() {
    let (feeder, output, cancellation) = remote_pipeline(6);
    feeder
        .send(Ok(b"prefix".to_vec()), &cancellation)
        .await
        .expect("first chunk");
    feeder
        .send(
            Err(FreshZipError::Source {
                entry: "remote.bin".into(),
                message: "late read refusal".into(),
            }),
            &cancellation,
        )
        .await
        .expect("late error");
    drop(feeder);
    assert!(matches!(collect(output).await, Err(FreshZipError::Source { .. })));
}

#[tokio::test]
async fn remote_channel_loss_is_not_successful_eof_even_at_the_planned_size() {
    let (feeder, output, cancellation) = remote_pipeline(6);
    feeder
        .send(Ok(b"prefix".to_vec()), &cancellation)
        .await
        .expect("first chunk");
    drop(feeder);

    assert!(matches!(collect(output).await, Err(FreshZipError::SourceChannelClosed)));
}

#[tokio::test]
async fn planned_source_count_mismatch_is_terminal_failure() {
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "drifted.bin".into(),
            size: 8,
            source: FreshZipSource::Bytes(b"short".to_vec()),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    assert!(matches!(
        collect(output).await,
        Err(FreshZipError::CountMismatch {
            expected: 8,
            actual: 5,
            ..
        })
    ));
}

#[tokio::test]
async fn a_source_that_grew_fails_one_byte_past_its_planned_size() {
    // A live log keeps growing: reading it to EOF compresses bytes the plan
    // never promised, and may never end.
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "live.log".into(),
            size: 5,
            source: FreshZipSource::Bytes(vec![b'x'; 4 * CHUNK_BYTES]),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    assert!(matches!(
        collect(output).await,
        Err(FreshZipError::CountMismatch {
            expected: 5,
            actual: 6,
            ..
        })
    ));
}

#[tokio::test]
async fn a_remote_source_that_grew_fails_without_waiting_for_its_end() {
    let (feeder, output, cancellation) = remote_pipeline(6);
    feeder
        .send(Ok(b"prefix".to_vec()), &cancellation)
        .await
        .expect("first chunk");
    feeder
        .send(Ok(b"and more".to_vec()), &cancellation)
        .await
        .expect("a chunk past the planned size");
    // No `finish`: a growing source may never reach one.
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), collect(output))
        .await
        .expect("the producer stops at the first byte past the plan");
    assert!(matches!(outcome, Err(FreshZipError::CountMismatch { expected: 6, .. })));
    drop(feeder);
}

#[tokio::test]
async fn dropping_destination_cancels_a_backpressured_producer() {
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "large.bin".into(),
            size: (CHUNK_BYTES * 20) as u64,
            source: FreshZipSource::Bytes(vec![7; CHUNK_BYTES * 20]),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    let (stream, completion) = output.into_parts();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), completion.shutdown(stream))
        .await
        .expect("shutdown must unblock a producer whose output queue is full");
    assert!(matches!(
        outcome,
        Err(FreshZipError::Cancelled | FreshZipError::OutputClosed)
    ));
}

#[tokio::test]
async fn a_feeder_closed_by_cancellation_reads_as_cancelled_not_channel_loss() {
    let (feeder, output, cancellation) = remote_pipeline(1);
    let (stream, completion) = output.into_parts();
    cancellation.request();
    drop(feeder); // what the coordinator's feed task does when cancellation lands
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), completion.shutdown(stream))
        .await
        .expect("a closed bridge must wake a producer waiting for remote source bytes");
    assert!(matches!(outcome, Err(FreshZipError::Cancelled)));
}

#[tokio::test]
async fn dropping_completion_stops_a_live_remote_feeder() {
    let (feeder, output, cancellation) = remote_pipeline(1);
    let (stream, completion) = output.into_parts();

    drop(completion);
    let feeder_outcome = tokio::time::timeout(std::time::Duration::from_secs(1), feeder.finish(&cancellation))
        .await
        .expect("completion drop must stop a feeder, not queue behind a dead producer");
    assert!(matches!(feeder_outcome, Err(FreshZipError::Cancelled)));
    drop(stream);
}

#[tokio::test]
async fn a_destination_never_sees_eof_for_a_failed_zip() {
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "drifted.bin".into(),
            size: 8,
            source: FreshZipSource::Bytes(b"short".to_vec()),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    let (mut stream, completion) = output.into_parts();
    let mut last = None;
    while let Some(chunk) = stream.next_chunk().await {
        let failed = chunk.is_err();
        last = Some(chunk);
        if failed {
            break;
        }
    }
    assert!(
        matches!(last, Some(Err(VolumeError::IoError { .. }))),
        "a producer failure reaches the destination as an error, never as a complete stream"
    );
    drop(stream);
    assert!(matches!(
        completion.finish().await,
        Err(FreshZipError::CountMismatch { .. })
    ));
}

#[tokio::test]
async fn dropping_completion_and_stream_never_blocks_on_a_full_output_queue() {
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "large.bin".into(),
            size: (CHUNK_BYTES * 20) as u64,
            source: FreshZipSource::Bytes(vec![3; CHUNK_BYTES * 20]),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    let (stream, completion) = output.into_parts();
    tokio::task::yield_now().await;

    tokio::time::timeout(std::time::Duration::from_secs(1), async move {
        drop(completion);
        drop(stream);
    })
    .await
    .expect("drop must signal cancellation without joining on the runtime");
}

#[tokio::test]
async fn destination_refusal_drops_its_stream_before_joining_the_producer() {
    let output = spawn_fresh_zip(
        vec![FreshZipEntry {
            name: "large.bin".into(),
            size: (CHUNK_BYTES * 20) as u64,
            source: FreshZipSource::Bytes(vec![3; CHUNK_BYTES * 20]),
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        None,
    )
    .expect("spawn producer");
    let (stream, completion) = output.into_parts();
    drop(stream); // the backend returned early without consuming the source
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(1), completion.finish())
        .await
        .expect("joining after backend refusal must not block the runtime");
    assert!(matches!(
        outcome,
        Err(FreshZipError::Cancelled | FreshZipError::OutputClosed)
    ));
}
