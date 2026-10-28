//! The history page's projection: what each tool line says after a reload, which turn every row
//! belongs to, and what each turn spent. Pure, over hand-built rows, so each shape an old or odd
//! thread can hold is pinned here.

use serde_json::{Value, json};

use super::*;
use crate::agent::chat::message_view::MessageBlock;
use crate::agent::llm::types::{AgentPart, AgentRole, AgentToolCall, AgentToolResult, ToolId};
use crate::agent::store::{ConversationEvent, StoredContent, StoredMessage, TurnContext, TurnStats};
use crate::agent::tools::line::{LinePath, LineRef, RefusalReason, ToolOutcome, ToolSubject};

/// Builds a thread row by row, minting ids and seqs in order (ids start at 100 so they never
/// read like seqs).
#[derive(Default)]
struct Thread {
    rows: Vec<StoredMessage>,
}

impl Thread {
    fn push(&mut self, content: StoredContent, tokens: Option<(u32, u32)>) -> i64 {
        let seq = self.rows.len() as i64;
        let id = 100 + seq;
        self.rows.push(StoredMessage {
            id,
            seq,
            content,
            text_for_search: String::new(),
            prompt_tokens: tokens.map(|(prompt, _)| prompt),
            completion_tokens: tokens.map(|(_, completion)| completion),
            created_at: 1_780_000_000,
        });
        id
    }

    fn user(&mut self, text: &str) -> i64 {
        self.message(AgentRole::User, vec![AgentPart::Text(text.into())], None)
    }

    fn assistant(&mut self, parts: Vec<AgentPart>, tokens: (u32, u32)) -> i64 {
        self.message(AgentRole::Assistant, parts, Some(tokens))
    }

    fn result(&mut self, call_id: &str, content: Value) -> i64 {
        let result = AgentToolResult {
            call_id: call_id.into(),
            content,
            elided: false,
        };
        self.message(AgentRole::Tool, vec![AgentPart::ToolResult(result)], None)
    }

    fn event(&mut self, event: ConversationEvent) -> i64 {
        self.push(StoredContent::Event(event), None)
    }

    fn message(&mut self, role: AgentRole, parts: Vec<AgentPart>, tokens: Option<(u32, u32)>) -> i64 {
        self.push(StoredContent::Message { role, parts }, tokens)
    }

    /// The whole thread as one page.
    fn project(self) -> Vec<MessageView> {
        self.project_with(None)
    }

    fn project_with(self, in_flight_since_seq: Option<i64>) -> Vec<MessageView> {
        project_page(
            TurnContext::default(),
            self.rows,
            PageContext {
                in_flight_since_seq,
                memory_root: None,
            },
        )
    }
}

fn call(call_id: &str, tool: ToolId, arguments: Value) -> AgentPart {
    AgentPart::ToolCall(AgentToolCall {
        call_id: call_id.into(),
        tool,
        arguments,
        reasoning: None,
    })
}

fn list_dir(call_id: &str, path: &str) -> AgentPart {
    call(call_id, ToolId::ListDir, json!({ "path": path }))
}

fn listing(returned: u32) -> Value {
    json!({ "path": "/a", "children": [], "total": returned, "returned": returned, "truncated": false })
}

fn problem() -> Value {
    json!({ "problem": "list_dir needs path." })
}

/// Every tool-call block of a view, as `(callId, subject, outcome, repeatOf)`.
fn lines(view: &MessageView) -> Vec<(String, ToolSubject, Option<ToolOutcome>, Option<LineRef>)> {
    view.blocks
        .iter()
        .filter_map(|block| match block {
            MessageBlock::ToolCall {
                call_id,
                subject,
                outcome,
                repeat_of,
                ..
            } => Some((call_id.clone(), subject.clone(), outcome.clone(), repeat_of.clone())),
            _ => None,
        })
        .collect()
}

fn view(views: &[MessageView], id: i64) -> &MessageView {
    views.iter().find(|view| view.id == id).expect("the row is on the page")
}

fn items(returned: u32) -> Option<ToolOutcome> {
    Some(ToolOutcome::Items {
        returned,
        total: Some(u64::from(returned)),
        lower_bound: false,
    })
}

fn refused_problem() -> Option<ToolOutcome> {
    Some(ToolOutcome::Refused {
        reason: RefusalReason::Problem,
    })
}

// ── Pairing ───────────────────────────────────────────────────────────────────

