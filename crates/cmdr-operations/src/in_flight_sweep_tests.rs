//! The sweep's rules, one cell per arm.
//!
//! These drive [`super::settle_kind`] directly against a [`Surface::Local`]
//! pointed at a `TestDir`, so each rule is pinned at the level it's decided
//! rather than through a launch. The ledger's own behavior (what replays, what
//! defers, what a producer records) lives in `in_flight_temps_tests.rs`.

use super::*;
use crate::host::testing::{FakeVolumes, RecordingEngineEvents};
use crate::transfer_sides::test_hook;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::Volume;
use cmdr_local::LocalPosixVolume;

/// The host a cell's sweep runs on: a registry holding only the drives the
/// cell registers, and the engine-wide facts the sweep reports, read back.
struct SweepHost {
    volumes: Arc<FakeVolumes>,
    events: Arc<RecordingEngineEvents>,
    host: EngineHost,
}

fn sweep_host() -> SweepHost {
    let volumes = Arc::new(FakeVolumes::default());
    let events = Arc::new(RecordingEngineEvents::default());
    let host = EngineHost::new(crate::host::testing::pane_order_host(), volumes.clone(), events.clone());
    SweepHost { volumes, events, host }
}

impl SweepHost {
    /// Every staging-folder notice the sweep raised, with the folder it named.
    fn kept_notices(&self) -> Vec<LeftoversKeptEvent> {
        self.events.leftovers_kept.lock_ignore_poison().clone()
    }
}

/// The aside a safe-overwrite would have left, with the destination it came
/// from, so each cell states its fixture in the same two lines.
struct Displaced {
    _dir: TestDir,
    aside: PathBuf,
    destination: PathBuf,
}

