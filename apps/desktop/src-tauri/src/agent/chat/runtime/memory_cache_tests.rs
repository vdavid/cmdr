//! A memory write must not cost the chat its prompt cache.
//!
//! A caching provider bills the prompt from its first changed byte at full price, so anything
//! that moves between two calls has to sit AFTER everything that doesn't. Memory used to lead
//! the system string: one `memory_write` and the next call re-read the whole thread at full
//! price (about $1 a write on a 200k-token Opus chat). These tests run two real turns with the
//! notes changed in between and compare what the provider was handed.

use super::test_support::*;
use super::*;
use crate::agent::llm::types::MemoryNote;

fn answer(text: &str) -> Program {
    Program::Answer {
        chunks: vec![text.to_string()],
        usage: AgentUsage::default(),
    }
}

fn tools() -> Vec<ToolDeclaration> {
    vec![ToolDeclaration {
        name: ToolId::AppState,
        description: "what the app shows".to_string(),
        schema: json!({ "type": "object" }),
    }]
}

/// Everything a message says to the provider, as text.
fn spoken(message: &AgentMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) | AgentPart::Envelope(text) => Some(text.clone()),
            AgentPart::MemoryNotes(notes) => Some(notes.render()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn hub(text: &str) -> Vec<MemoryNote> {
    vec![MemoryNote {
        file: "AGENTS.md".to_string(),
        text: text.to_string(),
    }]
}

const BEFORE: &str = "Keeps invoices by year.";
const AFTER: &str = "Keeps invoices by year.\nFiles screenshots under Pictures/Screens.";

/// Run two turns in one thread, with the notes `first` then `second`, and hand back the LLM
/// that saw both calls.
async fn two_turns(first: &[MemoryNote], second: &[MemoryNote]) -> ProgrammableLlm {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let tools = tools();
    let llm = ProgrammableLlm::new(vec![answer("Sure, sorted by year."), answer("Done.")]);
    let (tx, _rx) = unbounded_channel();

    let mut one = params(id, Some("tidy my Downloads"));
    one.memory = first;
    let result = run_turn(&llm, &OkDispatcher, &conn, &tools, &one, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the first turn answers");

    let mut two = params(id, Some("and the Desktop?"));
    two.memory = second;
    two.now_secs += 60;
    let result = run_turn(&llm, &OkDispatcher, &conn, &tools, &two, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the second turn answers");
    llm
}

/// The bug this file exists for: the agent saves a note, and the next call's cached prefix
/// (system prompt, tool declarations, and every earlier message) must reach the provider
/// byte for byte as the previous call sent it.
#[tokio::test]
async fn a_memory_write_between_calls_leaves_the_cached_prefix_byte_identical() {
    let llm = two_turns(&hub(BEFORE), &hub(AFTER)).await;

    let prefixes = llm.prefixes_seen();
    assert_eq!(prefixes.len(), 2, "two calls went out");
    // `assert!` rather than `assert_eq!`: a mismatch would print the whole system prompt twice.
    assert!(
        prefixes[0].0 == prefixes[1].0,
        "a memory write must not change the system string: it leads the cached prefix"
    );
    assert_eq!(prefixes[0].1, prefixes[1].1, "nor the tool declarations");

    let calls = llm.calls_seen();
    let earlier = &calls[0];
    assert_eq!(
        calls[1][..earlier.len()],
        earlier[..],
        "every message the previous call sent must go out again unchanged"
    );
}

/// The other half of the promise: moving memory out of the prefix must not leave the model
/// reading stale notes. The new note reaches the very next call, after the cached part.
#[tokio::test]
async fn the_next_call_still_reads_the_note_just_saved() {
    let llm = two_turns(&hub(BEFORE), &hub(AFTER)).await;

    let calls = llm.calls_seen();
    let latest_user = calls[1]
        .iter()
        .rev()
        .find(|message| message.role == AgentRole::User)
        .expect("the second turn's user message");
    assert!(
        spoken(latest_user).contains("Files screenshots under Pictures/Screens."),
        "the latest user turn must carry the updated notes"
    );
    assert!(
        !llm.prefixes_seen()[1].0.contains("Pictures/Screens"),
        "and the system string must not"
    );
}

/// A thread whose notes didn't change between turns carries them once, on the turn that first
/// saw them, so an unchanged memory costs nothing per turn beyond its cached copy.
#[tokio::test]
async fn unchanged_notes_ride_only_the_first_turn() {
    let llm = two_turns(&hub(BEFORE), &hub(BEFORE)).await;

    let calls = llm.calls_seen();
    let carrying: Vec<bool> = calls[1]
        .iter()
        .filter(|message| message.role == AgentRole::User)
        .map(|message| {
            message
                .parts
                .iter()
                .any(|part| matches!(part, AgentPart::MemoryNotes(_)))
        })
        .collect();
    assert_eq!(carrying, [true, false]);
}
