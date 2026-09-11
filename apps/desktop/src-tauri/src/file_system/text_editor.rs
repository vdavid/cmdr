//! Which app F4 opens a file in: the text editors macOS lists, the one the user
//! chose, and what happened when the file was handed over.
//!
//! The list is LaunchServices' own answer to "which apps EDIT plain text", so it
//! carries whatever macOS carries, oddities included. ❌ No editor table, no
//! `/Applications` scan. The stored choice arrives as an argument, because the
//! frontend owns the settings store.
//!
//! The wire types compile on every platform: Linux's `xdg-open` arm and the
//! off-macOS `playwright-e2e` arm of `commands/file_actions.rs::open_in_editor`
//! answer with them too. Everything else is macOS only, in `text_editor_macos.rs`.
//!
//! Evidence, the pick canonicalization, and what's verified about `open -t`:
//! `DETAILS.md` § "Text editor".

/// What `open_in_editor` did with the file, so the frontend acts on a variant rather
/// than reading a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum EditorOpenOutcome {
    /// The chosen app got the file (the system default, when that's the choice).
    /// ❗ It means `open` took the request, not that a window appeared.
    Opened,
    /// The chosen app isn't on this Mac right now, so the system default got the file.
    /// The frontend resets the setting and says so.
    ChosenAppMissingOpenedDefaultInstead,
}

/// The answer to one F4.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpenReport {
    pub outcome: EditorOpenOutcome,
    /// The app that got the file, named the way Finder shows it. Filled in only when
    /// something words it: a fallback outcome (the missing-app toast), or a press whose
    /// caller asked about other editors (the one-time hint). `None` otherwise, and when
    /// there's nothing to read a name from (no system default resolves, or off macOS).
    pub opened_in_name: Option<String>,
    /// `None` unless the caller asked. When asked: whether macOS lists any installed
    /// text editor besides the system default, which is what the one-time hint needs.
    pub other_editors_installed: Option<bool>,
}

/// Why `open_in_editor` couldn't answer at all. Distinct from [`EditorOpenOutcome`],
/// which reports things that DID happen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OpenInEditorError {
    /// The launcher couldn't be spawned. Carries the OS errno where there is one, so
    /// nothing has to read a message.
    LaunchRefused { errno: Option<i32> },
    /// The launch didn't finish inside the command's deadline.
    TimedOut,
}

impl std::fmt::Display for OpenInEditorError {
    /// ❗ For logs only; the frontend words the variant.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LaunchRefused { errno } => write!(f, "launch refused (errno {errno:?})"),
            Self::TimedOut => f.write_str("timed out"),
        }
    }
}

impl std::error::Error for OpenInEditorError {}

#[cfg(target_os = "macos")]
#[path = "text_editor_macos.rs"]
mod imp;

#[cfg(target_os = "macos")]
pub use imp::{TextEditorList, list_text_editors, open_in_editor};
