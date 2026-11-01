//! The `unlock` tool: answer the password prompt for a locked archive or
//! backup repo.
//!
//! **Initiate, never start.** This is the one thing to keep true here. A person
//! who types the password gets the copy re-dispatched for them; an agent that
//! supplies one must NOT, because that re-dispatch is a brand-new write (the
//! operation that hit the prompt is already settled — `retain_failure` excludes
//! `ArchiveNeedsPassword` by typed variant, so there is nothing to unpark) and a
//! new write goes through the confirmation and the token gate every other copy
//! goes through. So the transfer arm hands over the password, settles what is
//! left on screen, and tells the caller to start the copy again. ❌ Never make
//! this dispatch an operation, however convenient the round trip would be: it
//! would be the only write on this surface with no gate in front of it.
//!
//! The browse arm has no such boundary. Re-listing a directory is a READ, so
//! unlocking completes it, exactly as it does for a person.
//!
//! Shape, following `conflicts.rs`: an answer must NAME what it is answering
//! (`path`, off the `archive-password` entry in `cmdr://state`), the outcome
//! crosses the wire as a typed `outcome` field rather than only as prose, and
//! anything that changed nothing is a refusal. The container's own answer
//! decides the outcome: an archive only stores the password, a repo checks it,
//! and a password the repo refused is a `wrong_password` result that leaves the
//! prompt up and keeps nothing.
//!
//! **The secret's whole path**: the JSON param → `unlock_within` → the
//! container's `PasswordLocked::unlock`. It is never logged, never put in the
//! prompt mirror, never rendered by a resource, never echoed back in the reply,
//! and never saved to the Keychain (an agent's unlock passes `remember: false`).
//! The event this tool then emits to the frontend carries no payload at all —
//! the frontend re-reads through the backend, so the password never crosses
//! into the webview.

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::commands::file_system::{UnlockError, unlock_within};
use crate::file_system::volume::UnlockOutcome;
use crate::ui_state::archive_password::{ArchivePasswordPrompt, ArchivePasswordPromptStore, ArchivePromptMode};

use super::{AckSignal, DEFAULT_ACK_TIMEOUT, ToolError, ToolResult, expand_user_path, wait_for_ack};

/// The soft-dialog id the prompt registers under, shared by the ack and the
/// operation-start gate's `blockingDialog`.
const PROMPT_DIALOG_ID: &str = "archive-password";

/// How long the tool lets a container check a password: a repo's key
/// derivation plus its open over a slow NAS.
const UNLOCK_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

pub async fn execute_unlock<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let (path, password) = parse_params(params)?;

    let (prompt, generation) = live_prompt(app)?;
    // Required, never "whatever is asking": between the read of `cmdr://state`
    // and this call the prompt may have been answered elsewhere and a different
    // container may be asking now. Same reason `resolve_conflict` demands its
    // `conflictId`.
    if prompt.archive_path != path {
        return Err(different_container(&prompt, &path));
    }

    // An agent's unlock never saves to the Keychain: that's the person's call,
    // made with the dialog's checkbox.
    let outcome = unlock_within(
        &prompt.parent_volume_id,
        std::path::Path::new(&prompt.archive_path),
        password,
        false,
        &CancellationToken::new(),
        UNLOCK_BUDGET,
    )
    .await
    .map_err(unlock_refused)?;

    if outcome == UnlockOutcome::WrongPassword {
        // Nothing was kept, so the prompt stays up as it is and the frontend
        // has nothing to do.
        return Ok(report(&prompt, outcome));
    }

    // The frontend takes the prompt down and does the mode's follow-up. It reads
    // the password back from the container, so nothing about it rides this event.
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

    Ok(report(&prompt, outcome))
}

/// The two required params: which container is being answered, and with what.
///
/// Both are refused rather than defaulted. A `path` guessed from "whatever is
/// asking" is the mistake `resolve_conflict`'s `conflictId` exists to prevent,
/// and an empty password would spend the one attempt a caller has before the
/// prompt comes back saying it was wrong.
fn parse_params(params: &Value) -> Result<(String, String), ToolError> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .map(expand_user_path)
        .ok_or_else(|| {
            ToolError::invalid_params(
                "Missing 'path' parameter: name the archive or repo being asked about, using the path on the \
                 archive-password entry in cmdr://state dialogs.",
            )
        })?;
    let password = params
        .get("password")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| ToolError::invalid_params("Missing 'password' parameter."))?
        .to_string();
    Ok((path, password))
}

/// The prompt currently on screen with the mirror generation it was read at, or
/// the refusal for "nothing is asking".
fn live_prompt<R: Runtime>(app: &AppHandle<R>) -> Result<(ArchivePasswordPrompt, u64), ToolError> {
    app.try_state::<ArchivePasswordPromptStore>()
        .and_then(|store| store.get().map(|prompt| (prompt, store.generation())))
        .ok_or_else(|| {
            ToolError::invalid_params(
                "Nothing is asking for a password. Either nothing hit an encrypted archive or a locked backup \
                 repo, or the prompt was already answered or cancelled. Read cmdr://state dialogs.",
            )
            .with_data(json!({ "outcome": "no_password_prompt" }))
        })
}

