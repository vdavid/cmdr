//! How a session shows its rows: the file's text as decoded, or the debug log's view of it
//! (`log_view.rs`). The search matcher reads rows the same way the session shows them, so this
//! sits below both.

use super::LineChunk;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// How a session shows its rows. Picked at open by the caller that knows what the file is
/// (Help > View debug log), ❌ never by sniffing the content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowText {
    /// The file's text as decoded.
    #[default]
    Raw,
    /// Cmdr's own log: log markers hidden, each marked value shown as itself.
    LogDisplay,
}

impl RowText {
    /// One row's text as this view shows it.
    pub fn render(self, text: &str) -> Cow<'_, str> {
        match self {
            RowText::Raw => Cow::Borrowed(text),
            RowText::LogDisplay => (super::host::host().log_display)(text),
        }
    }

    /// Every row of `chunk` as this view shows it.
    pub fn apply(self, chunk: &mut LineChunk) {
        if self == RowText::Raw {
            return;
        }
        for row in &mut chunk.rows {
            if let Cow::Owned(shown) = self.render(&row.text) {
                row.text = shown;
            }
        }
    }
}
