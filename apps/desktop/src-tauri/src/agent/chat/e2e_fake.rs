//! What Ask Cmdr says under `CMDR_E2E_ASK_CMDR_FAKE`: the scripts the rail and a wake play with no provider
//! behind them, and how a spec picks one by what its message says.

use crate::agent::llm::fake::FakeAgentLlm;

use super::session::AgentSlot;

/// The scripted turn the E2E fake streams: a short multi-chunk reply, so the test sees
/// streamed text land and a `Done`. Kept trivially deterministic.
///
/// ⚠️ **One script per slot, and the RAIL's is load-bearing.** `ask-cmdr.spec.ts` counts
/// replies by matching "test assistant", so changing that phrasing breaks specs that have
/// nothing to do with the wake loop. A wake gets its own sentence instead, which is also what
/// lets a wake spec tell a thread the agent opened apart from one the user did.
///
/// The wake slot has THREE scripts, selected by `test_mode::wake_fake_script` (which only the
/// `playwright-e2e` force-wake command sets), because a wake ends three materially different
/// ways and two of them are decided by a TOOL CALL rather than by what it says: it can reply,
/// it can call `nothing_to_suggest` (and delete its own thread), or it can call
/// `propose_suggestions` (and leave something for the user to review, which is what raises the
/// toast). Nothing else can reach them, and the ordinary wake reply is what an untouched
/// selector plays.
pub(super) fn scripted_fake_llm(slot: AgentSlot) -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::test_mode::WakeFakeScript;
    if slot == AgentSlot::Wake {
        match crate::test_mode::wake_fake_script() {
            WakeFakeScript::Quiet => {
                return FakeAgentLlm::script(vec![
                    ScriptedTurn::CallTools(vec![(
                        crate::agent::llm::types::ToolId::NothingToSuggest,
                        serde_json::json!({ "reason": "it was all scripted test churn" }),
                    )]),
                    ScriptedTurn::Say(vec!["Nothing worth raising.".to_string()]),
                ]);
            }
            WakeFakeScript::Propose => return proposing_wake_fake(),
            WakeFakeScript::Reply => {}
        }
    }
    match slot {
        AgentSlot::Rail => FakeAgentLlm::by_prompt(rail_fake_turn),
        AgentSlot::Wake => FakeAgentLlm::script(vec![say(&["I had ", "a look at ", "what changed."])]),
    }
}

/// What a rail message starting with this plays: [`rail_fake_turn`]'s tool-running turn.
const RAIL_FAKE_TOOLS_MARKER: &str = "[tools]";
/// A rail message `[every-tool] <folder>` plays [`every_tool_round`], then the answer.
const RAIL_FAKE_EVERY_TOOL_MARKER: &str = "[every-tool]";
/// A rail message `[rename] <folder>` stages [`rename_plan_call`]'s plan, then signs off.
const RAIL_FAKE_RENAME_MARKER: &str = "[rename]";
/// A rail message `[fail:<kind>]` fails its call with [`rail_fake_failure`]'s typed error.
const RAIL_FAKE_FAIL_MARKER: &str = "[fail:";

/// A rail message `[tool:<name>] <json arguments>` calls that one tool, through the real gate, then
/// signs off. What `ask-cmdr-capability.spec.ts` drives Ask Cmdr with. Text that isn't JSON goes as the
/// arguments unparsed, so a spec can send a malformed call. Not compiled into a release build, where a
/// message's text is never an instruction to this file.
#[cfg(any(debug_assertions, feature = "playwright-e2e"))]
fn rail_fake_tool_trigger(text: &str, round: usize) -> Option<crate::agent::llm::fake::ScriptedTurn> {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::agent::llm::types::ToolId;
    let (name, arguments) = text.strip_prefix("[tool:")?.split_once(']')?;
    if round > 0 {
        return Some(say(&["That's the tool, ", "from the test assistant."]));
    }
    let arguments = arguments.trim();
    let arguments = match arguments {
        "" => serde_json::json!({}),
        text => serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_string())),
    };
    Some(ScriptedTurn::CallTools(vec![(ToolId::from_wire_name(name), arguments)]))
}

