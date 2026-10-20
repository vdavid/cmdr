//! Tests for "Open with" on a file only a route serves (`open_with_extract`).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::materialize::reap_orphan_temps;
use super::open_with_copies::SweepPolicy;
use super::open_with_extract::{
    OpenWithCopyLaunched, OpenWithCopyRefusal, OpenWithCopyRefused, OpenWithCopySource, RefusedCopy,
    is_open_with_temp_name, launch_paths_in, listing_path_in, reap_open_with_temps_with, sweep_open_with_copies_at,
};
use super::{ArchiveFailureKind, ViewerError};

/// Registers a real local-FS "root" volume so the archive route finds a parent.
fn ensure_root_volume() {
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_local::LocalPosixVolume;
    get_volume_manager().register_if_absent(
        "root",
        Arc::new(LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
}

fn build_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use zip::write::SimpleFileOptions;
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in entries {
        writer.start_file(*name, opts).expect("start file");
        writer.write_all(content).expect("write entry");
    }
    writer.finish().expect("finish zip");
}

const CAP: u64 = 1024 * 1024;

#[test]
fn an_ordinary_path_launches_as_it_is() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let plain = PathBuf::from("/Users/me/report.pdf");
    let got = launch_paths_in(std::slice::from_ref(&plain), dir.path(), CAP).expect("no extraction needed");
    assert_eq!(got, vec![plain]);
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("read dir").count(),
        0,
        "nothing written"
    );
}

#[test]
fn a_file_inside_an_archive_launches_as_a_fresh_read_only_copy() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("docs/report.pdf", b"%PDF-1.4 hello")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let plain = src.path().join("beside.txt");

    let got = launch_paths_in(&[zip.join("docs/report.pdf"), plain.clone()], dir.path(), CAP).expect("extract");

    let copy = &got[0];
    assert_eq!(
        copy.file_name().and_then(|n| n.to_str()),
        Some("report.pdf"),
        "keeps the name"
    );
    assert_eq!(std::fs::read(copy).expect("read copy"), b"%PDF-1.4 hello");
    let subdir = copy.parent().expect("subdir");
    assert_eq!(subdir.parent(), Some(dir.path()), "lands in the open-with dir");
    assert!(is_open_with_temp_name(
        &subdir.file_name().expect("name").to_string_lossy()
    ));
    let perms = std::fs::metadata(copy).expect("meta").permissions();
    assert!(
        perms.readonly(),
        "a read-only copy: edits can't look like they reach the archive"
    );
    assert_eq!(got[1], plain, "an ordinary path beside it is untouched");
}

#[test]
fn every_launch_gets_its_own_copy() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("a.txt", b"one")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let inner = zip.join("a.txt");

    let first = launch_paths_in(std::slice::from_ref(&inner), dir.path(), CAP).expect("first");
    let second = launch_paths_in(std::slice::from_ref(&inner), dir.path(), CAP).expect("second");

    assert_ne!(
        first[0], second[0],
        "no dedup: an earlier app may still hold the first copy"
    );
    assert!(first[0].exists() && second[0].exists());
}

#[test]
fn an_oversize_file_is_refused_before_anything_is_written() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("big.zip");
    build_zip(&zip, &[("big.bin", &[7u8; 4096])]);
    let dir = tempfile::tempdir().expect("open-with dir");

    let refused = launch_paths_in(&[zip.join("big.bin")], dir.path(), 100).expect_err("over the cap");

    assert_eq!(refused.path, zip.join("big.bin"), "names the file it couldn't copy");
    assert_eq!(refused.reason, OpenWithCopyRefusal::TooLarge { cap: 100 });
    assert_eq!(refused.source, OpenWithCopySource::Archive);
    assert!(
        matches!(refused.error, ViewerError::TooLargeToPreview { cap: 100, .. }),
        "keeps the pull's own failure for the log, got {:?}",
        refused.error
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).expect("read dir").count(),
        0,
        "nothing left behind"
    );
}

