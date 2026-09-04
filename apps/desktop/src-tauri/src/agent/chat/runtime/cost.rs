//! Metering one completed `respond` call's cost.
//!
//! Crash case (d): metered per completed `End`, so completed turns count once and are never
//! lost.

use chrono::{FixedOffset, TimeZone, Utc};

use super::LOG_TARGET;
use super::types::TurnParams;
use crate::agent::llm::types::AgentUsage;
use crate::agent::store::{self, CostRecord};

/// Fold one completed `respond` call's usage into the cost meter. Cost is priced via the
/// per-model table ([`crate::agent::pricing`]): a local/on-device model is free and priced, a
/// known cloud model gets an honest estimate, and an unknown cloud model records its tokens
/// with cost 0 but `priced = false` — shown "unknown", never a silent $0 (spec §2.4 honesty).
pub(super) fn meter_cost(conn: &rusqlite::Connection, params: &TurnParams<'_>, usage: AgentUsage) {
    let prompt_tokens = usage.prompt_tokens as u64;
    let completion_tokens = usage.completion_tokens as u64;
    let priced = crate::agent::pricing::price_call(params.provider, &params.model, prompt_tokens, completion_tokens);
    let record = CostRecord {
        day: day_for(params.now_secs, params.offset),
        conversation_id: params.conversation_id,
        provider: params.provider,
        model: params.model.clone(),
        prompt_tokens,
        completion_tokens,
        cost_micros: priced.cost_micros,
        priced: priced.priced,
    };
    if let Err(e) = store::record_cost(conn, &record) {
        log::warn!(target: LOG_TARGET, "metering chat cost failed: {e}");
    }
}

/// The local-day `YYYY-MM-DD` for the cost meter, from the send clock and the captured
/// offset (so the day matches the envelope's local timestamp).
///
/// ⚠️ **The one place a day is computed.** The wake loop's daily ceiling reads the meter back by
/// this key, and a second implementation of "which day is it" would let the two disagree across
/// a midnight or a timezone move.
pub(crate) fn day_for(now_secs: i64, offset: FixedOffset) -> String {
    let utc = Utc
        .timestamp_opt(now_secs, 0)
        .single()
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH);
    utc.with_timezone(&offset).format("%Y-%m-%d").to_string()
}
