//! What the binding must notice, and what it must leave alone.
//!
//! Every case here is a way a reviewed source can stop being the source that was
//! reviewed. The engine's own suites cover what happens to the survivors; these
//! cover which sources survive at all, driven through the same
//! `retain_bound_sources*` entry points the four starters call.

use std::path::{Path, PathBuf};

use super::{
    ExpectedSources, LocalContent, RemoteContent, SourceFingerprint, retain_bound_sources,
    retain_bound_sources_remote, retain_bound_sources_with_sizes,
};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{SourceItemOutcome, WriteOperationType};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume};

fn write_file(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write fixture file");
    path
}

fn expect_current(paths: &[PathBuf]) -> ExpectedSources {
    ExpectedSources::new(paths.iter().map(|path| {
        (
            path.clone(),
            SourceFingerprint::capture_local(path).expect("fixture is stat-able"),
        )
    }))
}

/// Drives the real local entry point, so every case here exercises what the
/// starters exercise.
fn retain(expected: &ExpectedSources, sink: &CollectorEventSink, sources: &[PathBuf]) -> Option<Vec<PathBuf>> {
    retain_bound_sources(
        sink,
        "op",
        WriteOperationType::Delete,
        Some(expected),
        sources.to_vec(),
    )
}

fn skipped_paths(sink: &CollectorEventSink) -> Vec<String> {
    sink.source_items_done
        .lock_ignore_poison()
        .iter()
        .filter(|event| event.outcome == SourceItemOutcome::Skipped)
        .map(|event| event.source_path.clone())
        .collect()
}

async fn seed_remote(volume: &dyn Volume, path: &Path, contents: &[u8]) {
    if volume.exists(path).await {
        volume.delete(path).await.expect("clear the previous entry");
    }
    volume.create_file(path, contents).await.expect("seed remote entry");
}

#[test]
fn an_unchanged_source_survives_and_says_nothing() {
    let dir = TestDir::new("binding_unchanged");
    let file = write_file(&dir, "report.pdf", b"one");
    let expected = expect_current(&[file.clone()]);
    let sink = CollectorEventSink::new();

    let kept = retain(&expected, &sink, std::slice::from_ref(&file));

    assert_eq!(kept, Some(vec![file]), "the source is exactly what was reviewed");
    assert!(
        sink.source_items_done.lock_ignore_poison().is_empty(),
        "a source that passed is reported by the operation that runs it, not by the pre-flight"
    );
}

/// An operation nobody bound runs untouched. This is the property that keeps the
/// pre-flight from being a second execution path: for every user-started copy,
/// move, delete, and trash it is a no-op.
#[test]
fn an_unbound_operation_keeps_every_source_and_emits_nothing() {
    let dir = TestDir::new("binding_absent");
    let file = write_file(&dir, "report.pdf", b"one");
    // A source that would fail any binding: it does not exist at all.
    let ghost = dir.join("never-existed.pdf");
    let sink = CollectorEventSink::new();

    let kept = retain_bound_sources(
        &sink,
        "op",
        WriteOperationType::Delete,
        None,
        vec![file.clone(), ghost.clone()],
    );

    assert_eq!(kept, Some(vec![file, ghost]));
    assert!(sink.source_items_done.lock_ignore_poison().is_empty());
    assert!(sink.complete.lock_ignore_poison().is_empty());
}

#[test]
fn a_source_rewritten_since_review_is_dropped_and_reported() {
    let dir = TestDir::new("binding_rewritten");
    let file = write_file(&dir, "report.pdf", b"one");
    let expected = expect_current(&[file.clone()]);

    // Same name, same inode, different bytes: exactly the case a name-only check
    // waves through.
    write_file(&dir, "report.pdf", b"a different document entirely");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, std::slice::from_ref(&file));

    assert!(kept.is_none(), "the reviewed bytes are gone, so the source is not ours");
    assert_eq!(skipped_paths(&sink), vec![file.display().to_string()]);
    assert!(
        !sink.source_items_done.lock_ignore_poison()[0].source_removed,
        "it changed, it didn't vanish, so no snapshot may drop its row"
    );
}

