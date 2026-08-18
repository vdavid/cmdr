//! PII-free PostHog events for the suggestion lifecycle.
//!
//! Acceptance rate is the agent's north-star metric (agent-spec D46): a suggestion feature
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

/// The user approved a group, and its ops went to the queue.
pub(super) fn group_approved(verb: ProposalVerb, op_count: u64) {
    capture("suggestion_group_approved", verb, op_count as usize);
}

/// The user rejected a group.
pub(super) fn group_rejected(verb: ProposalVerb, op_count: u64) {
    capture("suggestion_group_rejected", verb, op_count as usize);
}

fn capture(event: &str, verb: ProposalVerb, op_count: usize) {
    crate::analytics::posthog::capture(
        event,
        json!({ "verb": verb.as_token(), "op_count": item_count_bucket(op_count) }),
    );
}
