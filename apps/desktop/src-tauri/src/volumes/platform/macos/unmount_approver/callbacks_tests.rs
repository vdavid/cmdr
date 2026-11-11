//! The approver callbacks' tests, over the seams in `test_seams.rs` and a fake clock.

use std::time::Duration;

use super::super::ask::{APPROVAL_STOP_BUDGET, DA_RESPONSE_WINDOW};
use super::super::test_seams::{asked, fixture, mounted};
use super::*;
use crate::test_support::wait_until;

/// How long a test waits for a detached stop. Never a deadline under test.
const PATIENCE: Duration = Duration::from_secs(5);
/// Well past `RESUME_SETTLE`, so a resume batch's quiet period is over.
const SETTLED: Duration = Duration::from_secs(30);
const UNIT: u32 = 7;

fn path(name: &str) -> PathBuf {
    PathBuf::from(format!("/Volumes/cmdr-test-{name}"))
}

#[test]
fn the_first_ask_of_a_disk_lets_go_of_every_volume_on_it_and_the_second_finds_nothing_left() {
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);
    let group = vec![mounted("disk7s2", UNIT, &a), mounted("disk7s3", UNIT, &b)];

    let first = fx
        .approver
        .on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| Some(group.clone()));

    assert_eq!(first, Answer::Approve);
    assert_eq!(
        fx.index.stops_that_found_an_index(),
        2,
        "a whole-disk request's asks arrive back to back, so the first one lets go of the group"
    );
    assert!(fx.host.stops.all_ran_while_mounted(), "the stop is a PRE-unmount hook");

    let second = fx
        .approver
        .on_unmount_ask(&asked("disk7s3", UNIT, &b), |_| Some(group.clone()));

    assert_eq!(second, Answer::Approve);
    assert_eq!(
        fx.index.stops_that_found_an_index(),
        2,
        "the sibling's ask finds its own volume already let go of"
    );
}

#[test]
fn cmdrs_own_eject_of_one_volume_leaves_its_siblings_index_and_repos_alone() {
    // The one ask that can tell: Finder's one-volume unmount looks like a whole-disk request's.
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);
    fx.host.ejecting.lock_ignore_poison().insert("vol-a".to_string());
    fx.host.ejecting_alone.lock_ignore_poison().insert("vol-a".to_string());
    let asked_at = fx.gate.now();

    let answer = fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| {
        panic!("an ask about one volume has no reason to look at the rest of the unit")
    });

    assert_eq!(answer, Answer::Approve);
    assert_eq!(
        fx.index.stops_that_found_an_index(),
        1,
        "only the volume that's leaving"
    );
    assert_eq!(
        fx.host.repo_releases(),
        [(vec![a], asked_at + APPROVAL_STOP_BUDGET)],
        "and only its repos"
    );
}

#[test]
fn an_ask_lets_go_of_the_repos_on_every_volume_of_its_disk_by_the_chains_deadline() {
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);
    let group = vec![mounted("disk7s2", UNIT, &a), mounted("disk7s3", UNIT, &b)];
    let asked_at = fx.gate.now();

    let answer = fx
        .approver
        .on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| Some(group.clone()));

    assert_eq!(answer, Answer::Approve);
    assert_eq!(
        fx.host.repo_releases(),
        [(vec![a, b], asked_at + APPROVAL_STOP_BUDGET)],
        "a plain unmount fails while any repo on the disk has a pack mapped"
    );
}

#[test]
fn repos_that_wont_let_go_in_time_refuse_the_unmount() {
    let fx = fixture();
    let a = path("A");
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.host.repos_let_go.store(false, std::sync::atomic::Ordering::SeqCst);

    let answer = fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| {
        Some(vec![mounted("disk7s2", UNIT, &a)])
    });

    assert_eq!(
        answer,
        Answer::Dissent,
        "approving would only have the kernel refuse it, with the repo still reading the drive"
    );
}

#[test]
fn a_disk_cmdr_has_nothing_on_is_approved_without_a_group_lookup() {
    let fx = fixture();
    let unknown = path("SomeoneElses");
    fx.host.nodes.lock_ignore_poison().insert("disk7s2".to_string());

    let answer = fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &unknown), |_| {
        panic!("a disk with no Cmdr volume on it never needs its group")
    });

    assert_eq!(answer, Answer::Approve);
    assert!(fx.host.stops.asked().is_empty());

    // And a disk this session doesn't act on at all is approved before anything else is read.
    let elsewhere = path("AnotherMac");
    fx.indexed_volume("vol-elsewhere", "disk9s1", &elsewhere);
    fx.host.nodes.lock_ignore_poison().remove("disk9s1");
    let answer = fx
        .approver
        .on_unmount_ask(&asked("disk9s1", 9, &elsewhere), |_| panic!("not this session's disk"));
    assert_eq!(answer, Answer::Approve);
    assert!(fx.host.stops.asked().is_empty());
}

