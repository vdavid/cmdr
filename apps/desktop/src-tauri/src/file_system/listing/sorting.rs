//! Sorting configuration and logic for file listings.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::file_system::listing::collation::{NameCollator, NameKey, active_collator};
use crate::file_system::listing::metadata::FileEntry;

// ============================================================================
// Sorting configuration
// ============================================================================

/// Column to sort files by.
// DEFAULT-OK: a preference default (by name), not a claim about anything on disk.
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
// DEFAULT-OK: a preference default (ascending), not a claim about anything on disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SortOrder {
    #[default]
    Ascending,
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
    /// Directories always sort by name, regardless of the active sort column.
    AlwaysByName,
}

// ============================================================================
// Sorting logic
// ============================================================================

/// The fields [`entry_comparator`] reads, so ONE comparator body serves every row
/// shape the app orders.
///
/// Two implementors today: [`FileEntry`], a directory listing's row, and
/// `commands::search::SearchSortRow`, a search-results pane's row. The second
/// carries a strict SUBSET of the first (no creation time, no recursive size), and
/// answers `None` for what it doesn't have, which lands it on the comparator's
/// existing unknown-value arms rather than on a second set of rules. That is the
/// point: a name sorts the same way in a search-results pane as it does in a
/// folder, because there is one comparator and no copy of it anywhere.
pub trait SortableEntry {
    /// The row's own file name (the last path component).
    fn name(&self) -> &str;
    /// How this row's name ranks against another's.
    ///
    /// The default asks the collator live, which is what the one-off callers
    /// want (`insert_entry_sorted` probes ~17 rows to place one entry in a 100k
    /// listing). [`sort_entries`] overrides it through [`Keyed`], answering from
    /// a key built once per row, so a bulk sort pays one key per row instead of
    /// one collation per comparison. `DETAILS.md` § "Collating names".
    fn compare_name(&self, other: &Self, collator: &NameCollator) -> std::cmp::Ordering {
        collator.compare(self.name(), other.name())
    }
    fn is_directory(&self) -> bool;
    /// Logical size in bytes, `None` when unknown.
    fn size(&self) -> Option<u64>;
    /// Last-modified time in Unix seconds, `None` when unknown.
    fn modified_at(&self) -> Option<u64>;
    /// Creation time in Unix seconds, `None` when unknown.
    fn created_at(&self) -> Option<u64>;
    /// A directory's recursive size, `None` when it isn't known. See [`known_dir_size`].
    fn recursive_size(&self) -> Option<u64>;
    /// Whether [`SortableEntry::recursive_size`] is exact. See [`known_dir_size`].
    fn recursive_size_complete(&self) -> Option<bool>;
}

impl SortableEntry for FileEntry {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_directory(&self) -> bool {
        self.is_directory
    }
    fn size(&self) -> Option<u64> {
        self.size
    }
    fn modified_at(&self) -> Option<u64> {
        self.modified_at
    }
    fn created_at(&self) -> Option<u64> {
        self.created_at
    }
    fn recursive_size(&self) -> Option<u64> {
        self.recursive_size
    }
    fn recursive_size_complete(&self) -> Option<bool> {
        self.recursive_size_complete
    }
}

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

/// A row paired with the collation key for its name, so a bulk sort collates
/// each name once instead of once per comparison.
///
/// Private on purpose: it exists to feed [`entry_comparator`], never to become a
/// second way to describe a row.
struct Keyed<'a, E: ?Sized> {
    row: &'a E,
    name_key: NameKey,
}

