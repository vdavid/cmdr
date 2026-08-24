//! The one definition of "this process is not a real user's production install".
//!
//! Several subsystems must never let Cmdr's own dev, test, and capture runs reach a production
//! service: analytics heartbeats and events (`crate::analytics`) and the update check
//! (`crate::updater`) both write a row keyed to an install. They ask the same question, so they
//! ask it here rather than each keeping a list that drifts.
//!
//! The signal is the PRESENCE of an env var, never its value: `CMDR_E2E_MODE=0` still means a
//! harness composed this environment, and failing closed costs nothing. A production launch
//! (Finder, Dock, Spotlight, the updater's relaunch) sets none of them; every dev, E2E, and
//! capture launcher sets at least one. Per-launcher sources: `docs/tooling/instance-isolation.md`.
//!
//! This module answers "is this a real install", nothing else. A caller decides what to do about
//! it, and may add conditions of its own (analytics also suppresses debug builds; the updater
//! also requires a `.app` bundle).

/// Env vars whose mere presence proves this process is not a real user's production install.
///
/// - `CI`: any CI runner.
/// - `CMDR_INSTANCE_ID`: dev, per-worktree dev, and every E2E shard. Prod leaves it unset by
///   definition, which makes it the single strongest signal.
/// - `CMDR_DATA_DIR`: an isolated data dir. Prod resolves `app_data_dir()` instead, and an
///   isolated dir is exactly what mints a fresh install id. It also covers
///   `scripts/marketing-shots.ts`, which deliberately sets no other hook.
/// - `CMDR_E2E_MODE`: the Playwright and Linux Docker E2E lanes, plus `scripts/i18n-capture.ts`.
/// - `CMDR_MOCK_FDA`: the FDA mock. Only a harness ever sets it, and it's what made 1,550 phantom
///   installs report `fdaGranted: true` on their first-ever launch.
pub const NON_PROD_ENV_VARS: &[&str] = &[
    "CI",
    "CMDR_INSTANCE_ID",
    "CMDR_DATA_DIR",
    "CMDR_E2E_MODE",
    "CMDR_MOCK_FDA",
];

/// Returns the first of [`NON_PROD_ENV_VARS`] that `env_is_set` reports as present.
///
/// `env_is_set` is injected so the whole matrix is unit-testable without mutating the process
/// environment, which a parallel test runner can't do safely.
pub fn non_prod_env_var_in(env_is_set: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    NON_PROD_ENV_VARS.iter().copied().find(|name| env_is_set(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn found(vars: &[&str]) -> Option<&'static str> {
        let set: HashSet<&str> = vars.iter().copied().collect();
        non_prod_env_var_in(&|name| set.contains(name))
    }

    #[test]
    fn clean_env_is_a_production_install() {
        assert_eq!(found(&[]), None);
    }

    #[test]
    fn each_non_prod_env_var_is_found_alone() {
        for name in NON_PROD_ENV_VARS {
            assert_eq!(found(&[name]), Some(*name), "{name} alone must mark a non-prod install");
        }
    }

    /// Pinned by name so shrinking the list is a deliberate, visible act: every one of them was a
    /// live pollution source.
    #[test]
    fn the_list_covers_every_isolation_signal() {
        for name in [
            "CI",
            "CMDR_INSTANCE_ID",
            "CMDR_DATA_DIR",
            "CMDR_E2E_MODE",
            "CMDR_MOCK_FDA",
        ] {
            assert!(NON_PROD_ENV_VARS.contains(&name), "{name} must stay in the list");
        }
    }

    /// The env each tooling launcher actually stamps. If a launcher's env stops tripping this,
    /// that harness starts reaching production services again, so pin all of them here.
    #[test]
    fn every_tooling_launcher_is_recognized() {
        // `scripts/check/checks/desktop-svelte-e2e-playwright.go` and `e2e-playwright-app.go`.
        let e2e_checker = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR", "CMDR_E2E_MODE", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/i18n-capture.ts`.
        let i18n_capture = ["CMDR_E2E_MODE", "CMDR_DATA_DIR", "CMDR_MOCK_FDA"];
        // `apps/desktop/scripts/marketing-shots.ts` deliberately leaves `CMDR_E2E_MODE` unset.
        let marketing_shots = ["CMDR_DATA_DIR"];
        // `apps/desktop/scripts/tauri-wrapper.ts` (dev and per-worktree dev).
        let dev_wrapper = ["CMDR_INSTANCE_ID", "CMDR_DATA_DIR"];

        for (label, vars) in [
            ("e2e checker", &e2e_checker[..]),
            ("i18n capture", &i18n_capture[..]),
            ("marketing shots", &marketing_shots[..]),
            ("dev wrapper", &dev_wrapper[..]),
        ] {
            assert!(found(vars).is_some(), "{label} must not read as a production install");
        }
    }
}
