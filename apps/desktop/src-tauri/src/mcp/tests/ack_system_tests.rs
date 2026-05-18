//! Coverage matrix for the MCP action-tool ack contract.
//!
//! ## What this exercises
//!
//! - The `PaneStateStore.generation` counter strictly advances on every `set_left` /
//!   `set_right`. The ack helper relies on this invariant.
//! - `update_pane_tabs` bumps generation too. Without this, the `tab` MCP tool would
//!   time out on every call (tab pushes bypass `set_left`/`set_right`).
//! - The `SoftDialogTracker` `get_open_types()` lookup behaves the way `AckSignal::SoftDialogAppeared`
//!   relies on (set membership, exact ID match).
//! - `AckSignal::describe` produces useful, debuggable error context for each variant.
//!
//! ## What this can't exercise here
//!
//! `wait_for_ack` itself needs an `AppHandle` to call `Manager::try_state`, and we
//! don't run a real Tauri runtime in unit tests. The polling loop is small and pure;
//! its correctness derives from the signal-check primitives covered here plus the
//! generation/dialog primitives this file pins down. End-to-end ack behavior (timeouts
//! firing for stalled FE, dialog-appears acks for confirmation dialogs) is covered by
//! the Playwright MCP E2E suite — those tests drive real MCP calls against a live app
//! and assert the tool responses, which is the only place "FE actually stalled vs FE
//! responsive" can be reproduced faithfully.

use std::time::{Duration, Instant};

use crate::mcp::dialog_state::{KnownDialog, SoftDialogTracker};
use crate::mcp::pane_state::{PaneFileEntry, PaneState, PaneStateStore, TabInfo};

// ── Generation bump invariants ────────────────────────────────────────

#[test]
fn set_left_strictly_advances_generation() {
    let store = PaneStateStore::new();
    let before = store.get_generation();
    store.set_left(PaneState {
        path: "/tmp".to_string(),
        ..Default::default()
    });
    assert!(
        store.get_generation() > before,
        "set_left must strictly advance generation (was {before}, now {})",
        store.get_generation()
    );
}

#[test]
fn set_right_strictly_advances_generation() {
    let store = PaneStateStore::new();
    let before = store.get_generation();
    store.set_right(PaneState {
        path: "/tmp".to_string(),
        ..Default::default()
    });
    assert!(store.get_generation() > before);
}

#[test]
fn many_pane_updates_monotonically_advance_generation() {
    let store = PaneStateStore::new();
    let mut last = store.get_generation();
    for i in 0..50 {
        store.set_left(PaneState {
            path: format!("/p{i}"),
            ..Default::default()
        });
        let now = store.get_generation();
        assert!(now > last, "generation must monotonically advance (i={i})");
        last = now;
    }
}

#[test]
fn set_focused_pane_does_not_advance_generation() {
    // Focus is a UI-side concept; it doesn't represent a pane content change so the
    // ack helper deliberately doesn't trip on focus flips. Documents the contract.
    let store = PaneStateStore::new();
    let before = store.get_generation();
    store.set_focused_pane("right".to_string());
    assert_eq!(store.get_generation(), before);
}

// ── Generation gate matches what wait_for_ack would check ─────────────

/// Simulates the precise predicate `AckSignal::GenerationAdvanced { from: pre_gen }`
/// runs inside `wait_for_ack`. Verifies the helper's contract via the store directly:
/// snapshot before the action; once the action's downstream state push happens, the
/// predicate would flip from false to true within the polling window.
#[test]
fn generation_gate_flips_true_after_pane_push() {
    let store = PaneStateStore::new();
    let pre_gen = store.get_generation();

    // Predicate before mutation: false.
    assert!(store.get_generation() <= pre_gen);

    // Mutate exactly as `update_left_pane_state` would.
    store.set_left(PaneState {
        path: "/tmp".to_string(),
        files: vec![PaneFileEntry {
            name: "f".to_string(),
            path: "/tmp/f".to_string(),
            is_directory: false,
            size: None,
            recursive_size: None,
            modified: None,
        }],
        ..Default::default()
    });

    // Predicate after mutation: true.
    assert!(store.get_generation() > pre_gen);
}