/// The rail fake's turn for the prompt it's handed. A message starting with
/// [`RAIL_FAKE_TOOLS_MARKER`] runs three rounds, so `ask-cmdr.spec.ts` can pin a tool-running
/// turn's view live and after a reload: narration plus `app_state`, `list_volumes`, and a
/// hallucinated `format_drive` (refused); narration plus `list_volumes` twice (both repeats of round
/// one's, and reusing its call ids); then the answer. Anything else gets the one-line reply.
///
/// The round comes from the prompt (`latest_user_turn`), never a counter, so a retry or a second
/// thread can't desync it. `app_state` and `list_volumes` read the machine's real state: a spec
/// asserts their shape, never their values.
///
/// The other markers exist for the i18n capture (`i18n-capture-ask-cmdr-turns.ts`), which needs every
/// tool line, a staged rename plan, and the typed failures on screen. Each carries what it needs
/// in the message text, since a fake has no other way to learn the fixture tree's path.
fn rail_fake_turn(messages: &[crate::agent::llm::types::AgentMessage]) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::{ScriptedTurn, latest_user_turn};
    use crate::agent::llm::types::ToolId;
    let no_args = || serde_json::json!({});
    let (text, round) = latest_user_turn(messages).unwrap_or(("", 0));
    #[cfg(any(debug_assertions, feature = "playwright-e2e"))]
    if let Some(turn) = rail_fake_tool_trigger(text, round) {
        return turn;
    }
    if let Some(folder) = text.strip_prefix(RAIL_FAKE_EVERY_TOOL_MARKER) {
        return match round {
            0 => every_tool_round(folder.trim()),
            _ => say(&["That's every tool, ", "from the test assistant."]),
        };
    }
    if let Some(folder) = text.strip_prefix(RAIL_FAKE_RENAME_MARKER) {
        return match round {
            0 => rename_plan_call(folder.trim()),
            _ => say(&["Here's the plan, ", "from the test assistant."]),
        };
    }
    if let Some(kind) = text
        .strip_prefix(RAIL_FAKE_FAIL_MARKER)
        .and_then(|rest| rest.split(']').next())
    {
        return ScriptedTurn::Fail(rail_fake_failure(kind));
    }
    match latest_user_turn(messages) {
        Some((text, round)) if text.starts_with(RAIL_FAKE_TOOLS_MARKER) => match round {
            0 => ScriptedTurn::SayThenCall {
                say: vec!["Let me look at ".into(), "your panes and drives.".into()],
                calls: vec![
                    (ToolId::AppState, no_args()),
                    (ToolId::ListVolumes, no_args()),
                    (ToolId::Unrecognized("format_drive".into()), no_args()),
                ],
            },
            1 => ScriptedTurn::SayThenCall {
                say: vec!["Checking the drives ".into(), "once more.".into()],
                calls: vec![(ToolId::ListVolumes, no_args()), (ToolId::ListVolumes, no_args())],
            },
            _ => say(&["That's all, ", "from the test assistant."]),
        },
        _ => say(&["Hi! ", "I'm the ", "test assistant."]),
    }
}

/// One round through every read tool the `[tools]` script skips, plus the two memory tools. Harmless
/// on a shared app: the memory calls aim outside the jail and refuse, `connect_volume` names a place
/// nobody saved, and the search stays in `folder`.
fn every_tool_round(folder: &str) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::agent::llm::types::ToolId;
    use serde_json::json;
    let file = format!("{folder}/file-a.txt");
    ScriptedTurn::SayThenCall {
        say: vec!["Let me use ".into(), "everything I have.".into()],
        calls: vec![
            (ToolId::ListDir, json!({ "path": folder })),
            (ToolId::ListPaneFiles, json!({})),
            (ToolId::ImportantFolders, json!({})),
            (ToolId::FolderImportance, json!({ "path": folder })),
            (ToolId::OperationsList, json!({})),
            (ToolId::OperationsGet, json!({ "operationId": "no-such-operation" })),
            (ToolId::Search, json!({ "pattern": "file", "scope": folder })),
            (ToolId::SearchPhotos, json!({ "query": "a red bicycle" })),
            (ToolId::InspectFile, json!({ "paths": [file.clone()] })),
            (ToolId::ImageFacts, json!({ "paths": [file] })),
            (ToolId::ListSuggestions, json!({})),
            (ToolId::GetSuggestionGroup, json!({ "groupId": "no-such-group" })),
            (
                ToolId::MemoryWrite,
                json!({ "path": "../outside.md", "content": "Refused." }),
            ),
            (
                ToolId::MemoryEdit,
                json!({ "path": "../outside.md", "oldString": "a", "newString": "b" }),
            ),
            (ToolId::ConnectVolume, json!({ "volumeId": "no-such-place" })),
        ],
    }
}

