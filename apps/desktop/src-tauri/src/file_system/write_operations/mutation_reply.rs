//! What an instant mutation (new folder, new file, rename) answers within its
//! IPC deadline, and how it reports the end it reaches after that.
//!
//! A slow volume (a busy NAS, a phone, a hung kernel mount) can hold a create or
//! a rename for longer than the person should wait without a word. The deadline
//! used to answer `MutationError::TimedOut`, but it bounds only the reply, never
//! the work: the folder usually landed seconds later, after the dialog had said
//! it didn't (ERR-AREUV). So the deadline now answers [`MutationReply::StillRunning`]
//! with a `pending_id`, and the [`MutationSettled`] event carrying that id says
//! how the work really ended: [`MutationSettledOutcome::Landed`], or `Refused`
//! with the same typed `MutationError` an in-time answer would have carried.
//!
//! There's no hard limit behind the deadline. The work's own end is the truth,
//! and a dead session already ends it (smb2 declares a silent server dead); a
//! kernel mount that blocks for minutes keeps the frontend saying "still
//! working" for those minutes, which is what's happening.

use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri_specta::Event;

use super::mutation_error::MutationError;
use crate::deadline::{Raced, race_detached};

/// How long an instant mutation's IPC reply waits before it says "still
/// running". Not a limit on the work: past it the frontend switches from its
/// own immediate "working" state to telling the person the volume is slow, so
/// it's sized to stay clear of a healthy share's create or rename (well under a
/// second) without leaving anyone staring at a frozen dialog.
pub(crate) const MUTATION_REPLY_DEADLINE: Duration = Duration::from_secs(2);

/// The reply of `create_directory`, `create_file`, and `rename_file`. A refusal
/// inside the deadline is the command's `Err(MutationError)`, as before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MutationReply {
    /// It landed.
    Done,
    /// The deadline passed with the work still running. A [`MutationSettled`]
    /// carrying this `pending_id` follows when it ends.
    StillRunning {
        /// Names this one mutation on the settle event.
        pending_id: String,
    },
}

/// `mutation-settled`: how a mutation that answered `StillRunning` ended.
/// Broadcast; the waiting caller picks its own by `pending_id`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct MutationSettled {
    /// The id the `StillRunning` reply carried.
    pub pending_id: String,
    /// How it ended.
    pub outcome: MutationSettledOutcome,
}

/// How a mutation that outlived its deadline ended.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MutationSettledOutcome {
    /// It landed.
    Landed,
    /// It didn't, for this reason: the same refusal an in-time reply carries.
    Refused {
        /// Why.
        error: MutationError,
    },
}

/// Runs `work` detached and answers within `deadline`: its result if it ended
/// by then, else `StillRunning`, handing how it ends to `on_settled` once it does.
///
/// `on_settled` runs only for a `StillRunning` reply, exactly once, from the
/// task that waits the work out. The work itself is never dropped (see
/// `deadline::race_detached`): an MTP rename abandoned mid-transaction wedges the
/// phone.
pub(crate) async fn reply_within<T, W, S>(
    deadline: Duration,
    work: W,
    on_settled: S,
) -> Result<MutationReply, MutationError>
where
    T: Send + 'static,
    W: Future<Output = Result<T, MutationError>> + Send + 'static,
    S: FnOnce(MutationSettled) + Send + 'static,
{
    let replied = reply_or_hand_off(deadline, work, move |pending_id, result| {
        let outcome = match result {
            Ok(_) => MutationSettledOutcome::Landed,
            Err(error) => MutationSettledOutcome::Refused { error },
        };
        on_settled(MutationSettled { pending_id, outcome });
    })
    .await?;
    Ok(match replied {
        Replied::Done(_) => MutationReply::Done,
        Replied::StillRunning { pending_id } => MutationReply::StillRunning { pending_id },
    })
}

/// What [`reply_or_hand_off`] answered: the work's value, or the id its settle
/// will carry.
pub(crate) enum Replied<T> {
    Done(T),
    StillRunning { pending_id: String },
}

/// The value-carrying core of [`reply_within`], for a command whose late end
/// carries more than "landed" (`paste_clipboard_as_file` names the file it made)
/// and so speaks its own reply and settle types.
///
/// `on_settled(pending_id, result)` runs only for a `StillRunning` answer,
/// exactly once, from the task that waits the work out. A panicked task is
/// `Unexpected`.
pub(crate) async fn reply_or_hand_off<T, W, S>(
    deadline: Duration,
    work: W,
    on_settled: S,
) -> Result<Replied<T>, MutationError>
where
    T: Send + 'static,
    W: Future<Output = Result<T, MutationError>> + Send + 'static,
    S: FnOnce(String, Result<T, MutationError>) + Send + 'static,
{
    match race_detached(deadline, work).await {
        Raced::Answered(joined) => flatten_join(joined).map(Replied::Done),
        Raced::StillRunning(handle) => {
            let pending_id = uuid::Uuid::new_v4().to_string();
            let settled_id = pending_id.clone();
            tokio::spawn(async move {
                let result = flatten_join(handle.await);
                match &result {
                    Ok(_) => log::info!(target: "write_ops", "a mutation past its deadline landed: {settled_id}"),
                    Err(e) => {
                        log::info!(target: "write_ops", "a mutation past its deadline was refused: {settled_id} {e}")
                    }
                }
                on_settled(settled_id, result);
            });
            Ok(Replied::StillRunning { pending_id })
        }
    }
}

/// The work's own result, with a panicked task as the `Unexpected` fallback.
fn flatten_join<T>(joined: Result<Result<T, MutationError>, tokio::task::JoinError>) -> Result<T, MutationError> {
    match joined {
        Ok(result) => result,
        Err(join_err) => Err(MutationError::Unexpected {
            detail: join_err.to_string(),
        }),
    }
}

/// The production `on_settled`: broadcasts the settle event (`mutation-settled`,
/// or a command's own, like `clipboard-paste-settled`) to every window.
pub(crate) fn broadcast_settled<E>(app: tauri::AppHandle) -> impl FnOnce(E) + Send + 'static
where
    E: Event + Serialize + Clone + Send + 'static,
{
    move |settled| {
        if let Err(e) = settled.emit(&app) {
            log::warn!(target: "write_ops", "couldn't emit {}: {e}", E::NAME);
        }
    }
}

#[cfg(test)]
#[path = "mutation_reply_tests.rs"]
mod tests;
