//! E2E test support commands.

/// Returns the `CMDR_E2E_START_PATH` env var if set.
/// The frontend uses this to override startup paths for E2E tests.
/// Always compiled in. Reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn get_e2e_start_path() -> Option<String> {
    std::env::var("CMDR_E2E_START_PATH").ok()
}

/// Returns `true` when running under an E2E harness (`CMDR_E2E_MODE=1`).
/// The frontend uses this to switch the title-bar styling and decorate child
/// window titles so a tester can tell an automated window apart from prod or
/// dev. Always compiled in; reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn is_e2e_mode() -> bool {
    crate::test_mode::is_e2e_mode()
}

/// Returns `true` when the Ask Cmdr send path is served by the deterministic
/// scripted fake LLM (`CMDR_E2E_ASK_CMDR_FAKE`). The composer reads this to treat
/// the fake as an active provider, so send isn't gated off during E2E even though
/// `ai.provider` is `off`. Consults the same accessor `agent::resolve_agent_llm`
/// uses, so the send-allowed and send-answered decisions share one source of truth.
/// Always compiled in; reading an unset env var is a no-op in production.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_fake_active() -> bool {
    crate::test_mode::ask_cmdr_fake_active()
}

/// Returns `true` when `CMDR_FORCE_ONBOARDING` is set, regardless of value.
///
/// The frontend uses this to bypass the `isOnboarded` gate and force the
/// onboarding wizard open on every launch (mirrors `CMDR_MOCK_LICENSE` /
/// `CMDR_E2E_MODE`). Pair with `CMDR_MOCK_FDA` (in `permissions.rs`) to
/// drive each step's variants without ever touching real System Settings.
///
/// Synchronous + no filesystem access, so no `blocking_with_timeout` needed.
#[tauri::command]
#[specta::specta]
pub fn is_force_onboarding() -> bool {
    std::env::var("CMDR_FORCE_ONBOARDING").is_ok()
}

/// Sets the per-file copy throttle (milliseconds) for the next write operation.
///
/// `None` clears the override. Tests use this to slow down the copy loop by a
/// known amount per file so they can click Cancel/Rollback deterministically
/// without staging large fixtures. Feature-gated to `playwright-e2e` so the
/// command isn't available in production binaries.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn set_test_throttle(ms: Option<u64>) -> Result<(), String> {
    crate::test_mode::set_copy_throttle_override(ms);
    Ok(())
}

/// Holds every scan preview at its starting line for `ms` before it walks.
///
/// `None` clears the override. E2E fixture trees are deliberately tiny, so a
/// scan over one finishes before a test can click anything, and
/// `data-scan-state` signals "counting done" — the opposite of what a test
/// about the scanning phase needs to hold. This buys such a test a
/// deterministic window instead of a race against a 40-file fixture.
/// Feature-gated to `playwright-e2e` so the command isn't available in
/// production binaries.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn set_test_scan_preview_delay(ms: Option<u64>) -> Result<(), String> {
    crate::test_mode::set_scan_preview_delay_override(ms);
    Ok(())
}