#[test]
fn a_source_replaced_by_a_different_file_of_the_same_size_is_dropped() {
    let dir = TestDir::new("binding_swapped");
    let file = write_file(&dir, "invoice.pdf", b"aaaa");
    let expected = expect_current(&[file.clone()]);

    // A swap that preserves size, and at mtime granularity could preserve that
    // too. The inode is what still tells them apart.
    std::fs::remove_file(&file).expect("remove original");
    write_file(&dir, "invoice.pdf", b"bbbb");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, std::slice::from_ref(&file));

    assert!(kept.is_none(), "a new inode under the old name is a different file");
    assert_eq!(skipped_paths(&sink), vec![file.display().to_string()]);
}

#[test]
fn a_source_that_vanished_is_dropped_and_reported_as_removed() {
    let dir = TestDir::new("binding_vanished");
    let file = write_file(&dir, "temp.log", b"one");
    let expected = expect_current(&[file.clone()]);
    std::fs::remove_file(&file).expect("remove");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, std::slice::from_ref(&file));

    assert!(kept.is_none());
    assert!(
        sink.source_items_done.lock_ignore_poison()[0].source_removed,
        "the file really is gone, so every stale search snapshot may drop it"
    );
}

/// An operation the binding emptied is OVER, and it did not fail: every source
/// already went out as a `Skipped` item, and a failure dialog on top of that would
/// be the engine editorializing about a decision the person may make differently.
#[test]
fn an_operation_the_binding_emptied_completes_rather_than_failing() {
    let dir = TestDir::new("binding_emptied");
    let file = write_file(&dir, "report.pdf", b"one");
    let expected = expect_current(&[file.clone()]);
    write_file(&dir, "report.pdf", b"changed");

    let sink = CollectorEventSink::new();
    assert!(retain(&expected, &sink, std::slice::from_ref(&file)).is_none());

    let complete = sink.complete.lock_ignore_poison();
    assert_eq!(complete.len(), 1, "the operation announces its own end");
    assert_eq!(complete[0].files_processed, 0);
    assert_eq!(complete[0].files_skipped, 1);
    assert_eq!(complete[0].bytes_processed, 0);
    assert!(sink.errors.lock_ignore_poison().is_empty(), "nothing failed");
}

/// The reason a directory can't be bound to its own size and mtime: those move
/// with every child write, so a proposal to delete a build directory would be
/// refused the moment anything built.
#[test]
fn a_directory_whose_children_changed_is_still_the_same_directory() {
    let dir = TestDir::new("binding_dir");
    let target = dir.join("target");
    std::fs::create_dir(&target).expect("create dir");
    std::fs::write(target.join("a.o"), b"one").expect("seed child");
    let expected = expect_current(&[target.clone()]);

    std::fs::write(target.join("b.o"), b"two").expect("add a child");
    std::fs::remove_file(target.join("a.o")).expect("remove a child");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, std::slice::from_ref(&target));

    assert_eq!(
        kept,
        Some(vec![target]),
        "the folder the user picked is still that folder"
    );
}

#[test]
fn a_directory_that_became_a_file_is_dropped() {
    let dir = TestDir::new("binding_dir_to_file");
    let target = dir.join("notes");
    std::fs::create_dir(&target).expect("create dir");
    let expected = expect_current(&[target.clone()]);

    std::fs::remove_dir(&target).expect("remove dir");
    std::fs::write(&target, b"now a file").expect("write a file in its place");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, std::slice::from_ref(&target));

    assert!(kept.is_none(), "a file where a folder was is not the reviewed source");
}

/// Binding is all-or-nothing: a caller that names only some of its sources has a
/// bug, and acting on the unnamed ones would be acting on something nobody
/// reviewed.
#[test]
fn a_source_the_binding_does_not_name_is_dropped_rather_than_waved_through() {
    let dir = TestDir::new("binding_partial");
    let named = write_file(&dir, "named.txt", b"one");
    let unnamed = write_file(&dir, "unnamed.txt", b"two");
    let expected = expect_current(&[named.clone()]);

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, &[named.clone(), unnamed.clone()]);

    assert_eq!(kept, Some(vec![named]));
    assert_eq!(skipped_paths(&sink), vec![unnamed.display().to_string()]);
}

