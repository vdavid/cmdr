//! What the unmount approver does against real DiskArbitration and real synthetic disk images.
//!
//! `#[ignore]`d: each attaches an APFS or HFS+ image. Hand-run with
//! `cargo nextest run -p cmdr --run-ignored only -E 'test(volumes::platform::imp::unmount_approver::real_image::)'`,
//! or through `pnpm check disk-images`. Serialized in the `disk-image` nextest group, and
//! machine-wide by the harness's session lock.
//!
//! ❗ While it's installed, the test's session is asked about EVERY unmount on this Mac, so its host
//! acts only on the image's own BSD nodes and approves everything else at once. The `Approval` is
//! declared after the image, so it unschedules the session before the image detaches.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, FileHolder, ImageSpec};

use super::test_seams::{Fixture, real_fixture};
use super::{Approval, Host, install};
use crate::file_system::volume::drive_release::{Gated, SkipReason, StartKind};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until;

/// How long a test waits for DiskArbitration to deliver a callback, or for a resume to settle.
/// ❗ Never a deadline under test: the chain's own 7 s budget is what the overrun test spends.
const PATIENCE: Duration = Duration::from_secs(20);

/// An approver answering for `image`'s volumes, each registered under its own volume id and
/// indexing.
struct Lane {
    fixture: Fixture,
    _approval: Approval,
}

fn lane(image: &DiskImage) -> Lane {
    let fixture = real_fixture();
    for (index, volume) in image.volumes().iter().enumerate() {
        fixture.indexed_volume(&volume_id(index), &volume.node, &volume.mount_point);
    }
    let approval =
        install(fixture.gate.clone(), Arc::clone(&fixture.host) as Arc<dyn Host>).expect("install an approval session");
    Lane {
        fixture,
        _approval: approval,
    }
}

/// A volume id no other run on this machine uses.
fn volume_id(index: usize) -> String {
    format!("vol-cmdr-approver-{}-{index}", std::process::id())
}

fn is_mounted(path: &Path) -> bool {
    cmdr_local::mount_table::is_mount_point(&path.to_string_lossy()) == Some(true)
}

/// Waits for the idle after a settled request and lets its resume batches finish, so "nothing
/// started again" is a fact rather than a race.
fn let_the_resumes_settle(lane: &Lane) {
    wait_until(PATIENCE, "DiskArbitration to go idle", || lane.fixture.host.idles() > 0);
    for batch in lane.fixture.host.take_resumes() {
        batch.wait();
    }
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn an_idle_volume_is_asked_about_before_it_unmounts_and_never_starts_again() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();

    image.unmount_volume(0).expect("diskutil unmount");

    assert_eq!(
        lane.fixture.host.stops.asked(),
        [volume_id(0)],
        "DiskArbitration asked before it unmounted"
    );
    assert!(
        lane.fixture.host.stops.all_ran_while_mounted(),
        "the index let go while the drive was still mounted, which is what a pre-unmount hook is for"
    );
    assert!(!is_mounted(&mount_point));
    let_the_resumes_settle(&lane);
    assert!(
        lane.fixture.index.started().is_empty(),
        "a drive that really unmounted is never started again"
    );
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_refused_unmount_hands_the_index_back_once_diskarbitration_goes_quiet() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();
    let held = mount_point.join("held.txt");
    std::fs::write(&held, b"held").expect("write the held file");
    let holder = FileHolder::hold(&held).expect("hold the file open");

    let refusal = image.unmount_volume(0);

    assert!(refusal.is_err(), "a held volume can't unmount, got {refusal:?}");
    assert!(is_mounted(&mount_point), "a refused unmount leaves the drive mounted");
    assert_eq!(lane.fixture.host.stops.asked(), [volume_id(0)]);
    wait_until(PATIENCE, "the index the ask stopped to be handed back", || {
        lane.fixture.index.started() == [volume_id(0)]
    });
    drop(holder);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_raw_umount_nobody_asked_about_stops_the_index_and_never_hands_it_back() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();

    // `/sbin/umount` goes straight to the kernel, so DiskArbitration asks nobody. The approver
    // hears about it only from the description change that follows.
    image.raw_umount(0).expect("/sbin/umount");

    wait_until(PATIENCE, "the volume to leave the mount table", || {
        !is_mounted(&mount_point)
    });
    assert!(
        lane.fixture.host.stops.asked().is_empty(),
        "nothing was asked, so no pre-unmount stop could have run"
    );
    wait_until(PATIENCE, "the vanished drive's index to be stopped", || {
        lane.fixture.host.vanish_stops() == [volume_id(0)]
    });
    let_the_resumes_settle(&lane);
    assert!(
        lane.fixture.index.started().is_empty(),
        "a drive that vanished is owed nothing back"
    );
}

#[test]
#[ignore = "attaches a real two-partition HFS+ disk image via hdiutil; run with --run-ignored"]
fn the_first_ask_of_a_disk_lets_go_of_every_partition_so_the_next_unmount_meets_no_live_index() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::HfsTwoPartitions).expect("attach the image");
    let lane = lane(&image);
    let (first, second) = (
        image.volumes()[0].mount_point.clone(),
        image.volumes()[1].mount_point.clone(),
    );

    image
        .unmount_volume(0)
        .expect("diskutil unmount of the first partition");
    let after_the_first_ask = lane.fixture.host.stops.asked();
    image
        .unmount_volume(1)
        .expect("diskutil unmount of the second partition");

    assert!(
        after_the_first_ask.contains(&volume_id(0)) && after_the_first_ask.contains(&volume_id(1)),
        "the first ask let go of the whole disk, not just its own volume: {after_the_first_ask:?}"
    );
    assert!(!is_mounted(&first) && !is_mounted(&second));
    let_the_resumes_settle(&lane);
    assert!(
        lane.fixture.index.started().is_empty(),
        "nothing started between the two requests"
    );
}