/// A file in a repo's `.git` history snapshot goes through the same pull, but the toast
/// can't say "from inside the archive" there, so the refusal says where the file came from.
#[test]
fn an_oversize_file_in_a_repo_snapshot_is_refused_as_coming_from_the_repo_history() {
    use cmdr_git::test_fixtures::{Fixture, cleanup, temp_dir};

    ensure_root_volume();
    crate::file_system::git::wiring::set_virtual_portal_enabled(true);
    let repo = temp_dir("open_with_extract", "oversize_snapshot");
    let mut fixture = Fixture::init(repo.clone());
    fixture.commit_files(&[("big.bin", &[7u8; 4096])], "initial", 1_700_000_000);
    let dir = tempfile::tempdir().expect("open-with dir");
    let inner = repo.join(".git/branches/main/big.bin");

    let refused = launch_paths_in(std::slice::from_ref(&inner), dir.path(), 100).expect_err("over the cap");

    assert_eq!(refused.path, inner);
    assert_eq!(refused.reason, OpenWithCopyRefusal::TooLarge { cap: 100 });
    assert_eq!(refused.source, OpenWithCopySource::RepoHistory);
    cleanup(&repo);
}

/// A password the archive hasn't been given yet is its own refusal: the person can do
/// something about it (copy it out, which asks for the password), unlike a damaged one.
#[test]
fn a_file_in_a_locked_archive_is_refused_as_needing_its_password() {
    use cmdr_archive::test_fixtures::{CryptoFixtureFile, build_aes_zip};

    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("locked.zip");
    let bytes = build_aes_zip(
        &[CryptoFixtureFile {
            name: "secret.pdf".to_string(),
            content: b"%PDF-1.4 hush".to_vec(),
            encrypted: true,
        }],
        "open sesame",
        zip::AesMode::Aes256,
    );
    std::fs::write(&zip, bytes).expect("write the locked zip");
    let dir = tempfile::tempdir().expect("open-with dir");

    let refused = launch_paths_in(&[zip.join("secret.pdf")], dir.path(), CAP).expect_err("no password yet");

    assert_eq!(refused.reason, OpenWithCopyRefusal::NeedsPassword);
}

#[test]
fn every_other_pull_failure_lands_on_a_refusal_the_toast_can_word() {
    let archive = |failure| ViewerError::Archive {
        failure,
        message: "zip says no".into(),
    };
    let cases = [
        (
            ViewerError::TooLargeToPreview { size: 9, cap: 5 },
            OpenWithCopyRefusal::TooLarge { cap: 5 },
        ),
        (
            archive(ArchiveFailureKind::NeedsPassword),
            OpenWithCopyRefusal::NeedsPassword,
        ),
        (
            archive(ArchiveFailureKind::Unsupported),
            OpenWithCopyRefusal::ArchiveUnreadable,
        ),
        (
            archive(ArchiveFailureKind::Unreadable),
            OpenWithCopyRefusal::ArchiveUnreadable,
        ),
        (
            ViewerError::Io { message: "EIO".into() },
            OpenWithCopyRefusal::Unreadable,
        ),
        (
            ViewerError::NotFound {
                path: "/gone".to_string(),
            },
            OpenWithCopyRefusal::Unreadable,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(OpenWithCopyRefusal::from(&error), expected, "for {error:?}");
    }
}

#[test]
fn the_notice_names_the_file_and_the_app_as_the_person_sees_them() {
    let refused = RefusedCopy {
        path: PathBuf::from("/Users/me/bundle.zip/docs/report.pdf"),
        reason: OpenWithCopyRefusal::NeedsPassword,
        source: OpenWithCopySource::Archive,
        error: ViewerError::IsDirectory,
    };

    let notice = OpenWithCopyRefused::new(&refused, Path::new("/System/Applications/Preview.app"));

    assert_eq!(notice.file_name, "report.pdf");
    assert_eq!(notice.app_name, "Preview");
    assert_eq!(notice.reason, OpenWithCopyRefusal::NeedsPassword);
    assert_eq!(notice.source, OpenWithCopySource::Archive);
}

#[test]
fn each_reaper_takes_only_its_own_family() {
    let dir = tempfile::tempdir().expect("shared parent");
    let open_with = dir.path().join(".cmdr-open-with-1");
    let viewer = dir.path().join(".cmdr-viewer-1");
    std::fs::create_dir_all(&open_with).expect("mk open-with temp");
    std::fs::create_dir_all(&viewer).expect("mk viewer temp");

    reap_orphan_temps(dir.path());
    assert!(
        open_with.exists(),
        "the viewer's reaper must not touch an open-with temp"
    );
    assert!(!viewer.exists());

    std::fs::create_dir_all(&viewer).expect("mk viewer temp again");
    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &no_trash);
    assert!(!open_with.exists());
    assert!(viewer.exists(), "the open-with reaper must not touch a viewer temp");
}

