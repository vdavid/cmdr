//! Direct mode: the switch that lets an outside MCP client use the tools that go past a person.
//!
//! On in an E2E run of a build that honors it, or when the person turned on
//! `developer.mcpDirectMode` in Settings. The agent never gets it. The decision record, and what the
//! Keychain item does and doesn't prove: `DETAILS.md` § Direct mode.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;

use super::actions::ToolError;
use super::registry::{Access, Consumer};
use crate::secrets::{SecretStore, SecretStoreError};
use crate::test_mode::{self, AutomatedRun};

/// The setting a refusal names, so a client's author knows what to turn on.
const DIRECT_MODE_SETTING_ID: &str = "developer.mcpDirectMode";

/// The token a refusal carries in `data.reason`.
const DIRECT_MODE_OFF: &str = "directModeOff";

/// The `secrets` item that backs the setting. `settings.json` is display only, since any same-user
/// process can write it. ❌ Keep it out of reach of every other secret's key: a key an outsider
/// chooses must never land on this name.
const DIRECT_MODE_ITEM: &str = "mcp.directMode";
const DIRECT_MODE_ITEM_VALUE: &[u8] = b"on";

/// Whether this build lets an automated run turn direct mode on. `CMDR_E2E_MODE` is readable in
/// every build, so a release build ignoring it is what keeps an environment variable from being a
/// way past a person. The same guard as `secrets::PLAINTEXT_OVERRIDE_HONORED`.
const E2E_HONORED: bool = cfg!(any(debug_assertions, feature = "playwright-e2e"));

/// The person's setting, as this process believes it. Written only by [`seed_direct_mode_at_launch`]
/// and [`set_direct_mode`], and read per request, so a toggle applies to the next call.
static DIRECT_MODE_SETTING: AtomicBool = AtomicBool::new(false);

/// Whether an outside client is in direct mode right now.
pub fn direct_mode() -> bool {
    direct_mode_from(
        E2E_HONORED,
        test_mode::automated_run(),
        DIRECT_MODE_SETTING.load(Ordering::Relaxed),
    )
}

/// The person's setting alone, without an automated run: what the Settings row shows.
pub fn direct_mode_setting() -> bool {
    DIRECT_MODE_SETTING.load(Ordering::Relaxed)
}

/// Pure core of [`direct_mode`]. `e2e_honored` is [`E2E_HONORED`], passed in so tests can cover the
/// release arm. Any run counts, `capture` included: it's an E2E run that also takes screenshots.
fn direct_mode_from(e2e_honored: bool, run: AutomatedRun, setting: bool) -> bool {
    (e2e_honored && run != AutomatedRun::None) || setting
}

/// The refusal for a `DevOnly` tool an outside client asks for off direct mode. The agent's own
/// fence is its `consumers` list and gate, so it never reaches this.
pub(super) fn dev_only_refusal(
    consumer: Consumer,
    name: &str,
    access: Option<Access>,
    direct: bool,
) -> Option<ToolError> {
    let gated = consumer == Consumer::AiClient && matches!(access, Some(Access::DevOnly(_)));
    (gated && !direct).then(|| {
        ToolError::invalid_params(format!(
            "The {name} tool needs direct mode. A person can turn it on in Cmdr, in Settings > AI > MCP server."
        ))
        .with_data(json!({ "reason": DIRECT_MODE_OFF, "setting": DIRECT_MODE_SETTING_ID }))
    })
}

/// What launch makes of the saved setting: on only when `settings.json` says on AND the store's item
/// backs it. Both closures run only when the JSON says on, so nobody else pays at launch: asking
/// whether the store is file-backed opens it, which on Linux is a keyring probe. A file-backed store
/// can't vouch for anything, so there the toggle lasts one session.
fn direct_mode_at_launch(
    json_on: bool,
    file_backed: impl FnOnce() -> bool,
    item_backs_it: impl FnOnce() -> bool,
) -> bool {
    json_on && !file_backed() && item_backs_it()
}

