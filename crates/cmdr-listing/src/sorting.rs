//! Sorting configuration and logic for file listings.

use std::sync::Arc;

use cmdr_collation::{NameCollator, NameKey};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::file_extension::NameKind;
use cmdr_fs::volume::SupportedDates;
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

// ============================================================================
// Sorting configuration
// ============================================================================

// The vocabulary a pane asks with (`SortColumn`, `SortOrder`, `DirectorySortMode`)
// is `cmdr-sort`'s, so a crate that only names a sort needn't link this engine.

/// The sort a listing's rows are actually in: the pane's requested column, or
/// Modified when the requested date is one the listed volume doesn't record.
///
/// ❗ [`EffectiveSort::resolve`] is the ONLY constructor, and [`entry_comparator`]
/// and [`sort_entries`] take nothing else, so a sort site that forgets the
/// fallback doesn't compile. The requested column is what the pane, the
/// persisted setting, and every echo keep; this one only orders rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveSort {
    column: SortColumn,
    order: SortOrder,
    dir_mode: DirectorySortMode,
}

impl EffectiveSort {
    /// The sort a pane that asked for `requested` gets on a volume listing
    /// `supported`: Created or Added falls back to Modified where the volume
    /// doesn't record it, and every other column sorts as asked. The order and
    /// directory mode always stay the pane's.
    pub fn resolve(
        requested: SortColumn,
        order: SortOrder,
        dir_mode: DirectorySortMode,
        supported: SupportedDates,
    ) -> Self {
        let column = match requested {
            SortColumn::Created if !supported.created => SortColumn::Modified,
            SortColumn::Added if !supported.added => SortColumn::Modified,
            column => column,
        };
        Self {
            column,
            order,
            dir_mode,
        }
    }

    /// The column the rows are ordered by.
    pub fn column(&self) -> SortColumn {
        self.column
    }

    /// The direction.
    pub fn order(&self) -> SortOrder {
        self.order
    }

    /// Where directories sort.
    pub fn dir_mode(&self) -> DirectorySortMode {
        self.dir_mode
    }
}

// ============================================================================
// Sorting logic
// ============================================================================

/// The fields `entry_comparator` reads, so ONE comparator body serves every row
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
    /// listing). `sort_entries` overrides it through `Keyed`, answering from
    /// a key built once per row, so a bulk sort pays one key per row instead of
    /// one collation per comparison. `DETAILS.md` § "Collating names".
    fn compare_name(&self, other: &Self, collator: &NameCollator) -> std::cmp::Ordering {
        collator.compare(self.name(), other.name())
    }
    /// Whether the row is a directory.
    fn is_directory(&self) -> bool;
    /// Logical size in bytes, `None` when unknown.
    fn size(&self) -> Option<u64>;
    /// Last-modified time in Unix seconds, `None` when unknown.
    fn modified_at(&self) -> Option<u64>;
    /// Creation time in Unix seconds, `None` when unknown.
    fn created_at(&self) -> Option<u64>;
    /// When the row was added to its folder, in Unix seconds, `None` when unknown.
    fn added_at(&self) -> Option<u64>;
    /// A directory's recursive size, `None` when it isn't known. See [`known_dir_size`].
    fn recursive_size(&self) -> Option<u64>;
    /// Whether [`SortableEntry::recursive_size`] is exact. See [`known_dir_size`].
    fn recursive_size_complete(&self) -> Option<bool>;
    /// Whether the row's name is a file's, a folder's, or a package's, which
    /// decides its extension. The default reads the name through the package
    /// list; a row that knows better (`FileEntry::is_package`) says so.
    fn name_kind(&self) -> NameKind {
        NameKind::by_name(self.name(), self.is_directory())
    }
}

