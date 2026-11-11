//! `multi_rename_applied`: one event per apply (the sheet's Start, or the MCP
//! tool's), with what came of it, from `AppHost::rename_applied`, which the engine's
//! `MultiRename::apply` calls at its single exit.
//!
//! Categories and a bucketed count only: which rule kinds the spec used (the
//! engine's fixed tokens), how many rows the rename took, and the typed outcome.
//! Never a mask, a search, a replacement, or a file name.

use serde_json::{Value, json};

use super::error::MultiRenameError;
use super::run::MultiRenameStarted;
#[cfg(test)]
use crate::ignore_poison::IgnorePoison;

/// Reports one apply that ended with `result`, from a spec using `rules`.
pub(crate) fn emit_applied(rules: &[&'static str], result: &Result<MultiRenameStarted, MultiRenameError>) {
    let props = applied_props(rules, result);
    #[cfg(test)]
    APPLIED_FOR_TESTS.lock_ignore_poison().push(props.clone());
    crate::analytics::events::capture("multi_rename_applied", props);
}

/// Every apply's props this test binary reported, so a test can see the engine's
/// exit reach the capture (`capture` itself is suppressed in a debug build).
#[cfg(test)]
pub(super) static APPLIED_FOR_TESTS: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

/// `multi_rename_applied`'s props, built apart from the capture so a test pins them.
pub(crate) fn applied_props(rules: &[&'static str], result: &Result<MultiRenameStarted, MultiRenameError>) -> Value {
    let rules = if rules.is_empty() {
        "none".to_string()
    } else {
        rules.join(",")
    };
    let renaming = result.as_ref().map_or(0, |started| started.renaming);
    json!({
        "item_count": crate::analytics::item_count_bucket(renaming),
        "rules": rules,
        "outcome": outcome_token(result),
    })
}

/// `started` for a rename that began, else the refusal's typed variant. Exhaustive,
/// so a new refusal decides what it reports.
fn outcome_token(result: &Result<MultiRenameStarted, MultiRenameError>) -> &'static str {
    match result {
        Ok(_) => "started",
        Err(error) => match error {
            MultiRenameError::PreviewOutOfDate => "preview_out_of_date",
            MultiRenameError::StillReading => "still_reading",
            MultiRenameError::NothingToRename => "nothing_to_rename",
            MultiRenameError::ReadOnly { .. } => "read_only",
            MultiRenameError::NotConnected { .. } => "not_connected",
            MultiRenameError::CouldntStart { .. } => "couldnt_start",
            MultiRenameError::TimedOut => "timed_out",
            MultiRenameError::SessionClosed => "session_closed",
            MultiRenameError::Spec { .. } => "spec_invalid",
            MultiRenameError::Gone { .. } | MultiRenameError::SelectionChanged { .. } => "selection_gone",
            MultiRenameError::CouldntWriteNames { .. }
            | MultiRenameError::NamesFileGone { .. }
            | MultiRenameError::Internal { .. } => "failed",
        },
    }
}
