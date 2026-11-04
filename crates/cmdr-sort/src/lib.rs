// The lint set this crate is held to lives in the workspace root's
// `[workspace.lints]`, opted into by `Cargo.toml`'s `lints.workspace = true`.
// `unused_crate_dependencies` can't go with them: it's judged per compilation
// unit, so it belongs on the lib alone. `missing_docs` and `unreachable_pub` are
// this crate's own contract, as in every crate whose surface is capped.
#![warn(unused_crate_dependencies)]
#![warn(unreachable_pub)]
#![deny(missing_docs)]

//! What a pane asks to be sorted by: the column, the direction, and where
//! directories go. Vocabulary only; the ONE comparator that applies it, and the
//! fallback from a date a volume doesn't record, live in `cmdr-listing`
//! (`entry_comparator`, `EffectiveSort`), which re-exports these three.
//!
//! Its own crate so a crate that only NAMES a sort (the menu bar's Sort by rows)
//! doesn't link the listing engine. Why: `DETAILS.md`.

use serde::{Deserialize, Serialize};

/// Column to sort files by.
// DEFAULT-OK: a preference default (by name), not a claim about anything on disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    /// By name, in the reading language's collation.
    #[default]
    Name,
    /// By extension.
    Extension,
    /// By size.
    Size,
    /// By last-modified time.
    Modified,
    /// By creation time.
    Created,
    /// By when the item was added to its folder (macOS APFS and HFS+ only).
    Added,
}

/// Sort order (ascending or descending).
// DEFAULT-OK: a preference default (ascending), not a claim about anything on disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    /// Low to high.
    #[default]
    Ascending,
    /// High to low.
    Descending,
}

/// How to sort directories relative to the current sort column.
// DEFAULT-OK: a preference default (dirs sort like files), not a claim about disk state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DirectorySortMode {
    /// Directories sort by the same column as files (using recursive_size for Size column).
    #[default]
    LikeFiles,
    /// Directories always sort by name, A→Z, regardless of the active sort column
    /// and its direction. Only the Name column's arrow reverses them.
    AlwaysByName,
    /// Directories don't lead: they sort among the files by the same column ("Show
    /// folders first" off). Size ranks a directory by its `recursive_size`.
    MixedWithFiles,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire<T: Serialize>(value: T) -> String {
        serde_json::to_string(&value).expect("a unit variant serializes")
    }

    /// The frontend, the persisted settings, and `bindings.ts` read these exact
    /// words, so a rename here is a wire break.
    #[test]
    fn the_wire_words_are_the_frontends() {
        let columns = [
            (SortColumn::Name, "name"),
            (SortColumn::Extension, "extension"),
            (SortColumn::Size, "size"),
            (SortColumn::Modified, "modified"),
            (SortColumn::Created, "created"),
            (SortColumn::Added, "added"),
        ];
        for (column, word) in columns {
            assert_eq!(wire(column), format!("\"{word}\""));
        }
        assert_eq!(wire(SortOrder::Ascending), "\"ascending\"");
        assert_eq!(wire(SortOrder::Descending), "\"descending\"");
        assert_eq!(wire(DirectorySortMode::LikeFiles), "\"likeFiles\"");
        assert_eq!(wire(DirectorySortMode::AlwaysByName), "\"alwaysByName\"");
        assert_eq!(wire(DirectorySortMode::MixedWithFiles), "\"mixedWithFiles\"");
    }

    #[test]
    fn the_defaults_are_name_ascending_like_files() {
        assert_eq!(SortColumn::default(), SortColumn::Name);
        assert_eq!(SortOrder::default(), SortOrder::Ascending);
        assert_eq!(DirectorySortMode::default(), DirectorySortMode::LikeFiles);
    }
}
