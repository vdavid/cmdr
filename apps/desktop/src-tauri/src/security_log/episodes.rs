//! Per-episode coalescing for rejections that repeat: a misconfigured MCP client retries
//! constantly, and one bad network device can refuse many SMB replies in a row. The log gets the
//! first rejection of an episode and one summary with the count once it goes quiet, never one
//! line per rejection.
//!
//! An episode ends only after [`QUIET`] without a rejection for its key (or at quit), ❌ not on a
//! successful request: a working client and a stale one polling side by side would otherwise
//! open and close an episode on every alternation, which is the flood this exists to stop.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use super::types::{ContentTool, McpRejection, ProtectedContent, SecurityEvent};
use crate::ignore_poison::IgnorePoison;

/// How long a key must stay free of rejections for its episode to end.
pub const QUIET: Duration = Duration::from_secs(10 * 60);

/// What a call did to the episodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition<K> {
    Started(K),
    /// The episode for this key ended after this many rejections, the first one included.
    Ended(K, u64),
}

struct Open {
    count: u64,
    last: Instant,
}

/// The open episodes, keyed by whatever separates one source from another. Pure state over an
/// injected clock: callers pass `now`.
pub struct Episodes<K> {
    quiet: Duration,
    open: HashMap<K, Open>,
}

impl<K: Eq + Hash + Clone> Episodes<K> {
    pub fn new(quiet: Duration) -> Self {
        Self {
            quiet,
            open: HashMap::new(),
        }
    }

    /// One more rejection for `key`. An episode that went quiet before this one ends first, so a
    /// late closer never merges two episodes.
    pub fn note(&mut self, key: K, now: Instant) -> Vec<Transition<K>> {
        let mut transitions = Vec::new();
        if let Some(open) = self.open.get(&key)
            && now.saturating_duration_since(open.last) >= self.quiet
        {
            transitions.push(Transition::Ended(key.clone(), open.count));
            self.open.remove(&key);
        }
        match self.open.get_mut(&key) {
            Some(open) => {
                open.count += 1;
                open.last = now;
            }
            None => {
                self.open.insert(key.clone(), Open { count: 1, last: now });
                transitions.push(Transition::Started(key));
            }
        }
        transitions
    }

    /// Ends every episode quiet for at least the window.
    pub fn close_quiet(&mut self, now: Instant) -> Vec<Transition<K>> {
        let quiet = self.quiet;
        let mut ended = Vec::new();
        self.open.retain(|key, open| {
            let done = now.saturating_duration_since(open.last) >= quiet;
            if done {
                ended.push(Transition::Ended(key.clone(), open.count));
            }
            !done
        });
        ended
    }