/// The fake mints `fake-call-0` in every round, so a thread-wide lookup by call id would hand
/// round two's line round one's result. Each round reads the results after it.
#[test]
fn two_rounds_reusing_a_call_id_each_get_their_own_result() {
    let mut thread = Thread::default();
    thread.user("look");
    let first = thread.assistant(vec![list_dir("fake-call-0", "/a")], (10, 1));
    thread.result("fake-call-0", listing(3));
    let second = thread.assistant(vec![list_dir("fake-call-0", "/b")], (20, 2));
    thread.result("fake-call-0", problem());
    let views = thread.project();

    assert_eq!(lines(view(&views, first))[0].2, items(3));
    assert_eq!(lines(view(&views, second))[0].2, refused_problem());
}

/// Results persist in call order, so a row naming one call id twice pairs by occurrence.
#[test]
fn one_row_calling_the_same_id_twice_pairs_by_occurrence() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("dup", "/a"), list_dir("dup", "/b")], (10, 1));
    thread.result("dup", listing(1));
    thread.result("dup", problem());
    let views = thread.project();

    let lines = lines(view(&views, id));
    assert_eq!(lines[0].2, items(1));
    assert_eq!(lines[1].2, refused_problem());
}

/// A call whose result never landed (a crash mid-dispatch) is settled with no outcome, and its
/// subject still comes from its arguments.
#[test]
fn a_call_with_no_result_row_has_no_outcome() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    let views = thread.project();

    let (_, subject, outcome, _) = &lines(view(&views, id))[0];
    assert_eq!(
        subject,
        &ToolSubject::Folder {
            path: LinePath::of("/a")
        }
    );
    assert_eq!(outcome, &None);
}

/// A result never reaches past the next assistant row: a later round's result with the same id
/// belongs to that round's call.
#[test]
fn a_result_after_the_next_assistant_row_is_not_this_rounds() {
    let mut thread = Thread::default();
    thread.user("look");
    let first = thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(1));
    let views = thread.project();

    assert_eq!(lines(view(&views, first))[0].2, None);
}

/// Tool rows carry only results, already folded into the calls: they project to no blocks, so
/// no raw result content reaches the page.
#[test]
fn a_tool_row_projects_to_no_blocks_and_no_content() {
    let mut thread = Thread::default();
    thread.user("look");
    thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    let tool_row = thread.result("c1", json!({ "path": "/private/secret-receipt.png", "children": [] }));
    let views = thread.project();

    assert!(view(&views, tool_row).blocks.is_empty());
    let wire = serde_json::to_string(&views).expect("serializes");
    assert!(
        !wire.contains("secret-receipt"),
        "result content stays backend-only: {wire}"
    );
}

// ── Pages that cut a turn ─────────────────────────────────────────────────────

/// Marks and results are turn-scoped, so the page's own rows read the same whichever page they
/// land on: a result in the rows after the page still annotates its call.
#[test]
fn a_result_on_the_next_page_still_annotates_its_call() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(2));
    let mut rows = thread.rows;
    let after = rows.split_off(2);

    let views = project_page(
        TurnContext {
            before: Vec::new(),
            after,
        },
        rows,
        PageContext {
            in_flight_since_seq: None,
            memory_root: None,
        },
    );

    assert_eq!(views.len(), 2, "only the page's own rows come back");
    assert_eq!(lines(view(&views, id))[0].2, items(2));
}

/// A page starting mid-turn: its rows still know their turn's opener, and a call repeating one
/// from before the page is marked as such.
#[test]
fn a_page_starting_mid_turn_keeps_turn_ids_and_repeat_marks() {
    let mut thread = Thread::default();
    let opener = thread.user("look");
    let first = thread.assistant(vec![list_dir("fake-call-0", "/a")], (10, 1));
    thread.result("fake-call-0", listing(1));
    let second = thread.assistant(vec![list_dir("fake-call-0", "/a")], (10, 1));
    thread.result("fake-call-0", listing(1));
    let mut rows = thread.rows;
    let page = rows.split_off(3);

    let views = project_page(
        TurnContext {
            before: rows,
            after: Vec::new(),
        },
        page,
        PageContext {
            in_flight_since_seq: None,
            memory_root: None,
        },
    );

    assert!(views.iter().all(|view| view.turn_id == Some(opener)));
    assert_eq!(
        lines(view(&views, second))[0].3,
        Some(LineRef {
            message_id: first,
            call_id: "fake-call-0".into()
        })
    );
}

// ── Turn ids and decisions ────────────────────────────────────────────────────

#[test]
fn every_row_carries_its_turns_opener_and_rows_before_the_first_have_none() {
    let mut thread = Thread::default();
    let stray = thread.assistant(vec![AgentPart::Text("hello".into())], (1, 1));
    let opener = thread.user("look");
    let answer = thread.assistant(vec![AgentPart::Text("done".into())], (1, 1));
    let next = thread.user("again");
    let views = thread.project();

    assert_eq!(view(&views, stray).turn_id, None);
    assert_eq!(view(&views, opener).turn_id, Some(opener));
    assert_eq!(view(&views, answer).turn_id, Some(opener));
    assert_eq!(view(&views, next).turn_id, Some(next));
}

