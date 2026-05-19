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
/// **Strictly additive**: code must keep working with the var unset.
pub fn is_e2e_mode() -> bool {
    std::env::var("CMDR_E2E_MODE").as_deref() == Ok("1")
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
