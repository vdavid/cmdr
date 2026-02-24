//! Sorting configuration and logic for file listings.

use serde::{Deserialize, Serialize};

use crate::file_system::listing::metadata::FileEntry;

// ============================================================================
// Sorting configuration
// ============================================================================

/// Column to sort files by.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

/// How to sort directories relative to the current sort column.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
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
    entries.sort_by(|a, b| {
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
        // Handled separately because dirs with unknown (None) size must always sort last,
        // regardless of ascending/descending order.
        if a.is_directory && b.is_directory && sort_by == SortColumn::Size {
            return match (a.recursive_size, b.recursive_size) {
                (None, None) => {
                    // Both unknown — sort by name, respecting sort order
                    let cmp = compare_names_natural(&a.name, &b.name);
                    match sort_order {
                        SortOrder::Ascending => cmp,
                        SortOrder::Descending => cmp.reverse(),
                    }
                }
                (None, Some(_)) => std::cmp::Ordering::Greater, // None always last
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

                // Dotfiles first, then no extension, then by extension
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
    });
}
