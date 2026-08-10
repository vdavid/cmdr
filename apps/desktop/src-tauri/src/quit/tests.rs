//! The gate's contract, driven against a recording host instead of Tauri.
//!
//! Everything here runs on the real deadline thread with the real channel; only
//! the durations shrink and the outside world is a recorder. The one test that
//! matters most is [`the_deadline_fires_when_the_frontend_never_answers`]: it is
//! the wedged-webview case the whole design exists for.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::*;
use crate::file_system::write_operations::{LifecycleStatus, OperationSnapshot, WriteOperationType};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until;

/// One teardown step, recorded in the order the gate asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Call {
    CancelAll,
    AbortAll,
    FlushLedger,
    Exit,
}

/// A [`QuitHost`] that records instead of acting.
struct RecordingHost {
    /// What `operations` answers. `cancel_all` clears it when
    /// `settles_on_cancel`, which is a cooperative cancel being obeyed.
    blocking: Mutex<Vec<OperationSnapshot>>,
    settles_on_cancel: bool,
    calls: Mutex<Vec<Call>>,
    announced: Mutex<Vec<QuitRequested>>,
    /// Bumped by `operations` so a test can prove the gate asked once.
    asked: AtomicUsize,
}

impl RecordingHost {
    fn with(operations: Vec<OperationSnapshot>) -> Arc<Self> {
        Arc::new(Self {
            blocking: Mutex::new(operations),
            settles_on_cancel: true,
            calls: Mutex::new(Vec::new()),
            announced: Mutex::new(Vec::new()),
            asked: AtomicUsize::new(0),
        })
    }

    /// A host whose operations never answer the cooperative cancel, so the drain
    /// deadline is what ends the wait.
    fn wedged(operations: Vec<OperationSnapshot>) -> Arc<Self> {
        Arc::new(Self {
            blocking: Mutex::new(operations),
            settles_on_cancel: false,
            calls: Mutex::new(Vec::new()),
            announced: Mutex::new(Vec::new()),
            asked: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock_ignore_poison().clone()
    }

    fn has_exited(&self) -> bool {
        self.calls.lock_ignore_poison().contains(&Call::Exit)
    }

    fn announcements(&self) -> Vec<QuitRequested> {
        self.announced.lock_ignore_poison().clone()
    }
}

impl QuitHost for RecordingHost {
    fn operations(&self) -> Vec<OperationSnapshot> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.blocking.lock_ignore_poison().clone()
    }

    fn announce(&self, event: QuitRequested) {
        self.announced.lock_ignore_poison().push(event);
    }

    fn cancel_all(&self) {
        self.calls.lock_ignore_poison().push(Call::CancelAll);
        if self.settles_on_cancel {
            self.blocking.lock_ignore_poison().clear();
        }
    }

    fn abort_all(&self) {
        self.calls.lock_ignore_poison().push(Call::AbortAll);
    }

    fn flush_temp_ledger(&self) {
        self.calls.lock_ignore_poison().push(Call::FlushLedger);
    }

    fn exit(&self) {
        self.calls.lock_ignore_poison().push(Call::Exit);
    }
}

/// A gate whose countdown and drain are short enough for a test to sit through.
fn gate(countdown: Duration, drain: Duration) -> Arc<QuitGate> {
    Arc::new(QuitGate::with_timings(QuitTimings { countdown, drain }))
}

fn operation(operation_type: WriteOperationType, status: LifecycleStatus) -> OperationSnapshot {
    OperationSnapshot {
        operation_id: format!("{operation_type:?}-{status:?}"),
        operation_type,
        status,
        source: Some("Holiday.mov".to_string()),
        destination: Some("Backup".to_string()),
        supports_rollback: true,
        error: None,
    }
}

#[test]
fn nothing_running_means_no_prompt() {
    let host = RecordingHost::with(vec![]);
    let gate = gate(Duration::from_secs(15), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Proceed);
    assert!(
        host.announcements().is_empty(),
        "nothing to ask about, so nothing announced"
    );
    // The caller exits on `Proceed`; the gate itself never touches the process
    // on this path.
    assert!(host.calls().is_empty());
}

#[test]
fn a_running_copy_holds_the_quit_and_announces_it() {
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let gate = gate(Duration::from_secs(15), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);

    let announcements = host.announcements();
    assert_eq!(announcements.len(), 1);
    assert_eq!(announcements[0].operations.len(), 1);
    assert_eq!(announcements[0].operations[0].operation_type, WriteOperationType::Copy);
    assert_eq!(announcements[0].countdown_ms, COUNTDOWN.as_millis() as u32);
    assert!(
        host.calls().is_empty(),
        "the gate holds; it doesn't tear anything down yet"
    );
}

#[test]
fn a_queued_or_paused_operation_holds_the_quit_too() {
    for status in [LifecycleStatus::Queued, LifecycleStatus::Paused] {
        let host = RecordingHost::with(vec![operation(WriteOperationType::Move, status)]);
        let gate = gate(Duration::from_secs(15), Duration::from_millis(50));
        assert_eq!(
            gate.request_quit(Arc::clone(&host)),
            QuitOutcome::Held,
            "a {status:?} move still has work to lose"
        );
    }
}

#[test]
fn an_instant_metadata_operation_never_holds_the_quit() {
    for operation_type in [
        WriteOperationType::Rename,
        WriteOperationType::CreateFolder,
        WriteOperationType::CreateFile,
    ] {
        let host = RecordingHost::with(vec![operation(operation_type, LifecycleStatus::Running)]);
        let gate = gate(Duration::from_secs(15), Duration::from_millis(50));
        assert_eq!(
            gate.request_quit(Arc::clone(&host)),
            QuitOutcome::Proceed,
            "a {operation_type:?} finishes before a human could read a dialog"
        );
    }
}

#[test]
fn a_settled_operation_never_holds_the_quit() {
    for status in [
        LifecycleStatus::Done,
        LifecycleStatus::Cancelled,
        LifecycleStatus::Failed,
    ] {
        let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, status)]);
        let gate = gate(Duration::from_secs(15), Duration::from_millis(50));
        assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Proceed);
    }
}