/// Whether the store's item says direct mode is on. A refused read means the item is there and Cmdr
/// can't show it wrote it, so it doesn't count.
fn item_backs_direct_mode(store: &dyn SecretStore) -> bool {
    match store.get(DIRECT_MODE_ITEM) {
        Ok(value) => value == DIRECT_MODE_ITEM_VALUE,
        Err(SecretStoreError::NotFound(_)) => {
            log::warn!(target: "mcp::direct_mode", "The saved setting says on, with no item behind it, so direct mode stays off");
            false
        }
        Err(error) => {
            log::warn!(target: "mcp::direct_mode", "Direct mode stays off, since its item doesn't read as Cmdr's own: {error}");
            false
        }
    }
}

/// Reads the saved setting into the atomic, before the MCP server starts. Records a launch that came
/// up in direct mode.
pub fn seed_direct_mode_at_launch(saved: Option<bool>) {
    let on = direct_mode_at_launch(saved == Some(true), crate::secrets::is_file_backed, || {
        item_backs_direct_mode(crate::secrets::store().vouching())
    });
    DIRECT_MODE_SETTING.store(on, Ordering::Relaxed);
    if let Some(event) = launch_event(on) {
        crate::security_log::record(event);
    }
}

fn launch_event(on: bool) -> Option<crate::security_log::SecurityEvent> {
    on.then_some(crate::security_log::SecurityEvent::McpDirectModeChanged {
        on: true,
        by: crate::security_log::Initiator::Launch,
    })
}

/// What a toggle did. `survives_restart` is false when the setting lasts this session only, because
/// the store can't back it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectModeChange {
    pub changed: bool,
    pub survives_restart: bool,
}

/// Turns direct mode on or off for this session and writes the item that backs it across a restart.
/// Off takes effect first, so a store that refuses can't keep it on.
pub fn set_direct_mode(on: bool) -> DirectModeChange {
    let was = DIRECT_MODE_SETTING.swap(on, Ordering::Relaxed);
    let survives_restart =
        persist_direct_mode(crate::secrets::store().vouching(), crate::secrets::is_file_backed(), on);
    DirectModeChange {
        changed: was != on,
        survives_restart,
    }
}