impl SortableEntry for FileEntry {
    fn name(&self) -> &str {
        &self.name
    }
    fn name_kind(&self) -> NameKind {
        FileEntry::name_kind(self)
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
    fn added_at(&self) -> Option<u64> {
        self.added_at
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
/// Dotfiles (names starting with .) sort first, then names without extension, then by extension.
/// What an extension IS comes from the product's one rule, `cmdr_fs::file_extension`.
fn extract_extension_for_sort(name: &str, kind: NameKind) -> (bool, bool, String) {
    // Dotfiles (like .gitignore) sort first
    if name.starts_with('.') && !name[1..].contains('.') {
        return (true, false, String::new());
    }

    match cmdr_fs::file_extension::extension_of(name, kind) {
        Some(ext) => (false, true, ext.to_lowercase()),
        None => (false, false, String::new()),
    }
}

/// A row paired with the collation key for its name, so a bulk sort collates
/// each name once instead of once per comparison.
///
/// Private on purpose: it exists to feed `entry_comparator`, never to become a
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
    fn added_at(&self) -> Option<u64> {
        self.row.added_at()
    }
    fn recursive_size(&self) -> Option<u64> {
        self.row.recursive_size()
    }
    fn recursive_size_complete(&self) -> Option<bool> {
        self.row.recursive_size_complete()
    }
    fn name_kind(&self) -> NameKind {
        self.row.name_kind()
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

/// Returns a comparator that orders `FileEntry` values according to `sort`.
///
/// Directories come first, then files, unless the directory mode is
/// [`DirectorySortMode::MixedWithFiles`] ([`compare_mixed`]). Within each group the comparator
/// applies the effective column, order, and directory sort mode (including the
/// `recursive_size: None` sorts-last rule for Size).
///
/// Names rank by Unicode collation, resolved through [`SortableEntry::compare_name`]
/// so a caller that prebuilt its keys and one comparing live get the same order.
/// `cmdr_collation`, and `DETAILS.md` § "Collating names".
///
/// `collator` is resolved once per sort by the caller, ❗ never per comparison and
/// never under a cache lock: the engine's collator asks the host for the live
/// reading locale, and a language switch mid-sort would otherwise reorder rows
/// against each other and break the strict-weak-ordering `sort_by` needs.
pub fn entry_comparator<E: SortableEntry + ?Sized>(
    collator: Arc<NameCollator>,
    sort: EffectiveSort,
) -> impl Fn(&E, &E) -> std::cmp::Ordering {
    let EffectiveSort {
        column: sort_by,
        order: sort_order,
        dir_mode: dir_sort_mode,
    } = sort;
    move |a, b| {
        if dir_sort_mode == DirectorySortMode::MixedWithFiles {
            return compare_mixed(a, b, sort_by, sort_order, &collator);
        }

        // Directories always come first
        match (a.is_directory(), b.is_directory()) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // AlwaysByName keeps directories A→Z under every other column, whatever
        // the arrow says: Size and the three dates default to descending, so
        // following it showed Z→A folders, which reads as random (ERR-MJFJG). On
        // the Name column the arrow IS the name order, so they follow it there.
        if a.is_directory()
            && b.is_directory()
            && dir_sort_mode == DirectorySortMode::AlwaysByName
            && sort_by != SortColumn::Name
        {
            return a.compare_name(b, &collator);
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

        apply_order(compare_by_column(a, b, sort_by, &collator), sort_order)
    }
}

/// Flips an ascending comparison for a descending sort.
fn apply_order(cmp: std::cmp::Ordering, sort_order: SortOrder) -> std::cmp::Ordering {
    match sort_order {
        SortOrder::Ascending => cmp,
        SortOrder::Descending => cmp.reverse(),
    }
}

/// The [`DirectorySortMode::MixedWithFiles`] comparison: no folders-first step, so
/// a folder ranks among the files by the same column.
///
/// Size is the one column where a folder and a file read different fields: a
/// folder's key is its [`known_dir_size`], a file's its `size`. One rule covers
/// both so the order stays transitive: an unknown size sorts LAST whatever the
/// order (by name among themselves), the same promise the folders-first sort
/// makes for folders, so nothing unknown masquerades as the smallest or biggest.
fn compare_mixed<E: SortableEntry + ?Sized>(
    a: &E,
    b: &E,
    sort_by: SortColumn,
    sort_order: SortOrder,
    collator: &NameCollator,
) -> std::cmp::Ordering {
    if sort_by != SortColumn::Size {
        return apply_order(compare_by_column(a, b, sort_by, collator), sort_order);
    }
    let size_key = |e: &E| {
        if e.is_directory() { known_dir_size(e) } else { e.size() }
    };
    match (size_key(a), size_key(b)) {
        (None, None) => apply_order(a.compare_name(b, collator), sort_order),
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a_size), Some(b_size)) => apply_order(
            a_size.cmp(&b_size).then_with(|| a.compare_name(b, collator)),
            sort_order,
        ),
    }
}

/// How two rows rank under `sort_by`, ascending, before the sort order applies.
fn compare_by_column<E: SortableEntry + ?Sized>(
    a: &E,
    b: &E,
    sort_by: SortColumn,
    collator: &NameCollator,
) -> std::cmp::Ordering {
    match sort_by {
        SortColumn::Name => a.compare_name(b, collator),
        SortColumn::Extension => {
            let (a_dotfile, a_has_ext, a_ext) = extract_extension_for_sort(a.name(), a.name_kind());
            let (b_dotfile, b_has_ext, b_ext) = extract_extension_for_sort(b.name(), b.name_kind());

            // Dotfiles first, then no extension, then by extension alphabetically
            match (a_dotfile, b_dotfile) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (true, true) => a.compare_name(b, collator),
                (false, false) => match (a_has_ext, b_has_ext) {
                    (false, true) => std::cmp::Ordering::Less,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, false) => a.compare_name(b, collator),
                    (true, true) => {
                        let ext_cmp = collator.compare(&a_ext, &b_ext);
                        if ext_cmp == std::cmp::Ordering::Equal {
                            a.compare_name(b, collator)
                        } else {
                            ext_cmp
                        }
                    }
                },
            }
        }
        SortColumn::Size => match (a.size(), b.size()) {
            (None, None) => a.compare_name(b, collator),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a_size), Some(b_size)) => a_size.cmp(&b_size),
        },
        SortColumn::Modified => match (a.modified_at(), b.modified_at()) {
            (None, None) => a.compare_name(b, collator),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
        },
        SortColumn::Created => match (a.created_at(), b.created_at()) {
            (None, None) => a.compare_name(b, collator),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
        },
        SortColumn::Added => match (a.added_at(), b.added_at()) {
            (None, None) => a.compare_name(b, collator),
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
        },
    }
}