#[test]
fn a_file_inside_an_archive_lists_apps_against_a_real_stand_in_with_its_extension() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let inner = PathBuf::from("/Users/me/bundle.zip/photos/IMG_1.HEIC");

    let stand_in = listing_path_in(&inner, dir.path());

    assert_ne!(stand_in, inner);
    assert!(
        stand_in.is_file(),
        "LaunchServices answers no apps for a path with nothing at it"
    );
    assert_eq!(stand_in.extension().and_then(|e| e.to_str()), Some("HEIC"));
    assert_eq!(
        listing_path_in(&inner, dir.path()),
        stand_in,
        "one stand-in per extension"
    );
}

#[test]
fn an_ordinary_path_lists_apps_against_itself() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let plain = PathBuf::from("/Users/me/report.pdf");
    assert_eq!(listing_path_in(&plain, dir.path()), plain);
}

#[test]
fn the_launched_notice_names_the_first_copy_and_counts_the_copies() {
    let rows = [
        PathBuf::from("/Users/me/bundle.zip/a.pdf"),
        PathBuf::from("/Users/me/plain.pdf"),
        PathBuf::from("/Users/me/bundle.zip/b.pdf"),
    ];
    let launched = [
        PathBuf::from("/data/.cmdr-open-with-1/a.pdf"),
        rows[1].clone(),
        PathBuf::from("/data/.cmdr-open-with-2/b.pdf"),
    ];

    let notice = OpenWithCopyLaunched::new(&rows, &launched, Path::new("/System/Applications/Preview.app"))
        .expect("two rows were copies");

    assert_eq!(notice.file_name, "a.pdf");
    assert_eq!(notice.copy_count, 2);
    assert_eq!(notice.app_name, "Preview");
    assert!(
        OpenWithCopyLaunched::new(&rows[1..2], &launched[1..2], Path::new("/Applications/Preview.app")).is_none(),
        "a launch with no copy says nothing"
    );
}

/// The stand-ins are Cmdr's own empty files, never anything an app saw, so they go
/// without a trip to the Trash.
#[test]
fn the_startup_reaper_removes_the_stand_ins_outright() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let stand_in = listing_path_in(Path::new("/Users/me/bundle.zip/a.pdf"), dir.path());
    assert!(stand_in.is_file());

    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &|path: &Path| {
        panic!("trashed a stand-in: {}", path.display())
    });

    assert!(open_with_temps(dir.path()).is_empty());
}

/// A Trash that refuses everything: the reaper must then leave what it would have kept.
fn no_trash(_path: &Path) -> Result<(), String> {
    Err("no Trash in this test".to_string())
}

/// A stand-in Trash: moves each item into `trash`, so a test can find what was kept.
fn into(trash: &Path) -> impl Fn(&Path) -> Result<(), String> + '_ {
    move |path: &Path| {
        let name = path.file_name().ok_or("no name")?;
        std::fs::rename(path, trash.join(name)).map_err(|e| e.to_string())
    }
}

