//! A mirror of what the UI knows, so the backend can answer questions about it: what each pane
//! shows, which soft dialogs are open, what the archive password prompt is asking, and which
//! listings and write operations ended recently.
//!
//! The frontend and the listing and write hosts push into these stores. Tools and `cmdr://`
//! resources read them. See `CLAUDE.md`.

pub mod archive_password;
pub mod dialog_state;
pub mod listing_errors;
pub mod pane_state;
pub mod terminal_ops;

#[cfg(test)]
mod pane_state_tests;

pub use archive_password::ArchivePasswordPromptStore;
pub use dialog_state::SoftDialogTracker;
pub use pane_state::PaneStateStore;
