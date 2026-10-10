//! TC's per-field history (↓ in a field): what the name mask, extension mask, search, and
//! replace fields held when a rename ran, newest first, all four in one list. The
//! list machinery is `crate::recents`.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};

use super::plan::MultiRenameSpec;

/// How many history entries the list keeps, all fields together.
pub const MAX_FIELD_HISTORY: usize = 200;

/// The history. Recorded by apply, never by typing, so it holds values that
/// renamed something.
pub static FIELD_HISTORY: RecentsFile<FieldHistoryEntry> = RecentsFile::new();

/// The sheet's text fields that keep a history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HistoryField {
    NameMask,
    ExtensionMask,
    Search,
    Replace,
}

/// One value a field had when a rename ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldHistoryEntry {
    pub id: String,
    pub field: HistoryField,
    pub value: String,
}

impl RecentEntry for FieldHistoryEntry {
    const FILENAME: &'static str = "multi-rename-history.json";
    const LOG_TARGET: &'static str = "multi_rename::history";
    const LOG_NAME: &'static str = "multi-rename field history";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// One entry per field and value: using a value again moves it to the top.
    fn dedupe_key(&self) -> String {
        format!("{:?}\u{0}{}", self.field, self.value)
    }
}

/// The history entries a rename with `spec` adds: each field that holds
/// something other than its no-change default, oldest first so the name mask
/// ends up on top.
pub fn history_entries(spec: &MultiRenameSpec) -> Vec<FieldHistoryEntry> {
    [
        (HistoryField::Replace, spec.replace.as_str(), ""),
        (HistoryField::Search, spec.search.as_str(), ""),
        (HistoryField::ExtensionMask, spec.extension_mask.as_str(), "[E]"),
        (HistoryField::NameMask, spec.name_mask.as_str(), "[N]"),
    ]
    .into_iter()
    .filter(|(field, value, default)| {
        // A replacement is worth keeping only beside a search.
        !value.is_empty() && value != default && (*field != HistoryField::Replace || !spec.search.is_empty())
    })
    .map(|(field, value, _)| FieldHistoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        field,
        value: value.to_string(),
    })
    .collect()
}