/// Writes or removes the item. Answers whether what's stored now matches `on`.
fn persist_direct_mode(store: &dyn SecretStore, file_backed: bool, on: bool) -> bool {
    if !on {
        return match store.delete(DIRECT_MODE_ITEM) {
            Ok(()) | Err(SecretStoreError::NotFound(_)) => true,
            Err(error) => {
                log::warn!(target: "mcp::direct_mode", "Couldn't remove direct mode's item: {error}");
                false
            }
        };
    }
    if file_backed {
        return false;
    }
    match store.set(DIRECT_MODE_ITEM, DIRECT_MODE_ITEM_VALUE) {
        Ok(()) => true,
        Err(error) => {
            log::warn!(target: "mcp::direct_mode", "Direct mode lasts this session only: {error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNS: [AutomatedRun; 3] = [AutomatedRun::None, AutomatedRun::E2e, AutomatedRun::Capture];

    #[test]
    fn a_release_build_ignores_an_automated_run() {
        // `CMDR_E2E_MODE` is readable in every build, so honoring it there would make an
        // environment variable a way past a person.
        for run in RUNS {
            assert!(!direct_mode_from(false, run, false), "{run:?}");
        }
    }

    #[test]
    fn an_e2e_or_capture_run_is_direct_in_a_build_that_honors_it() {
        assert!(direct_mode_from(true, AutomatedRun::E2e, false));
        assert!(direct_mode_from(true, AutomatedRun::Capture, false));
        assert!(!direct_mode_from(true, AutomatedRun::None, false));
    }

    #[test]
    fn the_setting_works_in_every_build_and_every_run() {
        for honored in [false, true] {
            for run in RUNS {
                assert!(direct_mode_from(honored, run, true), "{honored} {run:?}");
            }
        }
    }

    /// The whole launch truth table, with the release guard in force.
    #[test]
    fn launch_comes_up_direct_only_when_the_json_and_the_item_agree() {
        let release = |json: bool, file_backed: bool, item: bool| {
            let setting = direct_mode_at_launch(json, || file_backed, || item);
            direct_mode_from(false, AutomatedRun::None, setting)
        };
        assert!(release(true, false, true), "JSON on, item there");
        assert!(!release(true, false, false), "JSON on, item absent");
        assert!(!release(false, false, true), "JSON off, item there");
        assert!(!release(false, false, false), "both off");
        assert!(!release(true, true, true), "a file-backed store vouches for nothing");
    }

    #[test]
    fn launch_reads_the_item_only_when_the_json_says_on() {
        // A locked Keychain raises a password prompt, so a person who never turned this on must
        // not pay for it at every launch.
        let on = direct_mode_at_launch(
            false,
            || panic!("the store was opened with the JSON off"),
            || panic!("the item was read with the JSON off"),
        );
        assert!(!on);
        let on = direct_mode_at_launch(true, || true, || panic!("a file-backed store's item was read"));
        assert!(!on);
    }

    #[test]
    fn a_dev_only_tool_refuses_an_outside_client_off_direct_mode_with_the_typed_reason() {
        let access = Some(Access::DevOnly("a reason"));
        let refusal = dev_only_refusal(Consumer::AiClient, "resolve_conflict", access, false).expect("refused");
        assert_eq!(
            refusal.data,
            Some(json!({ "reason": "directModeOff", "setting": "developer.mcpDirectMode" }))
        );
        assert!(dev_only_refusal(Consumer::AiClient, "resolve_conflict", access, true).is_none());
    }

    #[test]
    fn only_a_dev_only_tool_is_gated_this_way() {
        for access in [
            Some(Access::Read),
            Some(Access::ReadContents),
            Some(Access::Sensitive),
            Some(Access::Initiate),
            None,
        ] {
            assert!(
                dev_only_refusal(Consumer::AiClient, "x", access, false).is_none(),
                "{access:?}"
            );
        }
        // The agent's fence is its own: direct mode is never its answer, on or off.
        assert!(dev_only_refusal(Consumer::Agent, "x", Some(Access::DevOnly("r")), false).is_none());
    }

    #[test]
    fn the_item_round_trips_and_turning_off_removes_it() {
        let _guard = crate::test_support::isolate_secrets();
        let store = crate::secrets::store().vouching();

        assert!(!item_backs_direct_mode(store), "nothing is stored yet");
        assert!(persist_direct_mode(store, false, true));
        assert!(item_backs_direct_mode(store));
        assert!(persist_direct_mode(store, false, false));
        assert!(!item_backs_direct_mode(store), "off removes it");
        assert!(persist_direct_mode(store, false, false), "off while off is fine");
    }

    #[test]
    fn a_file_backed_store_writes_nothing_and_says_the_setting_lasts_this_session() {
        let _guard = crate::test_support::isolate_secrets();
        let store = crate::secrets::store().vouching();

        assert!(!persist_direct_mode(store, true, true));
        assert!(!item_backs_direct_mode(store));
    }

    #[test]
    fn an_item_with_some_other_value_backs_nothing() {
        let _guard = crate::test_support::isolate_secrets();
        let store = crate::secrets::store().vouching();

        store.set(DIRECT_MODE_ITEM, b"yes").expect("the test store writes");
        assert!(!item_backs_direct_mode(store));
    }

    #[test]
    fn only_a_launch_that_came_up_direct_is_recorded() {
        use crate::security_log::{Initiator, SecurityEvent};
        assert_eq!(
            launch_event(true),
            Some(SecurityEvent::McpDirectModeChanged {
                on: true,
                by: Initiator::Launch
            })
        );
        assert_eq!(launch_event(false), None);
    }
}