fn open_with_temps(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .expect("read dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_open_with_temp_name(&path.file_name().expect("name").to_string_lossy()))
        .collect()
}

/// An editor's atomic save: write beside the copy, then rename over it (vim `:w!`,
/// BBEdit's safe save). The copy's folder stays writable, so this gets past `0o444`.
fn save_over(copy: &Path, content: &[u8]) {
    let staged = copy.with_extension("saving");
    std::fs::write(&staged, content).expect("stage the save");
    std::fs::rename(&staged, copy).expect("land the save");
}

/// One pull of `inner` from `zip` into `dir`: the copy the app got.
fn pull(zip: &Path, inner: &str, dir: &Path) -> PathBuf {
    let mut got = launch_paths_in(&[zip.join(inner)], dir, CAP).expect("pull");
    got.remove(0)
}

/// The data-loss case: an app saved over its copy (and dropped a second file beside it),
/// and the next launch removed both with the rest. Both go to the Trash, under their
/// own names, while an untouched copy is still removed.
#[test]
fn the_startup_reaper_moves_a_changed_copy_and_its_siblings_to_the_trash() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("notes.txt", b"as pulled"), ("other.txt", b"never touched")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let edited = pull(&zip, "notes.txt", dir.path());
    let untouched = pull(&zip, "other.txt", dir.path());
    save_over(&edited, b"my edits");
    std::fs::write(edited.with_file_name("notes copy.txt"), b"saved as a copy").expect("save a copy beside it");
    let trash = tempfile::tempdir().expect("trash");

    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &into(trash.path()));

    assert_eq!(
        std::fs::read(trash.path().join("notes.txt")).expect("kept"),
        b"my edits"
    );
    assert_eq!(
        std::fs::read(trash.path().join("notes copy.txt")).expect("kept"),
        b"saved as a copy"
    );
    assert!(!trash.path().join("other.txt").exists(), "an untouched copy isn't kept");
    assert!(!untouched.exists());
    assert!(open_with_temps(dir.path()).is_empty(), "every launch folder is gone");
}

/// A file dropped beside an untouched copy ("Save a copy" into the same folder) is
/// content that exists nowhere else, so the folder counts as changed.
#[test]
fn a_new_file_beside_an_untouched_copy_is_kept_too() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("report.pdf", b"%PDF-1.4")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let copy = pull(&zip, "report.pdf", dir.path());
    std::fs::write(copy.with_file_name("report-signed.pdf"), b"%PDF-1.4 signed").expect("save beside");
    let trash = tempfile::tempdir().expect("trash");

    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &into(trash.path()));

    assert!(trash.path().join("report-signed.pdf").exists());
    assert!(
        trash.path().join("report.pdf").exists(),
        "its folder's copy goes with it"
    );
}

/// A Trash that can't take a kept file (a volume with none, a refusal) leaves it where
/// it is for the next launch to try again: never a delete in its place.
#[test]
fn a_changed_copy_the_trash_refuses_stays_where_it_is() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("notes.txt", b"as pulled")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let copy = pull(&zip, "notes.txt", dir.path());
    save_over(&copy, b"my edits");

    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &no_trash);

    assert_eq!(std::fs::read(&copy).expect("still there"), b"my edits");
}

/// The startup reaper runs in the background, so a launch can pull (and an app can
/// save into) a copy while it works. It only takes what an earlier run left: a folder
/// made or touched after this process started is this session's, and stays.
#[test]
fn the_startup_reaper_leaves_a_copy_pulled_after_this_process_started_alone() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("old.txt", b"as pulled"), ("new.txt", b"as pulled")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let leftover = pull(&zip, "old.txt", dir.path());
    save_over(&leftover, b"last run's edits");
    let started = std::time::SystemTime::now();
    let this_session = pull(&zip, "new.txt", dir.path());
    save_over(&this_session, b"edits in progress");
    let trash = tempfile::tempdir().expect("trash");

    reap_open_with_temps_with(dir.path(), started, &into(trash.path()));

    assert!(
        trash.path().join("old.txt").exists(),
        "last run's changed copy is kept in the Trash"
    );
    assert_eq!(std::fs::read(&this_session).expect("left alone"), b"edits in progress");
    assert!(!trash.path().join("new.txt").exists());
}

/// An atomic save that keeps the size and puts the old mtime back (`rsync -t`, a tool
/// that preserves times) still lands a new file, so the copy counts as changed.
#[test]
fn a_same_size_save_that_keeps_the_mtime_still_counts_as_changed() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("notes.txt", b"as pulled")]);
    let dir = tempfile::tempdir().expect("open-with dir");
    let copy = pull(&zip, "notes.txt", dir.path());
    let pulled_mtime = std::fs::metadata(&copy).expect("meta").modified().expect("mtime");
    save_over(&copy, b"my edits!");
    std::fs::File::options()
        .write(true)
        .open(&copy)
        .and_then(|file| file.set_modified(pulled_mtime))
        .expect("put the mtime back");
    let trash = tempfile::tempdir().expect("trash");

    reap_open_with_temps_with(dir.path(), std::time::SystemTime::now(), &into(trash.path()));

    assert_eq!(
        std::fs::read(trash.path().join("notes.txt")).expect("kept"),
        b"my edits!"
    );
}