/// A decision taken while the turn still ran sits before its last assistant row; one taken after
/// it ended doesn't. The rail hoists only the first kind, as the live stream showed it.
#[test]
fn an_event_is_during_its_turn_only_before_the_turns_last_assistant_row() {
    let mut thread = Thread::default();
    thread.user("look");
    thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    let mid = thread.event(ConversationEvent::ModelChanged { model: "m".into() });
    thread.result("c1", listing(1));
    thread.assistant(vec![AgentPart::Text("done".into())], (10, 1));
    let post = thread.event(ConversationEvent::ModelChanged { model: "n".into() });
    let views = thread.project();

    assert!(view(&views, mid).during_turn);
    assert!(!view(&views, post).during_turn);
}

// ── Turn stats ────────────────────────────────────────────────────────────────

/// A turn's stats ride its LAST assistant view, from its `TurnFinished` row; the row itself is an
/// empty event view.
#[test]
fn a_recorded_turn_carries_its_stored_stats_on_its_last_assistant_view() {
    let stats = TurnStats {
        tool_calls: 1,
        wall_ms: Some(4_200),
        prompt_tokens: 30,
        completion_tokens: 3,
        stopped: false,
        limit: None,
    };
    let mut thread = Thread::default();
    thread.user("look");
    let round = thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(1));
    let answer = thread.assistant(vec![AgentPart::Text("done".into())], (20, 2));
    let finished = thread.event(ConversationEvent::TurnFinished { stats });
    let views = thread.project();

    assert_eq!(view(&views, answer).turn_stats, Some(stats));
    assert_eq!(view(&views, round).turn_stats, None);
    assert!(view(&views, finished).blocks.is_empty());
}

/// A thread written before turns recorded their stats still gets a footer, rebuilt from its rows:
/// every result row counts as a call, tokens sum per row, and the wall time is unknown.
#[test]
fn an_old_turn_with_tools_gets_fallback_stats_rebuilt_from_its_rows() {
    let mut thread = Thread::default();
    thread.user("look");
    thread.assistant(vec![list_dir("c1", "/a"), list_dir("c2", "/b")], (10, 1));
    thread.result("c1", listing(1));
    thread.result("c2", problem());
    let answer = thread.assistant(vec![AgentPart::Text("done".into())], (20, 2));
    let views = thread.project();

    assert_eq!(
        view(&views, answer).turn_stats,
        Some(TurnStats {
            tool_calls: 2,
            wall_ms: None,
            prompt_tokens: 30,
            completion_tokens: 3,
            stopped: false,
            limit: None,
        })
    );
}

#[test]
fn a_tool_less_turn_has_no_stats() {
    let mut thread = Thread::default();
    thread.user("hi");
    let answer = thread.assistant(vec![AgentPart::Text("hello".into())], (10, 1));
    let views = thread.project();

    assert_eq!(view(&views, answer).turn_stats, None);
}

/// The turn still running holds rows after the in-flight mark; its stats come from the live
/// terminal event, so the page rebuilds none for it. An earlier turn keeps its own.
#[test]
fn the_in_flight_turn_gets_no_fallback_stats() {
    let mut thread = Thread::default();
    thread.user("first");
    thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(1));
    let earlier = thread.assistant(vec![AgentPart::Text("done".into())], (10, 1));
    let mark = thread.rows.last().expect("a row").seq;
    thread.user("second");
    let running = thread.assistant(vec![list_dir("c1", "/b")], (10, 1));
    thread.result("c1", listing(1));
    let views = thread.project_with(Some(mark));

    assert!(view(&views, earlier).turn_stats.is_some());
    assert_eq!(view(&views, running).turn_stats, None);
}

// ── Old shapes ────────────────────────────────────────────────────────────────

/// A stored legacy nameless call, and arguments of no expected shape, still load.
#[test]
fn a_legacy_nameless_call_and_odd_arguments_load_as_fallbacks() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(
        vec![
            call("c1", ToolId::Unrecognized(String::new()), json!({})),
            call("c2", ToolId::ListDir, json!("not an object")),
        ],
        (10, 1),
    );
    thread.result(
        "c1",
        json!({ "available": false, "requested": "", "reason": "unknown" }),
    );
    let views = thread.project();

    let lines = lines(view(&views, id));
    assert_eq!(lines[0].1, ToolSubject::UnknownTool { name: String::new() });
    assert_eq!(
        lines[0].2,
        Some(ToolOutcome::Refused {
            reason: RefusalReason::ToolUnavailable
        })
    );
    assert_eq!(lines[1].1, ToolSubject::Unparsed);
}