/// A destination file, renamed aside the way `stage_and_land_file` renames it.
fn displaced(name: &str, original: &[u8]) -> Displaced {
    let dir = TestDir::new(name);
    let destination = dir.join("notes.txt");
    let aside = dir.join(format!("notes.txt.cmdr-temp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&aside, original).expect("write the aside");
    Displaced {
        _dir: dir,
        aside,
        destination,
    }
}

async fn settle_aside(kind: ItemKind, fixture: &Displaced) -> Outcome {
    settle_kind(
        &sweep_host().host,
        &Surface::Local,
        &kind,
        &fixture.aside,
        kind.destination().map(|_| fixture.destination.as_path()),
        None,
    )
    .await
}

/// The crash this whole milestone exists for: the original was renamed aside and
/// the replacement never landed, so the aside holds the only copy. It goes back
/// to its own name, and ❌ is never removed.
#[tokio::test]
async fn an_aside_whose_replacement_never_landed_goes_back_to_its_own_name() {
    let fixture = displaced("sweep-aside-restore", b"the user's notes");

    let outcome = settle_aside(
        ItemKind::FileAside {
            destination: fixture.destination.clone(),
            expected_size: 4096,
        },
        &fixture,
    )
    .await;

    assert_eq!(outcome, Outcome::Restored);
    assert_eq!(
        std::fs::read(&fixture.destination).expect("the original is back at its own name"),
        b"the user's notes"
    );
    assert!(!fixture.aside.exists(), "and nothing is left wearing a scratch name");
}

/// The one case an aside may be removed: the replacement is a whole file of
/// exactly the size it was supposed to reach, so the aside is provably
/// redundant.
#[tokio::test]
async fn an_aside_goes_only_when_its_replacement_is_the_exact_size_recorded() {
    let fixture = displaced("sweep-aside-exact", b"the user's notes");
    std::fs::write(&fixture.destination, b"the replacement!").expect("land the replacement");
    let expected_size = std::fs::metadata(&fixture.destination).expect("stat").len();

    let outcome = settle_aside(
        ItemKind::FileAside {
            destination: fixture.destination.clone(),
            expected_size,
        },
        &fixture,
    )
    .await;

    assert_eq!(outcome, Outcome::Swept);
    assert!(!fixture.aside.exists());
    assert_eq!(std::fs::read(&fixture.destination).expect("read"), b"the replacement!");
}

/// A replacement that stopped part-way is exactly what a pulled drive leaves,
/// and it's the case where removing the aside would destroy the only whole copy.
/// The bytes keep a name a person can find instead.
#[tokio::test]
async fn an_aside_whose_replacement_is_short_is_kept_under_a_recovered_name() {
    let fixture = displaced("sweep-aside-short", b"the user's notes");
    std::fs::write(&fixture.destination, b"half").expect("land a short replacement");

    let outcome = settle_aside(
        ItemKind::FileAside {
            destination: fixture.destination.clone(),
            expected_size: 4096,
        },
        &fixture,
    )
    .await;

    assert_eq!(outcome, Outcome::Recovered);
    assert!(!fixture.aside.exists(), "the bytes moved to a real name");
    assert_eq!(
        std::fs::read(
            fixture
                .destination
                .parent()
                .expect("parent")
                .join("notes (recovered).txt")
        )
        .expect("the original is kept beside the short replacement"),
        b"the user's notes"
    );
    assert_eq!(
        std::fs::read(&fixture.destination).expect("read"),
        b"half",
        "and what landed is left exactly as it is"
    );
}

/// A file→folder overwrite sets a whole DIRECTORY aside. Even with the
/// replacement provably complete, removing it would mean recursing over
/// something that was the user's, so it gets a real name and they decide.
#[tokio::test]
async fn a_folder_set_aside_for_a_file_is_never_removed_recursively() {
    let dir = TestDir::new("sweep-aside-folder");
    let destination = dir.join("notes");
    let aside = dir.join(format!("notes.cmdr-temp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&aside).expect("the folder was set aside");
    std::fs::write(aside.join("kept.txt"), b"a file inside the folder").expect("fill it");
    std::fs::write(&destination, b"the replacement!").expect("land the replacement");
    let expected_size = std::fs::metadata(&destination).expect("stat").len();

    let outcome = settle_kind(
        &sweep_host().host,
        &Surface::Local,
        &ItemKind::FileAside {
            destination: destination.clone(),
            expected_size,
        },
        &aside,
        Some(&destination),
        None,
    )
    .await;

    assert_eq!(outcome, Outcome::Recovered);
    assert_eq!(
        std::fs::read(dir.join("notes (recovered)").join("kept.txt")).expect("the folder is kept whole"),
        b"a file inside the folder"
    );
}

/// A folder built leaf by leaf over a file's name can't be checked against a
/// size, so the displaced file is ALWAYS kept: it's the only copy, and the
/// folder is what the user asked for.
#[tokio::test]
async fn a_file_displaced_by_a_folder_is_always_kept() {
    let fixture = displaced("sweep-displaced-file", b"the user's notes");
    std::fs::create_dir(&fixture.destination).expect("the folder took its name");

    let outcome = settle_aside(
        ItemKind::DisplacedFile {
            destination: fixture.destination.clone(),
        },
        &fixture,
    )
    .await;

    assert_eq!(outcome, Outcome::Recovered);
    assert_eq!(
        std::fs::read(
            fixture
                .destination
                .parent()
                .expect("parent")
                .join("notes (recovered).txt")
        )
        .expect("the displaced file is kept beside the folder"),
        b"the user's notes"
    );
    assert!(fixture.destination.is_dir(), "and the folder stays");
}

/// The dir-overwrite aside's replacement is a whole subtree, so the same rule
/// applies: put it back if the name is free, keep it beside whatever took it.
#[tokio::test]
async fn a_dir_overwrite_aside_goes_back_when_its_replacement_never_landed() {
    let fixture = displaced("sweep-dir-overwrite", b"the user's notes");

    let outcome = settle_aside(
        ItemKind::DirOverwriteAside {
            destination: fixture.destination.clone(),
        },
        &fixture,
    )
    .await;

    assert_eq!(outcome, Outcome::Restored);
    assert_eq!(std::fs::read(&fixture.destination).expect("read"), b"the user's notes");
}

/// A record can name an aside that never happened: a rename aside the
/// transport cut off is recorded because it MAY have gone through. Every aside
/// kind reads the aside first, so one that isn't there retires, whatever is at
/// its destination, ❌ instead of a restore that refuses and defers forever.
#[tokio::test]
async fn an_aside_that_isnt_there_retires_whatever_its_destination_holds() {
    for (label, destination_holds) in [("missing", None), ("the original", Some(b"the user's notes" as &[u8]))] {
        let kinds = |destination: PathBuf| {
            [
                ItemKind::FileAside {
                    destination: destination.clone(),
                    expected_size: 16,
                },
                ItemKind::DisplacedFile {
                    destination: destination.clone(),
                },
                ItemKind::DirOverwriteAside {
                    destination: destination.clone(),
                },
                ItemKind::VolumeAside { destination },
            ]
        };
        let fixture = displaced("sweep-aside-never-happened", b"");
        std::fs::remove_file(&fixture.aside).expect("the rename aside never happened");
        if let Some(bytes) = destination_holds {
            std::fs::write(&fixture.destination, bytes).expect("the original, where it always was");
        }
        for kind in kinds(fixture.destination.clone()) {
            let outcome = settle_aside(kind.clone(), &fixture).await;
            assert_eq!(outcome, Outcome::AlreadyGone, "{label}: {kind:?}");
        }
        if let Some(bytes) = destination_holds {
            assert_eq!(
                std::fs::read(&fixture.destination).expect("read"),
                bytes,
                "{label}: untouched"
            );
        }
    }
}

/// ❗ The rule a mistake here loses somebody's only copy: a staging folder with
/// anything in it is left exactly where it is, whatever else the sweep would
/// like to do. A destination that left before Phase 3 leaves a whole staged tree
/// in one of these, and the originals can already be gone.
#[tokio::test]
async fn a_staging_folder_with_files_in_it_is_never_removed() {
    let dir = TestDir::new("sweep-staging-nonempty");
    let staging = dir.join(format!(".cmdr-staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).expect("create the staging folder");
    std::fs::write(staging.join("footage.mov"), b"a whole staged file").expect("stage a file");

    let outcome = settle_kind(
        &sweep_host().host,
        &Surface::Local,
        &ItemKind::StagingDir,
        &staging,
        None,
        None,
    )
    .await;

    assert_eq!(outcome, Outcome::LeftAlone);
    assert_eq!(
        std::fs::read(staging.join("footage.mov")).expect("the staged file is still there"),
        b"a whole staged file"
    );
}

/// The ordinary ending: Phase 3 renamed the tree out, so the shell is empty and
/// goes.
#[tokio::test]
async fn an_empty_staging_folder_is_removed() {
    let dir = TestDir::new("sweep-staging-empty");
    let staging = dir.join(format!(".cmdr-staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).expect("create the staging folder");

    let outcome = settle_kind(
        &sweep_host().host,
        &Surface::Local,
        &ItemKind::StagingDir,
        &staging,
        None,
        None,
    )
    .await;

    assert_eq!(outcome, Outcome::Swept);
    assert!(!staging.exists());
}

/// A drive that stood in for a pulled USB stick: a folder holding what a move
/// staged there, a registered volume rooted at it, and a mount table that says
/// it's back.
struct ReturnedDrive {
    _dir: TestDir,
    /// The host the drive is registered on, and that hears the sweep.
    sweep: SweepHost,
    _hook: test_hook::MountTableHook,
    volume_id: String,
    staging_name: String,
    staging: PathBuf,
}

fn returned_drive(name: &str, volume_id: &str, staged: Option<&[u8]>) -> ReturnedDrive {
    let dir = TestDir::new(name);
    let root = dir.join("Volumes").join("Testkamera");
    std::fs::create_dir_all(&root).expect("create the drive root");
    let staging_name = format!(".cmdr-staging-{}", uuid::Uuid::new_v4());
    let staging = root.join(&staging_name);
    std::fs::create_dir(&staging).expect("create the staging folder");
    if let Some(bytes) = staged {
        std::fs::write(staging.join("footage.mov"), bytes).expect("stage a file");
    }
    let volume = Arc::new(LocalPosixVolume::local_folder(
        "Tëstkamera",
        &root,
        crate::host::testing::pane_order_host(),
    )) as Arc<dyn Volume>;
    let sweep = sweep_host();
    sweep.volumes.register(volume_id, volume);
    ReturnedDrive {
        sweep,
        _hook: test_hook::answer_each(vec![(root, Some(true))]),
        volume_id: volume_id.to_string(),
        staging_name,
        staging,
        _dir: dir,
    }
}

fn staging_record(drive: &ReturnedDrive) -> Record {
    Record::Tracked(TrackedItem {
        kind: ItemKind::StagingDir,
        home: ItemHome::Mount {
            volume_id: drive.volume_id.clone(),
        },
        // Relative to the drive's root, the way `RecordHome` stores it.
        path: PathBuf::from(&drive.staging_name),
    })
}

/// The case M10 created and this milestone has to meet: a destination that left
/// the mount table returns BEFORE Phase 3, so its staging folder holds a whole
/// staged tree — and the user's originals may already be gone. The drive comes
/// back, and every byte in there stays exactly where it is.
#[tokio::test]
async fn a_staging_folder_on_a_drive_that_comes_back_keeps_what_is_inside_it() {
    let drive = returned_drive(
        "sweep-arrival-staging-full",
        "in-flight-sweep-test-stick-full",
        Some(b"the whole staged file"),
    );

    let outcome = settle(&drive.sweep.host, &staging_record(&drive)).await;

    assert_eq!(outcome, Outcome::LeftAlone);
    assert_eq!(
        std::fs::read(drive.staging.join("footage.mov")).expect("the staged file is still there"),
        b"the whole staged file"
    );
}

/// The same drive, with the shell Phase 3 emptied: that one goes, so a finished
/// move doesn't leave a stray folder in the destination for good.
#[tokio::test]
async fn an_empty_staging_folder_on_a_drive_that_comes_back_is_removed() {
    let drive = returned_drive("sweep-arrival-staging-empty", "in-flight-sweep-test-stick-empty", None);

    let outcome = settle(&drive.sweep.host, &staging_record(&drive)).await;

    assert_eq!(outcome, Outcome::Swept);
    assert!(!drive.staging.exists());
    assert!(
        drive.sweep.kept_notices().is_empty(),
        "nothing was kept, so nothing to say"
    );
}

/// A kept, non-empty staging folder on a drive raises the `leftovers-kept`
/// notice, naming the drive and the folder, so the files inside have a findable
/// home.
#[tokio::test]
async fn a_staging_folder_kept_on_a_drive_raises_the_notice() {
    let drive = returned_drive(
        "sweep-notice-drive",
        "in-flight-sweep-test-stick-notice",
        Some(b"the whole staged file"),
    );

    assert_eq!(
        settle(&drive.sweep.host, &staging_record(&drive)).await,
        Outcome::LeftAlone
    );

    let notices = drive.sweep.kept_notices();
    assert_eq!(notices.len(), 1, "one notice for the one kept folder");
    assert_eq!(notices[0].volume_name, "Tëstkamera");
    assert_eq!(
        notices[0].kept,
        KeptLeftover::StagingFolder {
            folder_name: drive.staging_name.clone()
        }
    );
}

/// A Mac-internal staging folder (a move onto the boot volume) is kept just the
/// same, but only `warn`s: its disk never leaves, so the next launch meets it
/// again.
#[tokio::test]
async fn a_staging_folder_kept_on_the_mac_only_warns() {
    let dir = TestDir::new("sweep-notice-local");
    let staging = dir.join(format!(".cmdr-staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).expect("create the staging folder");
    std::fs::write(staging.join("footage.mov"), b"a whole staged file").expect("stage a file");
    let record = Record::Tracked(TrackedItem {
        kind: ItemKind::StagingDir,
        home: ItemHome::Local,
        path: staging.clone(),
    });

    let sweep = sweep_host();
    assert_eq!(settle(&sweep.host, &record).await, Outcome::LeftAlone);
    assert!(staging.join("footage.mov").exists(), "the staged file stays");
    assert!(sweep.kept_notices().is_empty(), "no notice for a Mac-internal folder");
}

/// A record whose drive isn't registered decides nothing at all. ❗ It must not
/// resolve against a mount point that isn't there and read as already gone,
/// which is how the ledger used to forget a leftover still sitting on the drive.
#[tokio::test]
async fn a_record_whose_drive_isnt_here_decides_nothing() {
    let record = Record::Tracked(TrackedItem {
        kind: ItemKind::StagingDir,
        home: ItemHome::Mount {
            volume_id: "in-flight-sweep-test-stick-absent".to_string(),
        },
        path: PathBuf::from(".cmdr-staging-2a6f1f1e-0000-7000-8000-000000000000"),
    });

    assert_eq!(settle(&sweep_host().host, &record).await, Outcome::Deferred);
}

/// The name gate, which is the whole defense against a corrupted or hand-edited
/// ledger: a record can ask for a removal only for something wearing the name
/// that kind really wears.
#[test]
fn a_record_whose_name_isnt_its_kinds_shape_is_refused() {
    let real_id = uuid::Uuid::new_v4();
    let aside = ItemKind::FileAside {
        destination: PathBuf::from("/dir/notes.txt"),
        expected_size: 4,
    };

    assert!(!name_matches_kind(&ItemKind::Temp, Path::new("/dir/taxes.pdf")));
    assert!(
        !name_matches_kind(&ItemKind::Temp, Path::new("/dir/notes.txt.cmdr-temp-x")),
        "a temp record must no longer reach an ASIDE: that's the user's own file"
    );
    assert!(name_matches_kind(
        &ItemKind::Temp,
        Path::new("/dir/notes.txt.cmdr-tmp-x")
    ));
    assert!(name_matches_kind(&aside, Path::new("/dir/notes.txt.cmdr-temp-x")));
    assert!(!name_matches_kind(&aside, Path::new("/dir/notes.txt")));
    assert!(!name_matches_kind(
        &ItemKind::StagingDir,
        Path::new("/dir/.cmdr-staging-notes")
    ));
    assert!(name_matches_kind(
        &ItemKind::StagingDir,
        &PathBuf::from("/dir").join(format!(".cmdr-staging-{real_id}"))
    ));
}

/// A legacy `+` line means a plain removal, and a plain removal must never reach
/// an aside. The old shared name test accepted both markers, which made this the
/// one path that could delete a user's original on sight.
#[test]
fn a_legacy_record_naming_an_aside_is_not_a_plain_removal() {
    assert!(is_a_staged_partial(Path::new("/dir/notes.txt.cmdr-tmp-4242")));
    assert!(!is_a_staged_partial(Path::new("/dir/notes.txt.cmdr-temp-4242")));
    assert!(!is_a_staged_partial(Path::new("/dir/taxes.pdf")));
}

/// A temp the sweep has matched to its record is Cmdr's own, so a Locked one
/// (an APFS clone copies the source's `uchg` onto it in the same call, and a
/// crash before the landing clears it leaves it so) or a `deny delete` one is
/// cleared and removed rather than left forever.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_locked_or_deny_delete_temp_is_cleared_and_swept() {
    use cmdr_fs::testing::metadata_fixture::{LOCKED_FLAG, MetadataFixture, add_deny_delete_acl, set_flags};

    let fixture = MetadataFixture::new("sweep-blocked-temp");
    let locked = fixture.join(format!("locked.txt.cmdr-tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&locked, b"partial").expect("write the temp");
    set_flags(&locked, LOCKED_FLAG);
    let acl = fixture.join(format!("acl.txt.cmdr-tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&acl, b"partial").expect("write the temp");
    add_deny_delete_acl(&acl);

    for temp in [&locked, &acl] {
        let outcome = settle_kind(&sweep_host().host, &Surface::Local, &ItemKind::Temp, temp, None, None).await;
        assert_eq!(outcome, Outcome::Swept, "{}", temp.display());
        assert!(!temp.exists());
    }
}

/// A staged write's complete temp on its way to `notes.txt`, recorded as a
/// landing the way `staged_write.rs::land` records one before clearing the way.
struct Landing {
    dir: TestDir,
    temp: PathBuf,
    destination: PathBuf,
    kind: ItemKind,
}

fn landing(name: &str, new_bytes: &[u8]) -> Landing {
    let dir = TestDir::new(name);
    let destination = dir.join("notes.txt");
    let temp = dir.join(format!("notes.txt.cmdr-tmp-{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp, new_bytes).expect("write the complete temp");
    let kind = ItemKind::Landing {
        destination: destination.clone(),
        expected_size: new_bytes.len() as u64,
    };
    Landing {
        dir,
        temp,
        destination,
        kind,
    }
}

async fn settle_landing(fixture: &Landing) -> Outcome {
    settle_kind(
        &sweep_host().host,
        &Surface::Local,
        &fixture.kind,
        &fixture.temp,
        Some(&fixture.destination),
        None,
    )
    .await
}

/// ❗ The crash this kind exists for: the way was cleared (the delete of what
/// held the name went through) and the landing rename never ran, so the temp is
/// the destination's only copy. The sweep FINISHES the landing, ❌ never
/// deletes it.
#[tokio::test]
async fn a_landing_whose_destination_is_missing_finishes_at_its_name() {
    let fixture = landing("sweep-landing-restore", b"the new notes");

    assert_eq!(settle_landing(&fixture).await, Outcome::Restored);
    assert_eq!(
        std::fs::read(&fixture.destination).expect("the new bytes have their name"),
        b"the new notes"
    );
    assert!(!fixture.temp.exists(), "and nothing is left wearing a scratch name");
}

/// A crash after the record but before the clearing delete leaves the
/// destination as it was and the temp as a spare complete copy. Neither may be
/// lost: the temp's bytes get a real name beside it.
#[tokio::test]
async fn a_landing_whose_destination_is_occupied_is_kept_under_a_recovered_name() {
    let fixture = landing("sweep-landing-occupied", b"the new notes");
    std::fs::write(&fixture.destination, b"what was there").expect("the destination still holds its file");

    assert_eq!(settle_landing(&fixture).await, Outcome::Recovered);
    assert_eq!(
        std::fs::read(&fixture.destination).expect("read"),
        b"what was there",
        "what holds the name is left exactly as it is"
    );
    assert_eq!(
        std::fs::read(fixture.destination.with_file_name("notes (recovered).txt"))
            .expect("the new bytes are kept beside it"),
        b"the new notes"
    );
    assert!(!fixture.temp.exists());
}

/// A temp that isn't the size the landing recorded isn't provably the bytes we
/// wrote, so the sweep doesn't land it, and ❌ doesn't delete it. It doesn't
/// stay on its scratch name either: unrecorded, the name+age reap would take
/// it, and recorded, it would be retried forever. It gets a ` (recovered)`
/// name beside where it belongs, and the record retires.
#[tokio::test]
async fn a_landing_whose_temp_changed_size_is_kept_under_a_recovered_name() {
    let fixture = landing("sweep-landing-resized", b"the new notes");
    std::fs::write(&fixture.temp, b"something else entirely").expect("the temp changed under us");

    assert_eq!(settle_landing(&fixture).await, Outcome::Recovered);
    assert_eq!(
        std::fs::read(fixture.destination.with_file_name("notes (recovered).txt"))
            .expect("the bytes are kept under a real name"),
        b"something else entirely"
    );
    assert!(!fixture.temp.exists(), "nothing is left wearing a scratch name");
    assert!(!fixture.destination.exists(), "and nothing was put at the name");
}

/// The same landing on a drive tells the person, by name, where the kept file
/// is: it's a file they didn't make, beside one they may be looking for.
#[tokio::test]
async fn a_landing_kept_aside_on_a_drive_raises_the_notice() {
    let drive = returned_drive("sweep-landing-notice", "in-flight-sweep-test-stick-landing", None);
    let root = drive.staging.parent().expect("the drive root").to_path_buf();
    let temp_name = format!("notes.txt.cmdr-tmp-{}", uuid::Uuid::new_v4());
    std::fs::write(root.join(&temp_name), b"a short temp").expect("write the temp");
    let record = Record::Tracked(TrackedItem {
        kind: ItemKind::Landing {
            destination: PathBuf::from("notes.txt"),
            expected_size: 4096,
        },
        home: ItemHome::Mount {
            volume_id: drive.volume_id.clone(),
        },
        path: PathBuf::from(&temp_name),
    });

    assert_eq!(settle(&drive.sweep.host, &record).await, Outcome::Recovered);

    let notices = drive.sweep.kept_notices();
    assert_eq!(notices.len(), 1, "one notice for the one kept landing");
    assert_eq!(notices[0].volume_name, "Tëstkamera");
    assert_eq!(
        notices[0].kept,
        KeptLeftover::UnfinishedLanding {
            file_name: "notes.txt".to_string(),
            kept_name: "notes (recovered).txt".to_string(),
        }
    );
    assert!(root.join("notes (recovered).txt").exists());
}

/// The temp is gone: a crash after the landing rename went through but before
/// its record was retired. The record retires, rather than being retried at
/// every launch and every reconnect.
#[tokio::test]
async fn a_landing_whose_temp_is_gone_retires() {
    let fixture = landing("sweep-landing-gone", b"the new notes");
    std::fs::rename(&fixture.temp, &fixture.destination).expect("the landing went through");

    assert_eq!(settle_landing(&fixture).await, Outcome::AlreadyGone);
    assert_eq!(std::fs::read(&fixture.destination).expect("read"), b"the new notes");
}

/// ❗ The crash between the kind change's two lines: the `Landing` record is on
/// disk and the `Temp` one for the same path isn't retired yet. The path
/// settles as a landing, ❌ never as a temp (which would delete the only copy).
#[test]
fn a_path_recorded_as_both_temp_and_landing_settles_as_the_landing() {
    let fixture = landing("sweep-landing-both", b"the new notes");
    let data_dir = fixture.dir.join("data");
    std::fs::create_dir_all(&data_dir).expect("create the data dir");
    let previous_run = Ledger::recording_in(&data_dir);
    let state = Arc::new(
        crate::testing::TestOperations::new()
            .new_operation_state(std::time::Duration::from_millis(50))
            .with_in_flight_ledger(previous_run.clone()),
    );
    super::super::track(&state, ItemKind::Temp, &fixture.temp);
    super::super::track(&state, fixture.kind.clone(), &fixture.temp);
    previous_run.die();

    let next_run = Ledger::for_test();
    let next_app = crate::testing::TestOperations::new();
    let tally = next_run.launch_in(&data_dir, &next_app).wait();

    assert_eq!(
        std::fs::read(&fixture.destination).expect("the landing finished"),
        b"the new notes"
    );
    assert!(!fixture.temp.exists());
    assert_eq!(
        tally,
        SweepTally {
            restored: 1,
            ..SweepTally::default()
        },
        "one path, settled once, by its landing: {tally:?}"
    );
    assert!(next_run.live_paths().is_empty(), "and neither record is left to retry");
}