#[test]
fn the_deadline_fires_when_the_frontend_never_answers() {
    // The wedged-webview case: nobody calls `quit_confirm` or `quit_cancel`, and
    // the app still goes away. This is why the timer is Rust's.
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let gate = gate(Duration::from_millis(80), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);

    wait_until(Duration::from_secs(5), "the countdown to quit on its own", || {
        host.has_exited()
    });
    assert_eq!(
        host.calls(),
        vec![Call::CancelAll, Call::AbortAll, Call::FlushLedger, Call::Exit]
    );
}

#[test]
fn confirming_quits_without_waiting_out_the_countdown() {
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    // A countdown no test would sit through, so an exit proves the confirm drove it.
    let gate = gate(Duration::from_secs(600), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    gate.confirm();

    wait_until(
        Duration::from_secs(5),
        "the confirmed quit to tear down and exit",
        || host.has_exited(),
    );
    assert_eq!(
        host.calls(),
        vec![Call::CancelAll, Call::AbortAll, Call::FlushLedger, Call::Exit]
    );
}

#[test]
fn cancelling_releases_the_gate_and_the_timer_is_gone() {
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let countdown = Duration::from_millis(80);
    let gate = gate(countdown, Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    gate.cancel();

    // Well past the countdown: a snooze would have fired by now.
    // allowed-test-sleep: a negative assertion over a window — the deadline must
    // NOT fire, and the only way to know is to outlive it.
    std::thread::sleep(countdown * 4);
    assert!(
        !host.has_exited(),
        "\"Don't quit\" cancels the countdown, it doesn't defer it"
    );
    assert!(host.calls().is_empty());

    // And the gate is armed again: the next ⌘Q asks afresh.
    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    assert_eq!(host.announcements().len(), 2);
}

#[test]
fn a_second_quit_request_rides_the_countdown_already_running() {
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let gate = gate(Duration::from_secs(600), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);

    assert_eq!(
        host.announcements().len(),
        1,
        "pressing ⌘Q again must not restart the clock the user is watching"
    );
}

#[test]
fn once_the_decision_is_made_every_later_request_sails_through() {
    // The real shape of this: the teardown ends in `AppHandle::exit(0)`, which
    // comes back around as `RunEvent::ExitRequested`. If the gate asked again
    // there, a still-registered operation would re-prompt and the app would
    // never leave.
    let host = RecordingHost::wedged(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let gate = gate(Duration::from_secs(600), Duration::from_millis(50));

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    gate.confirm();

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Proceed);
}

#[test]
fn the_teardown_stops_waiting_at_the_drain_deadline() {
    // Nothing answers the cooperative cancel, so only the deadline can end it.
    let host = RecordingHost::wedged(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let drain = Duration::from_millis(150);
    let gate = gate(Duration::from_secs(600), drain);

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    let started = Instant::now();
    gate.confirm();

    wait_until(
        Duration::from_secs(5),
        "the wedged teardown to hit its deadline",
        || host.has_exited(),
    );
    let elapsed = started.elapsed();

    assert!(
        elapsed >= drain,
        "the cooperative tier must get its full window, took {elapsed:?}"
    );
    assert!(
        elapsed < drain * 4,
        "a wedged operation must not stretch the budget, took {elapsed:?}"
    );
    assert_eq!(
        host.calls(),
        vec![Call::CancelAll, Call::AbortAll, Call::FlushLedger, Call::Exit]
    );
}

#[test]
fn a_cooperative_cancel_that_lands_skips_the_rest_of_the_drain() {
    let host = RecordingHost::with(vec![operation(WriteOperationType::Copy, LifecycleStatus::Running)]);
    let drain = Duration::from_secs(30);
    let gate = gate(Duration::from_secs(600), drain);

    assert_eq!(gate.request_quit(Arc::clone(&host)), QuitOutcome::Held);
    let started = Instant::now();
    gate.confirm();

    wait_until(Duration::from_secs(5), "the settled teardown to exit", || {
        host.has_exited()
    });
    assert!(
        started.elapsed() < drain,
        "operations that answered must not be waited out"
    );
}

#[test]
fn the_shipped_budget_leaves_room_for_the_teardown() {
    // 15 s + the drain has to fit inside what macOS gives an app to answer a
    // logout or restart. The numbers are the contract; pin them.
    assert_eq!(COUNTDOWN, Duration::from_secs(15));
    assert_eq!(DRAIN, Duration::from_millis(1_500));
    assert!(DRAIN < Duration::from_secs(2), "the whole teardown budget is 2 s");
}