/// A refusal that names what IS being asked, so the caller can retry against the
/// live prompt rather than guess.
fn different_container(prompt: &ArchivePasswordPrompt, asked_for: &str) -> ToolError {
    ToolError::invalid_params(format!(
        "The prompt is asking about '{}', not '{asked_for}', so nothing was tried. \
         Answer the one that is up, naming its path from cmdr://state dialogs.",
        prompt.archive_path
    ))
    .with_data(json!({
        "outcome": "different_container",
        "container": prompt.container.token(),
        "name": prompt.archive_name,
        "path": prompt.archive_path,
        "mode": prompt.mode.token(),
    }))
}

/// The container couldn't be asked at all. Typed per reason, and never carrying
/// the password (the error never held it).
fn unlock_refused(error: UnlockError) -> ToolError {
    match error {
        UnlockError::NotLocked | UnlockError::VolumeNotFound { .. } => ToolError::invalid_params(
            "The prompt's path no longer leads to anything locked, so nothing was tried. Read cmdr://state.",
        )
        .with_data(json!({ "outcome": "not_locked" })),
        UnlockError::TimedOut => {
            ToolError::internal("Checking the password took too long; nothing was kept. Try again.")
                .with_data(json!({ "outcome": "timed_out" }))
        }
        UnlockError::Cancelled | UnlockError::Volume { .. } => {
            ToolError::internal("Couldn't reach the archive or repo to check the password; nothing was kept.")
                .with_data(json!({ "outcome": "unreachable" }))
        }
    }
}

