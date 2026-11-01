//! The suggested-ops tool surface: how the agent proposes file operations, and how it reads
//! back what it already proposed.
//!
//! Four tools, sitting on `agent/suggested_ops/` and `agent/store/proposals/`:
//!
//! - `list_suggestions` (`Access::Read`) — sweeps and groups as summaries with counts.
//! - `get_suggestion_group` (`Access::Read`) — one group's ops, paged.
//! - `propose_suggestions` (`Access::Propose`) — stage a sweep, or amend a pending group.
//! - `propose_server` (`Access::Propose`) — stage a server to add, or a change to a saved one.
//!
//! ## Why amend lives inside propose
//!
//! A standalone `amend` tool would mutate stored rows, which the registry's own tiebreaker
//! calls `Access::Write`, and a `Write` tool is unreachable from the agent's view by
//! construction (`test_agent_tool_view_never_writes`). Folding it in keeps the whole surface
//! honest: everything here stages a proposal for the user, and nothing here approves one.
//!
//! ## Scale is the design constraint
//!
//! 60 000 ops in one group is a legitimate group, so no tool here ever returns an op list to
//! answer a question about counts, and the one that does return ops pages and reports what it
//! left out. The agent proposes a set that size with a SELECTOR, which the backend resolves
//! against the drive index once, at creation.
//!
//! Depth: `DETAILS.md`.

mod group;
mod input;
mod list;
mod names;
mod place;
mod problem;
mod propose;
mod server;
mod server_plan;
pub(crate) mod snapshot;

#[cfg(test)]
mod tests;

pub use group::{execute_get_suggestion_group, get_suggestion_group_schema};
pub use list::{execute_list_suggestions, list_suggestions_schema};
pub(crate) use propose::propose_in_thread;
pub use propose::{execute_propose_suggestions, propose_suggestions_schema};
pub(crate) use server::propose_server_in_thread;
pub use server::{execute_propose_server, propose_server_schema};

/// Stage `call` into `conn` over `facts`, the handler's own plan-then-write path minus the app,
/// and answer the first group's id: for tests elsewhere that pin how a staged group reads.
#[cfg(test)]
pub(crate) fn stage_for_test(
    conn: &rusqlite::Connection,
    call: &serde_json::Value,
    facts: &crate::location::resolve::VolumeFacts,
) -> i64 {
    let planned = input::plan_sweep(call, 0, facts).expect("the call is valid");
    let report =
        propose::apply_planned_sweep(conn, &crate::agent::suggested_ops::DriveIndex, planned, None, 0).expect("staged");
    report.groups[0].group_id
}
