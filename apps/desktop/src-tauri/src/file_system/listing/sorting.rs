//! Sorting configuration and logic for file listings.

use serde::{Deserialize, Serialize};

use crate::file_system::listing::metadata::FileEntry;

// ============================================================================
// Sorting configuration
// ============================================================================

/// Column to sort files by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SortColumn {
    #[default]
    Name,
    Extension,
    Size,
    Modified,
    Created,
}

/// Sort order (ascending or descending).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

/// How to sort directories relative to the current sort column.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DirectorySortMode {
    /// Directories sort by the same column as files (using recursive_size for Size column).
    #[default]
    LikeFiles,
    /// Directories always sort by name, regardless of the active sort column.
    AlwaysByName,
}

// ============================================================================
// Sorting logic
// ============================================================================

/// Extracts file extension for sorting purposes.
/// Returns: (is_dotfile, has_extension, extension_lowercase)
/// Dotfiles (names starting with .) sort first, then files without extension, then by extension.
fn extract_extension_for_sort(name: &str) -> (bool, bool, String) {
    // Dotfiles (like .gitignore) sort first
    if name.starts_with('.') && !name[1..].contains('.') {
        return (true, false, String::new());
    }

    // Check for extension
    if let Some(dot_pos) = name.rfind('.')
        && dot_pos > 0
        && dot_pos < name.len() - 1
    {
        let ext = name[dot_pos + 1..].to_lowercase();
        return (false, true, ext);
    }

    // No extension
    (false, false, String::new())
}

/// Compares two strings using natural (alphanumeric) sort, case-insensitive.
fn compare_names_natural(a: &str, b: &str) -> std::cmp::Ordering {
    alphanumeric_sort::compare_str(a.to_lowercase(), b.to_lowercase())
}

/// The directory's recursive size for sorting, or `None` when it's unknown.
///
/// "Unknown" (sorts last, like the pre-honest-sizes `recursive_size == None`):
/// either the dir isn't enriched yet, or its subtree is incomplete with nothing
/// known below it (`recursive_size_complete == Some(false)` and size `0`, the
/// `—` render). A genuinely-empty dir (`complete == Some(true)`, size `0`) and a
/// lower-bound (`complete == Some(false)`, size `> 0`, the `≥N` render) are both
/// KNOWN and sort by their numeric value.
fn known_dir_size(e: &FileEntry) -> Option<u64> {
    match (e.recursive_size, e.recursive_size_complete) {
        (None, _) => None,
        (Some(0), Some(false)) => None, // `—` unknown
        (Some(size), _) => Some(size),  // genuinely-empty 0, lower-bound, or exact
    }
}

/// Returns a comparator that orders `FileEntry` values according to the given sort params.
///
/// Directories always come first, then files. Within each group the comparator
/// applies the requested column, order, and directory sort mode (including the
/// `recursive_size: None` sorts-last rule for Size).
pub fn entry_comparator(
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> impl Fn(&FileEntry, &FileEntry) -> std::cmp::Ordering {
    move |a, b| {
        // Directories always come first
        match (a.is_directory, b.is_directory) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // For directories in AlwaysByName mode, sort by name regardless of column
        if a.is_directory && b.is_directory && dir_sort_mode == DirectorySortMode::AlwaysByName {
            let name_cmp = compare_names_natural(&a.name, &b.name);
            return match sort_order {
                SortOrder::Ascending => name_cmp,
                SortOrder::Descending => name_cmp.reverse(),
            };
        }

        // For directories in LikeFiles mode sorting by Size, use recursive_size.
        // Handled separately because dirs with an UNKNOWN size must always sort
        // last (and stably by name), regardless of ascending/descending order,
        // so they don't masquerade as exact-0 dirs at the top of an ascending sort.
        //
        // Honest-size semantics: a dir's size is "unknown" when either
        // it isn't enriched yet (`recursive_size == None`) OR its subtree is
        // incomplete with nothing known below it (`recursive_size_complete ==
        // Some(false)` and size `0`, rendered as `—`). A genuinely-empty dir
        // (`complete == Some(true)`, size `0`) is a KNOWN `0 bytes` and sorts by
        // its value, ahead of unknowns. A lower-bound (`complete == Some(false)`,
        // size `> 0`, rendered `≥N`) sorts by its known floor `N`.
        if a.is_directory && b.is_directory && sort_by == SortColumn::Size {
            let a_known = known_dir_size(a);
            let b_known = known_dir_size(b);
            return match (a_known, b_known) {
                (None, None) => {
                    // Both unknown: sort by name, respecting sort order
                    let cmp = compare_names_natural(&a.name, &b.name);
                    match sort_order {
                        SortOrder::Ascending => cmp,
                        SortOrder::Descending => cmp.reverse(),
                    }
                }
                (None, Some(_)) => std::cmp::Ordering::Greater, // Unknown always last
                (Some(_), None) => std::cmp::Ordering::Less,    // Known always first
                (Some(a_size), Some(b_size)) => {
                    let cmp = a_size.cmp(&b_size);
                    let cmp = if cmp == std::cmp::Ordering::Equal {
                        compare_names_natural(&a.name, &b.name)
                    } else {
                        cmp
                    };
                    match sort_order {
                        SortOrder::Ascending => cmp,
                        SortOrder::Descending => cmp.reverse(),
                    }
                }
            };
        }

        // Compare by the active sorting column
        let primary = match sort_by {
            SortColumn::Name => compare_names_natural(&a.name, &b.name),
            SortColumn::Extension => {
                let (a_dotfile, a_has_ext, a_ext) = extract_extension_for_sort(&a.name);
                let (b_dotfile, b_has_ext, b_ext) = extract_extension_for_sort(&b.name);

                // Dotfiles first, then no extension, then by extension alphabetically
                match (a_dotfile, b_dotfile) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, true) => compare_names_natural(&a.name, &b.name),
                    (false, false) => match (a_has_ext, b_has_ext) {
                        (false, true) => std::cmp::Ordering::Less,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, false) => compare_names_natural(&a.name, &b.name),
                        (true, true) => {
                            let ext_cmp = alphanumeric_sort::compare_str(&a_ext, &b_ext);
                            if ext_cmp == std::cmp::Ordering::Equal {
                                compare_names_natural(&a.name, &b.name)
                            } else {
                                ext_cmp
                            }
                        }
                    },
                }
            }
            SortColumn::Size => match (a.size, b.size) {
                (None, None) => compare_names_natural(&a.name, &b.name),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_size), Some(b_size)) => a_size.cmp(&b_size),
            },
            SortColumn::Modified => match (a.modified_at, b.modified_at) {
                (None, None) => compare_names_natural(&a.name, &b.name),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
            SortColumn::Created => match (a.created_at, b.created_at) {
                (None, None) => compare_names_natural(&a.name, &b.name),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
        };

        // Apply sort order
        match sort_order {
            SortOrder::Ascending => primary,
            SortOrder::Descending => primary.reverse(),
        }
    }
}

/// Sorts file entries by the specified column and order.
/// Directories always come first, then files.
/// Uses natural sorting for string comparisons (for example, "img_2" before "img_10").
///
/// `dir_sort_mode` controls how directories are sorted among themselves:
/// - `LikeFiles`: directories sort by the same column as files (using `recursive_size` for Size)
/// - `AlwaysByName`: directories always sort by name, regardless of the active sort column
pub fn sort_entries(
    entries: &mut [FileEntry],
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) {
    entries.sort_by(entry_comparator(sort_by, sort_order, dir_sort_mode));
}