    /// Ends every open episode, quiet or not: Cmdr is quitting.
    pub fn close_all(&mut self) -> Vec<Transition<K>> {
        self.open
            .drain()
            .map(|(key, open)| Transition::Ended(key, open.count))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
}

// ── The process-wide episodes ────────────────────────────────────────────────────

/// How often the closer looks for episodes gone quiet. An episode's end lands up to this late.
const CLOSER_TICK: Duration = Duration::from_secs(60);

static MCP: LazyLock<Mutex<Episodes<McpRejection>>> = LazyLock::new(|| Mutex::new(Episodes::new(QUIET)));
static SMB: LazyLock<Mutex<Episodes<String>>> = LazyLock::new(|| Mutex::new(Episodes::new(QUIET)));
static CONTENT: LazyLock<Mutex<Episodes<(ContentTool, ProtectedContent)>>> =
    LazyLock::new(|| Mutex::new(Episodes::new(QUIET)));
static CLOSER_RUNNING: AtomicBool = AtomicBool::new(false);

/// The MCP server turned a request away for `reason`. Records the episode's start, if this opens
/// one, and hands every later one to subscribers as a repeat. Cheap enough for every rejected
/// request: a lock and a map lookup.
pub fn note_mcp_rejection(reason: McpRejection) {
    let started = SecurityEvent::McpRejectionsStarted { reason };
    #[cfg(test)]
    if let Some(transitions) = captured(|c| c.mcp.note(reason, Instant::now())) {
        return settle(transitions, mcp_event, started);
    }
    let transitions = MCP.lock_ignore_poison().note(reason, Instant::now());
    settle(transitions, mcp_event, started);
    ensure_closer();
}

/// An SMB reply on `volume_id` was refused for a bad or missing signature.
pub fn note_smb_signature_rejection(volume_id: &str) {
    let started = SecurityEvent::SmbSignatureRejectionsStarted {
        volume_id: volume_id.to_string(),
    };
    #[cfg(test)]
    if let Some(transitions) = captured(|c| c.smb.note(volume_id.to_string(), Instant::now())) {
        return settle(transitions, smb_event, started);
    }
    let transitions = SMB.lock_ignore_poison().note(volume_id.to_string(), Instant::now());
    settle(transitions, smb_event, started);
    ensure_closer();
}

/// An AI tool was refused a file's contents. Keyed by tool and reason: an injected turn can ask
/// for a thousand protected files, and the log still gets one start and one count.
pub fn note_content_refusal(tool: ContentTool, reason: ProtectedContent) {
    let started = SecurityEvent::ContentRefusalsStarted { tool, reason };
    #[cfg(test)]
    if let Some(transitions) = captured(|c| c.content.note((tool, reason), Instant::now())) {
        return settle(transitions, content_event, started);
    }
    let transitions = CONTENT.lock_ignore_poison().note((tool, reason), Instant::now());
    settle(transitions, content_event, started);
    ensure_closer();
}

/// Records what a note did, and when it opened no episode, hands `started` (the open one's
/// started event) to subscribers as a repeat.
fn settle<K>(transitions: Vec<Transition<K>>, to_event: fn(Transition<K>) -> SecurityEvent, started: SecurityEvent) {
    let opened = transitions.iter().any(|t| matches!(t, Transition::Started(_)));
    record_all(transitions.into_iter().map(to_event).collect());
    if !opened {
        super::repeat(started);
    }
}

/// Quit: ends every open episode so its count reaches the log.
pub(super) fn close_all_now() {
    let mcp = MCP.lock_ignore_poison().close_all();
    let smb = SMB.lock_ignore_poison().close_all();
    let content = CONTENT.lock_ignore_poison().close_all();
    record_all(
        mcp.into_iter()
            .map(mcp_event)
            .chain(smb.into_iter().map(smb_event))
            .chain(content.into_iter().map(content_event))
            .collect(),
    );
}

fn mcp_event(transition: Transition<McpRejection>) -> SecurityEvent {
    match transition {
        Transition::Started(reason) => SecurityEvent::McpRejectionsStarted { reason },
        Transition::Ended(reason, count) => SecurityEvent::McpRejectionsEnded { reason, count },
    }
}

fn smb_event(transition: Transition<String>) -> SecurityEvent {
    match transition {
        Transition::Started(volume_id) => SecurityEvent::SmbSignatureRejectionsStarted { volume_id },
        Transition::Ended(volume_id, count) => SecurityEvent::SmbSignatureRejectionsEnded { volume_id, count },
    }
}

fn content_event(transition: Transition<(ContentTool, ProtectedContent)>) -> SecurityEvent {
    match transition {
        Transition::Started((tool, reason)) => SecurityEvent::ContentRefusalsStarted { tool, reason },
        Transition::Ended((tool, reason), count) => SecurityEvent::ContentRefusalsEnded { tool, reason, count },
    }
}

/// Records outside any episode lock, so a subscriber can't deadlock against a rejection.
fn record_all(events: Vec<SecurityEvent>) {
    for event in events {
        super::record(event);
    }
}

/// Starts the closer thread unless it runs. It ticks while any episode is open and exits once none
/// is, so an idle app has no thread for this.
fn ensure_closer() {
    if CLOSER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("security-log-episodes".into())
        .spawn(|| {
            loop {
                std::thread::sleep(CLOSER_TICK);
                let now = Instant::now();
                let (mcp, mcp_left) = {
                    let mut episodes = MCP.lock_ignore_poison();
                    (episodes.close_quiet(now), !episodes.is_empty())
                };
                let (smb, smb_left) = {
                    let mut episodes = SMB.lock_ignore_poison();
                    (episodes.close_quiet(now), !episodes.is_empty())
                };
                let (content, content_left) = {
                    let mut episodes = CONTENT.lock_ignore_poison();
                    (episodes.close_quiet(now), !episodes.is_empty())
                };
                record_all(
                    mcp.into_iter()
                        .map(mcp_event)
                        .chain(smb.into_iter().map(smb_event))
                        .chain(content.into_iter().map(content_event))
                        .collect(),
                );
                if !mcp_left && !smb_left && !content_left {
                    CLOSER_RUNNING.store(false, Ordering::SeqCst);
                    // A rejection between the check and the store saw the flag set and didn't
                    // start a closer, so keep this one when that happened.
                    let reopened = !MCP.lock_ignore_poison().is_empty()
                        || !SMB.lock_ignore_poison().is_empty()
                        || !CONTENT.lock_ignore_poison().is_empty();
                    if !reopened || CLOSER_RUNNING.swap(true, Ordering::SeqCst) {
                        return;
                    }
                }
            }
        });
    if let Err(e) = spawned {
        CLOSER_RUNNING.store(false, Ordering::SeqCst);
        log::warn!(target: "security_log", "The episode closer didn't start, so episodes end at quit: {e}");
    }
}

/// Runs `f` against this thread's capture, if a test holds one.
#[cfg(test)]
fn captured<T>(f: impl FnOnce(&mut super::testing::Captured) -> T) -> Option<T> {
    super::testing::CAPTURE.with_borrow_mut(|slot| slot.as_mut().map(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(600);

    fn episodes() -> Episodes<&'static str> {
        Episodes::new(WINDOW)
    }

    #[test]
    fn rejections_inside_the_window_make_one_episode_with_their_count() {
        let mut e = episodes();
        let t0 = Instant::now();
        assert_eq!(e.note("wrong", t0), vec![Transition::Started("wrong")]);
        for i in 1..1_000u64 {
            assert!(e.note("wrong", t0 + Duration::from_millis(i)).is_empty());
        }
        let last = t0 + Duration::from_millis(999);
        assert!(
            e.close_quiet(last + WINDOW - Duration::from_secs(1)).is_empty(),
            "not quiet long enough"
        );
        assert_eq!(e.close_quiet(last + WINDOW), vec![Transition::Ended("wrong", 1_000)]);
        assert!(e.is_empty());
    }

    #[test]
    fn each_key_has_its_own_episode() {
        let mut e = episodes();
        let t0 = Instant::now();
        assert_eq!(e.note("a", t0), vec![Transition::Started("a")]);
        assert_eq!(e.note("b", t0), vec![Transition::Started("b")]);
        e.note("a", t0 + Duration::from_secs(1));
        let mut ended = e.close_all();
        ended.sort_by_key(|t| format!("{t:?}"));
        assert_eq!(ended, vec![Transition::Ended("a", 2), Transition::Ended("b", 1)]);
    }

    #[test]
    fn a_rejection_after_a_quiet_spell_ends_the_old_episode_before_starting_a_new_one() {
        let mut e = episodes();
        let t0 = Instant::now();
        e.note("wrong", t0);
        e.note("wrong", t0 + Duration::from_secs(1));
        assert_eq!(
            e.note("wrong", t0 + Duration::from_secs(1) + WINDOW),
            vec![Transition::Ended("wrong", 2), Transition::Started("wrong")]
        );
    }

    /// A subscriber (the notice gate) counts every rejection, so each one after the first in an
    /// episode reaches it as a repeat, while the file still gets only the start.
    #[test]
    fn every_rejection_after_an_episodes_first_is_a_repeat_for_subscribers() {
        let capture = crate::security_log::testing::capture_events();
        for _ in 0..3 {
            note_mcp_rejection(McpRejection::ForeignOrigin);
        }
        note_smb_signature_rejection("smb-nas");
        note_smb_signature_rejection("smb-nas");
        let mcp = SecurityEvent::McpRejectionsStarted {
            reason: McpRejection::ForeignOrigin,
        };
        let smb = SecurityEvent::SmbSignatureRejectionsStarted {
            volume_id: "smb-nas".into(),
        };
        assert_eq!(capture.events(), vec![mcp.clone(), smb.clone()]);
        assert_eq!(capture.repeats(), vec![mcp.clone(), mcp, smb]);
    }

    #[test]
    fn the_quiet_spell_counts_from_the_last_rejection_not_the_first() {
        let mut e = episodes();
        let t0 = Instant::now();
        e.note("wrong", t0);
        e.note("wrong", t0 + WINDOW - Duration::from_secs(1));
        assert!(e.close_quiet(t0 + WINDOW).is_empty());
    }
}
