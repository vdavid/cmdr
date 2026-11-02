//! What Ask Cmdr can do on a turn someone started, and what it can never finish alone: whole scripted
//! turns through `run_turn` and the real gate and handlers, with a [`FakeFrontend`] answering the events
//! the webview answers. `use_tool` goes through here too, since `tool_gate` only ever sees what it
//! unwraps to. `ask-cmdr-capability.spec.ts` proves the same things against the real webview.

use std::sync::Arc;

use tauri::test::{MockRuntime, mock_app};
use tauri::{App, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

use super::dispatch::AppHandleDispatcher;
use super::test_support::*;
use super::*;
use crate::tools::ask::{ToolAskRegistry, ToolAskResponse};
use crate::tools::{Access, tool_access};
use crate::ui_state::dialog_state::SoftDialogTracker;
use crate::ui_state::pane_state::{PaneFileEntry, PaneState, PaneStateStore};

/// Everything a tool can say to the frontend. `every_event_a_tool_can_emit_is_watched` keeps this whole,
/// so "no event" below can't quietly stop meaning it.
const FRONTEND_EVENTS: &[&str] = &[
    "close-about",
    "close-all-file-viewers",
    "close-confirmation",
    "close-file-viewer",
    "execute-command",
    "focus-about",
    "focus-confirmation",
    "focus-file-viewer",
    "mcp-close-dialog",
    "mcp-compress",
    "mcp-confirm-dialog",
    "mcp-copy",
    "mcp-delete",
    "mcp-describe-open-under-cursor",
    "mcp-describe-setting",
    "mcp-get-all-settings",
    "mcp-key",
    "mcp-mkdir",
    "mcp-mkfile",
    "mcp-move",
    "mcp-move-cursor",
    "mcp-multi-rename",
    "mcp-nav-to-path",
    "mcp-open-search-dialog",
    "mcp-open-under-cursor",
    "mcp-refresh",
    "mcp-rename",
    "mcp-rollback-dialog",
    "mcp-scroll-to",
    "mcp-select",
    "mcp-select-names",
    "mcp-set-setting",
    "mcp-set-view-mode",
    "mcp-settings-close",
    "mcp-sort",
    "mcp-swap-panes",
    "mcp-switch-pane",
    "mcp-sync-state",
    "mcp-tab",
    "mcp-volume-select",
    "open-file-viewer",
    "open-settings",
    "tool-ask-requested",
];

/// Events that only ask the frontend something, or flush what it already knows: they change nothing, so
/// a person's answer doesn't have to come first.
const QUESTIONS: &[&str] = &[
    "mcp-describe-open-under-cursor",
    "mcp-describe-setting",
    "mcp-sync-state",
    "tool-ask-requested",
];

const ALL_ORIGINS: [TurnOrigin; 4] = [
    TurnOrigin::Text,
    TurnOrigin::Resume,
    TurnOrigin::Wake,
    TurnOrigin::Outcomes,
];

type Heard = Arc<Mutex<Vec<(String, Value)>>>;

/// Answers what the webview answers, keeps everything it was told, and plays the person.
struct FakeFrontend {
    app: App<MockRuntime>,
    heard: Heard,
}

fn file(dir: &str, name: &str) -> PaneFileEntry {
    PaneFileEntry {
        name: name.to_string(),
        path: format!("{dir}/{name}"),
        ..Default::default()
    }
}

/// The pane a fence test starts from: one thing in it, selected, so nothing refuses for lack of a target.
fn pane_holding(dir: &str, names: &[&str]) -> PaneState {
    PaneState {
        path: dir.to_string(),
        files: names.iter().map(|name| file(dir, name)).collect(),
        selected_indices: vec![0],
        ..Default::default()
    }
}

impl FakeFrontend {
    /// A person who answers every question with `response`.
    fn with_person(response: ToolAskResponse) -> Self {
        let app = mock_app();
        app.manage(PaneStateStore::new());
        app.manage(SoftDialogTracker::new());
        app.manage(ToolAskRegistry::default());
        WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
            .build()
            .expect("a mock window");
        app.state::<PaneStateStore>()
            .set_left(pane_holding("/test", &["a.txt"]));
        app.state::<PaneStateStore>()
            .set_right(pane_holding("/other", &["b.txt"]));

        let heard: Heard = Arc::default();
        for event in FRONTEND_EVENTS {
            let (handle, heard, response) = (app.handle().clone(), heard.clone(), response.clone());
            app.listen_any(*event, move |raw| {
                let payload: Value = serde_json::from_str(raw.payload()).unwrap_or(Value::Null);
                heard
                    .lock()
                    .expect("no poison in a test")
                    .push((event.to_string(), payload.clone()));
                play(&handle, event, &payload, &response);
            });
        }
        Self { app, heard }
    }

    fn new() -> Self {
        Self::with_person(ToolAskResponse::Declined)
    }

    /// Every event it was sent, by name.
    fn heard(&self) -> Vec<String> {
        let heard = self.heard.lock().expect("no poison in a test");
        heard.iter().map(|(event, _)| event.clone()).collect()
    }

    /// What it heard that isn't only a question: anything here happened without a person's yes.
    fn acted_on(&self) -> Vec<String> {
        let heard = self.heard();
        heard
            .into_iter()
            .filter(|event| !QUESTIONS.contains(&event.as_str()))
            .collect()
    }

    fn payload_of(&self, event: &str) -> Value {
        let heard = self.heard.lock().expect("no poison in a test");
        let found = heard.iter().find(|(name, _)| name == event);
        found.unwrap_or_else(|| panic!("nobody sent {event}")).1.clone()
    }

    fn open_dialog(&self, dialog: &str) {
        self.app.state::<SoftDialogTracker>().open(dialog.to_string());
    }

    fn dialog_is_open(&self, dialog: &str) -> bool {
        self.app
            .state::<SoftDialogTracker>()
            .get_open_types()
            .iter()
            .any(|open| open == dialog)
    }

    /// Runs one scripted turn on `origin`, one round per entry, and hands back every result the model
    /// read, in order.
    async fn turn(&self, rounds: Vec<Vec<(ToolId, Value)>>, origin: TurnOrigin) -> Vec<Value> {
        let conn = migrated_conn();
        let id = conversation(&conn);
        let dispatcher = AppHandleDispatcher::new(self.app.handle().clone(), id);
        let mut programs: Vec<Program> = rounds
            .into_iter()
            .map(|calls| Program::Tools {
                calls,
                usage: AgentUsage::default(),
            })
            .collect();
        programs.push(Program::Answer {
            chunks: vec!["Done.".to_string()],
            usage: AgentUsage::default(),
        });
        let llm = ProgrammableLlm::new(programs);
        let (tx, _rx) = unbounded_channel();
        let mut params = params(id, Some("do it"));
        params.origin = origin;
        let result = run_turn(&llm, &dispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
        assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");

        let prompt = llm.calls_seen().into_iter().last().expect("a call");
        prompt
            .iter()
            .flat_map(|message| &message.parts)
            .filter_map(|part| match part {
                AgentPart::ToolResult(result) => Some(result.content.clone()),
                _ => None,
            })
            .collect()
    }

    /// One call on `origin`, and what the model read back.
    async fn call(&self, call: (ToolId, Value), origin: TurnOrigin) -> Value {
        let answers = self.turn(vec![vec![call]], origin).await;
        answers.into_iter().last().expect("a result")
    }
}

/// What the webview does with `event`. The dialogs register in the tracker the way `ModalDialog` does.
fn play(app: &AppHandle<MockRuntime>, event: &str, payload: &Value, person: &ToolAskResponse) {
    let opens = |dialog: &str| app.state::<SoftDialogTracker>().open(dialog.to_string());
    let store = app.state::<PaneStateStore>();
    let mut reply = json!({ "ok": true });
    match event {
        "tool-ask-requested" => {
            let id = payload["requestId"].as_str().expect("an id");
            app.state::<ToolAskRegistry>().answer(id, person.clone());
            return;
        }
        "mcp-copy" | "mcp-move" | "mcp-compress" => opens("transfer-confirmation"),
        "mcp-delete" => opens("delete-confirmation"),
        "mcp-rename" => opens("rename"),
        "mcp-mkdir" if payload.get("name").is_none() => opens("mkdir-confirmation"),
        "mcp-mkfile" if payload.get("name").is_none() => opens("new-file-confirmation"),
        "mcp-nav-to-path" => {
            reply = json!({ "ok": true, "outcome": "navigated", "path": payload["path"] });
        }
        "mcp-describe-setting" => {
            reply["setting"] = json!({
                "label": "Theme",
                "oldValue": "light",
                "askFree": false,
                "mcpSettable": true,
                "sensitive": false,
                "locked": false,
                "invalid": null,
            });
        }
        "mcp-describe-open-under-cursor" => {
            reply["item"] = json!({
                "activation": "launch",
                "path": "/test/run.command",
                "name": "run.command",
                "isDirectory": false,
            });
        }
        "mcp-sort" | "mcp-tab" | "mcp-key" | "mcp-set-view-mode" => {
            // These ack on the next pane push, which is what the webview answers with.
            store.set_left(store.get_left());
            return;
        }
        _ => {}
    }
    if payload.get("requestId").is_some() {
        reply["requestId"] = payload["requestId"].clone();
        app.emit("mcp-response", reply).expect("emit the reply");
    }
}

/// The two ways a model can name a tool: directly (a deferred tool parses like any other), and through
/// `use_tool`.
fn both_ways(tool: &ToolId, params: &Value) -> [(ToolId, Value); 2] {
    [
        (tool.clone(), params.clone()),
        (
            ToolId::UseTool,
            json!({ "name": tool.as_wire_name(), "params": params }),
        ),
    ]
}

/// The tools that open something for a person, and what opens.
fn dialog_openers() -> Vec<(ToolId, Value, &'static str, &'static str)> {
    let pane = json!({ "pane": "left" });
    vec![
        (ToolId::Copy, pane.clone(), "mcp-copy", "transfer-confirmation"),
        (ToolId::Move, pane.clone(), "mcp-move", "transfer-confirmation"),
        (ToolId::Compress, pane.clone(), "mcp-compress", "transfer-confirmation"),
        (ToolId::Delete, pane.clone(), "mcp-delete", "delete-confirmation"),
        (
            ToolId::Rename,
            json!({ "pane": "left", "name": "a.txt", "newName": "b.txt" }),
            "mcp-rename",
            "rename",
        ),
        (ToolId::Mkdir, pane.clone(), "mcp-mkdir", "mkdir-confirmation"),
        (ToolId::Mkfile, pane, "mcp-mkfile", "new-file-confirmation"),
    ]
}

// ── Demonstrably capable ──────────────────────────────────────────────────────

#[tokio::test]
async fn the_agent_navigates_selects_opens_tabs_and_sorts_either_way() {
    let dir = crate::test_support::TestDir::new("agent-navigates");
    let cases = [
        (
            ToolId::NavToPath,
            json!({ "pane": "left", "path": dir.to_string_lossy() }),
            "mcp-nav-to-path",
        ),
        (
            ToolId::Select,
            json!({ "pane": "left", "names": ["a.txt"] }),
            "mcp-select-names",
        ),
        (ToolId::Tab, json!({ "pane": "left", "action": "new" }), "mcp-tab"),
        (
            ToolId::Sort,
            json!({ "pane": "left", "by": "size", "order": "desc" }),
            "mcp-sort",
        ),
    ];
    for (tool, params, event) in cases {
        for call in both_ways(&tool, &params) {
            let front = FakeFrontend::new();
            let answer = front.call(call.clone(), TurnOrigin::Text).await;
            assert!(
                answer.as_str().is_some_and(|text| text.starts_with("OK")),
                "{call:?}: {answer}"
            );
            assert!(
                front.heard().contains(&event.to_string()),
                "{call:?}: {:?}",
                front.heard()
            );
        }
    }
}

/// A chat that's picked up again is a turn someone started, so it has the same reach.
#[tokio::test]
async fn a_resumed_turn_reaches_what_a_new_one_does() {
    let front = FakeFrontend::new();
    let answer = front
        .call((ToolId::Copy, json!({ "pane": "left" })), TurnOrigin::Resume)
        .await;
    assert_eq!(answer["awaitingPerson"], true, "{answer}");
}

/// Every dialog says who started it and has to arm, whichever way the call came: `initiator` is what
/// Skip confirmation and the operation log read, and only `user` ever skips.
#[tokio::test]
async fn every_dialog_the_agent_opens_waits_for_a_person_and_names_the_agent() {
    for (tool, params, event, dialog) in dialog_openers() {
        for call in both_ways(&tool, &params) {
            let front = FakeFrontend::new();
            let answer = front.call(call.clone(), TurnOrigin::Text).await;
            assert_eq!(answer["awaitingPerson"], true, "{call:?}: {answer}");
            assert_eq!(answer["dialog"], dialog, "{call:?}: {answer}");
            assert!(front.dialog_is_open(dialog), "{call:?}");

            let payload = front.payload_of(event);
            assert_eq!(payload["initiator"], "agent", "{call:?}: {payload}");
            assert_eq!(payload["guard"], true, "{call:?}: {payload}");
            assert!(!front.heard().contains(&"mcp-confirm-dialog".to_string()), "{call:?}");
        }
    }
}

/// The one written exception: a folder with a name is made with no dialog, and records the agent.
#[tokio::test]
async fn the_agent_creates_a_named_folder_itself_either_way() {
    for call in both_ways(&ToolId::Mkdir, &json!({ "pane": "left", "name": "Reports" })) {
        let front = FakeFrontend::new();
        let answer = front.call(call.clone(), TurnOrigin::Text).await;
        assert_eq!(answer, json!("OK: Created folder Reports."), "{call:?}");
        assert_eq!(front.payload_of("mcp-mkdir")["initiator"], "agent", "{call:?}");
        assert!(!front.dialog_is_open("mkdir-confirmation"), "{call:?}");
    }
}

/// The other one: a tag lands on the file itself, on a real disk.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn the_agent_tags_a_file_itself_either_way() {
    const DISK: &str = "agent-tags-disk";
    let disk = cmdr_local::LocalPosixVolume::local_folder("Disk", "/", cmdr_fs::volume::host::VolumeHost::detached());
    crate::file_system::volume::manager::get_volume_manager().register(DISK, Arc::new(disk));

    let dir = crate::test_support::TestDir::new("agent-tags");
    let params = json!({ "pane": "left", "names": ["a.txt"], "action": "set", "colors": ["red"] });
    for (way, call) in both_ways(&ToolId::Tag, &params).into_iter().enumerate() {
        let folder = dir.join(way.to_string());
        std::fs::create_dir(&folder).expect("make a folder");
        let path = folder.join("a.txt");
        std::fs::write(&path, "a").expect("write the file");
        let front = FakeFrontend::new();
        let pane = PaneState {
            volume_id: Some(DISK.to_string()),
            ..pane_holding(&folder.to_string_lossy(), &["a.txt"])
        };
        front.app.state::<PaneStateStore>().set_left(pane);

        let answer = front.call(call.clone(), TurnOrigin::Text).await;
        assert!(
            answer.as_str().is_some_and(|text| text.starts_with("OK")),
            "{call:?}: {answer}"
        );
        assert_eq!(
            crate::file_system::tags::read_tags(&path).len(),
            1,
            "{call:?}: {answer}"
        );
    }
}

/// The agent reads its logs redacted through the wrapper too. nextest gives each test its own process,
/// so the log dir is this test's.
#[tokio::test]
async fn the_agent_reads_the_logs_through_use_tool() {
    let dir = crate::test_support::TestDir::new("agent-reads-logs-through-use-tool");
    crate::logging::set_log_dir(dir.to_path_buf());
    std::fs::write(
        dir.join("cmdr.log"),
        "2026-11-02T08:00:00.000+01:00 WARN listing /Users/dorka/plan.pdf failed\n",
    )
    .expect("write the log");

    for call in both_ways(&ToolId::ReadResource, &json!({ "uri": "cmdr://logs" })) {
        let answer = FakeFrontend::new().call(call.clone(), TurnOrigin::Text).await;
        let text = answer["text"].as_str().unwrap_or_else(|| panic!("{call:?}: {answer}"));
        assert!(text.contains("WARN listing"), "{text}");
        assert!(!text.contains("dorka"), "{text}");
    }
}

// ── Sensitive: asked, and only a yes runs it ──────────────────────────────────

/// Each sensitive tool, and the event only its handler sends.
fn sensitive_tools() -> [(ToolId, Value, Option<&'static str>); 3] {
    [
        (ToolId::Quit, json!({}), None),
        (
            ToolId::SetSetting,
            json!({ "id": "appearance.theme", "value": "dark" }),
            Some("mcp-set-setting"),
        ),
        (ToolId::OpenUnderCursor, json!({}), Some("mcp-open-under-cursor")),
    ]
}

#[tokio::test]
async fn a_sensitive_tool_asks_and_a_no_stops_it_either_way() {
    for (tool, params, handlers_event) in sensitive_tools() {
        for call in both_ways(&tool, &params) {
            let front = FakeFrontend::new();
            let answer = front.call(call.clone(), TurnOrigin::Text).await;
            assert_eq!(answer["data"]["refusal"], "declined", "{call:?}: {answer}");
            assert_eq!(front.payload_of("tool-ask-requested")["startedBy"], "agent", "{call:?}");
            if let Some(event) = handlers_event {
                assert!(
                    !front.heard().contains(&event.to_string()),
                    "{call:?}: {:?}",
                    front.heard()
                );
            }
            assert_eq!(front.acted_on(), Vec::<String>::new(), "{call:?}");
        }
    }
}

/// The other half of "a person finishes it": the same call goes through on a yes.
#[tokio::test]
async fn a_yes_lets_a_sensitive_tool_finish() {
    let params = json!({ "id": "appearance.theme", "value": "dark" });
    for call in both_ways(&ToolId::SetSetting, &params) {
        let front = FakeFrontend::with_person(ToolAskResponse::Allowed);
        let answer = front.call(call.clone(), TurnOrigin::Text).await;
        assert!(
            answer.as_str().is_some_and(|text| text.starts_with("OK")),
            "{call:?}: {answer}"
        );
        assert!(front.heard().contains(&"mcp-set-setting".to_string()), "{call:?}");
    }
}

/// With nobody there to ask, nothing runs.
#[tokio::test]
async fn a_sensitive_tool_with_no_window_runs_nothing() {
    let app = mock_app();
    app.manage(PaneStateStore::new());
    app.manage(SoftDialogTracker::new());
    app.manage(ToolAskRegistry::default());
    let front = FakeFrontend {
        app,
        heard: Arc::default(),
    };
    let answer = front.call((ToolId::Quit, json!({})), TurnOrigin::Text).await;
    assert!(answer["problem"].as_str().is_some(), "{answer}");
}

/// A no is a no for the turn, however the model dresses the retry: the person gets asked once.
#[tokio::test]
async fn a_declined_question_isnt_asked_twice_in_a_turn() {
    let front = FakeFrontend::new();
    let [direct, wrapped] = both_ways(&ToolId::Quit, &json!({}));
    let answers = front.turn(vec![vec![direct], vec![wrapped]], TurnOrigin::Text).await;
    assert_eq!(answers[1]["repeatedCall"], true, "{answers:?}");
    let asked = front
        .heard()
        .iter()
        .filter(|event| *event == "tool-ask-requested")
        .count();
    assert_eq!(asked, 1);
}

// ── The fence ─────────────────────────────────────────────────────────────────

/// The ways past a person, with arguments their own handlers would act on.
fn ways_past_a_person() -> [(ToolId, Value); 3] {
    [
        (ToolId::DialogConfirm, json!({ "type": "transfer-confirmation" })),
        (
            ToolId::ResolveConflict,
            json!({ "operationId": "op-1", "conflictId": "c-1", "resolution": "overwrite" }),
        ),
        (ToolId::QueueMoveLocked, json!({ "operationId": "op-1" })),
    ]
}

async fn assert_the_fence_holds(front: &FakeFrontend, call: (ToolId, Value), origin: TurnOrigin) {
    let answer = front.call(call.clone(), origin).await;
    assert_eq!(answer["available"], false, "{call:?} on {origin:?}: {answer}");
    assert_eq!(answer["refusal"], "devOnly", "{call:?} on {origin:?}: {answer}");
    assert_eq!(front.heard(), Vec::<String>::new(), "{call:?} on {origin:?}");
    assert!(front.dialog_is_open("transfer-confirmation"), "{call:?} on {origin:?}");
}

/// With a dialog waiting to be confirmed, on every origin, called directly and through `use_tool`.
#[tokio::test]
async fn nothing_past_a_person_runs_on_any_origin_either_way() {
    for (tool, params) in ways_past_a_person() {
        for call in both_ways(&tool, &params) {
            for origin in ALL_ORIGINS {
                let front = FakeFrontend::new();
                front.open_dialog("transfer-confirmation");
                assert_the_fence_holds(&front, call.clone(), origin).await;
            }
        }
    }
}

/// E2E runs in direct mode, and so does anyone who turned it on: that's for outside clients, and the
/// agent never gets it.
#[tokio::test]
async fn direct_mode_never_reaches_the_agent() {
    crate::tools::policy::set_direct_mode_setting_for_test(true);
    assert!(crate::tools::policy::direct_mode());
    for (tool, params) in ways_past_a_person() {
        for call in both_ways(&tool, &params) {
            for origin in ALL_ORIGINS {
                let front = FakeFrontend::new();
                front.open_dialog("transfer-confirmation");
                assert_the_fence_holds(&front, call.clone(), origin).await;
            }
        }
    }
    // What it opens still has to arm, and the question still goes up.
    let front = FakeFrontend::new();
    front
        .call((ToolId::Copy, json!({ "pane": "left" })), TurnOrigin::Text)
        .await;
    assert_eq!(front.payload_of("mcp-copy")["guard"], true);
    let front = FakeFrontend::new();
    let answer = front.call((ToolId::Quit, json!({})), TurnOrigin::Text).await;
    assert_eq!(answer["data"]["refusal"], "declined", "{answer}");
    crate::tools::policy::set_direct_mode_setting_for_test(false);
}

/// Every tool the table has, read off it: a turn Cmdr started reaches reads, proposals, and memory, and
/// the rest refuse before anything reaches the frontend, even called well.
#[tokio::test]
async fn a_turn_cmdr_started_reaches_nothing_else_either_way() {
    let mut refused = 0;
    for tool in ToolId::KNOWN {
        let name = tool.as_wire_name();
        let access = tool_access(name).expect("a table entry has a class");
        if matches!(
            access,
            Access::Read | Access::ReadContents | Access::Propose | Access::Memory
        ) {
            continue;
        }
        for call in both_ways(&tool, &json!({ "pane": "left" })) {
            for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
                let front = FakeFrontend::new();
                let answer = front.call(call.clone(), origin).await;
                assert_eq!(answer["available"], false, "{name} on {origin:?}: {answer}");
                assert_eq!(front.heard(), Vec::<String>::new(), "{name} on {origin:?}");
                refused += 1;
            }
        }
    }
    assert!(refused > 100, "the walk covered too little: {refused}");
}

/// No argument is a way past a person, whatever it's called, and a claim to be someone else refuses
/// rather than opens a dialog for less than was asked.
#[tokio::test]
async fn no_argument_gets_a_tool_past_a_person() {
    for key in [
        "autoConfirm",
        "confirm",
        "skipConfirmation",
        "initiator",
        "guard",
        "startedBy",
        "force",
    ] {
        for (tool, params, _, dialog) in dialog_openers() {
            let mut params = params.clone();
            params[key] = if key == "initiator" {
                json!("user")
            } else {
                json!(false)
            };
            for call in both_ways(&tool, &params) {
                let front = FakeFrontend::new();
                let answer = front.call(call.clone(), TurnOrigin::Text).await;
                assert_eq!(answer["data"]["unknownProperties"], json!([key]), "{call:?}: {answer}");
                assert_eq!(front.heard(), Vec::<String>::new(), "{call:?}");
                assert!(!front.dialog_is_open(dialog), "{call:?}");
            }
        }
    }
}

/// The tools that were split so their power couldn't hide in a parameter: the old values refuse.
#[tokio::test]
async fn a_split_tool_has_no_way_back_to_what_left_it() {
    let cases = [
        (
            ToolId::Dialog,
            json!({ "action": "confirm", "type": "transfer-confirmation" }),
        ),
        (ToolId::Queue, json!({ "action": "move_locked", "operationId": "op-1" })),
        (ToolId::Queue, json!({ "action": "rollback", "operationId": "op-1" })),
        (
            ToolId::Queue,
            json!({ "action": "cancel", "operationId": "op-1", "rollback": true }),
        ),
        (
            ToolId::OperationsRollback,
            json!({ "operationId": "op-1", "autoConfirm": true }),
        ),
    ];
    for (tool, params) in cases {
        for call in both_ways(&tool, &params) {
            let front = FakeFrontend::new();
            front.open_dialog("transfer-confirmation");
            let answer = front.call(call.clone(), TurnOrigin::Text).await;
            assert!(answer["problem"].as_str().is_some(), "{call:?}: {answer}");
            assert_eq!(front.heard(), Vec::<String>::new(), "{call:?}");
            assert!(front.dialog_is_open("transfer-confirmation"), "{call:?}");
        }
    }
}

/// Nesting and malformed names: nothing here runs, and nothing reads as having worked.
#[tokio::test]
async fn use_tool_cant_be_talked_past_the_gate() {
    let confirm = json!({ "type": "transfer-confirmation" });
    let wrapped = |name: &str| json!({ "name": name, "params": confirm });
    let calls = [
        // Nested, at every depth a model might try.
        wrapped("use_tool"),
        json!({ "name": "use_tool", "params": wrapped("dialog_confirm") }),
        json!({ "name": "use_tool", "params": { "name": "use_tool", "params": wrapped("dialog_confirm") } }),
        json!({ "name": "tool_details", "params": { "names": ["dialog_confirm"] } }),
        // Spellings of the name.
        wrapped("dialog_confirm"),
        wrapped(" dialog_confirm "),
        wrapped("\tdialog_confirm\n"),
        wrapped("DIALOG_CONFIRM"),
        wrapped("dialog-confirm"),
        wrapped("dialogConfirm"),
        wrapped("dialog_confirm\u{0}"),
        wrapped("dіalog_confirm"), // A Cyrillic "і".
        // Shapes of the call.
        json!({ "name": ["dialog_confirm"], "params": confirm }),
        json!({ "name": { "name": "dialog_confirm" }, "params": confirm }),
        json!({ "params": wrapped("dialog_confirm") }),
        json!({ "name": "dialog_confirm", "params": "{\"type\":\"transfer-confirmation\"}" }),
        json!({ "name": "dialog_confirm", "params": [confirm] }),
        json!({ "name": "copy", "params": { "pane": "left" }, "tool": "dialog_confirm" }),
        json!("dialog_confirm"),
        json!(["dialog_confirm", confirm]),
        Value::Null,
    ];
    for arguments in calls {
        for origin in ALL_ORIGINS {
            let front = FakeFrontend::new();
            front.open_dialog("transfer-confirmation");
            let answer = front.call((ToolId::UseTool, arguments.clone()), origin).await;
            let refused = answer["available"] == false || answer["problem"].as_str().is_some();
            // `tool_details` is a read, and answers that the tool isn't one the agent has.
            let told = answer["unavailable"][0]["refusal"] == "devOnly";
            assert!(refused || told, "{arguments} on {origin:?}: {answer}");
            assert_eq!(front.heard(), Vec::<String>::new(), "{arguments} on {origin:?}");
            assert!(front.dialog_is_open("transfer-confirmation"), "{arguments}");
        }
    }
}

/// A name a provider hands back that didn't parse to the table's own variant still answers to the
/// table: the gate reads the wire name.
#[tokio::test]
async fn a_raw_provider_name_answers_to_the_table() {
    let front = FakeFrontend::new();
    front.open_dialog("transfer-confirmation");
    let call = (
        ToolId::Unrecognized("dialog_confirm".to_string()),
        json!({ "type": "transfer-confirmation" }),
    );
    assert_the_fence_holds(&front, call, TurnOrigin::Text).await;
}

/// The obvious plan: open the dialog and press it in the same round.
#[tokio::test]
async fn opening_a_dialog_and_confirming_it_in_one_round_leaves_it_open() {
    let [direct, wrapped] = both_ways(&ToolId::DialogConfirm, &json!({ "type": "transfer-confirmation" }));
    for confirm in [direct, wrapped] {
        let front = FakeFrontend::new();
        let copy = (ToolId::Copy, json!({ "pane": "left" }));
        let answers = front.turn(vec![vec![copy, confirm.clone()]], TurnOrigin::Text).await;
        assert_eq!(answers[0]["awaitingPerson"], true, "{answers:?}");
        assert_eq!(answers[1]["refusal"], "devOnly", "{answers:?}");
        assert!(front.dialog_is_open("transfer-confirmation"));
        assert!(!front.heard().contains(&"mcp-confirm-dialog".to_string()));
    }
}

// ── Keeping the proof whole ───────────────────────────────────────────────────

/// A new event a handler emits has to be added above, or a refusal test could pass while it fires.
#[test]
fn every_event_a_tool_can_emit_is_watched() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools");
    let mut pending = vec![root];
    let mut missing = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read the tools folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            if !name.ends_with(".rs") || name.contains("test") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("read a source file");
            // A file's own test module may make up events to test a round trip with.
            let source = source.split("#[cfg(test)]").next().unwrap_or_default();
            for quoted in source.split('"').skip(1).step_by(2) {
                let is_event = quoted.starts_with("mcp-")
                    && quoted.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                    && quoted != "mcp-response"
                    && !quoted.contains("-test-");
                if is_event && !FRONTEND_EVENTS.contains(&quoted) {
                    missing.push(format!("{quoted} in {name}"));
                }
            }
        }
    }
    assert_eq!(missing, Vec::<String>::new());
}
