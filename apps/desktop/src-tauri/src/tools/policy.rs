//! Who may run what: [`admit`], the one decision, over who's calling ([`Caller`], [`ToolCtx`]) and
//! what a tool does ([`Access`]). And direct mode: the switch that lets an outside MCP client use the
//! tools that go past a person.
//!
//! Direct mode is on in an E2E run of a build that honors it, or when the person turned on
//! `developer.mcpDirectMode` in Settings. The agent never gets it. The decision record, and what the
//! Keychain item does and doesn't prove: `DETAILS.md` § Direct mode.

use std::sync::atomic::{AtomicBool, Ordering};

use cmdr_operation_log::types::Initiator;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::actions::ToolError;
use super::ask::AskFacts;
use super::registry::Access;
use crate::agent::types::TurnOrigin;
use crate::secrets::{SecretStore, SecretStoreError};
use crate::security_log::{ContentTool, SecurityEvent};
use crate::test_mode::{self, AutomatedRun};

/// Who's calling a tool. Decides what [`admit`] answers, what a dialog says about who started it, what
/// the operation log records, and whether a stray keystroke can touch what opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// Ask Cmdr, on a turn with this origin. It never gets direct mode.
    Agent { origin: TurnOrigin },
    /// An outside MCP client. `direct` is [`direct_mode`] as this request read it.
    External { direct: bool },
}

/// What [`admit`] answers: the same three answers as Claude Code's permission modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Run the tool's `ask` plan first, and act only on a person's yes.
    Ask,
    Refuse(Refusal),
}

/// Why [`admit`] refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The tool needs a turn the user started, and Cmdr started this one.
    InBackground,
    /// Never the agent, for this one-line reason from the table.
    DevOnly(&'static str),
    /// An outside client needs direct mode for this.
    DirectModeOff,
}

/// Who may run what. **The agent can start anything. A person finishes anything sensitive.** Pure, and
/// the only thing deciding: dispatch reads the answer and owns the asking. The agent's arms never read
/// direct mode, since it never gets it. Why each cell is what it is: `DETAILS.md` § The rule.
pub fn admit(caller: Caller, access: Access) -> Verdict {
    match (caller, access) {
        (Caller::Agent { .. }, Access::DevOnly(reason)) => Verdict::Refuse(Refusal::DevOnly(reason)),
        (Caller::Agent { origin }, Access::Sensitive) if origin.user_started() => Verdict::Ask,
        (Caller::Agent { origin }, Access::Session | Access::AppState | Access::Initiate | Access::LightWrite)
            if origin.user_started() =>
        {
            Verdict::Allow
        }
        (Caller::Agent { .. }, Access::Read | Access::ReadContents | Access::Propose | Access::Memory) => {
            Verdict::Allow
        }
        (
            Caller::Agent { .. },
            Access::Session | Access::AppState | Access::Initiate | Access::LightWrite | Access::Sensitive,
        ) => Verdict::Refuse(Refusal::InBackground),
        (Caller::External { direct: true }, _) => Verdict::Allow,
        (Caller::External { direct: false }, Access::Sensitive) => Verdict::Ask,
        (Caller::External { direct: false }, Access::ReadContents | Access::Memory | Access::DevOnly(_)) => {
            Verdict::Refuse(Refusal::DirectModeOff)
        }
        (
            Caller::External { direct: false },
            Access::Read | Access::Propose | Access::Session | Access::AppState | Access::Initiate | Access::LightWrite,
        ) => Verdict::Allow,
    }
}

