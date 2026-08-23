//! The wake path: a turn the agent started, in a thread it opened for itself.
//!
//! It goes through `ChatRuntime` exactly as a rail send does, and these tests are what pin
//! that. The fixtures come from the sibling module.

use tokio::sync::mpsc::unbounded_channel;
use tokio_util::sync::CancellationToken;

use super::tests::{OkDispatcher, params, runtime_with_stamped_conversation};
use crate::agent::chat::runtime::{AgentChatEvent, TurnResult};
use crate::agent::llm::fake::{FakeAgentLlm, ScriptedTurn};
use crate::agent::store;

/// A wake goes through `ChatRuntime` like every other turn: the rail never calls `run_turn`
/// directly, and neither may the agent. What that buys here is the write connection, the
/// single-flight guard, and the persistence contract, unchanged between the user asking and
/// the agent noticing.
#[tokio::test]
async fn a_wake_runs_its_turn_in_the_thread_it_was_handed() {
    let (_dir, runtime, _stamped) = runtime_with_stamped_conversation();
    // A thread with no completed turn yet, the way the prepare step hands one over.
    let conn = store::open_write_connection(&runtime.db_path).expect("open");
    let id = store::create_conversation(&conn, "Downloads", 100, None).expect("create");
    drop(conn);
    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["Four files arrived.".to_string()])]);
    let (sink, _rx) = unbounded_channel();

    let result = runtime
        .wake(
            &llm,
            &OkDispatcher,
            &[],
            &params(id, Some("4 new in ~/Downloads")),
            &sink,
            &CancellationToken::new(),
        )
        .await
        .expect("the store took the turn");

    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 2, "the digest and the answer");
}

/// ⚠️ The CONVERSATION lock is held across a wake's turn, taken on the wake thread. A wake
/// thread is a real conversation the user can reply to, so skipping `ConversationLocks` would
/// let a reply and the wake's own turn run concurrently in one thread.
#[tokio::test]
async fn a_wake_queues_behind_whatever_already_holds_its_thread() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["Four files arrived.".to_string()])]);
    let (sink, mut rx) = unbounded_channel();

    // Stand in for a rail send already running in this thread.
    let guard = runtime.locks.acquire_quiet(id).await;

    let turn = params(id, Some("4 new in ~/Downloads"));
    let cancel = CancellationToken::new();
    let waking = runtime.wake(&llm, &OkDispatcher, &[], &turn, &sink, &cancel);
    // Releasing only once `Queued` has landed proves the wake actually waited for the lock
    // rather than racing past it.
    let releasing = async {
        let queued = rx.recv().await;
        assert!(matches!(queued, Some(AgentChatEvent::Queued)), "{queued:?}");
        drop(guard);
    };

    let (result, ()) = tokio::join!(waking, releasing);
    assert!(
        matches!(result.expect("the store took the turn"), TurnResult::Answered { .. }),
        "the wake ran once the thread was free"
    );
}
