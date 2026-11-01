//! The security event log: a small, local, append-only record of security-relevant changes and
//! refusals (`security-events.jsonl` in the app data dir), with an opt-out. Meets the CRA's
//! Annex I Part I (2)(l). Design and the event catalog: `DETAILS.md`.
//!
//! Every source calls [`record`] with a typed [`SecurityEvent`]. That's the ONE emitter: it hands
//! the event to every [`subscribe`]d consumer, whatever the opt-out says (a notice isn't the
//! record), and to the log file while recording is on.

mod episodes;
mod types;
mod writer;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tauri::{AppHandle, Runtime};

use crate::ignore_poison::RwLockIgnorePoison;
use crate::managed_policy::ManagedPolicy;

pub use episodes::{note_content_refusal, note_mcp_rejection, note_smb_signature_rejection};
// Only the macOS updater records a policy refusal, so the re-export would be unused elsewhere.
#[cfg(target_os = "macos")]
pub use types::PolicyRefusal;
pub use types::{
    ContentTool, Initiator, LogLine, McpRejection, PolicyTrigger, ProtectedContent, RecordingSwitch, SecurityEvent,
    SignatureCheck, StateFile,
};
pub use writer::file_path;

const LOG_TARGET: &str = "security_log";

/// How long quit waits for queued lines to reach the file.
const EXIT_FLUSH_TIMEOUT: Duration = Duration::from_millis(500);

/// How a subscriber heard an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// It went through [`record`], so the file has it too while recording is on.
    Recorded,
    /// One more rejection inside an episode that's already open, delivered as that episode's
    /// started event. ❗ Only subscribers hear it: the file gets the count when the episode ends.
    Repeat,
}

/// A consumer of every event, like the in-app notice gate (`crate::security_notices`).
pub type Subscriber = Arc<dyn Fn(&SecurityEvent, Heard) + Send + Sync>;

type LogSink = Arc<dyn Fn(LogLine) + Send + Sync>;

static RECORDER: Recorder = Recorder::new();
static WRITER: std::sync::OnceLock<writer::Writer> = std::sync::OnceLock::new();

/// Records `event`: every subscriber hears it, and the log file gets it while recording is on.
/// Never blocks, never fails.
pub fn record(event: SecurityEvent) {
    #[cfg(test)]
    if testing::capture(&event) {
        return;
    }
    RECORDER.record(event);
}

/// Adds a consumer that hears every event from now on, and every repeat inside an open episode,
/// on the recording thread. Keep it quick.
pub fn subscribe(subscriber: Subscriber) {
    RECORDER.subscribers.write_ignore_poison().push(subscriber);
}

/// One more rejection inside an open episode: `started` is that episode's started event. Only
/// subscribers hear it, whatever the opt-out says.
fn repeat(started: SecurityEvent) {
    #[cfg(test)]
    if testing::capture_repeat(&started) {
        return;
    }
    RECORDER.repeat(&started);
}

/// The person's switch (`securityLog.enabled`), pushed live by the settings applier.
pub fn set_user_enabled(enabled: bool) {
    RECORDER.set_user_enabled(enabled);
}

/// The organization's policy took effect (`old` is `None` at launch) or changed. Records which
/// keys are set, and switches recording on or off when the policy starts or stops requiring it.
pub fn on_policy_applied(old: Option<&ManagedPolicy>, new: &ManagedPolicy) {
    let keys = |policy: &ManagedPolicy| -> Vec<String> {
        crate::managed_policy::keys::restricting(policy)
            .into_iter()
            .map(String::from)
            .collect()
    };
    let applied = SecurityEvent::ManagedPolicyApplied {
        trigger: if old.is_some() {
            PolicyTrigger::Change
        } else {
            PolicyTrigger::Launch
        },
        keys_before: old.map(keys).unwrap_or_default(),
        keys_after: keys(new),
    };
    #[cfg(test)]
    if testing::capture(&applied) {
        return;
    }
    RECORDER.apply_policy(new.security_log_required(), applied);
}