/// A plan renaming the E2E fixture tree's two files in `folder`, which has to be what the focused
/// pane shows or the tool refuses it. Names come from the file names (`filename` evidence), so no
/// content claim needs backing.
fn rename_plan_call(folder: &str) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::agent::llm::types::ToolId;
    let row = |from: &str, to: &str| {
        serde_json::json!({
            "sourcePath": format!("{folder}/{from}"),
            "volumeId": "root",
            "destinationName": to,
            "evidence": { "source": "filename", "detail": format!("The old name, {from}") },
        })
    };
    ScriptedTurn::SayThenCall {
        say: vec!["I'd name ".into(), "these two like this.".into()],
        calls: vec![(
            ToolId::ProposeRenamePlan,
            serde_json::json!({ "renames": [row("file-a.txt", "notes-a.txt"), row("file-b.txt", "notes-b.txt")] }),
        )],
    }
}

/// The typed failure a `[fail:<kind>]` message asks for, by its `AskCmdrErrorKind` name, with
/// `provider` for anything else.
fn rail_fake_failure(kind: &str) -> crate::agent::llm::types::AgentLlmError {
    use crate::agent::llm::types::AgentLlmError;
    match kind {
        "noKey" => AgentLlmError::NoKey,
        "notConfigured" => AgentLlmError::NotConfigured,
        "unavailable" => AgentLlmError::Unavailable,
        "timeout" => AgentLlmError::Timeout,
        "authFailed" => AgentLlmError::AuthFailed("Scripted key refusal".to_string()),
        "rateLimited" => AgentLlmError::RateLimited("Scripted rate limit".to_string()),
        "requestTooLarge" => AgentLlmError::RequestTooLarge("Scripted request too large".to_string()),
        "budgetExhausted" => AgentLlmError::BudgetExhausted,
        _ => AgentLlmError::Provider("Scripted provider failure".to_string()),
    }
}

fn say(chunks: &[&str]) -> crate::agent::llm::fake::ScriptedTurn {
    crate::agent::llm::fake::ScriptedTurn::Say(chunks.iter().map(|chunk| (*chunk).to_string()).collect())
}