// ── The expanded line ─────────────────────────────────────────────────────────

impl Thread {
    /// The row `id` and the rest of its turn, as `store::row_and_rest_of_turn` reads them.
    fn rest_of_turn_at(&self, id: i64) -> Vec<StoredMessage> {
        let start = self.rows.iter().position(|row| row.id == id).expect("the row exists");
        let end = self.rows[start + 1..]
            .iter()
            .position(|row| is_role(row, AgentRole::User))
            .map_or(self.rows.len(), |offset| start + 1 + offset);
        self.rows[start..end].to_vec()
    }
}

#[test]
fn a_found_call_gives_its_preview_and_the_pretty_raw_result() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(3));

    let detail = tool_result_detail(thread.rest_of_turn_at(id), "c1", 0).expect("found");
    assert_eq!(detail.preview.rows_given_to_model, Some(3));
    assert_eq!(
        detail.raw_json,
        serde_json::to_string_pretty(&listing(3)).expect("json"),
        "the very result the model read, indented"
    );
}

#[test]
fn an_unknown_call_or_one_with_no_result_has_no_detail() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a"), list_dir("c2", "/b")], (10, 1));
    thread.result("c1", listing(1));

    assert!(tool_result_detail(thread.rest_of_turn_at(id), "nope", 0).is_none());
    assert!(
        tool_result_detail(thread.rest_of_turn_at(id), "c2", 0).is_none(),
        "no result row"
    );
}

#[test]
fn a_refusal_shows_its_problem_and_a_set_aside_row_says_so() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a"), list_dir("c2", "/b")], (10, 1));
    thread.result("c1", problem());
    thread.message(
        AgentRole::Tool,
        vec![AgentPart::ToolResult(AgentToolResult {
            call_id: "c2".into(),
            content: json!({ "set": "aside" }),
            elided: true,
        })],
        None,
    );

    let refused = tool_result_detail(thread.rest_of_turn_at(id), "c1", 0).expect("found");
    assert_eq!(refused.preview.problem.as_deref(), Some("list_dir needs path."));
    let set_aside = tool_result_detail(thread.rest_of_turn_at(id), "c2", 0).expect("found");
    assert!(set_aside.preview.set_aside);
    assert!(set_aside.preview.rows.is_empty());
}

/// The fake reuses `fake-call-0` in every round, so each round's row must read its OWN result.
#[test]
fn two_rounds_reusing_a_call_id_each_read_their_own_result() {
    let mut thread = Thread::default();
    thread.user("look");
    let first = thread.assistant(vec![list_dir("fake-call-0", "/a")], (10, 1));
    thread.result("fake-call-0", listing(3));
    let second = thread.assistant(vec![list_dir("fake-call-0", "/b")], (20, 2));
    thread.result("fake-call-0", problem());

    let first = tool_result_detail(thread.rest_of_turn_at(first), "fake-call-0", 0).expect("found");
    let second = tool_result_detail(thread.rest_of_turn_at(second), "fake-call-0", 0).expect("found");
    assert_eq!(first.preview.rows_given_to_model, Some(3));
    assert!(first.preview.problem.is_none());
    assert_eq!(second.preview.problem.as_deref(), Some("list_dir needs path."));
}

#[test]
fn a_row_that_is_not_an_assistant_row_has_no_detail() {
    let mut thread = Thread::default();
    let user = thread.user("look");
    thread.assistant(vec![list_dir("c1", "/a")], (10, 1));
    thread.result("c1", listing(1));

    assert!(tool_result_detail(thread.rest_of_turn_at(user), "c1", 0).is_none());
    assert!(tool_result_detail(Vec::new(), "c1", 0).is_none());
}

/// A provider that names one call id twice in a round: each line reads its own result, by
/// occurrence, the way the page pairs them.
#[test]
fn an_id_named_twice_in_one_round_reads_each_occurrence_own_result() {
    let mut thread = Thread::default();
    thread.user("look");
    let id = thread.assistant(vec![list_dir("c1", "/a"), list_dir("c1", "/b")], (10, 1));
    thread.result("c1", listing(3));
    thread.result("c1", problem());

    let first = tool_result_detail(thread.rest_of_turn_at(id), "c1", 0).expect("found");
    let second = tool_result_detail(thread.rest_of_turn_at(id), "c1", 1).expect("found");
    assert_eq!(first.preview.rows_given_to_model, Some(3));
    assert_eq!(second.preview.problem.as_deref(), Some("list_dir needs path."));
    assert!(
        tool_result_detail(thread.rest_of_turn_at(id), "c1", 2).is_none(),
        "no third call"
    );
}