#[test]
fn the_survivors_keep_the_callers_order() {
    let dir = TestDir::new("binding_order");
    let first = write_file(&dir, "1.txt", b"one");
    let stale = write_file(&dir, "2.txt", b"two");
    let third = write_file(&dir, "3.txt", b"three");
    let expected = expect_current(&[first.clone(), stale.clone(), third.clone()]);
    write_file(&dir, "2.txt", b"changed");

    let sink = CollectorEventSink::new();
    let kept = retain(&expected, &sink, &[first.clone(), stale, third.clone()]);

    assert_eq!(kept, Some(vec![first, third]));
}

/// Trash reports bytes from a list indexed by position. Filtering the sources
/// without the sizes would credit every later item with its predecessor's bytes,
/// which is a progress bar that lies for the whole operation.
#[test]
fn a_dropped_source_takes_its_size_with_it() {
    let dir = TestDir::new("binding_sizes");
    let first = write_file(&dir, "1.txt", b"a");
    let stale = write_file(&dir, "2.txt", b"bb");
    let third = write_file(&dir, "3.txt", b"ccc");
    let expected = expect_current(&[first.clone(), stale.clone(), third.clone()]);
    write_file(&dir, "2.txt", b"changed");

    let sink = CollectorEventSink::new();
    let kept = retain_bound_sources_with_sizes(
        &sink,
        "op",
        WriteOperationType::Trash,
        Some(&expected),
        vec![first.clone(), stale, third.clone()],
        Some(vec![1, 2, 3]),
    );

    assert_eq!(kept, Some((vec![first, third], Some(vec![1, 3]))));
}

#[tokio::test]
async fn a_remote_source_rewritten_since_review_is_dropped_and_reported() {
    let volume = InMemoryVolume::new("Share");
    let path = PathBuf::from("/photos/holiday.jpg");
    seed_remote(&volume, &path, b"original").await;
    let expected = ExpectedSources::new([(
        path.clone(),
        SourceFingerprint::capture_remote(&volume, &path)
            .await
            .expect("capture remote"),
    )]);

    seed_remote(&volume, &path, b"replaced with something longer").await;

    let sink = CollectorEventSink::new();
    let kept = retain_bound_sources_remote(
        &volume,
        &sink,
        "op",
        WriteOperationType::Delete,
        Some(&expected),
        vec![path.clone()],
    )
    .await;

    assert!(kept.is_none());
    assert_eq!(skipped_paths(&sink), vec![path.display().to_string()]);
    assert!(
        !sink.source_items_done.lock_ignore_poison()[0].source_removed,
        "it changed on the share; it is still there"
    );
}

#[tokio::test]
async fn an_unchanged_remote_source_survives() {
    let volume = InMemoryVolume::new("Share");
    let path = PathBuf::from("/photos/holiday.jpg");
    seed_remote(&volume, &path, b"original").await;
    let expected = ExpectedSources::new([(
        path.clone(),
        SourceFingerprint::capture_remote(&volume, &path)
            .await
            .expect("capture remote"),
    )]);

    let sink = CollectorEventSink::new();
    let kept = retain_bound_sources_remote(
        &volume,
        &sink,
        "op",
        WriteOperationType::Delete,
        Some(&expected),
        vec![path.clone()],
    )
    .await;

    assert_eq!(kept, Some(vec![path]));
    assert!(sink.source_items_done.lock_ignore_poison().is_empty());
}

#[tokio::test]
async fn a_remote_source_that_vanished_is_reported_as_removed() {
    let volume = InMemoryVolume::new("Share");
    let path = PathBuf::from("/photos/holiday.jpg");
    seed_remote(&volume, &path, b"original").await;
    let expected = ExpectedSources::new([(
        path.clone(),
        SourceFingerprint::capture_remote(&volume, &path)
            .await
            .expect("capture remote"),
    )]);
    volume.delete(&path).await.expect("delete remote file");

    let sink = CollectorEventSink::new();
    let kept = retain_bound_sources_remote(
        &volume,
        &sink,
        "op",
        WriteOperationType::Delete,
        Some(&expected),
        vec![path.clone()],
    )
    .await;

    assert!(kept.is_none());
    assert!(sink.source_items_done.lock_ignore_poison()[0].source_removed);
}

