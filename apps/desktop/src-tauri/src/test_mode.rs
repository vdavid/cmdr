//! Centralized helpers for **soft** E2E test hooks driven by environment variables.
//!
//! ## Convention
//!
//! We split test hooks along two axes:
//!
//! - **Hard hooks** (changes the binary shape) live behind Cargo features, e.g. `playwright-e2e`,
//!   `virtual-mtp`, `smb-e2e`. They add commands, plugins, or alternative backends and are compiled
//!   out of production binaries.
//! - **Soft hooks** (runtime-only) live behind environment variables read by this module. They are
//!   **strictly additive**: they may add a delay, skip a non-essential step, or emit extra
//!   telemetry, but they must never replace production logic. With the env var unset, the code path
//!   is exactly what production runs.
//!
//! The canonical env vars handled here are documented in
//! `docs/testing.md` § "E2E env-var hooks". New soft hooks should be wired
//! through helpers in this file rather than reading env vars from random call
//! sites, that way the convention stays discoverable and the list of test
//! hooks is grep-able from one place.
//!
//! Reading an unset env var is cheap (single syscall on Linux/macOS, cached by
//! libc on most platforms), but for hooks called in tight loops we still
//! recommend caching the parsed result behind an `AtomicU64` or similar. The
//! `COPY_THROTTLE_OVERRIDE` static below is the canonical shape, set via the
//! `set_test_throttle` IPC command from a test, read on every copy loop tick.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};

/// Runtime override for the per-file copy throttle, settable via the
/// `set_test_throttle` IPC command (feature-gated to `playwright-e2e`).
///
/// `-1` means "no override; use `CMDR_E2E_COPY_THROTTLE_MS` instead". Any
/// non-negative value is the throttle in milliseconds. Stored as `i64` so we
/// can encode the "unset" sentinel without a separate flag.
static COPY_THROTTLE_OVERRIDE: AtomicI64 = AtomicI64::new(-1);

/// Runtime override for the scan-preview start delay, settable via the
/// `set_test_scan_preview_delay` IPC command (feature-gated to
/// `playwright-e2e`). Same `-1`-means-unset shape as the copy throttle above.
static SCAN_PREVIEW_DELAY_OVERRIDE: AtomicI64 = AtomicI64::new(-1);

/// Sets the IPC-driven copy throttle override.
///
/// `None` clears the override and falls back to `CMDR_E2E_COPY_THROTTLE_MS`.
/// `Some(ms)` pins the copy loop to that per-file delay. Used by E2E specs
/// that need a known window in which to click Cancel/Rollback.
pub fn set_copy_throttle_override(ms: Option<u64>) {
    let v = match ms {
        Some(n) => n.min(i64::MAX as u64) as i64,
        None => -1,
    };
    COPY_THROTTLE_OVERRIDE.store(v, Ordering::Relaxed);
}

/// Returns the effective per-file copy throttle: IPC override wins, then the
/// `CMDR_E2E_COPY_THROTTLE_MS` env var, then `None`.
pub fn effective_copy_throttle_ms() -> Option<u64> {
    let override_val = COPY_THROTTLE_OVERRIDE.load(Ordering::Relaxed);
    if override_val >= 0 {
        return Some(override_val as u64);
    }
    e2e_copy_throttle_ms()
}

/// Runtime override for the per-item ROLLBACK throttle, settable via the
/// `set_test_rollback_throttle` IPC command (feature-gated to `playwright-e2e`).
/// Same `-1`-means-unset shape as the copy throttle above.
///
/// Separate from the copy throttle on purpose: a spec that wants a window inside a
/// REVERSAL would otherwise have to slow every copy in the process down to get one,
/// including the copy it staged the reversal with.
static ROLLBACK_THROTTLE_OVERRIDE: AtomicI64 = AtomicI64::new(-1);