/// Opens the log under the app data dir and reads the person's switch and the policy. Call from
/// `setup()` before anything that records: earlier events have nowhere to go and are dropped.
pub fn init<R: Runtime>(app: &AppHandle<R>) {
    RECORDER
        .user_enabled
        .store(crate::settings::load_security_log_enabled(app), Ordering::SeqCst);
    RECORDER.policy_requires.store(
        crate::managed_policy::current().security_log_required(),
        Ordering::SeqCst,
    );
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "No app data dir, so no security log this session: {e}");
            return;
        }
    };
    let file = writer::LogFile::new(
        file_path(&data_dir),
        writer::MAX_BYTES,
        Box::new(|| !crate::data_wipe::has_wiped()),
    );
    match writer::Writer::spawn(file) {
        Ok(writer) => {
            let sink = writer.clone();
            if WRITER.set(writer).is_ok() {
                *RECORDER.log.write_ignore_poison() = Some(Arc::new(move |line| sink.send(line)));
            }
        }
        Err(e) => log::warn!(target: LOG_TARGET, "The security log writer didn't start: {e}"),
    }
}

/// What "Show in Finder" reveals: the log file, or the folder it will appear in.
pub fn reveal_target(data_dir: &std::path::Path) -> std::path::PathBuf {
    let file = file_path(data_dir);
    if file.exists() { file } else { data_dir.to_path_buf() }
}

/// Quit: ends every open rejection episode (so its count lands) and waits briefly for the file.
pub fn on_exit() {
    episodes::close_all_now();
    if let Some(writer) = WRITER.get()
        && !writer.flush(EXIT_FLUSH_TIMEOUT)
    {
        log::warn!(target: LOG_TARGET, "Quit before the last security events reached the file");
    }
}

/// Whether recording is on, and who decides it. Its own type (not bare statics) so tests build
/// one with a capturing sink.
struct Recorder {
    user_enabled: AtomicBool,
    policy_requires: AtomicBool,
    log: RwLock<Option<LogSink>>,
    subscribers: RwLock<Vec<Subscriber>>,
}

impl Recorder {
    const fn new() -> Self {
        Self {
            user_enabled: AtomicBool::new(true),
            policy_requires: AtomicBool::new(false),
            log: RwLock::new(None),
            subscribers: RwLock::new(Vec::new()),
        }
    }

    fn is_recording(&self) -> bool {
        self.user_enabled.load(Ordering::SeqCst) || self.policy_requires.load(Ordering::SeqCst)
    }

    fn record(&self, event: SecurityEvent) {
        self.deliver(event, self.is_recording());
    }

    fn repeat(&self, started: &SecurityEvent) {
        let subscribers = self.subscribers.read_ignore_poison().clone();
        for subscriber in &subscribers {
            subscriber(started, Heard::Repeat);
        }
    }

    /// "Turned on" is the first line after switching on; "turned off" the last one before
    /// stopping, written although recording is now off.
    fn set_user_enabled(&self, enabled: bool) {
        let before = self.is_recording();
        self.user_enabled.store(enabled, Ordering::SeqCst);
        self.announce_switch(before, RecordingSwitch::User);
    }

    /// Records `applied` while the log is on: a policy that starts requiring the log turns it on
    /// first, and one that stops requiring it turns it off only after its own line.
    fn apply_policy(&self, requires: bool, applied: SecurityEvent) {
        let before = self.is_recording();
        if requires {
            self.policy_requires.store(true, Ordering::SeqCst);
            self.announce_switch(before, RecordingSwitch::Organization);
            self.record(applied);
        } else {
            self.deliver(applied, before);
            self.policy_requires.store(false, Ordering::SeqCst);
            self.announce_switch(before, RecordingSwitch::Organization);
        }
    }

    fn announce_switch(&self, before: bool, by: RecordingSwitch) {
        match (before, self.is_recording()) {
            (false, true) => self.deliver(SecurityEvent::RecordingTurnedOn { by }, true),
            (true, false) => self.deliver(SecurityEvent::RecordingTurnedOff { by }, true),
            _ => {}
        }
    }

    /// Hands `event` to every subscriber, and to the file when `to_file`.
    fn deliver(&self, event: SecurityEvent, to_file: bool) {
        let subscribers = self.subscribers.read_ignore_poison().clone();
        for subscriber in &subscribers {
            subscriber(&event, Heard::Recorded);
        }
        if !to_file {
            return;
        }
        match self.log.read_ignore_poison().as_ref() {
            Some(sink) => sink(LogLine::now(event)),
            None => log::debug!(target: LOG_TARGET, "No security log open, dropping {event:?}"),
        }
    }
}