/// No app ever opens the first row's copy when a later row's pull fails, so it goes
/// right away instead of waiting out the session.
#[test]
fn a_launch_whose_later_row_fails_leaves_no_copy_of_the_earlier_rows() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("small.txt", b"fits"), ("big.bin", &[7u8; 4096])]);
    let dir = tempfile::tempdir().expect("open-with dir");

    let refused =
        launch_paths_in(&[zip.join("small.txt"), zip.join("big.bin")], dir.path(), 100).expect_err("over the cap");

    assert_eq!(refused.path, zip.join("big.bin"));
    assert!(open_with_temps(dir.path()).is_empty(), "the first row's copy went too");
}

const HOUR: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// `count` untouched copies in `dir`, oldest first.
fn pulls(count: usize, dir: &Path) -> (tempfile::TempDir, Vec<PathBuf>) {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    let names: Vec<String> = (0..count).map(|i| format!("f{i}.txt")).collect();
    let entries: Vec<(&str, &[u8])> = names
        .iter()
        .map(|name| (name.as_str(), b"0123456789".as_slice()))
        .collect();
    build_zip(&zip, &entries);
    let copies = names.iter().map(|name| pull(&zip, name, dir)).collect();
    (src, copies)
}

/// Copies pile up all session otherwise, up to 2 GiB each. Old ones go, but never the
/// newest few (an app may still be showing one), and never one an app changed.
#[test]
fn the_session_sweep_removes_old_untouched_copies_but_keeps_the_newest_and_the_changed() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let (_src, copies) = pulls(6, dir.path());
    save_over(&copies[0], b"my edits");
    let policy = SweepPolicy {
        keep_newest: 4,
        max_age: 12 * HOUR,
        budget_bytes: u64::MAX,
    };

    sweep_open_with_copies_at(dir.path(), std::time::SystemTime::now() + 13 * HOUR, &policy);

    assert_eq!(std::fs::read(&copies[0]).expect("changed: kept"), b"my edits");
    assert!(!copies[1].exists(), "the oldest untouched copy went");
    for copy in &copies[2..] {
        assert!(copy.exists(), "the newest four stay: {}", copy.display());
    }
}

#[test]
fn the_session_sweep_leaves_young_copies_alone() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let (_src, copies) = pulls(6, dir.path());
    let policy = SweepPolicy {
        keep_newest: 4,
        max_age: 12 * HOUR,
        budget_bytes: u64::MAX,
    };

    sweep_open_with_copies_at(dir.path(), std::time::SystemTime::now(), &policy);

    assert!(copies.iter().all(|copy| copy.exists()));
}

/// Over the size budget, the oldest go first, whatever their age.
#[test]
fn the_session_sweep_evicts_the_oldest_past_the_size_budget() {
    let dir = tempfile::tempdir().expect("open-with dir");
    let (_src, copies) = pulls(4, dir.path());
    let policy = SweepPolicy {
        keep_newest: 1,
        max_age: 12 * HOUR,
        budget_bytes: 25,
    };

    sweep_open_with_copies_at(dir.path(), std::time::SystemTime::now(), &policy);

    assert!(!copies[0].exists() && !copies[1].exists(), "10 + 10 + 10 is over 25");
    assert!(copies[2].exists() && copies[3].exists());
}

#[test]
fn a_pull_with_no_room_for_it_is_refused() {
    use super::materialize::room_refusal;

    assert!(matches!(
        room_refusal(500, Some(499)),
        Some(ViewerError::NotEnoughSpace {
            needed: 500,
            available: 499
        })
    ));
    assert!(room_refusal(500, Some(500)).is_none());
    assert!(
        room_refusal(500, None).is_none(),
        "no answer goes ahead: a full disk then stops the write"
    );
}
