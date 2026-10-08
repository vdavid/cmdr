//! The process-wide policy: read lazily on first use, re-read on the refresh triggers, and handed
//! out as a cheap `Arc`. Triggers and why each exists: `DETAILS.md` § Refresh.

use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_specta::Event as _;

use crate::ignore_poison::{IgnorePoison, RwLockIgnorePoison};

use super::keys::{self, Parsed};
use super::{ManagedPolicy, ManagedPolicyChanged, ManagedPolicyView};

/// Names a plist file that replaces the managed layer, in debug and E2E builds only.
#[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
const OVERRIDE_ENV: &str = "CMDR_MANAGED_PREFS_FILE";

/// An egress read younger than this reuses the last one: an Ask Cmdr tool loop or a suggestion
/// stream fires many requests, and one `cfprefsd` round trip per second is plenty.
const EGRESS_MAX_AGE: Duration = Duration::from_secs(1);

static CACHE: PolicyCache = PolicyCache::new(read_configured_source);
static APP: OnceLock<AppHandle> = OnceLock::new();

/// The cached policy. The first call anywhere in the process reads it synchronously, so no caller
/// ever sees the "not loaded yet" `Default` (which would be no restriction).
pub fn current() -> Arc<ManagedPolicy> {
    #[cfg(test)]
    if let Some(policy) = test_override() {
        return policy;
    }
    CACHE.current()
}

/// Re-reads the policy now and applies a change (event plus immediate stops). Blocking: call it off
/// the main thread. Returns whether the policy changed.
pub fn refresh() -> bool {
    match CACHE.refresh() {
        Some(change) => {
            apply_change(&change);
            true
        }
        None => false,
    }
}

/// The policy as it is right now, for a path about to send bytes off the Mac (rule 6). Reads
/// fresh unless the last read is under a second old, on a blocking thread so a slow `cfprefsd`
/// never stalls a tokio worker.
pub async fn for_egress() -> Arc<ManagedPolicy> {
    #[cfg(test)]
    if let Some(policy) = test_override() {
        return policy;
    }
    if !CACHE.is_fresh(EGRESS_MAX_AGE) {
        match tauri::async_runtime::spawn_blocking(|| CACHE.refresh_if_older_than(EGRESS_MAX_AGE)).await {
            Ok(Some(change)) => apply_change(&change),
            Ok(None) => {}
            Err(e) => {
                log::warn!(target: "managed_policy", "The egress policy read panicked ({e}); using the cached policy")
            }
        }
    }
    current()
}

#[cfg(test)]
thread_local! {
    static TEST_POLICY: std::cell::RefCell<Option<Arc<ManagedPolicy>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_override() -> Option<Arc<ManagedPolicy>> {
    TEST_POLICY.with_borrow(Clone::clone)
}

/// While alive, [`current`] and [`for_egress`] on THIS thread answer `policy`. Per thread, so
/// parallel tests can't see each other's policy; a `#[tokio::test]` (current-thread runtime) runs
/// its tasks on the test thread, so the override reaches them too.
#[cfg(test)]
#[must_use = "the override ends when the guard drops"]
pub struct PolicyOverride(());

#[cfg(test)]
pub fn override_for_test(policy: ManagedPolicy) -> PolicyOverride {
    TEST_POLICY.with_borrow_mut(|slot| *slot = Some(Arc::new(policy)));
    PolicyOverride(())
}

#[cfg(test)]
impl Drop for PolicyOverride {
    fn drop(&mut self) {
        TEST_POLICY.with_borrow_mut(|slot| *slot = None);
    }
}

/// Reads the policy (if nothing has yet), keeps the app handle for change events, and starts the
/// refresh triggers. Call from `setup()` before anything that might send.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    let policy = current();
    log::info!(target: "managed_policy", "Managed policy at launch: {}", summary(&policy));
    #[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
    if let Some(path) = std::env::var_os(OVERRIDE_ENV) {
        // The file stands in for the whole managed layer, so its own watch is the trigger that
        // matters; the macOS ones below only re-read the same file.
        super::override_watch::start(std::path::PathBuf::from(path));
    }
    #[cfg(target_os = "macos")]
    super::watch::start();
}

/// Everything a policy change does, in one visible list: log it, tell every window, and make AI
/// stop what the new policy refuses (in-flight calls, the local server, a model download).
fn apply_change(change: &Change) {
    log::info!(
        target: "managed_policy",
        "Managed policy changed: {} (was {})",
        summary(&change.new),
        summary(&change.old)
    );
    if let Some(app) = APP.get() {
        let payload = ManagedPolicyChanged {
            policy: ManagedPolicyView::from(&*change.new),
        };
        if let Err(e) = payload.emit(app) {
            log::warn!(target: "managed_policy", "Couldn't emit managed-policy-changed: {e}");
        }
        crate::ai::managed::apply_policy_change(app, &change.old, &change.new);
    }
}

fn summary(policy: &ManagedPolicy) -> String {
    if policy.is_managed() {
        format!("{policy:?}")
    } else {
        "no restrictions".to_string()
    }
}

fn read_configured_source() -> Parsed {
    #[cfg(any(debug_assertions, feature = "playwright-e2e", test))]
    if let Some(path) = std::env::var_os(OVERRIDE_ENV) {
        log::debug!(target: "managed_policy", "Reading the managed policy from {OVERRIDE_ENV}");
        return keys::parse(&super::source::PlistFileSource::read(std::path::Path::new(&path)));
    }
    #[cfg(target_os = "macos")]
    {
        keys::parse(&super::source::CfPrefsSource::synchronized(crate::config::BUNDLE_ID))
    }
    #[cfg(not(target_os = "macos"))]
    {
        keys::parse(&super::source::NoManagedPrefs)
    }
}