/// The `CMDR_E2E_ROLLBACK_THROTTLE_MS` fallback, parsed once.
///
/// Cached rather than re-read per call, unlike its copy-throttle sibling: the copy
/// loop's tick costs a file's worth of I/O, while a rollback item can be a single
/// `unlink`, and the engine streams up to a million of them. One `LazyLock` deref
/// per item is a price worth paying; a `getenv` plus a `String` allocation per item
/// is not.
///
/// Gated on [`is_e2e_mode`], so a stray variable in a production environment can
/// never pace a user's rollback. The IPC override above needs no such gate: its
/// command doesn't exist outside `playwright-e2e` builds.
static ROLLBACK_THROTTLE_ENV_MS: LazyLock<Option<u64>> = LazyLock::new(|| {
    if !is_e2e_mode() {
        return None;
    }
    std::env::var("CMDR_E2E_ROLLBACK_THROTTLE_MS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|ms| *ms > 0)
});

/// Sets the IPC-driven rollback throttle override.
///
/// `None` clears it and falls back to `CMDR_E2E_ROLLBACK_THROTTLE_MS`. `Some(ms)`
/// pauses the rollback engine that long before each item it reverses, which is what
/// gives an E2E spec a known window in which to watch a reversal run and press
/// Cancel on it. `Some(0)` reads as "no pause": a zero-length sleep would still
/// yield to the runtime on every item.
pub fn set_rollback_throttle_override(ms: Option<u64>) {
    let v = match ms {
        Some(n) => n.min(i64::MAX as u64) as i64,
        None => -1,
    };
    ROLLBACK_THROTTLE_OVERRIDE.store(v, Ordering::Relaxed);
}

/// How long the rollback engine pauses before reversing each item: the IPC override
/// wins, then `CMDR_E2E_ROLLBACK_THROTTLE_MS`, then nothing.
///
/// `None` in every real build, and `None` is the production path: the call site
/// (`operation_log::rollback::execute_rollback`) does nothing at all with it.
pub fn effective_rollback_throttle_ms() -> Option<u64> {
    let override_val = ROLLBACK_THROTTLE_OVERRIDE.load(Ordering::Relaxed);
    if override_val > 0 {
        return Some(override_val as u64);
    }
    if override_val == 0 {
        return None;
    }
    *ROLLBACK_THROTTLE_ENV_MS
}

/// Paces the rollback engine at `ms` per item for one test, and clears the pacing
/// on drop.
///
/// The override is ONE process-wide value and `cargo test` runs a crate's tests as
/// threads in one process, so two tests setting it at once would each un-pace the
/// other's reversal — and the one measuring a window would fail for a reason
/// nothing inside it points at. Taking the lock serializes them instead. Held
/// across awaits, hence the async mutex.
#[cfg(test)]
pub(crate) async fn pace_rollback_for_test(ms: u64) -> RollbackPacing {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let held = LOCK.lock().await;
    set_rollback_throttle_override(Some(ms));
    RollbackPacing { _held: held }
}

/// The pacing [`pace_rollback_for_test`] holds. Keep it on the stack for as long
/// as the reversal under test runs.
#[cfg(test)]
pub(crate) struct RollbackPacing {
    _held: tokio::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for RollbackPacing {
    fn drop(&mut self) {
        set_rollback_throttle_override(None);
    }
}

/// `CMDR_E2E_MODE=1` signals that the running binary is under an E2E run.
/// Subsystems may use this to enable diagnostics or skip behaviors that don't
/// make sense during automated tests (popping the AI offer, mDNS, etc.).
///
/// On macOS it also keeps the run's windows out of the developer's way: the app
/// sets `ActivationPolicy::Prohibited` (so it can never become active, in
/// `crate::run`) and every window is ordered to the back without focus (see
/// `crate::commands::window_ordering::show_main_window` and `order_window_to_back`). Tests
/// drive the app over the playwright socket, never OS input, so a backgrounded
/// window passes every test while no longer stealing keystrokes.
///
/// **Strictly additive**: code must keep working with the var unset.
pub fn is_e2e_mode() -> bool {
    std::env::var("CMDR_E2E_MODE").as_deref() == Ok("1")
}

/// `CMDR_E2E_ASK_CMDR_FAKE` routes the Ask Cmdr send path through the deterministic
/// scripted fake LLM (`agent::resolve_agent_llm`), so the rail's send-and-render can
/// be tested with no real provider. This is the single source of truth for "the fake
/// backend is serving": both `resolve_agent_llm` (which answers the send) and the
/// composer's provider gate (which allows the send, via the `ask_cmdr_fake_active`
/// command) key off it, so the two can't disagree.
///
/// **Strictly additive**: code must keep working with the var unset.
pub fn ask_cmdr_fake_active() -> bool {
    std::env::var("CMDR_E2E_ASK_CMDR_FAKE").is_ok()
}

/// Which script the WAKE slot's fake assistant plays, set by the `playwright-e2e`
/// `force_agent_wake` command.
///
/// A wake can end three materially different ways, and each has a surface a spec has to be
/// able to reach: an ordinary reply (a thread appears), nothing to suggest (the thread goes
/// away again), and a staged proposal (the toast). None of them is reachable from a scripted
/// reply alone, because two of the three are decided by a TOOL CALL.
///
/// **Strictly additive**: it only picks between scripts of a fake that exists solely under
/// `CMDR_E2E_ASK_CMDR_FAKE`. Left at its default, the wake path is exactly what it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeFakeScript {
    /// "I had a look at what changed." A thread, and nothing staged.
    Reply,
    /// Calls `nothing_to_suggest`, so the wake deletes its own thread.
    Quiet,
    /// Calls `propose_suggestions`, so the wake leaves something to review.
    Propose,
}

/// The selected script, as its discriminant. An atomic rather than a lock: it is written by
/// an IPC command and read on the wake thread, and neither may wait on the other.
static WAKE_FAKE_SCRIPT: AtomicU8 = AtomicU8::new(0);

/// Point the next wake's scripted fake at one of the three scripts.
pub fn set_wake_fake_script(script: WakeFakeScript) {
    let value = match script {
        WakeFakeScript::Reply => 0,
        WakeFakeScript::Quiet => 1,
        WakeFakeScript::Propose => 2,
    };
    WAKE_FAKE_SCRIPT.store(value, Ordering::Relaxed);
}

/// Which script the wake slot's fake should play. Always [`WakeFakeScript::Reply`] in
/// production: only the feature-gated force-wake command ever sets it, and the fake itself is
/// unreachable without `CMDR_E2E_ASK_CMDR_FAKE`.
pub fn wake_fake_script() -> WakeFakeScript {
    match WAKE_FAKE_SCRIPT.load(Ordering::Relaxed) {
        1 => WakeFakeScript::Quiet,
        2 => WakeFakeScript::Propose,
        _ => WakeFakeScript::Reply,
    }
}

/// Whether the app may adopt network mounts that were already on the machine when
/// it launched.
///
/// False under E2E, because those mounts can only be the developer's own. Every
/// fixture share an E2E run uses is mounted AFTER the app is up (`setupSmb` runs
/// when a spec file loads, and the suite connects to a running app), so the
/// startup adopter has nothing of the test's to find and everything of the
/// developer's: on this machine it reliably finds `/Volumes/naspi`, waits on mDNS
/// for it, reaches for its Keychain entry, opens an smb2 session to a NAS on the
/// real LAN, and raises a toast about it that then fails whichever spec is running.
///
/// The wider rule this serves: **a test run must not observe or react to the
/// developer's real machine.** Anything the app discovers rather than creates is a
/// candidate — the real-USB half of MTP enumeration is the known remaining one
/// (`mtp/watcher.rs`, `docs/testing.md` § "The host machine is not a fixture"). Note
/// that MTP's `ptpcamerad` suppression is gated on the DEVICE being virtual rather
/// than on this flag, which covers a `CMDR_VIRTUAL_MTP=1` dev session too.
pub fn may_adopt_preexisting_network_mounts() -> bool {
    !is_e2e_mode()
}

/// Pure core of [`guard_e2e_requires_data_dir`]: true when E2E mode is on but no usable
/// `CMDR_DATA_DIR` is set. Empty is treated as unset, matching `config::data_dir_from_env`.
fn e2e_data_dir_missing(is_e2e: bool, data_dir: Option<&str>) -> bool {
    is_e2e && data_dir.map(str::is_empty).unwrap_or(true)
}

/// Hard guard against an E2E run leaking persisted state into the developer's real production
/// data dir. Call this once at the very top of `crate::run`, before anything resolves a data dir.
///
/// `CMDR_E2E_MODE=1` with no `CMDR_DATA_DIR` resolves every persisted store (favorites, settings,
/// secrets, analytics, install id, go-to history) to the OS-default prod dir, since each subsystem
/// falls back there independently (`favorites/store.rs`, `settings/loader.rs`, `secrets/mod.rs`,
/// `install_id.rs`, …). A manually launched E2E app that then mutates state (e.g. `favorites.add`
/// during a screenshot capture) writes straight into prod. Production never sets `CMDR_E2E_MODE`,
/// and every real harness sets `CMDR_DATA_DIR`, so this combination is always a misconfiguration:
/// fail fast rather than silently corrupt the developer's prod state.
pub fn guard_e2e_requires_data_dir() {
    if e2e_data_dir_missing(is_e2e_mode(), std::env::var("CMDR_DATA_DIR").ok().as_deref()) {
        panic!(
            "CMDR_E2E_MODE=1 requires CMDR_DATA_DIR to be set to an isolated path. Without it, \
             persisted state (favorites, settings, secrets) would write to your real production \
             data dir. Set CMDR_DATA_DIR=/tmp/cmdr-e2e-data (or another throwaway path) and relaunch."
        );
    }
}

/// Pure core of [`e2e_downloads_dir`]. Under E2E this ALWAYS resolves to some
/// isolated path and never to `None`, because `None` is what sends the caller
/// back to the developer's real `~/Downloads`.
fn e2e_downloads_dir_from(is_e2e: bool, override_dir: Option<&str>, data_dir: Option<&str>) -> Option<PathBuf> {
    if !is_e2e {
        return None;
    }
    // A spec that needs to place its own files picks the dir explicitly.
    if let Some(dir) = override_dir.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    // Otherwise nest it under the run's throwaway data dir, so it's created and
    // discarded with the run and can't collide with a concurrent one.
    if let Some(dir) = data_dir.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir).join("downloads"));
    }
    // Unreachable while `guard_e2e_requires_data_dir` runs at startup, kept
    // because the safe answer here is a throwaway path, not the real Downloads.
    Some(PathBuf::from("/tmp/cmdr-e2e-downloads"))
}

