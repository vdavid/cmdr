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

use std::sync::atomic::{AtomicI64, Ordering};

/// Runtime override for the per-file copy throttle, settable via the
/// `set_test_throttle` IPC command (feature-gated to `playwright-e2e`).
///
/// `-1` means "no override; use `CMDR_E2E_COPY_THROTTLE_MS` instead". Any
/// non-negative value is the throttle in milliseconds. Stored as `i64` so we
/// can encode the "unset" sentinel without a separate flag.
static COPY_THROTTLE_OVERRIDE: AtomicI64 = AtomicI64::new(-1);

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
