//! Who may run what: [`admit`], the one decision, over who's calling ([`Caller`], [`ToolCtx`]) and
//! what a tool does ([`Access`]). And direct mode: what lets an outside MCP client use the tools that
//! go past a person.
//!
//! Direct mode is on only in a build that honors the environment (debug or `playwright-e2e`), during
//! an automated run or when `CMDR_MCP_DIRECT_MODE=1` asks. A release build is never in it, and the
//! agent never gets it. `DETAILS.md` § Direct mode.

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use cmdr_operation_log::types::Initiator;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::actions::ToolError;
use super::ask::AskFacts;
use super::registry::Access;
use crate::agent::types::TurnOrigin;
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
                "Outside clients can't run {name}. Tell the person what you'd like done, and they'll do it in Cmdr."
            ),
        }
    }

    /// The refusal as a tool answer. `data` is the contract; the message is the human half.
    pub fn into_error(self, name: &str) -> ToolError {
        let data = match self {
            Refusal::DirectModeOff => json!({ "reason": DIRECT_MODE_OFF }),
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
        "about_item" => Some(ContentTool::AboutItem),
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
    /// typing elsewhere as it appears. Direct mode is the one way out: it means a test run or a
    /// developer's environment said this client may go past a person.
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

/// The token a refusal carries in `data.reason`.
const DIRECT_MODE_OFF: &str = "directModeOff";

/// The variable that asks for direct mode outside an automated run, so an agent can drive a
/// `pnpm dev` app fully. `1` is the only value that asks.
const DIRECT_MODE_ENV: &str = "CMDR_MCP_DIRECT_MODE";

/// Whether this build honors the environment's say on direct mode at all. Every variable is
/// readable in every build, so a release build ignoring them is what keeps an environment variable
/// from being a way past a person. The same guard as `secrets::PLAINTEXT_OVERRIDE_HONORED`.
const E2E_HONORED: bool = cfg!(any(debug_assertions, feature = "playwright-e2e"));

/// A test's say on direct mode, so it can show what direct mode changes without touching the
/// process environment other tests read.
#[cfg(test)]
static DIRECT_MODE_FOR_TEST: AtomicBool = AtomicBool::new(false);

/// Whether an outside client is in direct mode right now. Read per request, from the environment
/// only: no setting, no file, and no Keychain item can turn it on.
pub fn direct_mode() -> bool {
    #[cfg(test)]
    if DIRECT_MODE_FOR_TEST.load(Ordering::Relaxed) {
        return true;
    }
    direct_mode_from(
        E2E_HONORED,
        test_mode::automated_run(),
        env_asks_for_direct_mode(std::env::var(DIRECT_MODE_ENV).ok().as_deref()),
    )
}

/// Turns direct mode on or off for this process's tests.
#[cfg(test)]
pub(crate) fn set_direct_mode_for_test(on: bool) {
    DIRECT_MODE_FOR_TEST.store(on, Ordering::Relaxed);
}

fn env_asks_for_direct_mode(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Pure core of [`direct_mode`]. `e2e_honored` is [`E2E_HONORED`], passed in so tests can cover the
/// release arm, where it's always off. Any run counts, `capture` included: it's an E2E run that
/// also takes screenshots.
fn direct_mode_from(e2e_honored: bool, run: AutomatedRun, env_asks: bool) -> bool {
    e2e_honored && (run != AutomatedRun::None || env_asks)
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
    fn a_release_build_is_never_direct_whatever_the_environment_says() {
        // Both variables are readable in every build, so honoring either there would make an
        // environment variable a way past a person.
        for run in RUNS {
            for env_asks in [false, true] {
                assert!(!direct_mode_from(false, run, env_asks), "{run:?} {env_asks}");
            }
        }
    }

    #[test]
    fn an_e2e_or_capture_run_is_direct_in_a_build_that_honors_it() {
        assert!(direct_mode_from(true, AutomatedRun::E2e, false));
        assert!(direct_mode_from(true, AutomatedRun::Capture, false));
        assert!(!direct_mode_from(true, AutomatedRun::None, false));
    }

    #[test]
    fn the_env_var_turns_it_on_in_a_build_that_honors_it() {
        for run in RUNS {
            assert!(direct_mode_from(true, run, true), "{run:?}");
        }
    }

    #[test]
    fn only_exactly_one_asks_for_it() {
        assert!(env_asks_for_direct_mode(Some("1")));
        for value in [None, Some(""), Some("0"), Some("true"), Some("yes"), Some(" 1")] {
            assert!(!env_asks_for_direct_mode(value), "{value:?}");
        }
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
    fn a_direct_mode_refusal_carries_its_typed_reason_and_no_setting_to_flip() {
        for access in [Access::DevOnly("a reason"), Access::ReadContents, Access::Memory] {
            let Verdict::Refuse(refusal) = admit(OFF_DIRECT, access) else {
                panic!("{access:?} refuses off direct mode");
            };
            let error = refusal.into_error("resolve_conflict");
            assert_eq!(error.data, Some(json!({ "reason": "directModeOff" })));
            assert_eq!(
                error.message,
                "Outside clients can't run resolve_conflict. Tell the person what you'd like done, and they'll do \
                 it in Cmdr."
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
}