/// The Downloads root an E2E run watches instead of the developer's real
/// `~/Downloads`, or `None` in production.
///
/// Set `CMDR_E2E_DOWNLOADS_DIR` to steer it explicitly; otherwise it lands at
/// `$CMDR_DATA_DIR/downloads`. Without this the E2E app watches the real
/// Downloads folder, so any browser download during a run emits a
/// `download-detected` toast into whatever spec happens to be mid-flight, and
/// the overlay-leak guard fails that spec with a message about a file the test
/// never touched.
pub fn e2e_downloads_dir() -> Option<PathBuf> {
    e2e_downloads_dir_from(
        is_e2e_mode(),
        std::env::var("CMDR_E2E_DOWNLOADS_DIR").ok().as_deref(),
        std::env::var("CMDR_DATA_DIR").ok().as_deref(),
    )
}

/// Parses `CMDR_E2E_COPY_THROTTLE_MS` into milliseconds, or `None` when unset
/// or invalid. The copy loop calls this once per file (between committing one
/// and starting the next) to give E2E specs a deterministic window in which
/// to click Cancel/Rollback without staging 170 MB of bulk fixtures.
///
/// Reading the env var on every iteration is fine: the value only matters
/// under E2E, and the syscall is in the noise next to a real file copy.
pub fn e2e_copy_throttle_ms() -> Option<u64> {
    std::env::var("CMDR_E2E_COPY_THROTTLE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
}

/// How long every scan-preview worker waits at its starting line before it
/// walks: the `set_test_scan_preview_delay` IPC override wins, then
/// `CMDR_E2E_SCAN_PREVIEW_DELAY_MS`, then nothing.
///
/// E2E fixture trees are deliberately tiny, so a scan over one finishes faster
/// than a test can click anything, and `data-scan-state` signals "counting
/// done" — the opposite of what a test about the scanning phase needs to hold.
/// This gives such a test a deterministic window instead of a race against a
/// 40-file fixture. Returns `None` outside `CMDR_E2E_MODE`, so an
/// accidentally-set var or override can never slow production down.
pub fn e2e_scan_preview_delay_ms() -> Option<u64> {
    if !is_e2e_mode() {
        return None;
    }
    let override_val = SCAN_PREVIEW_DELAY_OVERRIDE.load(Ordering::Relaxed);
    if override_val > 0 {
        return Some(override_val as u64);
    }
    if override_val == 0 {
        return None;
    }
    std::env::var("CMDR_E2E_SCAN_PREVIEW_DELAY_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|ms| *ms > 0)
}

/// Sets the IPC-driven scan-preview delay override. `None` clears it and falls
/// back to `CMDR_E2E_SCAN_PREVIEW_DELAY_MS`. Per-test rather than per-process,
/// so one spec's scanning window doesn't slow down the whole run.
pub fn set_scan_preview_delay_override(ms: Option<u64>) {
    let v = match ms {
        Some(n) => n.min(i64::MAX as u64) as i64,
        None => -1,
    };
    SCAN_PREVIEW_DELAY_OVERRIDE.store(v, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test mode reads exactly `"1"`. Anything else is off. This guards the
    /// `as_deref() == Ok("1")` shape (replacing it with `is_ok()` would let
    /// `CMDR_E2E_MODE=0` accidentally enable test mode in CI, where the
    /// variable is sometimes set to `0` to explicitly disable).
    ///
    /// `serial_test`-free: we never mutate the env in the test; we just read
    /// what's there and assert the helper's parse rules through a private
    /// re-implementation. The helper is a one-liner so the surface is small.
    #[test]
    fn is_e2e_mode_parses_exactly_one() {
        // We can't safely mutate env in a parallel test runner. Instead, mirror
        // the helper's parsing logic and confirm the matrix matches.
        fn matches_one(s: &str) -> bool {
            Ok(s) == Ok::<&str, ()>("1")
        }
        assert!(matches_one("1"));
        assert!(!matches_one("0"));
        assert!(!matches_one(""));
        assert!(!matches_one("true"));
        // Real helper: with the var (likely) unset, it returns false.
        // We don't assert this; it depends on the test environment. The
        // mirror above is what we actually want pinned.

        // Reference call to keep the helper from being dead-coded out of
        // test builds; the result is environment-dependent so we don't assert.
        let _ = is_e2e_mode();
    }

    /// Adopting the machine's pre-existing network mounts is exactly "not under E2E".
    /// Pinned as its own assertion because the gate reads as a niche startup
    /// optimization at its call site, while what it actually protects is that a test
    /// run can't reach the developer's NAS. Someone tidying the call site needs the
    /// failing test to tell them that.
    #[test]
    fn preexisting_network_mounts_are_adopted_outside_e2e_only() {
        assert_eq!(may_adopt_preexisting_network_mounts(), !is_e2e_mode());
    }

    /// The data-dir guard fires only when E2E mode is on AND no usable `CMDR_DATA_DIR`
    /// is set (empty counts as unset). Every real harness sets the var, so they pass; a
    /// bare `CMDR_E2E_MODE=1` manual launch (the prod-bleed footgun) is the one that trips.
    #[test]
    fn e2e_data_dir_missing_only_when_e2e_and_no_dir() {
        // E2E off: never fires, regardless of the data dir.
        assert!(!e2e_data_dir_missing(false, None));
        assert!(!e2e_data_dir_missing(false, Some("")));
        assert!(!e2e_data_dir_missing(false, Some("/tmp/cmdr-e2e-data")));
        // E2E on with a real path: passes.
        assert!(!e2e_data_dir_missing(true, Some("/tmp/cmdr-e2e-data")));
        // E2E on with unset or empty: the violation we guard against.
        assert!(e2e_data_dir_missing(true, None));
        assert!(e2e_data_dir_missing(true, Some("")));
    }

    /// Under E2E the Downloads root is ALWAYS an isolated path, never `None`.
    /// `None` is the answer that sends `resolved_downloads_dir` on to
    /// `dirs::download_dir()`, i.e. the developer's real `~/Downloads` — which
    /// is how a real browser download once failed an unrelated viewer spec by
    /// leaking a toast into it. Production (E2E off) must still get `None`.
    #[test]
    fn e2e_downloads_dir_is_isolated_and_never_falls_back() {
        // Production: always None, whatever the vars say.
        assert_eq!(e2e_downloads_dir_from(false, None, None), None);
        assert_eq!(e2e_downloads_dir_from(false, Some("/tmp/x"), Some("/tmp/data")), None);

        // E2E with an explicit override wins.
        assert_eq!(
            e2e_downloads_dir_from(true, Some("/tmp/spec-downloads"), Some("/tmp/data")),
            Some(PathBuf::from("/tmp/spec-downloads"))
        );

        // E2E with only a data dir: nested under it, so it dies with the run.
        assert_eq!(
            e2e_downloads_dir_from(true, None, Some("/tmp/cmdr-e2e-data")),
            Some(PathBuf::from("/tmp/cmdr-e2e-data/downloads"))
        );

        // Empty strings count as unset, matching `e2e_data_dir_missing`.
        assert_eq!(
            e2e_downloads_dir_from(true, Some(""), Some("/tmp/cmdr-e2e-data")),
            Some(PathBuf::from("/tmp/cmdr-e2e-data/downloads"))
        );

        // The invariant that matters: E2E on with nothing configured still
        // yields an isolated path rather than `None`. The startup guard should
        // make this unreachable, but if it ever regresses, the failure mode
        // must not be "watch the developer's real Downloads".
        let bare = e2e_downloads_dir_from(true, None, None);
        assert!(bare.is_some(), "E2E must never resolve Downloads to None");
        assert_ne!(
            bare,
            dirs::download_dir(),
            "E2E must never resolve to the real Downloads"
        );
    }

    /// Same shape: `e2e_copy_throttle_ms` should return `None` for unset,
    /// non-numeric, or empty values, and `Some(n)` for valid `u64` strings.
    #[test]
    fn copy_throttle_ms_parses_numbers() {
        fn parse(s: Option<&str>) -> Option<u64> {
            s.and_then(|s| s.parse().ok())
        }
        assert_eq!(parse(None), None);
        assert_eq!(parse(Some("")), None);
        assert_eq!(parse(Some("abc")), None);
        assert_eq!(parse(Some("0")), Some(0));
        assert_eq!(parse(Some("200")), Some(200));
        // Reference call to ensure the public helper survives `#![deny(unused)]`.
        let _ = e2e_copy_throttle_ms();
    }

    /// The rollback throttle round-trips through its own override, independent of
    /// the copy throttle: a spec that wants a window inside a REVERSAL must not
    /// have to slow every copy in the process down to get one.
    #[test]
    fn rollback_throttle_override_round_trip() {
        let prior = ROLLBACK_THROTTLE_OVERRIDE.load(Ordering::Relaxed);

        set_rollback_throttle_override(Some(25));
        assert_eq!(effective_rollback_throttle_ms(), Some(25));
        // The two throttles are separate knobs; setting one leaves the other alone.
        assert_ne!(effective_copy_throttle_ms(), Some(25));

        // Zero is "no pause", not "pause for zero": a sleep of 0 would still yield
        // to the runtime on every item of a million-item reversal.
        set_rollback_throttle_override(Some(0));
        assert_eq!(effective_rollback_throttle_ms(), None);

        set_rollback_throttle_override(None);
        assert_eq!(
            effective_rollback_throttle_ms(),
            None,
            "cleared, and the env fallback is inert outside E2E mode"
        );

        ROLLBACK_THROTTLE_OVERRIDE.store(prior, Ordering::Relaxed);
    }

    /// The IPC-set override beats the env var; clearing it goes back to env.
    /// The override is process-global, so this test is serial within the same
    /// process. We restore the state to `-1` (unset) at the end so other tests
    /// see the same baseline.
    #[test]
    fn copy_throttle_override_round_trip() {
        // Save and restore the override so this test is safe to run in any order.
        let prior = COPY_THROTTLE_OVERRIDE.load(Ordering::Relaxed);

        set_copy_throttle_override(Some(150));
        assert_eq!(effective_copy_throttle_ms(), Some(150));

        set_copy_throttle_override(Some(0));
        assert_eq!(effective_copy_throttle_ms(), Some(0));

        set_copy_throttle_override(None);
        // With the override cleared, we fall back to whatever the env says.
        // We don't assert the exact result because the env is test-runner-dependent;
        // we only assert the call doesn't panic and behaves as documented.
        let _ = effective_copy_throttle_ms();

        COPY_THROTTLE_OVERRIDE.store(prior, Ordering::Relaxed);
    }
}