/// For hook tests elsewhere: capture what a source records, on this thread only.
#[cfg(test)]
pub mod testing {
    use std::cell::RefCell;

    use super::episodes::Episodes;
    use super::{ContentTool, McpRejection, ProtectedContent, SecurityEvent};

    pub(super) struct Captured {
        pub events: Vec<SecurityEvent>,
        pub repeats: Vec<SecurityEvent>,
        pub mcp: Episodes<McpRejection>,
        pub smb: Episodes<String>,
        pub content: Episodes<(ContentTool, ProtectedContent)>,
    }

    thread_local! {
        pub(super) static CAPTURE: RefCell<Option<Captured>> = const { RefCell::new(None) };
    }

    /// While alive, [`super::record`] and the episode notes on THIS thread land here instead of
    /// the log, so parallel tests can't see each other's events. A `#[tokio::test]` runs its tasks
    /// on the test thread, so the capture reaches them too.
    #[must_use = "the capture ends when the guard drops"]
    pub struct CaptureGuard(());

    pub fn capture_events() -> CaptureGuard {
        CAPTURE.with_borrow_mut(|slot| {
            *slot = Some(Captured {
                events: Vec::new(),
                repeats: Vec::new(),
                mcp: Episodes::new(super::episodes::QUIET),
                smb: Episodes::new(super::episodes::QUIET),
                content: Episodes::new(super::episodes::QUIET),
            });
        });
        CaptureGuard(())
    }

    impl CaptureGuard {
        /// Everything recorded so far, in order.
        pub fn events(&self) -> Vec<SecurityEvent> {
            CAPTURE.with_borrow(|slot| slot.as_ref().map(|c| c.events.clone()).unwrap_or_default())
        }

        /// Every repeat inside an open episode so far, each as its episode's started event.
        pub fn repeats(&self) -> Vec<SecurityEvent> {
            CAPTURE.with_borrow(|slot| slot.as_ref().map(|c| c.repeats.clone()).unwrap_or_default())
        }
    }

    impl Drop for CaptureGuard {
        fn drop(&mut self) {
            CAPTURE.with_borrow_mut(|slot| *slot = None);
        }
    }

    /// Whether a capture took the repeat `started`.
    pub(super) fn capture_repeat(started: &SecurityEvent) -> bool {
        CAPTURE.with_borrow_mut(|slot| match slot {
            Some(captured) => {
                captured.repeats.push(started.clone());
                true
            }
            None => false,
        })
    }