/// A policy change: the one in force before and the one now.
#[derive(Debug)]
struct Change {
    old: Arc<ManagedPolicy>,
    new: Arc<ManagedPolicy>,
}

struct Snapshot {
    policy: Arc<ManagedPolicy>,
    warnings: Vec<String>,
    read_at: Instant,
}

impl Snapshot {
    fn new(parsed: Parsed) -> Self {
        log_warnings(&parsed.warnings);
        Self {
            policy: Arc::new(parsed.policy),
            warnings: parsed.warnings,
            read_at: Instant::now(),
        }
    }
}

fn log_warnings(warnings: &[String]) {
    for warning in warnings {
        log::warn!(target: "managed_policy", "{warning}");
    }
}

/// The lazily loaded, refreshable policy. A struct (not bare statics) so tests can build one over a
/// fake loader and prove the lazy first read.
struct PolicyCache {
    load: fn() -> Parsed,
    state: OnceLock<RwLock<Snapshot>>,
    /// Serializes reads, so callers racing for a stale cache make one `cfprefsd` trip, not many.
    refresh_lock: Mutex<()>,
}

impl PolicyCache {
    const fn new(load: fn() -> Parsed) -> Self {
        Self {
            load,
            state: OnceLock::new(),
            refresh_lock: Mutex::new(()),
        }
    }

    /// The snapshot, reading it synchronously on first use. ❗ This lazy read is what makes
    /// `current()` never fail open: there is no "not loaded yet" state to observe.
    fn state(&self) -> &RwLock<Snapshot> {
        self.state.get_or_init(|| RwLock::new(Snapshot::new((self.load)())))
    }

    fn current(&self) -> Arc<ManagedPolicy> {
        Arc::clone(&self.state().read_ignore_poison().policy)
    }

    fn is_fresh(&self, max_age: Duration) -> bool {
        self.state().read_ignore_poison().read_at.elapsed() < max_age
    }

    fn refresh(&self) -> Option<Change> {
        self.reload(None)
    }

    fn refresh_if_older_than(&self, max_age: Duration) -> Option<Change> {
        self.reload(Some(max_age))
    }

    /// Reads the source again (unless a read under `max_age` old already happened while this one
    /// waited its turn) and swaps the result in. Returns the change, if the policy changed.
    fn reload(&self, max_age: Option<Duration>) -> Option<Change> {
        let state = self.state();
        let _turn = self.refresh_lock.lock_ignore_poison();
        if max_age.is_some_and(|max_age| self.is_fresh(max_age)) {
            return None;
        }
        let parsed = (self.load)();

        let mut snapshot = state.write_ignore_poison();
        if parsed.warnings != snapshot.warnings {
            log_warnings(&parsed.warnings);
            snapshot.warnings = parsed.warnings;
        }
        snapshot.read_at = Instant::now();
        if *snapshot.policy == parsed.policy {
            return None;
        }
        let new = Arc::new(parsed.policy);
        let old = std::mem::replace(&mut snapshot.policy, Arc::clone(&new));
        Some(Change { old, new })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::UpdatePolicy;
    use crate::managed_policy::keys::{DISABLE_UPDATES, DISABLE_USAGE_STATS};
    use crate::managed_policy::source::FakeSource;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn updates_off() -> Parsed {
        keys::parse(&FakeSource::with(&[(DISABLE_UPDATES, plist::Value::Boolean(true))]))
    }

    #[test]
    fn the_first_read_returns_the_real_policy_not_the_default() {
        let cache = PolicyCache::new(updates_off);
        assert_eq!(cache.current().updates(), UpdatePolicy::Disabled);
    }

    static READS: AtomicUsize = AtomicUsize::new(0);

    /// Each read returns the next policy in a fixed script: usage stats off, then also updates off.
    fn scripted() -> Parsed {
        let n = READS.fetch_add(1, Ordering::SeqCst);
        let mut entries = vec![(DISABLE_USAGE_STATS, plist::Value::Boolean(true))];
        if n >= 1 {
            entries.push((DISABLE_UPDATES, plist::Value::Boolean(true)));
        }
        keys::parse(&FakeSource::with(&entries))
    }

    #[test]
    fn refresh_reports_a_change_once_and_keeps_the_rest_quiet() {
        let cache = PolicyCache::new(scripted);
        assert!(cache.current().usage_stats_disabled());
        assert!(!matches!(cache.current().updates(), UpdatePolicy::Disabled));

        let change = cache.refresh().expect("the second read changes the policy");
        assert!(!matches!(change.old.updates(), UpdatePolicy::Disabled));
        assert_eq!(change.new.updates(), UpdatePolicy::Disabled);
        assert_eq!(cache.current().updates(), UpdatePolicy::Disabled);

        assert!(cache.refresh().is_none(), "the same policy again is no change");
    }

    #[test]
    fn an_egress_read_inside_the_window_reuses_the_last_one() {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        fn counting() -> Parsed {
            COUNT.fetch_add(1, Ordering::SeqCst);
            keys::parse(&FakeSource::default())
        }
        let cache = PolicyCache::new(counting);
        let _ = cache.current();
        assert_eq!(COUNT.load(Ordering::SeqCst), 1);

        assert!(cache.is_fresh(Duration::from_secs(60)));
        assert!(cache.refresh_if_older_than(Duration::from_secs(60)).is_none());
        assert_eq!(COUNT.load(Ordering::SeqCst), 1, "a fresh cache isn't re-read");

        assert!(!cache.is_fresh(Duration::ZERO));
        assert!(cache.refresh_if_older_than(Duration::ZERO).is_none());
        assert_eq!(COUNT.load(Ordering::SeqCst), 2, "a stale cache is re-read");
    }
}
