//! PII-free PostHog events for the suggestion lifecycle.
//!
//! Acceptance rate is the agent's north-star metric (agent decision D46): a suggestion feature
//! whose suggestions get rejected is worse than none, and the only way to know is to count
//! both. These three events are what answer it.
//!
//! Every property is categorical: the verb and a coarse count bucket. ❌ Never a path, a file
//! name, the agent's rationale, or a selector pattern — all four are the user's own data, and
//! `main.db` is a map of their life that stays local.

use serde_json::json;

use crate::agent::types::ProposalVerb;
use crate::analytics::item_count_bucket;

/// A group was proposed to the user.
pub(super) fn group_proposed(verb: ProposalVerb, op_count: usize) {
    capture("suggestion_group_proposed", verb, op_count);
}

/// The user approved a group, and its operation started. A start the engine refused is not one.
pub(super) fn group_approved(verb: ProposalVerb, op_count: u64) {
    capture("suggestion_group_approved", verb, op_count as usize);
}

/// The user rejected a group.
pub(super) fn group_rejected(verb: ProposalVerb, op_count: u64) {
    capture("suggestion_group_rejected", verb, op_count as usize);
}

fn capture(event: &str, verb: ProposalVerb, op_count: usize) {
    #[cfg(test)]
    CAPTURED.with(|captured| captured.borrow_mut().push(event.to_string()));
    crate::analytics::events::capture(
        event,
        json!({ "verb": verb.as_token(), "op_count": item_count_bucket(op_count) }),
    );
}

#[cfg(test)]
thread_local! {
    /// Every lifecycle event this thread captured. Analytics are suppressed in tests, so this is
    /// how one asks what WOULD have been counted. Per thread, so side-by-side tests never read
    /// each other's events.
    static CAPTURED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Take the event names the calling thread has captured so far, leaving nothing behind.
#[cfg(test)]
pub(crate) fn take_captured() -> Vec<String> {
    CAPTURED.with(|captured| std::mem::take(&mut *captured.borrow_mut()))
}
