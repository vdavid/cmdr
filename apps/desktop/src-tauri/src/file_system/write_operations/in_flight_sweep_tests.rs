//! The sweep's rules, one cell per arm.
//!
//! These drive [`super::settle_kind`] directly against a [`Surface::Local`]
//! pointed at a `TestDir`, so each rule is pinned at the level it's decided
//! rather than through a launch. The ledger's own behavior (what replays, what
//! defers, what a producer records) lives in `in_flight_temps_tests.rs`.

use super::*;
use crate::file_system::volume::backends::LocalPosixVolume;
use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
use crate::file_system::write_operations::transfer_sides::test_hook;
use crate::test_support::TestDir;

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

    let outcome = settle_kind(&Surface::Local, &ItemKind::StagingDir, &staging, None, None).await;

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

    let outcome = settle_kind(&Surface::Local, &ItemKind::StagingDir, &staging, None, None).await;

    assert_eq!(outcome, Outcome::Swept);
    assert!(!staging.exists());
}

/// A drive that stood in for a pulled USB stick: a folder holding what a move
/// staged there, a registered volume rooted at it, and a mount table that says
/// it's back.
struct ReturnedDrive {
    _dir: TestDir,
    _registration: TestVolumeRegistration,
    _hook: test_hook::MountTableHook,
    volume_id: String,
    staging_name: String,
    staging: PathBuf,
}

fn returned_drive(name: &str, volume_id: &str, staged: Option<&[u8]>) -> ReturnedDrive {
    let dir = TestDir::new(name);
    let root = dir.join("Volumes").join("Faltkamera");
    std::fs::create_dir_all(&root).expect("create the drive root");
    let staging_name = format!(".cmdr-staging-{}", uuid::Uuid::new_v4());
    let staging = root.join(&staging_name);
    std::fs::create_dir(&staging).expect("create the staging folder");
    if let Some(bytes) = staged {
        std::fs::write(staging.join("footage.mov"), bytes).expect("stage a file");
    }
    let volume = Arc::new(LocalPosixVolume::local_folder("Fältkamera", &root)) as Arc<dyn Volume>;
    ReturnedDrive {
        _registration: TestVolumeRegistration::install(volume_id, volume),
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

    let outcome = settle(&staging_record(&drive)).await;

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

    let outcome = settle(&staging_record(&drive)).await;

    assert_eq!(outcome, Outcome::Swept);
    assert!(!drive.staging.exists());
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

    assert_eq!(settle(&record).await, Outcome::Deferred);
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