/// A local fingerprint can never satisfy a remote expectation, or the other way
/// round: the two describe different kinds of identity, and treating them as
/// interchangeable is how a path-only match would slip through.
#[tokio::test]
async fn a_remote_source_never_matches_a_local_expectation() {
    let volume = InMemoryVolume::new("Share");
    let path = PathBuf::from("/photos/holiday.jpg");
    seed_remote(&volume, &path, b"original").await;
    let expected = ExpectedSources::new([(
        path.clone(),
        SourceFingerprint::Local {
            device: 1,
            inode: 2,
            content: LocalContent::File {
                size: 8,
                modified_nanos: None,
            },
        },
    )]);

    let sink = CollectorEventSink::new();
    let kept = retain_bound_sources_remote(
        &volume,
        &sink,
        "op",
        WriteOperationType::Delete,
        Some(&expected),
        vec![path],
    )
    .await;

    assert!(kept.is_none());
}

/// The journal's mtime column is Unix SECONDS, and undo compares it against
/// `FileEntry::modified_at`, also Unix seconds. A local fingerprint holds
/// NANOseconds, so recording it raw would make every undo report drift and refuse,
/// silently disabling undo.
#[test]
fn a_local_fingerprint_journals_its_mtime_in_whole_seconds() {
    let fingerprint = SourceFingerprint::Local {
        device: 1,
        inode: 2,
        content: LocalContent::File {
            size: 4096,
            // Truncated, not rounded: `Duration::as_secs` on the read side floors
            // too, so the two readings of one file agree exactly.
            modified_nanos: Some(1_700_000_000_987_654_321),
        },
    };
    assert_eq!(fingerprint.journal_snapshot(), (Some(4096), Some(1_700_000_000)));
}

/// The unit-agreement test: one real file, read through both sides of the
/// contract. This is what a wrong conversion breaks, and asserting `is_some()`
/// alone would not catch it.
#[test]
fn a_journaled_mtime_equals_the_live_reading_undo_rechecks_it_against() {
    let dir = TestDir::new("binding_snapshot_unit");
    let source = write_file(&dir, "receipt.pdf", b"reviewed");
    let fingerprint = SourceFingerprint::capture_local(&source).expect("capture");

    let (size, mtime) = fingerprint.journal_snapshot();
    let live = crate::file_system::listing::get_single_entry(&source).expect("read the live entry");

    assert_eq!(mtime, live.modified_at.map(|secs| secs as i64));
    assert_eq!(size, live.size.map(|size| size as i64));
    assert!(mtime.is_some(), "a local file must journal a verifiable mtime");
}

/// A remote fingerprint already holds `FileEntry::modified_at`, so it needs no
/// conversion — and must not get one.
#[test]
fn a_remote_fingerprint_journals_the_seconds_it_already_holds() {
    let fingerprint = SourceFingerprint::Remote {
        normalized_path: "/photos/holiday.jpg".to_string(),
        content: RemoteContent::File {
            size: Some(4096),
            modified: Some(1_700_000_000),
        },
    };
    assert_eq!(fingerprint.journal_snapshot(), (Some(4096), Some(1_700_000_000)));
}

/// A backend that reports no mtime (MTP, some SMB servers) journals none, so the
/// recheck falls back to size alone rather than inventing a value that would read
/// as a match.
#[test]
fn a_remote_fingerprint_with_no_mtime_journals_none() {
    let fingerprint = SourceFingerprint::Remote {
        normalized_path: "/share/receipt.pdf".to_string(),
        content: RemoteContent::File {
            size: Some(1_024),
            modified: None,
        },
    };
    assert_eq!(fingerprint.journal_snapshot(), (Some(1_024), None));
}

/// A directory has no bytes of its own to be held to, so undo falls back to its
/// other checks rather than reading a size that means nothing.
#[test]
fn a_directory_journals_no_size_and_no_mtime() {
    let fingerprint = SourceFingerprint::Local {
        device: 1,
        inode: 2,
        content: LocalContent::Directory,
    };
    assert_eq!(fingerprint.journal_snapshot(), (None, None));
}