/// A wake that stages one group and signs off, so a spec can drive the path where the user is
/// told something is waiting.
///
/// ⚠️ **An explicit `paths` group, ❌ never a selector.** A selector resolves against the drive
/// index at creation, which makes what the fake stages depend on whether the E2E fixture tree
/// happens to be indexed yet. Explicit paths stage exactly one op, deterministically. Nothing
/// runs from a staged group — approval is the user's, and no test approves this one — so the
/// paths do not need to exist.
fn proposing_wake_fake() -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    FakeAgentLlm::script(vec![
        ScriptedTurn::CallTools(vec![(
            crate::agent::llm::types::ToolId::ProposeSuggestions,
            serde_json::json!({
                "rationale": "scripted test churn",
                "groups": [{
                    "verb": "trash",
                    "sourceVolumeId": "root",
                    "displayName": "Scripted wake suggestion",
                    "paths": ["/Users/e2e/wake-staged/scripted.txt"],
                }],
            }),
        )]),
        ScriptedTurn::Say(vec!["I staged something for you.".to_string()]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rail fake's tool turn plays its rounds by what the prompt holds, and every other
    /// message keeps the one-line reply `ask-cmdr.spec.ts` counts.
    #[test]
    fn the_rail_fake_picks_its_round_from_the_prompt() {
        use crate::agent::llm::fake::ScriptedTurn;
        use crate::agent::llm::types::{AgentMessage, AgentPart, AgentRole, ToolId};
        let message = |role, text: &str| AgentMessage {
            role,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text(text.into())],
            at: 0,
        };
        let tools = message(AgentRole::User, "[tools] look around");
        let answer = message(AgentRole::Assistant, "…");
        let tools_of = |turn: ScriptedTurn| match turn {
            ScriptedTurn::SayThenCall { calls, .. } => calls.into_iter().map(|(tool, _)| tool).collect(),
            _ => Vec::new(),
        };

        assert_eq!(
            tools_of(rail_fake_turn(std::slice::from_ref(&tools))),
            [
                ToolId::AppState,
                ToolId::ListVolumes,
                ToolId::Unrecognized("format_drive".into())
            ]
        );
        assert_eq!(
            tools_of(rail_fake_turn(&[tools.clone(), answer.clone()])),
            [ToolId::ListVolumes, ToolId::ListVolumes]
        );
        assert!(matches!(
            rail_fake_turn(&[tools, answer.clone(), answer.clone()]),
            ScriptedTurn::Say(_)
        ));
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[message(AgentRole::User, "hello")]) else {
            panic!("a plain message gets the one-line reply");
        };
        assert_eq!(chunks.concat(), "Hi! I'm the test assistant.");
    }

    /// `[tool:copy]` calls what it names with what it's given, then signs off in the words the specs wait on.
    #[test]
    fn the_rail_fake_calls_the_tool_a_message_names() {
        use crate::agent::llm::fake::ScriptedTurn;
        use crate::agent::llm::types::{AgentMessage, AgentPart, AgentRole, ToolId};
        let message = |role, text: &str| AgentMessage {
            role,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text(text.into())],
            at: 0,
        };
        let call_of = |text: &str| match rail_fake_turn(&[message(AgentRole::User, text)]) {
            ScriptedTurn::CallTools(calls) => calls,
            _ => panic!("{text} should call a tool"),
        };

        assert_eq!(
            call_of(r#"[tool:copy] {"pane":"left"}"#),
            [(ToolId::Copy, serde_json::json!({ "pane": "left" }))]
        );
        assert_eq!(call_of("[tool:nav_back]"), [(ToolId::NavBack, serde_json::json!({}))]);
        assert_eq!(
            call_of("[tool:use_tool] {not json"),
            [(ToolId::UseTool, serde_json::json!("{not json"))]
        );

        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[
            message(AgentRole::User, r#"[tool:copy] {"pane":"left"}"#),
            message(AgentRole::Assistant, "…"),
        ]) else {
            panic!("a tool call's second round signs off");
        };
        assert!(chunks.concat().contains("test assistant"));
    }

    /// The capture sends these markers (`i18n-capture-ask-cmdr-turns.ts`) and waits on "test assistant".
    #[test]
    fn the_rail_fake_plays_the_capture_scripts() {
        use crate::agent::llm::fake::ScriptedTurn;
        use crate::agent::llm::types::{AgentLlmError, AgentMessage, AgentPart, AgentRole, ToolId};
        let message = |role, text: &str| AgentMessage {
            role,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text(text.into())],
            at: 0,
        };
        let answer = message(AgentRole::Assistant, "…");

        let rename = message(AgentRole::User, "[rename] /fixtures/left");
        let ScriptedTurn::SayThenCall { calls, .. } = rail_fake_turn(std::slice::from_ref(&rename)) else {
            panic!("a rename message opens with its plan");
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, ToolId::ProposeRenamePlan);
        assert_eq!(calls[0].1["renames"][0]["sourcePath"], "/fixtures/left/file-a.txt");
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[rename, answer.clone()]) else {
            panic!("a rename message signs off after its plan");
        };
        assert!(chunks.concat().contains("test assistant"));

        let every = message(AgentRole::User, "[every-tool] /fixtures/left");
        let ScriptedTurn::SayThenCall { calls, .. } = rail_fake_turn(std::slice::from_ref(&every)) else {
            panic!("an every-tool message opens with its round");
        };
        let tools: Vec<ToolId> = calls.into_iter().map(|(tool, _)| tool).collect();
        assert!(tools.contains(&ToolId::MemoryWrite) && tools.contains(&ToolId::ConnectVolume));
        assert!(
            !tools.contains(&ToolId::ProposeRenamePlan) && !tools.contains(&ToolId::ProposeSuggestions),
            "nothing in this round may stage anything for the user to answer"
        );
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[every, answer]) else {
            panic!("an every-tool message signs off after its round");
        };
        assert!(chunks.concat().contains("test assistant"));

        let fail = message(AgentRole::User, "[fail:timeout] anything");
        assert!(matches!(
            rail_fake_turn(std::slice::from_ref(&fail)),
            ScriptedTurn::Fail(AgentLlmError::Timeout)
        ));
    }
}