/// Stages one folder's activity for the proactive agent, then makes it act NOW.
///
/// Verifying the wake loop otherwise means sitting out a deadline (five seconds at the
/// attentive end of the cadence slider, half an hour at the calm one) and hoping the fixture
/// tree is somewhere the indexer walks. This drives the real lane end to end instead: the
/// rollup goes through `send_rollup` like the tap's, the writer thread does the importance
/// lookup and the admit, and `ForceWake` skips the timer and the proactive toggle.
///
/// ⚠️ **A Cargo feature, ❌ not an env-var hook.** `test_mode.rs` draws the line at soft hooks
/// being "strictly additive", and forcing a wake REPLACES the timer. The three gates that
/// protect the user (consent, Full Disk Access, a configured provider) are untouched, so a
/// forced wake on an unconsented profile still stores nothing and runs nothing.
///
/// `folder` names the directory the changes happened IN, absolute, and it is the ONLY folder
/// the wake reports on: the inbox is cut down to it as the wake is prepared, so a spec sees
/// what it staged rather than that plus whatever the indexer's tap picked up from the rest of
/// the suite. `None` forces a wake against everything already waiting and cuts nothing.
///
/// `script` picks which of the three scripts the wake's fake assistant plays: `"reply"` (the
/// ordinary answer), `"quiet"` (it calls `nothing_to_suggest`, deletes its own thread, and the
/// session list must end up untouched), or `"propose"` (it stages a group, so the toast fires).
/// It STICKS until changed, so a spec always says which one it wants. An unknown value reads as
/// `"reply"`, which is the harmless script.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn force_agent_wake(folder: Option<String>, script: Option<String>) -> Result<(), String> {
    use crate::agent::wake::{ForcedWake, WakeControl, send_control};
    use crate::test_mode::WakeFakeScript;

    if let Some(script) = script {
        crate::test_mode::set_wake_fake_script(match script.as_str() {
            "quiet" => WakeFakeScript::Quiet,
            "propose" => WakeFakeScript::Propose,
            _ => WakeFakeScript::Reply,
        });
    }
    // Staged and named from ONE binding: the force tells the loop which folder it may report
    // on, and a second spelling could name one nobody staged.
    if let Some(folder) = folder.as_deref() {
        stage_rollup(folder);
    }
    send_control(WakeControl::ForceWake(ForcedWake { only_folder: folder }));
    Ok(())
}

/// Stages one folder's activity for the proactive agent WITHOUT waking it.
///
/// What the indexer's tap does all run long, on demand. A spec uses it to put something in the
/// inbox that it did NOT stage for its own wake, which is the one premise `force_agent_wake`'s
/// isolation exists to defend and the one a test otherwise cannot reproduce on purpose: on CI
/// it depends on how many files the specs before it happened to churn.
///
/// Feature-gated to `playwright-e2e`, like every other hook here.
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub fn stage_agent_rollup(folder: String) -> Result<(), String> {
    stage_rollup(&folder);
    Ok(())
}

/// Hand one folder's activity to the wake loop the way the indexer's tap does: through
/// `send_rollup`, so the writer thread does the importance lookup and the admit.
#[cfg(feature = "playwright-e2e")]
fn stage_rollup(folder: &str) {
    use crate::agent::wake::{ChangeCounters, FolderActivity, send_rollup};

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    send_rollup(FolderActivity {
        volume_id: "root".to_string(),
        folder: folder.to_string(),
        // Arrivals, so the bundle scores on intent rather than on sheer volume: that is
        // the flagship "something landed in a folder you care about" shape.
        counters: ChangeCounters {
            created: 5,
            ..ChangeCounters::default()
        },
        observed_at: now,
        last_event_at: now,
    });
}

/// Flushes any pending file-watcher events for E2E synchronization.
///
/// The notify-debouncer-full crate buffers events for `DEBOUNCE_MS` (200 ms by
/// default), plus the OS itself coalesces FSEvents on macOS over a longer
/// window, so a single FS mutation can take 1–10 s to land in the UI. For
/// tests, that's pure waste.
///
/// This command sidesteps the debouncer: it iterates every active listing and
/// calls `handle_directory_change` (re-reads via the Volume trait, computes
/// the diff, updates LISTING_CACHE, emits `directory-diff`). After it
/// returns, the frontend has the full delta.
///
/// Feature-gated to `playwright-e2e` so production builds can't accidentally
/// bypass the debouncer (which exists to prevent thrash on bursts of events).
#[cfg(feature = "playwright-e2e")]
#[tauri::command]
#[specta::specta]
pub async fn flush_file_watcher() -> Result<(), String> {
    crate::file_system::flush_all_watchers().await;
    Ok(())
}