impl Refusal {
    /// The token a caller branches on.
    pub fn token(self) -> &'static str {
        match self {
            Refusal::InBackground => "refusedInBackground",
            Refusal::DevOnly(_) => "devOnly",
            Refusal::DirectModeOff => DIRECT_MODE_OFF,
        }
    }

    /// The sentence the caller reads, naming what's possible instead.
    pub fn message(self, name: &str) -> String {
        match self {
            Refusal::InBackground => {
                "This tool runs only when the user asks for something, and Cmdr started this turn \
                                      on its own. Say what you'd do; the user can ask for it."
                    .to_string()
            }
            Refusal::DevOnly(reason) => format!(
                "Ask Cmdr can't run {name}: {reason}. Say what you'd like the user to do; they answer it in Cmdr."
            ),
            Refusal::DirectModeOff => format!(
                "The {name} tool needs direct mode. A person can turn it on in Cmdr, in Settings > AI > MCP server."
            ),
        }
    }

    /// The refusal as a tool answer. `data` is the contract; the message is the human half.
    pub fn into_error(self, name: &str) -> ToolError {
        let data = match self {
            Refusal::DirectModeOff => json!({ "reason": DIRECT_MODE_OFF, "setting": DIRECT_MODE_SETTING_ID }),
            Refusal::InBackground | Refusal::DevOnly(_) => json!({ "refusal": self.token() }),
        };
        ToolError::invalid_params(self.message(name)).with_data(data)
    }
}

/// Records that an outside client read file contents through Cmdr, by tool and never by path. The agent
/// has the cloud AI disclosure and the operation of its own rail; an outside client has only this.
pub(super) fn note_content_read(caller: Caller, name: &str, access: Access) {
    if !matches!((caller, access), (Caller::External { .. }, Access::ReadContents)) {
        return;
    }
    if let Some(tool) = content_tool(name) {
        crate::security_log::record(SecurityEvent::McpContentRead { tool });
    }
}

/// The log's word for a tool that reads contents.
fn content_tool(name: &str) -> Option<ContentTool> {
    match name {
        "inspect_file" => Some(ContentTool::InspectFile),
        "image_facts" => Some(ContentTool::ImageFacts),
        "search_photos" => Some(ContentTool::SearchPhotos),
        "multi_rename" => Some(ContentTool::MultiRename),
        _ => None,
    }
}

impl Caller {
    /// Who the operation log credits with what this caller starts, whoever confirms it.
    pub fn initiator(self) -> Initiator {
        match self {
            Caller::Agent { .. } => Initiator::Agent,
            Caller::External { .. } => Initiator::AiClient,
        }
    }

    /// Whether what this caller opens must arm before a keystroke can touch it, since someone may be
    /// typing elsewhere as it appears. Direct mode is the one way out: it means a person said this
    /// client may go past them.
    pub fn guarded(self) -> bool {
        !matches!(self, Caller::External { direct: true })
    }
}

/// What a tool call knows about who asked and how to stop. A handler that needs it takes the
/// `ctx_params` shape in the table.
#[derive(Debug, Clone)]
pub struct ToolCtx {
    pub caller: Caller,
    /// The turn's Stop for the agent, the client going away for an outside client.
    pub cancel: CancellationToken,
    /// The chat thread that asked. An outside client has none.
    pub conversation_id: Option<i64>,
    /// What a person just agreed to, when the table's `ask` plan put a question to them. A handler
    /// that acts on a fact it can't re-check in one step (`open_under_cursor`'s path) reads it here.
    pub asked: Option<AskFacts>,
}

impl ToolCtx {
    /// An outside client's call, under direct mode as it stands right now.
    pub fn external(cancel: CancellationToken) -> Self {
        Self {
            caller: Caller::External { direct: direct_mode() },
            cancel,
            conversation_id: None,
            asked: None,
        }
    }

    /// `payload`, an object, plus what the frontend needs to know about who's asking: `initiator`
    /// rides through to the operation log, and `guard` decides whether what opens must arm first.
    pub fn stamp(&self, mut payload: Value) -> Value {
        payload["initiator"] = serde_json::to_value(self.caller.initiator()).unwrap_or(Value::Null);
        payload["guard"] = self.caller.guarded().into();
        payload
    }
}

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

/// A reset of `settings.json` takes the item with it, so a reset setting stays reset. This is
/// for the recovery that replaces the file; the Settings row's own reset goes through
/// [`set_direct_mode`].
fn forget_item_after_settings_reset(store: &dyn SecretStore) {
    // allowed-discarded-outcome: it logs a refusal, and launch has nobody to answer
    persist_direct_mode(store, false, false);
}

