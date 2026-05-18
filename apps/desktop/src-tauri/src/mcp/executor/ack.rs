//! Action-tool ack contract.
//!
//! MCP "action" tools used to return `OK` the instant they dispatched an event to the
//! frontend. If the FE was stalled (modal blocking input, error pane up, race during
//! startup), the action was silently dropped but the tool still reported success.
//! Real QA hit this. To make MCP a trustworthy automation surface, every fire-and-forget
//! action now waits for a small ack signal before returning.
//!
//! ## Signals
//!
//! - `GenerationAdvanced`: the `PaneStateStore` generation counter strictly advanced past
//!   a captured value. Use this for actions that mutate pane state (navigation, refresh,
//!   selection, view mode, sort, tabs, cursor moves, auto-confirmed copy/move/delete).
//! - `SoftDialogAppeared`: a soft (overlay) dialog with the given ID appeared in the
//!   `SoftDialogTracker`. Use this for confirmation dialogs (transfer, delete, mkdir,
//!   mkfile) when `autoConfirm: false`.
//! - `WindowAppeared` / `WindowDisappeared`: a Tauri webview window with the given label
//!   prefix appeared (or vanished). Use this for child windows (settings, file-viewer,
//!   about) and for `dialog close` actions.
//! - `Any`: succeeds when any of the inner signals fire (logical OR). Used by
//!   `open_under_cursor` where opening a directory bumps the pane generation but opening
//!   a file launches a viewer window.
//!
//! ## Timeout
//!
//! Default `DEFAULT_ACK_TIMEOUT` = 1500 ms. Not exposed as an MCP-tool parameter —
//! MCP clients shouldn't have to tune this, the value is a backend-side latency
//! budget. Tunable per-call via the `Duration` argument to `wait_for_ack`.
//!
//! ## Decision/Why
//!
//! Polling cadence matches the existing `await` tool (250 ms for state checks, 100 ms
//! for window checks, since window state changes are typically faster than full pane
//! refreshes). The two loops aren't unified into a shared `poll_until` core yet: the
//! `await` tool exposes a few extra knobs (per-pane conditions, after_generation gate,
//! rich match summaries) that don't apply here, and the ack helper's loop is ~15 lines.
//! Extracting now would be premature abstraction. Revisit if we add a third polling
//! site or if the `await` tool grows AckSignal-shaped conditions.

use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

use super::ToolError;
use crate::mcp::dialog_state::SoftDialogTracker;
use crate::mcp::pane_state::PaneStateStore;

/// Default ack budget. Backend-side latency budget; not a client-facing knob.
pub const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_millis(1500);