#[test]
fn a_refused_unmount_hands_the_index_back_at_the_next_idle_and_never_twice() {
    let fx = fixture();
    let a = path("A");
    fx.indexed_volume("vol-a", "disk7s2", &a);

    fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| {
        Some(vec![mounted("disk7s2", UNIT, &a)])
    });
    assert!(!fx.index.is_indexing("vol-a"), "the ask let go of the drive");

    // The kernel refused the unmount, so DiskArbitration went quiet with the volume still there.
    fx.approver.on_idle();
    fx.gate.advance(SETTLED);
    for batch in fx.host.take_resumes() {
        batch.wait();
    }
    assert_eq!(fx.index.started(), ["vol-a"]);

    fx.approver.on_idle();
    fx.gate.advance(SETTLED);
    for batch in fx.host.take_resumes() {
        batch.wait();
    }
    assert_eq!(
        fx.index.started(),
        ["vol-a"],
        "the record was spent by the first resume"
    );
}

#[test]
fn a_raw_umount_stops_the_index_nobody_asked_the_approver_to_stop() {
    let fx = fixture();
    let a = path("A");
    fx.indexed_volume("vol-a", "disk7s2", &a);
    // The volume was there when the session came up; no ask ever came for it.
    fx.approver.on_appeared(&asked("disk7s2", UNIT, &a), false);

    // `/sbin/umount` bypasses DiskArbitration entirely: the only thing that arrives is the
    // description change saying the volume path is gone.
    fx.approver.on_volume_path_cleared("disk7s2");

    wait_until(PATIENCE, "the vanish stop to let go of the drive", || {
        fx.host.vanish_stops() == ["vol-a"]
    });
    assert!(
        fx.host.stops.asked().is_empty(),
        "nothing was asked, so no pre-unmount stop ran"
    );
    assert!(
        fx.index.started().is_empty(),
        "a drive that vanished is owed nothing back"
    );
}

#[test]
fn a_pulled_disk_stops_every_volume_that_was_still_mounted_on_it() {
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);
    fx.approver.on_appeared(&asked("disk7s2", UNIT, &a), false);
    fx.approver.on_appeared(&asked("disk7s3", UNIT, &b), false);

    // The cable was pulled: DiskArbitration sends no eject approval and no description change,
    // only the disappearance.
    fx.approver.on_disappeared(UNIT);

    wait_until(PATIENCE, "both volumes to be stopped", || {
        let mut stopped = fx.host.vanish_stops();
        stopped.sort();
        stopped == ["vol-a", "vol-b"]
    });
}

#[test]
fn an_ejected_disk_is_never_stopped_after_the_fact() {
    let fx = fixture();
    let a = path("A");
    fx.indexed_volume("vol-a", "disk7s2", &a);

    // The whole sequence of an ordinary eject: the ask lets go of the drive, the path clears,
    // the eject is approved, the disk goes.
    fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| {
        Some(vec![mounted("disk7s2", UNIT, &a)])
    });
    fx.approver.on_volume_path_cleared("disk7s2");
    fx.approver.on_eject_approved(UNIT);
    fx.approver.on_disappeared(UNIT);

    assert_eq!(fx.host.stops.asked(), ["vol-a"], "the ask is what let go of the drive");
    assert!(
        fx.host.vanish_stops().is_empty(),
        "the ask already stopped it, so nothing is owed a second stop"
    );
}

