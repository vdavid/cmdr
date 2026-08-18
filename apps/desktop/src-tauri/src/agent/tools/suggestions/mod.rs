//! The suggested-ops tool surface: how the agent proposes file operations, and how it reads
//! back what it already proposed.
//!
//! Three tools, sitting on `agent/suggested_ops/` and `agent/store/proposals/`:
//!
//! - `list_suggestions` (`Access::Read`) — sweeps and groups as summaries with counts.
//! - `get_suggestion_group` (`Access::Read`) — one group's ops, paged.
//! - `propose_suggestions` (`Access::Propose`) — stage a sweep, or amend a pending group.
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
mod propose;

#[cfg(test)]
mod tests;

pub use group::{execute_get_suggestion_group, get_suggestion_group_schema};
pub use list::{execute_list_suggestions, list_suggestions_schema};
pub use propose::{execute_propose_suggestions, propose_suggestions_schema};
pub(crate) use propose::propose_in_thread;
