//! A session's media read: what it reads first, what it reports, what a changed
//! file costs, and that a closed sheet stops it. The reader is a stand-in (the
//! parsers have their own tests in `cmdr-media-meta`), so these pin the loader.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use cmdr_fs::volume::DirectoryChange;
use cmdr_listing::testing::{TestListing, TestListingGuard};
use cmdr_local::reading::list_directory_core;
use cmdr_media_meta::{MediaFacts, MediaField, ReadError};
use cmdr_operations::testing::CollectorEventSink;

use super::app_host::app_multi_rename;
use super::error::MultiRenameError;
use super::mask::FieldGap;
use super::media::{FieldUsage, FileJob, Loader, MediaProgress, MediaProgressEvent};
use super::plan::{MultiRenameSpec, RowStatus};
use super::run_test::strip_diacritics;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::{TestDir, wait_until_async};

/// Under nextest's 8 s limit, so a stuck loader fails with the wait's message.
const WAIT: Duration = Duration::from_secs(5);

/// A scratch folder with `files` and `folders`, and a cached listing showing every row.
fn folder(tag: &str, files: &[&str], folders: &[&str]) -> (TestDir, TestListingGuard) {
    let dir = TestDir::new(tag);
    for name in files {
        std::fs::write(dir.join(name), b"x").expect("scratch dir is writable");
    }
    for name in folders {
        std::fs::create_dir(dir.join(name)).expect("scratch dir is writable");
    }
    let listing = TestListing::new()
        .volume("root")
        .path(dir.to_path_buf())
        .entries(list_directory_core(&dir, &crate::listing_host::backends::AppListings).expect("the dir lists"))
        .insert(crate::listing_host::listings(), tag);
    (dir, listing)
}

/// Rewrites `name` with `bytes` and tells the listing, as a watcher would.
fn change_file(listing: &TestListingGuard, dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).expect("scratch dir is writable");
    let entry = list_directory_core(dir, &crate::listing_host::backends::AppListings)
        .expect("the dir lists")
        .into_iter()
        .find(|e| e.name == name)
        .expect("the file lists");
    listing
        .engine()
        .directory_changed("root", dir, DirectoryChange::Modified(entry));
}

fn titled(title: &str) -> MediaFacts {
    MediaFacts {
        title: Some(title.to_string()),
        ..MediaFacts::default()
    }
}

/// Every progress event the loader sent.
#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<MediaProgressEvent>>>);

impl Events {
    fn all(&self) -> Vec<MediaProgressEvent> {
        self.0.lock_ignore_poison().clone()
    }
}

/// Which files the reader was handed, in order.
#[derive(Clone, Default)]
struct Calls(Arc<Mutex<Vec<String>>>);

impl Calls {
    fn names(&self) -> Vec<String> {
        self.0.lock_ignore_poison().clone()
    }

    fn count_of(&self, name: &str) -> usize {
        self.names().iter().filter(|n| *n == name).count()
    }
}

/// Holds a reader until the test opens it.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    /// Waits for `open`, at most `WAIT`: a test that fails before opening its gate
    /// must not leave a blocking read that the runtime's shutdown waits on forever.
    fn wait(&self) {
        let (open, opened) = &*self.0;
        let is_open = open.lock_ignore_poison();
        let _ = opened
            .wait_timeout_while(is_open, WAIT, |is_open| !*is_open)
            .expect("the gate's lock isn't poisoned: nothing panics holding it");
    }

    fn open(&self) {
        let (open, opened) = &*self.0;
        *open.lock_ignore_poison() = true;
        opened.notify_all();
    }
}

/// A loader whose reader records each file and answers with `answer`.
fn loader(
    parallel: usize,
    events: &Events,
    calls: &Calls,
    answer: impl Fn(&FileJob) -> Result<MediaFacts, ReadError> + Send + Sync + 'static,
) -> Loader {
    let (events, calls) = (events.clone(), calls.clone());
    Loader {
        reader: Arc::new(move |job: &FileJob| {
            calls.0.lock_ignore_poison().push(job.name.clone());
            answer(job)
        }),
        emit: Arc::new(move |progress| events.0.lock_ignore_poison().push(progress)),
        parallel,
        volume_gone: Arc::new(|| false),
    }
}