/// Whether replacing `old` with `new` can move the row under `sort`: a change to
/// any field the comparator reads for that column, or to the row's kind.
///
/// Reads the same [`SortableEntry`] accessors [`compare_by_column`] and the
/// directory size arms read, so the two can't disagree about what a sort key is.
/// A false positive costs a harmless re-place; a false negative leaves the row
/// out of order, so anything the comparator might read counts.
pub(crate) fn sort_key_changed<E: SortableEntry + ?Sized>(old: &E, new: &E, sort: &EffectiveSort) -> bool {
    if old.is_directory() != new.is_directory() || old.name() != new.name() {
        return true;
    }
    match sort.column {
        // The name already compared above.
        SortColumn::Name => false,
        // The name compared above, but a folder macOS just called a package
        // gains an extension under the same name.
        SortColumn::Extension => old.name_kind() != new.name_kind(),
        // A directory ranks by its `known_dir_size` under every Size sort but
        // AlwaysByName's, a file by its `size`; both, so no mode is missed.
        SortColumn::Size => {
            old.size() != new.size()
                || old.recursive_size() != new.recursive_size()
                || old.recursive_size_complete() != new.recursive_size_complete()
        }
        SortColumn::Modified => old.modified_at() != new.modified_at(),
        SortColumn::Created => old.created_at() != new.created_at(),
        SortColumn::Added => old.added_at() != new.added_at(),
    }
}

/// Sorts file entries by `sort`'s column and order.
/// Names order by Unicode collation, so digit runs compare numerically ("img_2"
/// before "img_10") and an accent's spelling doesn't decide anything.
///
/// The directory mode controls where directories go:
/// - `LikeFiles`: first, sorted by the same column as files (using `recursive_size` for Size)
/// - `AlwaysByName`: first, A→Z by name, regardless of the active sort column (the
///   Name column's arrow still reverses them)
/// - `MixedWithFiles`: among the files, by the same column
///
/// Collates each name ONCE into a [`NameKey`] and sorts on those, rather than
/// collating a pair per comparison: a 100k listing does 100k collations instead
/// of ~1.7M. `DETAILS.md` § "Collating names" carries the measurement.
pub fn sort_entries(collator: &Arc<NameCollator>, entries: &mut [FileEntry], sort: EffectiveSort) {
    let mut order: Vec<u32> = (0..entries.len() as u32).collect();

    // Scoped so the keys (and the comparator holding their lifetime) release
    // their borrow of `entries` before the permutation moves rows around.
    {
        let keyed: Vec<Keyed<'_, FileEntry>> = entries
            .iter()
            .map(|row| Keyed {
                row,
                name_key: collator.key(&row.name),
            })
            .collect();

        let comparator = entry_comparator::<Keyed<'_, FileEntry>>(Arc::clone(collator), sort);
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
