//! How a session shows its rows: the file's text as decoded, or the debug log's view of it
//! (`log_view.rs`). The search matcher reads rows the same way the session shows them, so this
//! sits below both.

use super::LineChunk;
use super::host::{LogDisplay, as_stored};
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

/// A [`RowText`] resolved against its host: what `LogDisplay` means comes from the host
/// (`ViewerHost::log_display`, the app's log-marker reader).
#[derive(Debug, Clone, Copy)]
pub struct RowView {
    rows: RowText,
    log_display: LogDisplay,
}

impl RowView {
    pub(super) fn new(rows: RowText, log_display: LogDisplay) -> Self {
        Self { rows, log_display }
    }

    /// Rows as decoded, with no host to ask.
    pub fn raw() -> Self {
        Self::new(RowText::Raw, as_stored)
    }

    pub fn is_raw(&self) -> bool {
        self.rows == RowText::Raw
    }

    /// One row's text as this view shows it.
    pub fn render<'a>(&self, text: &'a str) -> Cow<'a, str> {
        match self.rows {
            RowText::Raw => Cow::Borrowed(text),
            RowText::LogDisplay => (self.log_display)(text),
        }
    }

    /// Every row of `chunk` as this view shows it.
    pub fn apply(&self, chunk: &mut LineChunk) {
        if self.is_raw() {
            return;
        }
        for row in &mut chunk.rows {
            if let Cow::Owned(shown) = self.render(&row.text) {
                row.text = shown;
            }
        }
    }
}