/// Reads the saved setting into the atomic, before the MCP server starts. Records a launch that came
/// up in direct mode.
pub fn seed_direct_mode_at_launch(saved: Option<bool>) {
    if crate::state_recovery::settings_file_recovered() {
        forget_item_after_settings_reset(crate::secrets::store().vouching());
    }
    let on = direct_mode_at_launch(saved == Some(true), crate::secrets::is_file_backed, || {
        item_backs_direct_mode(crate::secrets::store().vouching())
    });
    DIRECT_MODE_SETTING.store(on, Ordering::Relaxed);
    if let Some(event) = launch_event(on) {
        crate::security_log::record(event);
    }
}

fn launch_event(on: bool) -> Option<SecurityEvent> {
    on.then_some(SecurityEvent::McpDirectModeChanged {
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
    const AGENT: Caller = Caller::Agent {
        origin: TurnOrigin::Text,
    };
    const OFF_DIRECT: Caller = Caller::External { direct: false };
    const DIRECT: Caller = Caller::External { direct: true };

    #[test]
    fn the_log_credits_whoever_started_it() {
        for origin in [
            TurnOrigin::Text,
            TurnOrigin::Wake,
            TurnOrigin::Outcomes,
            TurnOrigin::Resume,
        ] {
            assert_eq!(Caller::Agent { origin }.initiator(), Initiator::Agent);
        }
        assert_eq!(OFF_DIRECT.initiator(), Initiator::AiClient);
        assert_eq!(DIRECT.initiator(), Initiator::AiClient);
    }

    #[test]
    fn only_direct_mode_opens_without_the_guard() {
        for origin in [
            TurnOrigin::Text,
            TurnOrigin::Wake,
            TurnOrigin::Outcomes,
            TurnOrigin::Resume,
        ] {
            assert!(Caller::Agent { origin }.guarded(), "{origin:?}");
        }
        assert!(OFF_DIRECT.guarded());
        assert!(!DIRECT.guarded());
    }

    #[test]
    fn a_stamped_payload_says_who_asked_in_the_frontends_words() {
        let ctx = |caller| ToolCtx {
            caller,
            cancel: CancellationToken::new(),
            conversation_id: None,
            asked: None,
        };
        assert_eq!(
            ctx(AGENT).stamp(json!({ "pane": "left" })),
            json!({ "pane": "left", "initiator": "agent", "guard": true })
        );
        assert_eq!(
            ctx(OFF_DIRECT).stamp(json!({})),
            json!({ "initiator": "aiClient", "guard": true })
        );
        assert_eq!(
            ctx(DIRECT).stamp(json!({})),
            json!({ "initiator": "aiClient", "guard": false })
        );
    }

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

    /// Every class, by hand, so a new one can't reach the matrix unseen (`every_class` is exhaustive).
    const EVERY_CLASS: [Access; 10] = [
        Access::Read,
        Access::ReadContents,
        Access::Propose,
        Access::Memory,
        Access::Session,
        Access::AppState,
        Access::Initiate,
        Access::LightWrite,
        Access::Sensitive,
        Access::DevOnly("a reason"),
    ];

    /// Fails to compile when a class joins [`Access`], which sends you to [`EVERY_CLASS`].
    fn _every_class(access: Access) {
        match access {
            Access::Read
            | Access::ReadContents
            | Access::Propose
            | Access::Memory
            | Access::Session
            | Access::AppState
            | Access::Initiate
            | Access::LightWrite
            | Access::Sensitive
            | Access::DevOnly(_) => {}
        }
    }

    const ALL_ORIGINS: [TurnOrigin; 4] = [
        TurnOrigin::Text,
        TurnOrigin::Resume,
        TurnOrigin::Wake,
        TurnOrigin::Outcomes,
    ];

    /// The whole matrix, cell by cell, from the rule rather than from the code: every class × every
    /// caller.
    #[test]
    fn admit_answers_the_whole_matrix() {
        for access in EVERY_CLASS {
            for origin in ALL_ORIGINS {
                let want = match access {
                    Access::DevOnly(reason) => Verdict::Refuse(Refusal::DevOnly(reason)),
                    Access::Read | Access::ReadContents | Access::Propose | Access::Memory => Verdict::Allow,
                    Access::Sensitive if origin.user_started() => Verdict::Ask,
                    _ if origin.user_started() => Verdict::Allow,
                    _ => Verdict::Refuse(Refusal::InBackground),
                };
                assert_eq!(
                    admit(Caller::Agent { origin }, access),
                    want,
                    "{access:?} on {origin:?}"
                );
            }

            assert_eq!(admit(DIRECT, access), Verdict::Allow, "direct: {access:?}");

            let want = match access {
                Access::Sensitive => Verdict::Ask,
                Access::ReadContents | Access::Memory | Access::DevOnly(_) => Verdict::Refuse(Refusal::DirectModeOff),
                _ => Verdict::Allow,
            };
            assert_eq!(admit(OFF_DIRECT, access), want, "off direct: {access:?}");
        }
    }

    #[test]
    fn only_a_user_started_turn_counts_for_the_agent_and_resume_is_one() {
        assert!(TurnOrigin::Text.user_started());
        assert!(TurnOrigin::Resume.user_started());
        assert!(!TurnOrigin::Wake.user_started());
        assert!(!TurnOrigin::Outcomes.user_started());
    }

    #[test]
    fn a_direct_mode_refusal_names_the_setting_in_its_typed_data() {
        for access in [Access::DevOnly("a reason"), Access::ReadContents, Access::Memory] {
            let Verdict::Refuse(refusal) = admit(OFF_DIRECT, access) else {
                panic!("{access:?} refuses off direct mode");
            };
            assert_eq!(
                refusal.into_error("resolve_conflict").data,
                Some(json!({ "reason": "directModeOff", "setting": "developer.mcpDirectMode" }))
            );
        }
    }

    #[test]
    fn the_agents_refusals_carry_a_token_and_say_what_it_can_do_instead() {
        let error = Refusal::InBackground.into_error("copy");
        assert_eq!(error.data, Some(json!({ "refusal": "refusedInBackground" })));

        let error = Refusal::DevOnly("the question is already in front of the person").into_error("resolve_conflict");
        assert_eq!(error.data, Some(json!({ "refusal": "devOnly" })));
        assert_eq!(
            error.message,
            "Ask Cmdr can't run resolve_conflict: the question is already in front of the person. Say what you'd like \
             the user to do; they answer it in Cmdr."
        );
    }

    #[test]
    fn every_contents_tool_has_a_word_in_the_log() {
        for tool in crate::tools::ToolId::KNOWN {
            let name = tool.as_wire_name();
            if crate::tools::tool_access(name) == Some(Access::ReadContents) {
                assert!(content_tool(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn only_an_outside_clients_content_read_is_logged_and_only_by_tool() {
        let capture = crate::security_log::testing::capture_events();
        note_content_read(AGENT, "inspect_file", Access::ReadContents);
        note_content_read(DIRECT, "list_dir", Access::Read);
        note_content_read(DIRECT, "inspect_file", Access::ReadContents);
        assert_eq!(
            capture.events(),
            vec![SecurityEvent::McpContentRead {
                tool: ContentTool::InspectFile
            }]
        );
    }

    /// The annotations are the rule in MCP's words, so a client that trusts them skips no prompt it
    /// shouldn't.
    #[test]
    fn annotations_follow_the_class() {
        for access in EVERY_CLASS {
            let hints = access.hints();
            match access {
                Access::Read | Access::ReadContents => {
                    assert_eq!(hints.read_only_hint, Some(true), "{access:?}");
                }
                Access::Initiate | Access::Sensitive | Access::DevOnly(_) => {
                    assert_eq!(hints.destructive_hint, Some(true), "{access:?}");
                }
                _ => assert_eq!(hints.destructive_hint, Some(false), "{access:?}"),
            }
        }
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
    fn a_recovered_settings_file_takes_the_item_with_it() {
        let _guard = crate::test_support::isolate_secrets();
        let store = crate::secrets::store().vouching();
        assert!(persist_direct_mode(store, false, true));

        forget_item_after_settings_reset(store);

        assert!(
            !item_backs_direct_mode(store),
            "nothing is left to vouch for a later JSON value"
        );
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
        use crate::security_log::Initiator;
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