#[test]
#[ignore = "attaches a real two-partition HFS+ disk image via hdiutil; run with --run-ignored"]
fn an_unmount_disk_with_one_partition_held_resumes_only_the_partition_that_stayed() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::HfsTwoPartitions).expect("attach the image");
    let lane = lane(&image);
    let (held_partition, other) = (
        image.volumes()[0].mount_point.clone(),
        image.volumes()[1].mount_point.clone(),
    );
    let held = held_partition.join("held.txt");
    std::fs::write(&held, b"held").expect("write the held file");
    let holder = FileHolder::hold(&held).expect("hold the file open");

    let outcome = image.unmount_disk();

    assert!(
        outcome.is_err(),
        "a partial unmountDisk answers nonzero, got {outcome:?}"
    );
    assert!(is_mounted(&held_partition), "the held partition stayed");
    assert!(!is_mounted(&other), "its sibling unmounted");
    wait_until(PATIENCE, "the held partition's index to be handed back", || {
        lane.fixture.index.started() == [volume_id(0)]
    });
    drop(holder);
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_stop_that_overruns_the_budget_dissents_and_a_forced_detach_ignores_the_dissent() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();
    // The stop never answers inside the chain's budget.
    lane.fixture.host.stops.hold(&volume_id(0));

    let refusal = image.unmount_volume(0);

    assert!(
        refusal.is_err(),
        "an ask that can't let go in time dissents, got {refusal:?}"
    );
    assert!(
        is_mounted(&mount_point),
        "nothing unmounts under an index still letting go"
    );

    image.force_detach().expect("force-detach the image");

    wait_until(
        PATIENCE,
        "the forced detach to take the volume out of the mount table",
        || !is_mounted(&mount_point),
    );
    // Let the stops the asks detached finish, so nothing outlives the test blocked.
    lane.fixture.host.stops.open(&volume_id(0));
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_persons_enable_during_an_ask_never_lands_an_index_on_a_drive_thats_leaving() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();
    let id = volume_id(0);
    lane.fixture.host.stops.hold(&id);

    let enabled = std::thread::scope(|scope| {
        let unmount = scope.spawn(|| image.unmount_volume(0));
        wait_until(PATIENCE, "the ask to reach the drive's stop", || {
            lane.fixture.host.stops.asked() == [id.clone()]
        });
        let enable = scope.spawn(|| {
            lane.fixture
                .gate
                .start_blocking(&id, StartKind::UserEnable, || "the index started")
        });
        wait_until(PATIENCE, "the person's enable to wait the unmount out", || {
            lane.fixture.gate.parked() >= 2
        });

        lane.fixture.host.stops.open(&id);
        unmount.join().expect("the unmount thread").expect("diskutil unmount");
        enable.join().expect("the enable thread")
    });

    assert_eq!(
        enabled,
        Gated::Skipped(SkipReason::DriveLeaving),
        "the drive left the mount table while the person's enable waited it out"
    );
    assert!(!is_mounted(&mount_point));
    assert!(lane.fixture.index.started().is_empty());
}

#[test]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
fn a_repo_open_on_the_drive_lets_go_so_a_plain_unmount_goes_through() {
    use cmdr_git::test_fixtures::{Fixture as RepoFixture, git_cli};
    use cmdr_git::{GitPortal, no_git_state_sink};

    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let lane = lane(&image);
    let mount_point = image.volumes()[0].mount_point.clone();
    let portal = Arc::new(GitPortal::with_scripted_watcher(
        cmdr_fs::volume::host::VolumeHost::detached(),
        no_git_state_sink(),
    ));
    *lane.fixture.host.git.lock_ignore_poison() = Some(Arc::clone(&portal));
    let dir = mount_point.join("repo");
    std::fs::create_dir(&dir).expect("make the repo dir");
    let mut repo = RepoFixture::init(dir.clone());
    repo.commit_file("README.md", b"hello\n", "initial");
    repo.commit_file("data.bin", &[0x5a; 512 * 1024], "some data");
    git_cli(&dir, &["repack", "-a", "-d", "-q"]);
    drop(repo);
    {
        // Reading HEAD's tree maps the pack; the portal's cached handle keeps it mapped.
        let (handle, _) = portal.discover(&dir).expect("open the repo on the drive");
        let opened = handle.to_thread_local();
        let _tree = opened.head_commit().expect("HEAD").tree().expect("its tree");
    }

    image
        .unmount_volume(0)
        .expect("diskutil unmount goes through: the ask let go of the repo before the kernel looked");

    assert!(!is_mounted(&mount_point));
    let released: Vec<_> = lane
        .fixture
        .host
        .repo_releases()
        .into_iter()
        .flat_map(|(roots, _)| roots)
        .collect();
    assert_eq!(released, [mount_point]);
}