impl<E: SortableEntry + ?Sized> SortableEntry for Keyed<'_, E> {
    fn name(&self) -> &str {
        self.row.name()
    }
    /// The whole point of the wrapper: rank on the prebuilt keys, then apply the
    /// same raw-name tiebreak [`NameCollator::compare`] ends with, so both
    /// readings of the order stay identical.
    fn compare_name(&self, other: &Self, _collator: &NameCollator) -> std::cmp::Ordering {
        self.name_key
            .compare(&other.name_key)
            .then_with(|| self.name().as_bytes().cmp(other.name().as_bytes()))
    }
    fn is_directory(&self) -> bool {
        self.row.is_directory()
    }
    fn size(&self) -> Option<u64> {
        self.row.size()
    }
    fn modified_at(&self) -> Option<u64> {
        self.row.modified_at()
    }
    fn created_at(&self) -> Option<u64> {
        self.row.created_at()
    }
    fn recursive_size(&self) -> Option<u64> {
        self.row.recursive_size()
    }
    fn recursive_size_complete(&self) -> Option<bool> {
        self.row.recursive_size_complete()
    }
}

/// The directory's recursive size for sorting, or `None` when it's unknown.
///
/// "Unknown" (sorts last, like the pre-honest-sizes `recursive_size == None`):
/// either the dir isn't enriched yet, or its subtree is incomplete with nothing
/// known below it (`recursive_size_complete == Some(false)` and size `0`, the
/// `—` render). A genuinely-empty dir (`complete == Some(true)`, size `0`) and a
/// lower-bound (`complete == Some(false)`, size `> 0`, the `≥N` render) are both
/// KNOWN and sort by their numeric value.
fn known_dir_size<E: SortableEntry + ?Sized>(e: &E) -> Option<u64> {
    match (e.recursive_size(), e.recursive_size_complete()) {
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
///
/// Names rank by Unicode collation, resolved through [`SortableEntry::compare_name`]
/// so a caller that prebuilt its keys and one comparing live get the same order.
/// `collation.rs`, and `DETAILS.md` § "Collating names".
pub fn entry_comparator<E: SortableEntry + ?Sized>(
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) -> impl Fn(&E, &E) -> std::cmp::Ordering {
    // Resolved once per sort, not once per comparison: `active_collator` reads
    // the live UI locale, and a language switch mid-sort would otherwise reorder
    // rows against each other and break the strict-weak-ordering `sort_by` needs.
    let collator: Arc<NameCollator> = active_collator();

    move |a, b| {
        // Directories always come first
        match (a.is_directory(), b.is_directory()) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // For directories in AlwaysByName mode, sort by name regardless of column
        if a.is_directory() && b.is_directory() && dir_sort_mode == DirectorySortMode::AlwaysByName {
            let name_cmp = a.compare_name(b, &collator);
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
        if a.is_directory() && b.is_directory() && sort_by == SortColumn::Size {
            let a_known = known_dir_size(a);
            let b_known = known_dir_size(b);
            return match (a_known, b_known) {
                (None, None) => {
                    // Both unknown: sort by name, respecting sort order
                    let cmp = a.compare_name(b, &collator);
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
                        a.compare_name(b, &collator)
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
            SortColumn::Name => a.compare_name(b, &collator),
            SortColumn::Extension => {
                let (a_dotfile, a_has_ext, a_ext) = extract_extension_for_sort(a.name());
                let (b_dotfile, b_has_ext, b_ext) = extract_extension_for_sort(b.name());

                // Dotfiles first, then no extension, then by extension alphabetically
                match (a_dotfile, b_dotfile) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (true, true) => a.compare_name(b, &collator),
                    (false, false) => match (a_has_ext, b_has_ext) {
                        (false, true) => std::cmp::Ordering::Less,
                        (true, false) => std::cmp::Ordering::Greater,
                        (false, false) => a.compare_name(b, &collator),
                        (true, true) => {
                            let ext_cmp = collator.compare(&a_ext, &b_ext);
                            if ext_cmp == std::cmp::Ordering::Equal {
                                a.compare_name(b, &collator)
                            } else {
                                ext_cmp
                            }
                        }
                    },
                }
            }
            SortColumn::Size => match (a.size(), b.size()) {
                (None, None) => a.compare_name(b, &collator),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_size), Some(b_size)) => a_size.cmp(&b_size),
            },
            SortColumn::Modified => match (a.modified_at(), b.modified_at()) {
                (None, None) => a.compare_name(b, &collator),
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            },
            SortColumn::Created => match (a.created_at(), b.created_at()) {
                (None, None) => a.compare_name(b, &collator),
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
/// Names order by Unicode collation, so digit runs compare numerically ("img_2"
/// before "img_10") and an accent's spelling doesn't decide anything.
///
/// `dir_sort_mode` controls how directories are sorted among themselves:
/// - `LikeFiles`: directories sort by the same column as files (using `recursive_size` for Size)
/// - `AlwaysByName`: directories always sort by name, regardless of the active sort column
///
/// Collates each name ONCE into a [`NameKey`] and sorts on those, rather than
/// collating a pair per comparison: a 100k listing does 100k collations instead
/// of ~1.7M. `DETAILS.md` § "Collating names" carries the measurement.
pub fn sort_entries(
    entries: &mut [FileEntry],
    sort_by: SortColumn,
    sort_order: SortOrder,
    dir_sort_mode: DirectorySortMode,
) {
    let mut order: Vec<u32> = (0..entries.len() as u32).collect();

    // Scoped so the keys (and the comparator holding their lifetime) release
    // their borrow of `entries` before the permutation moves rows around.
    {
        let collator = active_collator();
        let keyed: Vec<Keyed<'_, FileEntry>> = entries
            .iter()
            .map(|row| Keyed {
                row,
                name_key: collator.key(&row.name),
            })
            .collect();

        let comparator = entry_comparator::<Keyed<'_, FileEntry>>(sort_by, sort_order, dir_sort_mode);
        order.sort_by(|a, b| comparator(&keyed[*a as usize], &keyed[*b as usize]));
    }

    // `order[i]` names the row that belongs AT position `i`; the applier below
    // wants the other direction, where each row currently sitting at `i` GOES.
    let mut destination = vec![0u32; order.len()];
    for (position, &source) in order.iter().enumerate() {
        destination[source as usize] = position as u32;
    }
    apply_permutation(entries, &mut destination);
}

/// Moves the row at each index `i` to `destination[i]`.
///
/// Follows each cycle rather than allocating a second `Vec<FileEntry>`: a 100k
/// listing's rows are large enough that cloning them costs more than the sort
/// does. `destination` is scratch and ends as the identity permutation.
fn apply_permutation(entries: &mut [FileEntry], destination: &mut [u32]) {
    for start in 0..destination.len() {
        // Walk this cycle until the slot holds the row that belongs in it. Every
        // swap puts one row home, so the whole pass is O(n) moves.
        while destination[start] != start as u32 {
            let target = destination[start] as usize;
            entries.swap(start, target);
            destination.swap(start, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> FileEntry {
        FileEntry::new(name.to_string(), format!("/{name}"), false, false)
    }

    /// Reads as if it moved row `i` to `destination[i]`, in every direction a
    /// cycle can run. The inverse of this permutation is a different order, and
    /// applying that one instead is a silent, whole-listing scramble.
    #[test]
    fn apply_permutation_moves_each_row_to_its_destination() {
        // A 3-cycle: A goes to slot 2, B to slot 0, C to slot 1.
        let cases: [(&[u32], &[&str]); 4] = [
            (&[2, 0, 1], &["B", "C", "A"]),
            (&[1, 2, 0], &["C", "A", "B"]),
            (&[0, 1, 2], &["A", "B", "C"]),
            (&[1, 0, 2], &["B", "A", "C"]),
        ];

        for (destination, expected) in cases {
            let mut entries = vec![entry("A"), entry("B"), entry("C")];
            let mut scratch = destination.to_vec();
            apply_permutation(&mut entries, &mut scratch);

            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert_eq!(names, expected, "applying {destination:?}");
            assert_eq!(scratch, [0, 1, 2], "{destination:?} left scratch dirty");
        }
    }
}
