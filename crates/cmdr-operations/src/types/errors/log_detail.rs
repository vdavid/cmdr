//! How a `WriteOperationError` reads in a log line: its JSON with every field
//! marked by kind, so a report pass knows what each value holds.

use super::WriteOperationError;

impl WriteOperationError {
    /// The error for a log line's `detail={:?}` field: the JSON the frontend gets, with each
    /// field marked by its kind (`log_field`) and each `message` written as its `ErrorDetail`'s
    /// marked log text, so a typed cause marks its own values. Its derived `Debug` would print
    /// `volume_name: "…"` under snake_case keys no report pass knows; the JSON's keys are the
    /// ones a frontend line carries too, so the same error ships the same way from either side.
    /// The frontend's JSON itself never carries a marker.
    pub fn log_detail(&self) -> WriteErrorDetail {
        WriteErrorDetail(cmdr_fs::log_json::log_json(self, log_field))
    }
}

/// The one text behind `Display` and `LogError`: the JSON IPC carries, each field marked in a
/// log line. It's what lets an error that WRAPS a `WriteOperationError` (a `MutationError`'s
/// detail, a refusal folded into a `VolumeError`) keep it typed as an `ErrorDetail`, so its
/// paths stay marked. A log site printing the error itself uses `log_detail()`, whose
/// `detail=` field is capped and keyed the way the frontend's lines are.
impl cmdr_fs::log_error::ErrorLogText for WriteOperationError {
    fn write_log_text(&self, out: &mut cmdr_fs::log_error::ErrorText<'_>) -> std::fmt::Result {
        out.json(self, log_field)
    }
}

/// The plain JSON, for technical details and tests. ❗ Never prose: the frontend words every
/// variant (`transfer-error-messages.ts`).
impl std::fmt::Display for WriteOperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        cmdr_fs::log_error::write_plain(self, f)
    }
}

/// [`WriteOperationError::log_detail`]: a log line's `detail={:?}` value, never data.
pub struct WriteErrorDetail(cmdr_fs::log_json::LoggedJson);

impl std::fmt::Debug for WriteErrorDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What each of `WriteOperationError`'s JSON fields holds, by its camelCase key, for
/// [`WriteOperationError::log_detail`] (nested `cause`, `kept`, `recovered`, and `files`
/// included). ❗ Fails closed: a string under a key not named here is marked as a path, so a
/// new field leaks nothing, it only hashes until it's named.
fn log_field(key: &str) -> cmdr_fs::log_json::JsonField {
    use cmdr_fs::log_json::JsonField::{Detail, Mark, Words};
    use cmdr_fs::log_private::LogKind;
    match key {
        // The variant tag and the typed vocabularies: fixed words, never user data.
        "type" | "side" | "reason" | "refusal" | "filesystem" | "fate" => Words,
        "message" => Detail,
        "volumeName" | "deviceName" => Mark(LogKind::Volume),
        "name" | "exampleName" => Mark(LogKind::Name),
        "mountPoint" | "refusedFolder" => Mark(LogKind::DirPath),
        _ => Mark(LogKind::Path),
    }
}
