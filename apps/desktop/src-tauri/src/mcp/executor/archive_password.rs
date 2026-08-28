//! The `unlock_archive` tool: answer the encrypted-archive password prompt.
//!
//! **Initiate, never start.** This is the one thing to keep true here. A person
//! who types the password gets the copy re-dispatched for them; an agent that
//! supplies one must NOT, because that re-dispatch is a brand-new write (the
//! operation that hit the prompt is already settled — `record_failure` excludes
//! `ArchiveNeedsPassword` by typed variant, so there is nothing to unpark) and a
//! new write goes through the confirmation and the token gate every other copy
//! goes through. So the transfer arm stores the password, settles what is left
//! on screen, and tells the caller to start the copy again. ❌ Never make this
//! dispatch an operation, however convenient the round trip would be: it would
//! be the only write on this surface with no gate in front of it.
//!
//! The browse arm has no such boundary. Re-listing a directory is a READ, so
//! unlocking completes it, exactly as it does for a person.
//!
//! Shape, following `conflicts.rs`: an answer must NAME what it is answering
//! (`archivePath`, off the `archive-password` entry in `cmdr://state`), the
//! outcome crosses the wire as a typed `outcome` field rather than only as
//! prose, and anything that changed nothing is a refusal.
//!
//! **The secret's whole path**: the JSON param → `store_archive_password` →
//! the archive volume's `Zeroizing` slot. It is never logged, never put in the
//! prompt mirror, never rendered by a resource, and never echoed back in the
//! reply. The event this tool then emits to the frontend carries no payload at
//! all — the frontend re-reads through the backend, so the password never
//! crosses into the webview.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::mcp::archive_password::{ArchivePasswordPrompt, ArchivePasswordPromptStore, ArchivePromptMode};

use super::{AckSignal, DEFAULT_ACK_TIMEOUT, ToolError, ToolResult, expand_user_path, wait_for_ack};

/// The soft-dialog id the prompt registers under, shared by the ack and the
/// operation-start gate's `blockingDialog`.
const PROMPT_DIALOG_ID: &str = "archive-password";

pub async fn execute_unlock_archive<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let (archive_path, password) = parse_params(params)?;

    let (prompt, generation) = live_prompt(app)?;
    // Required, never "whatever is asking": between the read of `cmdr://state`
    // and this call the prompt may have been answered elsewhere and a different
    // archive may be asking now. Same reason `resolve_conflict` demands its
    // `conflictId`.
    if prompt.archive_path != archive_path {
        return Err(different_archive(&prompt, &archive_path));
    }

    crate::commands::file_system::store_archive_password(
        &prompt.parent_volume_id,
        &prompt.archive_path,
        password,
    )
    .await
    .map_err(ToolError::internal)?;

    // The frontend takes the prompt down and does the mode's follow-up. It reads
    // the password back from the volume, so nothing about it rides this event.
    app.emit("mcp-confirm-dialog", json!({ "type": PROMPT_DIALOG_ID }))?;
    // ❌ Not "the dialog closed": a browse unlock re-lists at once, and a wrong
    // password puts the prompt back up fast enough that it may never unmount, so
    // that wait would time out on a flow that worked. The mirror's generation
    // moves whichever way it went.
    wait_for_ack(
        app,
        AckSignal::ArchivePromptAdvanced { from: generation },
        DEFAULT_ACK_TIMEOUT,
    )
    .await?;

    Ok(report(&prompt))
}

/// The two required params: which archive is being answered, and with what.
///
/// Both are refused rather than defaulted. An `archivePath` guessed from
/// "whatever is asking" is the mistake `resolve_conflict`'s `conflictId` exists
/// to prevent, and an empty password would spend the one attempt a caller has
/// before the prompt comes back saying it was wrong.
fn parse_params(params: &Value) -> Result<(String, String), ToolError> {
    let archive_path = params
        .get("archivePath")
        .and_then(Value::as_str)
        .map(expand_user_path)
        .ok_or_else(|| {
            ToolError::invalid_params(
                "Missing 'archivePath' parameter: name the archive being asked about, using the archivePath \
                 on the archive-password entry in cmdr://state dialogs.",
            )
        })?;
    let password = params
        .get("password")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'password' parameter."))?
        .to_string();
    Ok((archive_path, password))
}

/// The prompt currently on screen with the mirror generation it was read at, or
/// the refusal for "nothing is asking".
fn live_prompt<R: Runtime>(app: &AppHandle<R>) -> Result<(ArchivePasswordPrompt, u64), ToolError> {
    app.try_state::<ArchivePasswordPromptStore>()
        .and_then(|store| store.get().map(|prompt| (prompt, store.generation())))
        .ok_or_else(|| {
            ToolError::invalid_params(
                "No archive is asking for a password. Either nothing hit an encrypted archive, or the prompt \
                 was already answered or cancelled. Read cmdr://state dialogs.",
            )
            .with_data(json!({ "outcome": "no_password_prompt" }))
        })
}

/// A refusal that names what IS being asked, so the caller can retry against the
/// live prompt rather than guess.
fn different_archive(prompt: &ArchivePasswordPrompt, asked_for: &str) -> ToolError {
    ToolError::invalid_params(format!(
        "The prompt is asking about '{}', not '{asked_for}', so nothing was stored. \
         Answer the one that is up, naming its archivePath from cmdr://state dialogs.",
        prompt.archive_path
    ))
    .with_data(json!({
        "outcome": "different_archive",
        "archive": prompt.archive_name,
        "archivePath": prompt.archive_path,
        "mode": prompt.mode.token(),
    }))
}