/// Polling cadence for state-driven signals. Matches the existing `await` tool.
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Polling cadence for window/dialog appearance signals. Windows show up faster than
/// full pane state pushes, so we poll a bit tighter for snappier acks.
const WINDOW_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// What the backend should wait for to consider an action "actually processed."
pub enum AckSignal {
    /// State generation strictly advanced past `from`.
    GenerationAdvanced { from: u64 },
    /// A soft dialog with this ID appeared in `SoftDialogTracker`.
    SoftDialogAppeared(&'static str),
    /// A Tauri webview window whose label equals (or starts with, for viewers)
    /// the given pattern appeared.
    WindowAppeared(&'static str),
    /// A Tauri webview window matching the pattern vanished.
    WindowDisappeared(&'static str),
    /// Succeeds when any inner signal fires. Used for tools where the ack
    /// kind depends on what got opened (for example `open_under_cursor`).
    Any(Vec<AckSignal>),
}

impl AckSignal {
    /// Human-readable description for error messages.
    fn describe(&self) -> String {
        match self {
            AckSignal::GenerationAdvanced { from } => {
                format!("pane state generation > {from}")
            }
            AckSignal::SoftDialogAppeared(id) => format!("soft dialog '{id}' opened"),
            AckSignal::WindowAppeared(label) => format!("window '{label}' opened"),
            AckSignal::WindowDisappeared(label) => format!("window '{label}' closed"),
            AckSignal::Any(signals) => {
                let parts: Vec<String> = signals.iter().map(|s| s.describe()).collect();
                format!("any of [{}]", parts.join(", "))
            }
        }
    }
}

/// Wait for an ack signal to arrive within `timeout`.
///
/// On success returns `Ok(())`. On timeout returns a `ToolError::internal` whose message
/// names the missing signal and the elapsed budget, so callers can surface a useful
/// failure rather than a false-positive OK.
pub async fn wait_for_ack<R: Runtime>(
    app: &AppHandle<R>,
    signal: AckSignal,
    timeout: Duration,
) -> Result<(), ToolError> {
    let start = tokio::time::Instant::now();
    let deadline = start + timeout;

    // Pick the tighter cadence if any leaf signal is window-driven; this matters
    // for `Any` mixtures (open_under_cursor) where we want to react to a viewer
    // window as fast as a pane generation bump.
    let poll_interval = if signal_uses_windows(&signal) {
        WINDOW_POLL_INTERVAL
    } else {
        STATE_POLL_INTERVAL
    };

    loop {
        if check_signal(app, &signal) {
            return Ok(());
        }

        if tokio::time::Instant::now() >= deadline {
            let elapsed_ms = start.elapsed().as_millis();
            return Err(ToolError::internal(format!(
                "Action not acknowledged by backend within {} ms (waiting for: {}). The frontend may be stalled (modal blocking input, error pane up, race during startup). Inspect cmdr://state to triage.",
                elapsed_ms,
                signal.describe()
            )));
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Check whether the signal is currently satisfied. Pure read; no side effects.
fn check_signal<R: Runtime>(app: &AppHandle<R>, signal: &AckSignal) -> bool {
    match signal {
        AckSignal::GenerationAdvanced { from } => app
            .try_state::<PaneStateStore>()
            .map(|store| store.get_generation() > *from)
            .unwrap_or(false),
        AckSignal::SoftDialogAppeared(id) => app
            .try_state::<SoftDialogTracker>()
            .map(|tracker| tracker.get_open_types().iter().any(|d| d == id))
            .unwrap_or(false),
        AckSignal::WindowAppeared(pattern) => window_matches(app, pattern),
        AckSignal::WindowDisappeared(pattern) => !window_matches(app, pattern),
        AckSignal::Any(signals) => signals.iter().any(|s| check_signal(app, s)),
    }
}

/// True if any Tauri webview window has a label exactly equal to `pattern`,
/// or (for the `viewer` family) starting with `pattern-`.
fn window_matches<R: Runtime>(app: &AppHandle<R>, pattern: &str) -> bool {
    let windows = app.webview_windows();
    // `viewer-` is a label prefix family — each opened file has its own
    // `viewer-<id>` window. Other tracked labels (settings, about) are exact.
    if pattern == "viewer" {
        windows.keys().any(|k| k.starts_with("viewer-"))
    } else {
        windows.contains_key(pattern)
    }
}

/// Whether any leaf in the signal tree references windows. Drives poll cadence.
fn signal_uses_windows(signal: &AckSignal) -> bool {
    match signal {
        AckSignal::WindowAppeared(_) | AckSignal::WindowDisappeared(_) => true,
        AckSignal::Any(signals) => signals.iter().any(signal_uses_windows),
        _ => false,
    }
}

/// Capture the current pane-state generation. Used to build a
/// `GenerationAdvanced { from }` signal just before dispatching an action.
///
/// Returns 0 when the store isn't registered (test contexts); callers wrap the
/// resulting signal in a normal `wait_for_ack` call that will immediately succeed
/// in those cases because the test fixture either bumps generation or skips the wait.
pub fn snapshot_generation<R: Runtime>(app: &AppHandle<R>) -> u64 {
    app.try_state::<PaneStateStore>()
        .map(|store| store.get_generation())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // The signal-checking core is pure (it reads `Manager::try_state` and
    // `webview_windows`), so the only piece we can unit-test without spinning
    // up a Tauri app is the `PaneStateStore` interaction. The `tests/`
    // module covers that case via `tests::ack_system_tests` against a real
    // `PaneStateStore`. The window-driven branch is exercised by E2E tests
    // and by the dialog integration tests.

    #[test]
    fn describe_renders_each_variant() {
        assert!(AckSignal::GenerationAdvanced { from: 42 }.describe().contains("42"));
        assert!(
            AckSignal::SoftDialogAppeared("delete-confirmation")
                .describe()
                .contains("delete-confirmation")
        );
        assert!(AckSignal::WindowAppeared("settings").describe().contains("settings"));
        assert!(AckSignal::WindowDisappeared("settings").describe().contains("settings"));
        let any = AckSignal::Any(vec![
            AckSignal::GenerationAdvanced { from: 1 },
            AckSignal::WindowAppeared("viewer"),
        ]);
        let s = any.describe();
        assert!(s.contains("any of"));
        assert!(s.contains("viewer"));
    }

    #[test]
    fn signal_uses_windows_picks_tighter_cadence() {
        assert!(!signal_uses_windows(&AckSignal::GenerationAdvanced { from: 0 }));
        assert!(signal_uses_windows(&AckSignal::WindowAppeared("settings")));
        assert!(signal_uses_windows(&AckSignal::Any(vec![
            AckSignal::GenerationAdvanced { from: 0 },
            AckSignal::WindowAppeared("viewer"),
        ])));
    }

    // Verifies the core promise: once the generation strictly advances past
    // the snapshot, a future polling for `GenerationAdvanced` would return
    // true. We exercise this through the store directly because we can't
    // construct a real `AppHandle` here.
    #[test]
    fn generation_strictly_advances_after_set_left() {
        let store = Arc::new(PaneStateStore::new());
        let before = store.get_generation();
        // Snapshot before mutation
        let snapshot = before;
        // Mutate
        store.set_left(crate::mcp::pane_state::PaneState {
            path: "/tmp".to_string(),
            ..Default::default()
        });
        assert!(
            store.get_generation() > snapshot,
            "generation should strictly advance after set_left"
        );
    }
}