    /// Whether a capture took `event`.
    pub(super) fn capture(event: &SecurityEvent) -> bool {
        CAPTURE.with_borrow_mut(|slot| match slot {
            Some(captured) => {
                captured.events.push(event.clone());
                true
            }
            None => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    type Seen = Arc<Mutex<Vec<SecurityEvent>>>;
    type HeardHow = Arc<Mutex<Vec<(SecurityEvent, Heard)>>>;

    /// A recorder whose file is a list, plus a subscriber's list.
    fn recorder() -> (Recorder, Seen, Seen) {
        let recorder = Recorder::new();
        let file = Arc::new(Mutex::new(Vec::new()));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let file_sink = Arc::clone(&file);
        *recorder.log.write_ignore_poison() = Some(Arc::new(move |line: LogLine| {
            file_sink.lock().expect("test lock").push(line.event);
        }));
        let heard_sink = Arc::clone(&heard);
        recorder
            .subscribers
            .write_ignore_poison()
            .push(Arc::new(move |event: &SecurityEvent, _: Heard| {
                heard_sink.lock().expect("test lock").push(event.clone());
            }));
        (recorder, file, heard)
    }

    fn taken(list: &Seen) -> Vec<SecurityEvent> {
        std::mem::take(&mut *list.lock().expect("test lock"))
    }

    fn consent() -> SecurityEvent {
        SecurityEvent::CloudAiConsentGiven
    }

    fn applied() -> SecurityEvent {
        SecurityEvent::ManagedPolicyApplied {
            trigger: PolicyTrigger::Change,
            keys_before: vec![],
            keys_after: vec![],
        }
    }

    #[test]
    fn recording_on_writes_every_event_and_subscribers_hear_it() {
        let (recorder, file, heard) = recorder();
        recorder.record(consent());
        assert_eq!(taken(&file), vec![consent()]);
        assert_eq!(taken(&heard), vec![consent()]);
    }

    #[test]
    fn turning_off_writes_exactly_the_turned_off_line_then_nothing() {
        let (recorder, file, heard) = recorder();
        recorder.set_user_enabled(false);
        assert_eq!(
            taken(&file),
            vec![SecurityEvent::RecordingTurnedOff {
                by: RecordingSwitch::User
            }]
        );
        recorder.record(consent());
        recorder.set_user_enabled(false);
        assert!(
            taken(&file).is_empty(),
            "off writes nothing, and off again is no change"
        );
        assert_eq!(taken(&heard).last(), Some(&consent()), "a subscriber still hears it");
    }

    #[test]
    fn turning_on_writes_the_turned_on_line_first() {
        let (recorder, file, _) = recorder();
        recorder.set_user_enabled(false);
        taken(&file);
        recorder.set_user_enabled(true);
        recorder.record(consent());
        assert_eq!(
            taken(&file),
            vec![
                SecurityEvent::RecordingTurnedOn {
                    by: RecordingSwitch::User
                },
                consent()
            ]
        );
        recorder.set_user_enabled(true);
        assert!(taken(&file).is_empty(), "on again is no change");
    }

    #[test]
    fn a_required_log_ignores_the_switch() {
        let (recorder, file, _) = recorder();
        recorder.apply_policy(true, applied());
        assert_eq!(taken(&file), vec![applied()]);
        recorder.set_user_enabled(false);
        recorder.record(consent());
        assert_eq!(
            taken(&file),
            vec![consent()],
            "the switch can't turn a required log off"
        );
    }

    #[test]
    fn a_policy_requiring_the_log_turns_it_back_on_and_records_itself() {
        let (recorder, file, _) = recorder();
        recorder.set_user_enabled(false);
        taken(&file);
        recorder.apply_policy(true, applied());
        assert_eq!(
            taken(&file),
            vec![
                SecurityEvent::RecordingTurnedOn {
                    by: RecordingSwitch::Organization
                },
                applied()
            ]
        );
    }

    #[test]
    fn a_policy_letting_go_of_the_log_records_itself_then_stops_when_the_switch_is_off() {
        let (recorder, file, _) = recorder();
        recorder.apply_policy(true, applied());
        recorder.set_user_enabled(false);
        taken(&file);
        recorder.apply_policy(false, applied());
        assert_eq!(
            taken(&file),
            vec![
                applied(),
                SecurityEvent::RecordingTurnedOff {
                    by: RecordingSwitch::Organization
                }
            ]
        );
        recorder.record(consent());
        assert!(taken(&file).is_empty());
    }

    #[test]
    fn a_policy_change_while_off_and_not_required_writes_nothing() {
        let (recorder, file, heard) = recorder();
        recorder.set_user_enabled(false);
        taken(&file);
        recorder.apply_policy(false, applied());
        assert!(taken(&file).is_empty());
        assert_eq!(taken(&heard).last(), Some(&applied()));
    }

    #[test]
    fn show_in_finder_reveals_the_file_or_its_folder_before_it_exists() {
        let dir = crate::test_support::TestDir::new("security-log-reveal");
        assert_eq!(reveal_target(&dir), dir.to_path_buf());
        std::fs::write(file_path(&dir), "").expect("write");
        assert_eq!(reveal_target(&dir), file_path(&dir));
    }

    #[test]
    fn a_repeat_reaches_subscribers_but_never_the_file() {
        let (recorder, file, _) = recorder();
        let how: HeardHow = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&how);
        recorder
            .subscribers
            .write_ignore_poison()
            .push(Arc::new(move |event: &SecurityEvent, heard: Heard| {
                sink.lock().expect("test lock").push((event.clone(), heard));
            }));
        recorder.record(consent());
        recorder.repeat(&consent());
        assert_eq!(taken(&file), vec![consent()], "the file has the record, not the repeat");
        assert_eq!(
            *how.lock().expect("test lock"),
            vec![(consent(), Heard::Recorded), (consent(), Heard::Repeat)]
        );
    }

    #[test]
    fn a_capture_takes_what_this_thread_records() {
        let capture = testing::capture_events();
        record(consent());
        assert_eq!(capture.events(), vec![consent()]);
    }
}