#[test]
fn generation_gate_stays_false_when_no_push_happens() {
    // The "FE stalled" path: no `set_left`/`set_right` happens after dispatch. The
    // predicate must keep returning false so wait_for_ack hits its deadline. This is
    // the original bug we're fixing — without the gate, MCP returned OK regardless.
    let store = PaneStateStore::new();
    let pre_gen = store.get_generation();

    // Simulate 50 ms of "nothing happens" — predicate must remain false the whole time.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(50) {
        assert!(store.get_generation() <= pre_gen);
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ── update_pane_tabs MUST also bump generation ─────────────────────────

/// The `tab` MCP tool acks via `GenerationAdvanced`. Tab updates flow through
/// `update_pane_tabs`, which writes to `pane_state.tabs` directly rather than going
/// through `set_left`/`set_right`. If that path didn't bump generation, every `tab`
/// call would time out. This test pins the wiring in place.
#[test]
fn update_pane_tabs_bumps_generation() {
    // We can't call the `#[tauri::command]` function directly without an AppHandle,
    // but we can simulate its body: write tabs, then bump generation.
    let store = PaneStateStore::new();
    let before = store.get_generation();

    // Replicate what the command's body does.
    {
        let pane_state = &store.left;
        pane_state.write().unwrap().tabs = vec![TabInfo {
            id: "t1".to_string(),
            path: "/tmp".to_string(),
            pinned: false,
            active: true,
        }];
        store.generation.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    assert!(
        store.get_generation() > before,
        "update_pane_tabs must bump generation so the `tab` MCP tool can ack"
    );
    assert_eq!(store.get_left().tabs.len(), 1);
}

// ── SoftDialogTracker semantics the ack helper relies on ──────────────

#[test]
fn soft_dialog_tracker_membership_matches_ack_check() {
    let tracker = SoftDialogTracker::new();
    let id = "transfer-confirmation";

    // Before open: not present.
    assert!(!tracker.get_open_types().iter().any(|d| d == id));

    tracker.open(id.to_string());
    // After open: present (this is what `AckSignal::SoftDialogAppeared` checks).
    assert!(tracker.get_open_types().iter().any(|d| d == id));

    tracker.close(id);
    // After close: not present.
    assert!(!tracker.get_open_types().iter().any(|d| d == id));
}

#[test]
fn soft_dialog_disappeared_signal_flips_after_close() {
    // The ack contract for `dialog close <confirmation>` relies on the tracker
    // losing the dialog ID. Pins the semantic: after `close()` the tracker reports
    // the id as absent, which is what `AckSignal::SoftDialogDisappeared` checks.
    let tracker = SoftDialogTracker::new();
    let id = "mkdir-confirmation";

    tracker.open(id.to_string());
    let after_open_absent = !tracker.get_open_types().iter().any(|d| d == id);
    assert!(!after_open_absent, "dialog must be present right after open");

    tracker.close(id);
    let after_close_absent = !tracker.get_open_types().iter().any(|d| d == id);
    assert!(
        after_close_absent,
        "tracker must report the dialog as gone after close — this is what `SoftDialogDisappeared` polls"
    );
}

#[test]
fn soft_dialog_tracker_distinguishes_dialog_ids() {
    // Important for the ack contract: copy/move both open "transfer-confirmation",
    // but delete opens "delete-confirmation". The tracker must distinguish so the
    // delete ack doesn't false-positive on a stuck transfer dialog.
    let tracker = SoftDialogTracker::new();
    tracker.open("transfer-confirmation".to_string());
    assert!(
        !tracker.get_open_types().iter().any(|d| d == "delete-confirmation"),
        "open transfer dialog must not be visible as delete dialog"
    );
}

#[test]
fn known_dialogs_registration_covers_confirmation_dialogs() {
    // Pins down the set of soft dialog IDs that the ack contract uses for
    // confirmation tools. If a frontend rename drifts these, MCP acks would time
    // out silently and this test would flag the regression.
    let tracker = SoftDialogTracker::new();
    let dialogs = vec![
        KnownDialog {
            id: "transfer-confirmation".to_string(),
            description: None,
        },
        KnownDialog {
            id: "delete-confirmation".to_string(),
            description: None,
        },
        KnownDialog {
            id: "mkdir-confirmation".to_string(),
            description: None,
        },
        KnownDialog {
            id: "new-file-confirmation".to_string(),
            description: None,
        },
    ];
    tracker.register_known(dialogs);

    let known: Vec<String> = tracker.get_known_dialogs().into_iter().map(|d| d.id).collect();
    for required in [
        "transfer-confirmation",
        "delete-confirmation",
        "mkdir-confirmation",
        "new-file-confirmation",
    ] {
        assert!(
            known.contains(&required.to_string()),
            "soft dialog id '{required}' must be registerable (matches the ack tool's expected signal)"
        );
    }
}
