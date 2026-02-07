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

/// Sorts file entries by the specified column and order.
/// Directories always come first, then files.
/// Uses natural sorting for string comparisons (for example, "img_2" before "img_10").
pub fn sort_entries(entries: &mut [FileEntry], sort_by: SortColumn, sort_order: SortOrder) {
    entries.sort_by(|a, b| {
        // Directories always come first
        match (a.is_directory, b.is_directory) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // Compare by the active sorting column
        let primary = match sort_by {
            SortColumn::Name => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
            SortColumn::Extension => {
                let (a_dotfile, a_has_ext, a_ext) = extract_extension_for_sort(&a.name);
                let (b_dotfile, b_has_ext, b_ext) = extract_extension_for_sort(&b.name);

                // Dotfiles first, then no extension, then by extension
                match (a_dotfile, b_dotfile) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, true) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                    (false, false) => match (a_has_ext, b_has_ext) {
                        (false, true) => std::cmp::Ordering::Less,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, false) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                        (true, true) => {
                            let ext_cmp = alphanumeric_sort::compare_str(&a_ext, &b_ext);
                            if ext_cmp == std::cmp::Ordering::Equal {
                                alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase())
                            } else {
                                ext_cmp
                            }
                        }
                    },
                }
            }
            SortColumn::Size => {
                // For directories, size is None - sort them by name among themselves
                match (a.size, b.size) {
                    (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(a_size), Some(b_size)) => a_size.cmp(&b_size),
                }
            }
            SortColumn::Modified => match (a.modified_at, b.modified_at) {
                (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
            SortColumn::Created => match (a.created_at, b.created_at) {
                (None, None) => alphanumeric_sort::compare_str(a.name.to_lowercase(), b.name.to_lowercase()),
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