/// What the unlock did, worded per mode.
///
/// The two are not the same event and must not read as one. A browse is
/// finished: the listing is being re-read and an agent can carry on. A transfer
/// is NOT: the password is stored and nothing is running, and an agent that
/// reads this as "the copy resumed" walks away from a copy that never happened.
/// Both say how to find out whether the password was even right, which is the
/// one thing neither can answer yet.
fn report(prompt: &ArchivePasswordPrompt) -> Value {
    let outcome = match prompt.mode {
        ArchivePromptMode::Browse => "retrying_listing",
        ArchivePromptMode::Transfer => "password_stored",
    };
    let message = match prompt.mode {
        ArchivePromptMode::Browse => format!(
            "Stored the password for {} and re-read its listing; browsing is a read, so nothing else is needed. \
             Read cmdr://state: the pane shows the contents if the password was right, and the prompt is back \
             with wrongAttempt: true if it wasn't.",
            prompt.archive_name
        ),
        ArchivePromptMode::Transfer => format!(
            "Stored the password for {}. NOTHING is running: the copy that hit the prompt was already over \
             (a password failure settles it), and supplying a password doesn't start a write. Run copy or move \
             again to extract; it goes through the usual confirmation, and raises this prompt again with \
             wrongAttempt: true if the password was wrong.",
            prompt.archive_name
        ),
    };
    json!({
        "outcome": outcome,
        "archive": prompt.archive_name,
        "archivePath": prompt.archive_path,
        "mode": prompt.mode.token(),
        "message": message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(mode: ArchivePromptMode) -> ArchivePasswordPrompt {
        ArchivePasswordPrompt {
            archive_name: "photos.zip".to_string(),
            archive_path: "/tmp/photos.zip".to_string(),
            parent_volume_id: "drive-1".to_string(),
            mode,
            wrong_attempt: false,
            operation_id: Some("op-3".to_string()),
        }
    }

    #[test]
    fn each_mode_reports_a_distinct_typed_outcome() {
        // An agent that can't tell a finished browse from a stored-but-unused
        // password acts on a copy that never ran.
        assert_eq!(report(&prompt(ArchivePromptMode::Browse))["outcome"], "retrying_listing");
        assert_eq!(
            report(&prompt(ArchivePromptMode::Transfer))["outcome"],
            "password_stored"
        );
    }

    #[test]
    fn a_transfer_unlock_says_nothing_started_and_names_what_starts_it() {
        // This sentence is the boundary in words. The behavior is enforced by
        // the handler dispatching no operation; this keeps the caller from
        // assuming otherwise.
        let sentence = report(&prompt(ArchivePromptMode::Transfer))["message"]
            .as_str()
            .expect("the report carries a message")
            .to_string();
        assert!(sentence.contains("NOTHING is running"), "{sentence}");
        assert!(sentence.contains("copy or move"), "{sentence}");
    }

    #[test]
    fn every_report_says_how_to_learn_the_password_was_wrong() {
        // Storing a password proves nothing about it. Both modes point at the
        // one observable that settles it.
        for mode in [ArchivePromptMode::Browse, ArchivePromptMode::Transfer] {
            let sentence = report(&prompt(mode))["message"]
                .as_str()
                .expect("the report carries a message")
                .to_string();
            assert!(sentence.contains("wrongAttempt: true"), "for {mode:?}: {sentence}");
        }
    }

    #[test]
    fn no_report_ever_carries_the_password() {
        // The secret's path ends at the volume's slot. A reply is a transcript
        // line, and a transcript is the last place it belongs.
        let rendered = report(&prompt(ArchivePromptMode::Transfer)).to_string();
        assert!(!rendered.contains("hunter2"), "{rendered}");
        assert!(rendered.contains("photos.zip"), "{rendered}");
    }

    #[test]
    fn answering_a_different_archive_is_a_refusal_carrying_the_live_one() {
        let error = different_archive(&prompt(ArchivePromptMode::Browse), "/tmp/other.zip");
        let data = error.data.expect("the refusal carries data");
        assert_eq!(data["outcome"], "different_archive");
        assert_eq!(data["archivePath"], "/tmp/photos.zip");
    }

    #[test]
    fn an_answer_that_names_no_archive_is_refused_before_anything_is_stored() {
        assert!(
            parse_params(&json!({ "password": "hunter2" })).is_err(),
            "a missing archivePath can't be guessed"
        );
    }

    #[test]
    fn an_empty_password_is_refused_rather_than_tried() {
        assert!(
            parse_params(&json!({ "archivePath": "/tmp/photos.zip", "password": "" })).is_err(),
            "an empty password can't unlock anything"
        );
        assert!(
            parse_params(&json!({ "archivePath": "/tmp/photos.zip" })).is_err(),
            "and neither can a missing one"
        );
    }

    #[test]
    fn a_tilde_path_resolves_the_way_every_other_path_param_does() {
        // Agents routinely send `~/…`; a literal tilde would never match the
        // prompt and would spend the whole call on a confusing refusal.
        let (archive_path, password) =
            parse_params(&json!({ "archivePath": "~/photos.zip", "password": "hunter2" })).expect("both params parse");
        assert!(!archive_path.starts_with('~'), "{archive_path}");
        assert_eq!(password, "hunter2");
    }
}