async fn read_through(session_id: &str) {
    wait_until_async(WAIT, "the media read to finish", || {
        app_multi_rename().media_progress(session_id).is_ok_and(|p| p.done)
    })
    .await;
}

#[test]
fn opening_counts_the_media_files_only() {
    let (_dir, listing) = folder("media-seed", &["a.jpg", "b.MP3", "notes.txt", ".jpg"], &["trip.jpg"]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");

    let progress = app_multi_rename()
        .media_progress(&session.session_id)
        .expect("progress");

    assert_eq!(
        progress,
        MediaProgress {
            read: 0,
            total: 2,
            done: false
        }
    );
    app_multi_rename().close(&session.session_id);
}

#[test]
fn a_session_without_media_files_is_done_from_the_start() {
    let (_dir, listing) = folder("media-none", &["a.txt"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");

    assert!(
        app_multi_rename()
            .media_progress(&session.session_id)
            .expect("progress")
            .done
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_media_file_is_read_once_and_progress_ends_with_done() {
    let files: Vec<String> = (0..30).map(|i| format!("{i:02}.jpg")).collect();
    let names: Vec<&str> = files.iter().map(String::as_str).chain(["readme.txt"]).collect();
    let (_dir, listing) = folder("media-all", &names, &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(8, &events, &calls, |job| Ok(titled(&job.name))),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;
    wait_until_async(WAIT, "the final event", || events.all().last().is_some_and(|e| e.done)).await;

    let mut read = calls.names();
    read.sort();
    assert_eq!(read, files, "each media file once, nothing else");
    let all = events.all();
    let last = all.last().expect("an event");
    assert_eq!((last.read, last.total, last.done), (30, 30, true));
    assert_eq!(last.session_id, session.session_id);
    // Throttled: 30 instant reads don't make 30 events.
    assert!(all.len() < 30, "{} events", all.len());
    assert!(all.windows(2).all(|pair| pair[0].read <= pair[1].read));
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_file_goes_first_then_the_rows_on_screen_then_the_rest() {
    let (_dir, listing) = folder(
        "media-order",
        &["1.jpg", "2.jpg", "3.jpg", "4.jpg", "5.jpg", "6.jpg"],
        &[],
    );
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls, gate) = (Events::default(), Calls::default(), Gate::default());
    let held = gate.clone();

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, move |job| {
                if job.name == "1.jpg" {
                    held.wait();
                }
                Ok(MediaFacts::default())
            }),
        )
        .expect("the read starts");
    wait_until_async(WAIT, "the first read to start", || calls.count_of("1.jpg") == 1).await;
    app_multi_rename()
        .prioritize_media(&session.session_id, &[4, 5, 99])
        .expect("the session is open");
    gate.open();
    read_through(&session.session_id).await;

    assert_eq!(
        calls.names(),
        vec!["1.jpg", "5.jpg", "6.jpg", "2.jpg", "3.jpg", "4.jpg"]
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_sheet_stops_the_read_and_tells_the_read_in_flight() {
    let (_dir, listing) = folder("media-close", &["1.jpg", "2.jpg", "3.jpg"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls, gate) = (Events::default(), Calls::default(), Gate::default());
    let held = gate.clone();
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let saw = Arc::clone(&saw_cancel);
    // Lives in the reader, so it's freed only once the loader and the session are gone.
    let alive = Arc::new(());
    let in_reader = Arc::clone(&alive);

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, move |job| {
                let _alive = &in_reader;
                held.wait();
                saw.store(job.cancel.load(Ordering::Relaxed), Ordering::Relaxed);
                Err(ReadError::Cancelled)
            }),
        )
        .expect("the read starts");
    wait_until_async(WAIT, "the first read to start", || calls.names().len() == 1).await;
    app_multi_rename().close(&session.session_id);
    gate.open();
    wait_until_async(WAIT, "the loader to let go", || Arc::strong_count(&alive) == 1).await;

    assert_eq!(calls.names(), vec!["1.jpg"]);
    assert!(saw_cancel.load(Ordering::Relaxed), "the read in flight saw the cancel");
    assert!(
        events.all().iter().all(|e| !e.done),
        "a closed session says nothing more"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_changed_since_it_was_read_reads_again() {
    let (dir, listing) = folder("media-fresh", &["a.jpg", "b.jpg"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());
    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(8, &events, &calls, |_| Ok(MediaFacts::default())),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;
    assert_eq!(calls.count_of("a.jpg"), 1);

    change_file(&listing, &dir, "a.jpg", b"a bigger file");
    app_multi_rename()
        .preview(&session.session_id, &strip_diacritics())
        .expect("a preview");

    wait_until_async(WAIT, "the changed file to read again", || calls.count_of("a.jpg") == 2).await;
    read_through(&session.session_id).await;
    assert_eq!(calls.count_of("b.jpg"), 1, "the unchanged file stays read");
    // Across both runs, an event says done exactly when every media file is read.
    for event in events.all() {
        assert_eq!(event.done, event.read == event.total, "{event:?}");
    }
    app_multi_rename().close(&session.session_id);
}

/// `count` media files, `00.jpg` on.
fn photos(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("{i:02}.jpg")).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_keeps_timing_out_is_given_up_on_after_three_files() {
    let files = photos(20);
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_dir, listing) = folder("media-dead-volume", &names, &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, |_| Err(ReadError::TimedOut)),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;

    assert_eq!(calls.names().len(), 3, "{:?}", calls.names());
    let progress = app_multi_rename()
        .media_progress(&session.session_id)
        .expect("progress");
    assert_eq!(
        (progress.read, progress.total),
        (20, 20),
        "the rest are unreadable, not reading"
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_reads_resets_the_count_of_failures_in_a_row() {
    let files = photos(10);
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_dir, listing) = folder("media-flaky-volume", &names, &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, |job| match job.name.as_str() {
                "02.jpg" => Ok(MediaFacts::default()),
                "00.jpg" | "01.jpg" => Err(ReadError::Io("a dropped packet".to_string())),
                _ => Err(ReadError::TimedOut),
            }),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;

    assert_eq!(
        calls.names(),
        vec!["00.jpg", "01.jpg", "02.jpg", "03.jpg", "04.jpg", "05.jpg"]
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_that_went_away_reads_nothing_and_every_file_is_unreadable() {
    let (_dir, listing) = folder("media-gone-volume", &["a.jpg", "b.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());
    let gone = Loader {
        volume_gone: Arc::new(|| true),
        ..loader(8, &events, &calls, |_| Ok(MediaFacts::default()))
    };

    app_multi_rename()
        .start_media_with(&session.session_id, gone)
        .expect("the read starts");
    read_through(&session.session_id).await;

    assert!(calls.names().is_empty());
    let samples = app_multi_rename()
        .field_samples(&session.session_id, "=tag.title", &[0])
        .expect("samples");
    assert_eq!(
        samples.samples[0].gap,
        Some(FieldGap::Unreadable {
            field: MediaField::TagTitle
        })
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_fails_or_panics_counts_as_read_and_the_rest_go_on() {
    let (_dir, listing) = folder("media-unreadable", &["boom.jpg", "late.jpg", "ok.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, |job| match job.name.as_str() {
                "boom.jpg" => panic!("a parser met a hostile file"),
                "late.jpg" => Err(ReadError::TimedOut),
                _ => Ok(titled("Digital Love")),
            }),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;

    let overview = app_multi_rename()
        .media_overview(&session.session_id)
        .expect("an overview");
    assert_eq!(
        overview.progress,
        MediaProgress {
            read: 3,
            total: 3,
            done: true
        }
    );
    let title = overview
        .fields
        .iter()
        .find(|f| f.field == MediaField::TagTitle)
        .expect("every field is listed");
    assert_eq!(title.files_with_it, 1);
    assert_eq!(overview.fields.len(), MediaField::ALL.len());
    app_multi_rename().close(&session.session_id);
}

/// A reader that reads nothing (the browser build's, where no parser runs): every
/// file settles `Unreadable` at once, none counts toward giving up on the volume, and
/// the read ends `done`. A mask with the field alone flags every row; one with a
/// `|modified` fallback renames them all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reader_that_reads_nothing_settles_every_file_unreadable_without_giving_up() {
    let files = photos(10);
    let names: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_dir, listing) = folder("media-not-supported", &names, &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());

    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(1, &events, &calls, |_| Err(ReadError::NotSupported)),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;
    wait_until_async(WAIT, "the final event", || events.all().last().is_some_and(|e| e.done)).await;

    assert_eq!(calls.names(), files, "every file asked, one by one: no give-up");
    let last = events.all().last().cloned().expect("an event");
    assert_eq!((last.read, last.total, last.done), (10, 10, true));

    let field = MediaField::ExifTaken;
    let taken = MultiRenameSpec {
        name_mask: "[=exif.taken]".to_string(),
        ..title_mask()
    };
    let preview = app_multi_rename()
        .preview(&session.session_id, &taken)
        .expect("a preview");
    assert!(
        preview
            .rows
            .iter()
            .all(|row| row.status == RowStatus::Unreadable { field }),
        "{:?}",
        preview.rows
    );
    assert_eq!((preview.counts.problems, preview.counts.reading), (10, 0));

    let with_fallback = MultiRenameSpec {
        name_mask: "[=exif.taken|modified:YMD] [N]".to_string(),
        ..title_mask()
    };
    let preview = app_multi_rename()
        .preview(&session.session_id, &with_fallback)
        .expect("a preview");
    assert_eq!(preview.counts.ready, 10, "{:?}", preview.rows);
    let samples = app_multi_rename()
        .field_samples(&session.session_id, "=exif.taken|modified:YMD", &[0])
        .expect("samples");
    assert_eq!(samples.samples[0].gap, None);
    assert_eq!((samples.usage.fallback, samples.usage.missing), (10, 0));
    app_multi_rename().close(&session.session_id);
}

fn title_mask() -> MultiRenameSpec {
    MultiRenameSpec {
        name_mask: "[=tag.title]".to_string(),
        remove_diacritics: false,
        ..strip_diacritics()
    }
}

/// A preview the user saw fully read never turns into a queued wait: a file that
/// changed under it is the folder changing, so apply asks for a fresh look.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_changing_under_a_fully_read_preview_makes_it_out_of_date_not_still_reading() {
    let (dir, listing) = folder("media-changed-under", &["song.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let (events, calls) = (Events::default(), Calls::default());
    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(8, &events, &calls, |_| Ok(titled("Harder"))),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;
    let shown = app_multi_rename()
        .preview(&session.session_id, &title_mask())
        .expect("a preview");
    assert_eq!(shown.counts.ready, 1);

    change_file(&listing, &dir, "song.mp3", b"a re-tagged song");
    let refused = app_multi_rename()
        .apply(
            Arc::new(CollectorEventSink::new()),
            session.session_id.clone(),
            shown.preview_id,
        )
        .await;

    assert!(
        matches!(refused, Err(MultiRenameError::PreviewOutOfDate)),
        "{refused:?}"
    );
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_waits_while_a_row_is_still_reading() {
    let (_dir, listing) = folder("media-still-reading", &["song.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");

    // Never started: the file stays `Reading`.
    let preview = app_multi_rename()
        .preview(&session.session_id, &title_mask())
        .expect("a preview");
    assert_eq!(preview.rows[0].status, RowStatus::Reading);
    assert_eq!(preview.counts.reading, 1);

    let events = Arc::new(CollectorEventSink::new());
    let refused = app_multi_rename()
        .apply(events, session.session_id.clone(), preview.preview_id)
        .await;
    assert!(matches!(refused, Err(MultiRenameError::StillReading)), "{refused:?}");
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_preview_uses_what_was_read() {
    let (_dir, listing) = folder("media-preview", &["track01.mp3", "track02.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let titles = HashMap::from([("track01.mp3", "One More Time"), ("track02.mp3", "Aerodynamic")]);
    let (events, calls) = (Events::default(), Calls::default());
    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(8, &events, &calls, move |job| Ok(titled(titles[job.name.as_str()]))),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;

    let preview = app_multi_rename()
        .preview(&session.session_id, &title_mask())
        .expect("a preview");

    let names: Vec<&str> = preview.rows.iter().map(|row| row.new_name.as_str()).collect();
    assert_eq!(names, vec!["One More Time.mp3", "Aerodynamic.mp3"]);
    assert_eq!(preview.counts.ready, 2);
    app_multi_rename().close(&session.session_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn field_samples_show_the_token_on_the_rows_asked_and_count_the_whole_session() {
    let (_dir, listing) = folder("media-samples", &["a.mp3", "b.mp3", "c.txt"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");
    let before = app_multi_rename()
        .field_samples(&session.session_id, "=tag.title", &[0])
        .expect("samples");
    assert_eq!(before.usage.reading, 2, "asked before the read");
    let (events, calls) = (Events::default(), Calls::default());
    app_multi_rename()
        .start_media_with(
            &session.session_id,
            loader(8, &events, &calls, |job| {
                Ok(if job.name == "a.mp3" {
                    titled("Veridis Quo")
                } else {
                    MediaFacts::default()
                })
            }),
        )
        .expect("the read starts");
    read_through(&session.session_id).await;

    let samples = app_multi_rename()
        .field_samples(&session.session_id, "=tag.title", &[0, 1, 7])
        .expect("samples");

    assert_eq!(samples.samples.len(), 2, "a row past the end is left out");
    assert_eq!(samples.samples[0].text.as_deref(), Some("Veridis Quo"));
    assert_eq!(
        samples.samples[1].gap,
        Some(FieldGap::NoValue {
            field: MediaField::TagTitle
        })
    );
    assert_eq!(
        samples.usage,
        FieldUsage {
            with_value: 1,
            fallback: 0,
            missing: 2,
            reading: 0
        }
    );
    app_multi_rename().close(&session.session_id);
}

#[test]
fn field_samples_say_reading_before_the_read_and_refuse_other_tokens() {
    let (_dir, listing) = folder("media-samples-early", &["a.mp3"], &[]);
    let session = app_multi_rename()
        .open(listing.id(), true, None, 0)
        .expect("the session opens");

    let samples = app_multi_rename()
        .field_samples(&session.session_id, "=tag.title", &[0])
        .expect("samples");
    assert_eq!(samples.samples[0].gap, Some(FieldGap::Reading));
    assert_eq!(samples.usage.reading, 1);
    // Asked again with nothing read since: the same answer (from the usage cache).
    assert_eq!(
        app_multi_rename()
            .field_samples(&session.session_id, "=tag.title", &[0])
            .expect("samples"),
        samples
    );

    assert!(matches!(
        app_multi_rename().field_samples(&session.session_id, "N2-5", &[0]),
        Err(MultiRenameError::Spec { .. })
    ));
    app_multi_rename().close(&session.session_id);
}

#[test]
fn asking_a_closed_session_says_so() {
    assert!(matches!(
        app_multi_rename().media_progress("multi-rename-never-opened"),
        Err(MultiRenameError::SessionClosed)
    ));
    assert!(matches!(
        app_multi_rename().prioritize_media("multi-rename-never-opened", &[0]),
        Err(MultiRenameError::SessionClosed)
    ));
}