#[test]
fn an_ask_with_no_time_left_dissents_and_leaves_its_stop_to_a_thread() {
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);

    let first = fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| {
        Some(vec![mounted("disk7s2", UNIT, &a)])
    });
    assert_eq!(first, Answer::Approve);

    // The queue delivers the next ask late in the chain's window: its own DA timer started
    // inside it, so the budget is spent.
    fx.gate.advance(APPROVAL_STOP_BUDGET + Duration::from_secs(1));
    let second = fx.approver.on_unmount_ask(&asked("disk7s3", UNIT, &b), |_| {
        Some(vec![mounted("disk7s3", UNIT, &b)])
    });

    assert_eq!(
        second,
        Answer::Dissent,
        "with no time left, a drive that still has an index on it is refused, never unmounted under one"
    );
    wait_until(PATIENCE, "the detached stop to let go of B", || {
        !fx.index.is_indexing("vol-b")
    });

    // An ask after the window starts a chain of its own, so it waits for its stop again.
    fx.gate.advance(DA_RESPONSE_WINDOW);
    let third = fx.approver.on_unmount_ask(&asked("disk7s3", UNIT, &b), |_| {
        Some(vec![mounted("disk7s3", UNIT, &b)])
    });
    assert_eq!(third, Answer::Approve);
}

/// Runs an ask about `bsd_name` at `mount` whose stop is held open, and moves the clock past the
/// chain's deadline once the ask is parked waiting on that stop.
fn ask_whose_stop_the_deadline_cuts_short(
    fx: &super::super::test_seams::Fixture,
    bsd_name: &str,
    mount: &Path,
    volume_id: &str,
) -> Answer {
    fx.host.stops.hold(volume_id);
    let answer = std::thread::scope(|scope| {
        let ask = scope.spawn(|| {
            fx.approver.on_unmount_ask(&asked(bsd_name, UNIT, mount), |_| {
                Some(vec![mounted(bsd_name, UNIT, mount)])
            })
        });
        wait_until(PATIENCE, "the ask to wait on the drive's stop", || {
            fx.host.stops.asked().iter().any(|id| id == volume_id) && fx.gate.parked() >= 1
        });
        fx.gate.advance(APPROVAL_STOP_BUDGET);
        ask.join().expect("the ask thread")
    });
    fx.host.stops.open(volume_id);
    answer
}

#[test]
fn a_volume_with_no_index_is_approved_even_when_the_deadline_cuts_its_stop_short() {
    // Regression: production Cmdr refused the disk-images lane's unmount of a disk image it never
    // indexed, because the ask came with milliseconds of a busy chain's budget left and couldn't
    // even collect its no-op stop's answer (2026-11-02).
    let fx = fixture();
    let a = path("A");
    fx.registered_volume("vol-a", "disk7s2", &a);

    let answer = ask_whose_stop_the_deadline_cuts_short(&fx, "disk7s2", &a, "vol-a");

    assert_eq!(
        answer,
        Answer::Approve,
        "background work that was never there never blocks an eject"
    );
}

#[test]
fn an_indexed_volume_whose_stop_the_deadline_cuts_short_still_dissents() {
    let fx = fixture();
    let a = path("A");
    fx.indexed_volume("vol-a", "disk7s2", &a);

    let answer = ask_whose_stop_the_deadline_cuts_short(&fx, "disk7s2", &a, "vol-a");

    assert_eq!(
        answer,
        Answer::Dissent,
        "a watcher may still be live on the drive, and unmounting under one can wedge FSKit"
    );
}

#[test]
fn an_ask_that_cannot_read_its_disk_dissents_rather_than_calling_the_disk_empty() {
    let fx = fixture();
    let (a, b) = (path("A"), path("B"));
    fx.indexed_volume("vol-a", "disk7s2", &a);
    fx.indexed_volume("vol-b", "disk7s3", &b);

    // The mount table wouldn't answer, so the ask can't name the sibling, can't have stopped it,
    // and must not let the unmount take the disk down under its live watcher.
    let answer = fx.approver.on_unmount_ask(&asked("disk7s2", UNIT, &a), |_| None);

    assert_eq!(
        answer,
        Answer::Dissent,
        "a disk nobody could read is refused, ❌ never approved as an empty one"
    );
    assert!(
        fx.index.is_indexing("vol-b"),
        "the sibling was never seen, so it was never let go of — which is exactly why the ask dissents"
    );

    // A read that DID answer and found one volume still approves, so the fix can't be mistaken
    // for "dissent whenever a disk looks small".
    let alone = path("Alone");
    fx.indexed_volume("vol-alone", "disk8s1", &alone);
    fx.gate.advance(DA_RESPONSE_WINDOW);
    let answer = fx.approver.on_unmount_ask(&asked("disk8s1", 8, &alone), |_| {
        Some(vec![mounted("disk8s1", 8, &alone)])
    });
    assert_eq!(answer, Answer::Approve);
}
