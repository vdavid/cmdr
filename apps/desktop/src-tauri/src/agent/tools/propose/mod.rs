//! Agent-only proposals. A proposal is server-owned data: the agent can stage it, while later
//! user actions decide what to approve, and may replace one row's name with their own.

pub mod evidence;
/// The review surface's own eval: can a human still catch a wrong name? Test-only, since the
/// fixtures and their verdicts exist to be asserted, not called.
#[cfg(test)]
mod name_quality_eval;
pub mod rename;