/// What the unlock did, worded per mode and per the container's answer.
///
/// A browse and a transfer are not the same event and must not read as one. A
/// browse is finished: the listing is being re-read and an agent can carry on.
/// A transfer is NOT: the password is held and nothing is running, and an agent
/// that reads this as "the copy resumed" walks away from a copy that never
/// happened. A password the container only STORED is unproven, so that report
/// says how to find out whether it was right; a checked one says it was.
fn report(prompt: &ArchivePasswordPrompt, unlock: UnlockOutcome) -> Value {
    let name = &prompt.archive_name;
    let (outcome, message) = match (unlock, prompt.mode) {
        (UnlockOutcome::WrongPassword, _) => (
            "wrong_password",
            format!(
                "{name} refused that password, so nothing was kept and the prompt is still up. Try another \
                 password, or close the dialog to give up."
            ),
        ),
        (UnlockOutcome::Stored, ArchivePromptMode::Browse) => (
            "retrying_listing",
            format!(
                "Stored the password for {name} and re-read its listing; browsing is a read, so nothing else is \
                 needed. Read cmdr://state: the pane shows the contents if the password was right, and the \
                 prompt is back with wrongAttempt: true if it wasn't."
            ),
        ),
        (UnlockOutcome::Unlocked, ArchivePromptMode::Browse) => (
            "retrying_listing",
            format!(
                "Unlocked {name}: the password was right, and its listing is being re-read; browsing is a read, \
                 so nothing else is needed."
            ),
        ),
        (UnlockOutcome::Stored, ArchivePromptMode::Transfer) => (
            "password_stored",
            format!(
                "Stored the password for {name}. NOTHING is running: the copy that hit the prompt was already \
                 over (a password failure settles it), and supplying a password doesn't start a write. Run copy \
                 or move again to extract; it goes through the usual confirmation, and raises this prompt again \
                 with wrongAttempt: true if the password was wrong."
            ),
        ),
        (UnlockOutcome::Unlocked, ArchivePromptMode::Transfer) => (
            "password_stored",
            format!(
                "Unlocked {name}: the password was right. NOTHING is running: the copy that hit the prompt was \
                 already over (a password failure settles it), and supplying a password doesn't start a write. \
                 Run copy or move again; it goes through the usual confirmation."
            ),
        ),
    };
    json!({
        "outcome": outcome,
        "verified": unlock != UnlockOutcome::Stored,
        "container": prompt.container.token(),
        "name": name,
        "path": prompt.archive_path,
        "mode": prompt.mode.token(),
        "message": message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_state::archive_password::LockedContainer;

    fn prompt(mode: ArchivePromptMode) -> ArchivePasswordPrompt {
        ArchivePasswordPrompt {
            archive_name: "photos.zip".to_string(),
            archive_path: "/tmp/photos.zip".to_string(),
            parent_volume_id: "drive-1".to_string(),
            container: LockedContainer::Archive,
            mode,
            wrong_attempt: false,
            operation_id: Some("op-3".to_string()),
        }
    }

    fn message(report: &Value) -> String {
        report["message"]
            .as_str()
            .expect("the report carries a message")
            .to_string()
    }

    const MODES: [ArchivePromptMode; 2] = [ArchivePromptMode::Browse, ArchivePromptMode::Transfer];

    #[test]
    fn each_mode_reports_a_distinct_typed_outcome() {
        // An agent that can't tell a finished browse from a held-but-unused
        // password acts on a copy that never ran. Holds whether the container
        // checked the password or only stored it.
        for unlock in [UnlockOutcome::Stored, UnlockOutcome::Unlocked] {
            assert_eq!(
                report(&prompt(ArchivePromptMode::Browse), unlock)["outcome"],
                "retrying_listing"
            );
            assert_eq!(
                report(&prompt(ArchivePromptMode::Transfer), unlock)["outcome"],
                "password_stored"
            );
        }
    }

    #[test]
    fn a_refused_password_is_a_typed_result_that_kept_nothing() {
        for mode in MODES {
            let answer = report(&prompt(mode), UnlockOutcome::WrongPassword);
            assert_eq!(answer["outcome"], "wrong_password", "for {mode:?}");
            assert!(message(&answer).contains("nothing was kept"), "for {mode:?}");
        }
    }

    #[test]
    fn only_a_checked_password_reports_verified() {
        assert_eq!(
            report(&prompt(ArchivePromptMode::Browse), UnlockOutcome::Stored)["verified"],
            false
        );
        assert_eq!(
            report(&prompt(ArchivePromptMode::Browse), UnlockOutcome::Unlocked)["verified"],
            true
        );
    }

    #[test]
    fn a_transfer_unlock_says_nothing_started_and_names_what_starts_it() {
        // This sentence is the boundary in words. The behavior is enforced by
        // the handler dispatching no operation; this keeps the caller from
        // assuming otherwise.
        for unlock in [UnlockOutcome::Stored, UnlockOutcome::Unlocked] {
            let sentence = message(&report(&prompt(ArchivePromptMode::Transfer), unlock));
            assert!(sentence.contains("NOTHING is running"), "{sentence}");
            assert!(sentence.contains("copy or move"), "{sentence}");
        }
    }

    #[test]
    fn a_stored_password_says_how_to_learn_it_was_wrong() {
        // Storing a password proves nothing about it. Both modes point at the
        // one observable that settles it.
        for mode in MODES {
            let sentence = message(&report(&prompt(mode), UnlockOutcome::Stored));
            assert!(sentence.contains("wrongAttempt: true"), "for {mode:?}: {sentence}");
        }
    }

    #[test]
    fn the_report_names_the_container_kind() {
        let mut repo = prompt(ArchivePromptMode::Browse);
        repo.container = LockedContainer::ResticRepo;
        assert_eq!(report(&repo, UnlockOutcome::Unlocked)["container"], "resticRepo");
    }

    #[test]
    fn no_report_ever_carries_the_password() {
        // The secret's path ends at the container. A reply is a transcript
        // line, and a transcript is the last place it belongs.
        for unlock in [
            UnlockOutcome::Stored,
            UnlockOutcome::Unlocked,
            UnlockOutcome::WrongPassword,
        ] {
            let rendered = report(&prompt(ArchivePromptMode::Transfer), unlock).to_string();
            assert!(!rendered.contains("hunter2"), "{rendered}");
            assert!(rendered.contains("photos.zip"), "{rendered}");
        }
    }

    #[test]
    fn answering_a_different_container_is_a_refusal_carrying_the_live_one() {
        let error = different_container(&prompt(ArchivePromptMode::Browse), "/tmp/other.zip");
        let data = error.data.expect("the refusal carries data");
        assert_eq!(data["outcome"], "different_container");
        assert_eq!(data["path"], "/tmp/photos.zip");
    }

    #[test]
    fn every_refusal_reason_is_typed() {
        let outcome =
            |error: UnlockError| unlock_refused(error).data.expect("the refusal carries data")["outcome"].clone();
        assert_eq!(outcome(UnlockError::NotLocked), "not_locked");
        assert_eq!(outcome(UnlockError::TimedOut), "timed_out");
        assert_eq!(
            outcome(UnlockError::Volume {
                error: crate::file_system::volume::VolumeError::DeviceDisconnected("gone".into())
            }),
            "unreachable"
        );
    }

    #[test]
    fn an_answer_that_names_nothing_is_refused_before_anything_is_tried() {
        assert!(
            parse_params(&json!({ "password": "hunter2" })).is_err(),
            "a missing path can't be guessed"
        );
    }

    #[test]
    fn an_empty_password_is_refused_rather_than_tried() {
        assert!(
            parse_params(&json!({ "path": "/tmp/photos.zip", "password": "" })).is_err(),
            "an empty password can't unlock anything"
        );
        assert!(
            parse_params(&json!({ "path": "/tmp/photos.zip" })).is_err(),
            "and neither can a missing one"
        );
    }

    #[test]
    fn a_tilde_path_resolves_the_way_every_other_path_param_does() {
        // Agents routinely send `~/…`; a literal tilde would never match the
        // prompt and would spend the whole call on a confusing refusal.
        let (path, password) =
            parse_params(&json!({ "path": "~/photos.zip", "password": "hunter2" })).expect("both params parse");
        assert!(!path.starts_with('~'), "{path}");
        assert_eq!(password, "hunter2");
    }
}
