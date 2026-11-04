//! The in-memory arena: one row per entry on a volume, and the builder that makes it.
//!
//! One `SearchIndex` per volume. A host reads its rows from wherever they live (the
//! desktop from the index database, the demo from its tree) and pushes them through
//! [`ArenaBuilder`]; the arena's lifecycle (caching, idle drops, catch-up timing) is
//! the host's policy.

#[cfg(test)]
mod memory_tests;

// ── An optional u64 that costs eight bytes ───────────────────────────

/// A `u64` that may be absent, in eight bytes instead of `Option<u64>`'s sixteen.
///
/// **Why it exists.** A `u64` uses every one of its bit patterns, so `Option<u64>` has
/// no niche to hide `None` in and Rust adds a whole discriminant word. Two of those in
/// [`SearchEntry`] were 32 of its 56 bytes, for two values needing 8 each — and the
/// arena holds one entry per file on the volume (6.0 M rows on David's boot disk), so
/// that padding alone was ~97 MB of resident memory whenever someone searched.
///
/// **Why `u64::MAX` can't collide with a real value.** Both values this wraps come out
/// of SQLite `INTEGER` columns, which are SIGNED 64-bit, so the index physically cannot
/// store or return anything above `i64::MAX` — `u64::MAX` is a full bit outside the
/// representable range, not merely an implausible value in it. It's unreachable on the
/// physics too: `u64::MAX` bytes is 16 EiB, twice APFS's own 8 EiB per-file ceiling, and
/// `u64::MAX` seconds is ~5.8 × 10^11 years after 1970.
///
/// ❌ **Don't "simplify" this back to `Option<u64>`.** The 16 bytes it costs are real
/// and the encoding is invisible from outside: nothing can read the sentinel, because
/// [`get`](Self::get) is the only way in.
///
/// **`None` is meaningful in both fields and must survive exactly.** `size` is NULL for
/// every 2nd+ name of a hardlinked inode (the index counts an inode's bytes once, on one
/// name, and stores NULL on the rest — 934,793 of 6.0 M rows on David's disk), so
/// collapsing `None` into `0` would quietly change what folder totals and size filters
/// report on a hardlink-heavy tree. `modified_at` is NULL where the time is unknown.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct OptU64(u64);

impl OptU64 {
    /// The absent value.
    pub const NONE: Self = Self(u64::MAX);

    /// Encode an optional value.
    ///
    /// `Some(u64::MAX)` would encode as absent, which is why the type's doc comment
    /// argues at length that no such value can reach here. The `debug_assert` catches a
    /// future caller that finds a way; it compiles out of the shipping build, which
    /// matters because this runs once per row per arena load.
    #[inline]
    pub fn new(value: Option<u64>) -> Self {
        debug_assert!(
            value != Some(u64::MAX),
            "u64::MAX is OptU64's absent marker, so a real u64::MAX would read back as None. \
             SQLite INTEGER columns are signed, so this shouldn't be reachable from the index."
        );
        Self(value.unwrap_or(u64::MAX))
    }

    /// Decode back to an `Option<u64>`. The only way to read one.
    #[inline]
    pub fn get(self) -> Option<u64> {
        (self.0 != u64::MAX).then_some(self.0)
    }
}

/// Prints like the `Option<u64>` it stands for, so a debug dump of an arena row never
/// shows a bare `18446744073709551615` for "unknown".
impl std::fmt::Debug for OptU64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.get(), f)
    }
}

/// The arena's root sentinel id: the row every top-level entry's `parent_id` names.
/// The index database's `ROOT_ID` is the same number, which the loader asserts at
/// compile time.
pub const ARENA_ROOT_ID: i64 = 1;

// ── Search entry (in-memory representation) ──────────────────────────

/// One row of the arena, 40 bytes. See [`OptU64`] before adding a field: the arena holds
/// one of these per file on the volume, so a byte here is megabytes of peak footprint,
/// and `search/index/memory_tests.rs` pins the size.
///
/// Its fields are private: a row is read through [`SearchIndex`]'s accessors and
/// written through [`ArenaBuilder::push`], so how a row is stored can change (a mapped
/// arena, GitHub #114) without touching a caller.
#[derive(Debug, Clone)]
pub struct SearchEntry {
    id: i64,
    parent_id: i64,
    name_offset: u32, // byte offset into SearchIndex.names
    name_len: u16,    // byte length (max filename 255 chars = up to 765 bytes UTF-8)
    is_directory: bool,
    /// Apparent size in bytes, absent for a directory (its recursive size lives in
    /// `dir_stats`) and for every 2nd+ name of a hardlinked inode.
    size: OptU64,
    /// Last-modified time in seconds since the Unix epoch, absent where unknown.
    modified_at: OptU64,
}

// ── Search index ─────────────────────────────────────────────────────

/// The loaded arena for one volume, read by row position (`idx`, `0..len()`).
///
/// **Rows are sorted by `id`, ascending, and every lookup depends on it.** `id` is
/// the `entries` table's `INTEGER PRIMARY KEY`, so it is the rowid and a table scan
/// yields it in order for free; [`ArenaBuilder::push`] asserts it in debug builds,
/// and the loader's `entries_are_sorted_by_id` pins it.
///
/// The accessors are the whole read surface (the arena-snapshot spec's Contract B,
/// `docs/specs/search-arena-snapshot.md`): ❌ no caller sees the rows or the names
/// buffer, so a mapped backend can replace them.
#[derive(Debug)]
pub struct SearchIndex {
    names: String, // arena: all filenames concatenated
    entries: Vec<SearchEntry>,
    generation: u64,
}

impl SearchIndex {
    /// Empty sentinel index used during async load.
    pub fn empty() -> Self {
        Self {
            names: String::new(),
            entries: Vec::new(),
            generation: 0,
        }
    }

    /// How many rows the arena holds, the root sentinel included.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the arena holds no rows at all (not even the root sentinel).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The `entries` row id at `idx`.
    #[inline]
    pub fn id(&self, idx: usize) -> i64 {
        self.entries[idx].id
    }

    /// The parent's row id at `idx` (`0` for the root sentinel).
    #[inline]
    pub fn parent_id(&self, idx: usize) -> i64 {
        self.entries[idx].parent_id
    }

    /// The file name at `idx`, borrowed from the names buffer.
    #[inline]
    pub fn name(&self, idx: usize) -> &str {
        let entry = &self.entries[idx];
        &self.names[entry.name_offset as usize..entry.name_offset as usize + entry.name_len as usize]
    }

    /// Whether the row at `idx` is a folder.
    #[inline]
    pub fn is_directory(&self, idx: usize) -> bool {
        self.entries[idx].is_directory
    }

    /// The apparent size at `idx`: `None` for a folder and for a hardlink-deduped
    /// name, ❌ never collapsed into `0`.
    #[inline]
    pub fn size(&self, idx: usize) -> Option<u64> {
        self.entries[idx].size.get()
    }

    /// The modification time at `idx` in Unix seconds, `None` where unknown.
    #[inline]
    pub fn modified_at(&self, idx: usize) -> Option<u64> {
        self.entries[idx].modified_at.get()
    }

    /// Which writer generation this arena is a snapshot of.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The highest row id the arena holds, where a catch-up starts reading.
    pub fn last_id(&self) -> Option<i64> {
        self.entries.last().map(|entry| entry.id)
    }

    /// Where the row with this id sits, if it is here.
    ///
    /// A binary search over the id-sorted rows rather than a `HashMap<i64, usize>`.
    /// The map cost ~143 MB on David's 5.39 M-row boot index (5.39 M entries round up
    /// to 8,388,608 hashbrown buckets at 17 bytes each) and bought a ~20 ns lookup over
    /// a ~23-probe one, for a call made a few hundred times per search while ranking
    /// reconstructs paths — microseconds either way against a 100-400 ms scan.
    /// ❌ Don't reintroduce the map: measure the ranking phase first.
    #[inline]
    pub fn index_of_id(&self, id: i64) -> Option<usize> {
        self.entries.binary_search_by_key(&id, |entry| entry.id).ok()
    }

    /// Append the rows created since this arena was read (each past [`Self::last_id`]),
    /// returning how many.
    ///
    /// Into the arena itself when nobody else holds it and it has room
    /// ([`spare_rows`]), which is the normal case: no copy at all. Otherwise into ONE
    /// new arena sized for its rows plus the new ones plus fresh room: a search still
    /// reading the old one keeps it, and a full one grows once rather than doubling.
    pub fn append_created(index: &mut std::sync::Arc<Self>, created: ArenaBuilder) -> usize {
        let rows = created.entries.len();
        if rows == 0 {
            return 0;
        }
        match std::sync::Arc::get_mut(index) {
            Some(owned) if owned.has_room(&created) => append_rows(&mut owned.names, &mut owned.entries, created),
            _ => *index = std::sync::Arc::new(index.copy_with_room(created)),
        }
        debug_assert!(
            index.entries.windows(2).all(|w| w[0].id < w[1].id),
            "the arena must stay strictly ascending by id: index_of_id binary-searches it"
        );
        rows
    }

    /// Whether a batch fits in the arena's spare capacity, rows and names both.
    fn has_room(&self, created: &ArenaBuilder) -> bool {
        self.entries.capacity() - self.entries.len() >= created.entries.len()
            && self.names.capacity() - self.names.len() >= created.names.len()
    }

    /// A new arena holding this one's rows, then `created`'s, with fresh room past both.
    fn copy_with_room(&self, created: ArenaBuilder) -> Self {
        let rows = self.entries.len() + created.entries.len();
        let name_bytes = self.names.len() + created.names.len();
        let spare = spare_rows(rows);

        let mut names = String::with_capacity(name_bytes + spare_name_bytes(spare, name_bytes, rows));
        let mut entries = Vec::with_capacity(rows + spare);
        names.push_str(&self.names);
        entries.extend_from_slice(&self.entries);
        append_rows(&mut names, &mut entries, created);
        Self {
            names,
            entries,
            generation: self.generation,
        }
    }

    /// Rows of room left before a catch-up has to copy the arena.
    #[cfg(any(test, feature = "testing"))]
    pub fn spare_rows_left(&self) -> usize {
        self.entries.capacity() - self.entries.len()
    }

    /// Where the rows live in memory, so a test can tell an in-place append from a copy.
    #[cfg(any(test, feature = "testing"))]
    pub fn rows_address(&self) -> usize {
        self.entries.as_ptr() as usize
    }
}

// ── Building an arena ────────────────────────────────────────────────

/// Rows pushed in ascending id order, then turned into a [`SearchIndex`]. The only way
/// to make one: a loader pushes what it reads (one builder per parallel range, merged
/// in range order by [`ArenaBuilder::merge`]), and a test pushes its fixture.
#[derive(Debug, Default)]
pub struct ArenaBuilder {
    names: String,
    entries: Vec<SearchEntry>,
}

impl ArenaBuilder {
    /// An empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// A builder with room for `rows` rows and `name_bytes` bytes of names.
    pub fn with_capacity(rows: usize, name_bytes: usize) -> Self {
        Self {
            names: String::with_capacity(name_bytes),
            entries: Vec::with_capacity(rows),
        }
    }

    /// Add one row. Ids must strictly ascend (`debug_assert`ed; ❌ never sorted in
    /// release, the loader reads them in order for free).
    ///
    /// `#[inline]`: the loader calls it once per row across the crate edge, and the
    /// dev and `e2e` profiles run without LTO.
    #[inline]
    pub fn push(
        &mut self,
        id: i64,
        parent_id: i64,
        name: &str,
        is_directory: bool,
        size: Option<u64>,
        modified_at: Option<u64>,
    ) {
        debug_assert!(
            self.entries.last().is_none_or(|last| last.id < id),
            "arena rows must arrive in ascending id order: index_of_id binary-searches them"
        );
        let name_offset = self.names.len() as u32;
        self.names.push_str(name);
        self.entries.push(SearchEntry {
            id,
            parent_id,
            name_offset,
            name_len: name.len() as u16,
            is_directory,
            size: OptU64::new(size),
            modified_at: OptU64::new(modified_at),
        });
    }

    /// How many rows are in so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no row is in yet.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The finished arena, with [`spare_rows`] of room for a later catch-up.
    pub fn finish(self, generation: u64) -> SearchIndex {
        Self::merge(vec![self], generation)
    }

    /// Concatenate builders that hold consecutive id ranges, in order, rebasing each
    /// one's name offsets.
    ///
    /// ⚠️ **This is the load's memory peak**: the destination is reserved at full size
    /// while the segments still hold the same bytes, so a 320 MB arena transiently costs
    /// ~640 MB. It falls monotonically as each segment drops at the end of its iteration.
    /// Removing the peak entirely needs the mapped arena (GitHub #114), not a bigger loop.
    ///
    /// The destination also reserves [`spare_rows`] of room past the rows it holds, which
    /// is what lets [`SearchIndex::append_created`] append without copying the arena.
    pub fn merge(segments: Vec<ArenaBuilder>, generation: u64) -> SearchIndex {
        let total_rows: usize = segments.iter().map(|s| s.entries.len()).sum();
        let total_names: usize = segments.iter().map(|s| s.names.len()).sum();
        let spare = spare_rows(total_rows);

        let mut names = String::with_capacity(total_names + spare_name_bytes(spare, total_names, total_rows));
        let mut entries = Vec::with_capacity(total_rows + spare);
        for segment in segments {
            append_rows(&mut names, &mut entries, segment);
        }
        debug_assert!(
            entries.windows(2).all(|w| w[0].id < w[1].id),
            "the arena must stay strictly ascending by id: index_of_id binary-searches it"
        );
        SearchIndex {
            names,
            entries,
            generation,
        }
    }
}

/// One hand-written fixture row: `(id, parent_id, name, is_directory, size, modified_at)`.
#[cfg(any(test, feature = "testing"))]
pub type FixtureRow<'a> = (i64, i64, &'a str, bool, Option<u64>, Option<u64>);

/// A fixture arena: its rows put in id order (the order the loader reads them in, so
/// a fixture can list them however reads best) and pushed through [`ArenaBuilder`].
#[cfg(any(test, feature = "testing"))]
pub fn arena_of(rows: &[FixtureRow<'_>], generation: u64) -> SearchIndex {
    let mut sorted = rows.to_vec();
    sorted.sort_unstable_by_key(|row| row.0);
    let mut builder = ArenaBuilder::new();
    for (id, parent_id, name, is_directory, size, modified_at) in sorted {
        builder.push(id, parent_id, name, is_directory, size, modified_at);
    }
    builder.finish(generation)
}

/// Move a builder's rows onto the end of an arena, rebasing their name offsets.
fn append_rows(names: &mut String, entries: &mut Vec<SearchEntry>, mut segment: ArenaBuilder) {
    let base = names.len() as u32;
    names.push_str(&segment.names);
    for entry in &mut segment.entries {
        entry.name_offset += base;
    }
    entries.append(&mut segment.entries);
}

/// How many rows of room an arena keeps past the ones it holds: 1/128 of them, and
/// never fewer than 64.
///
/// A catch-up appends the rows created since the arena was read, and a walk's rows
/// plus a few minutes of background indexing are thousands, not millions. Room for
/// 49,000 of them costs 2 MB of rows and about 1 MB of names on a 6.3 M-row boot
/// index, where running out means reallocating the whole arena: a second copy of it,
/// transiently. A small arena keeps little room because copying it is cheap.
pub fn spare_rows(rows: usize) -> usize {
    (rows / 128).max(64)
}

/// Name bytes to keep for `spare` rows, at the arena's own average name length.
fn spare_name_bytes(spare: usize, name_bytes: usize, rows: usize) -> usize {
    const FALLBACK_AVG_NAME_BYTES: usize = 20;
    let average = name_bytes.checked_div(rows).unwrap_or(FALLBACK_AVG_NAME_BYTES);
    spare.saturating_mul(average.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT_ID: i64 = ARENA_ROOT_ID;

    // ── Building ─────────────────────────────────────────────────────

    /// A rebased offset is the one thing the merge can silently corrupt: every row
    /// keeps its own name offset until the builder it came from is concatenated, so
    /// an off-by-one base shifts a whole segment's names by a few bytes and search
    /// starts matching neighbours' names.
    #[test]
    fn merging_builders_rebases_every_name_offset() {
        let segment = |id: i64, name: &str| {
            let mut builder = ArenaBuilder::new();
            builder.push(id, ROOT_ID, name, false, None, None);
            builder
        };
        let index = ArenaBuilder::merge(vec![segment(1, "alpha"), segment(2, "beta"), segment(3, "gamma")], 0);
        let read: Vec<&str> = (0..index.len()).map(|i| index.name(i)).collect();
        assert_eq!(read, vec!["alpha", "beta", "gamma"]);
    }

    // ── The sentinel encoding ────────────────────────────────────────

    /// Every value the index can hold must survive the eight-byte encoding unchanged,
    /// `None` included and `0` above all: a zero-byte file is a real thing, and reading
    /// it back as "size unknown" would let it slip through a size filter.
    #[test]
    fn every_representable_value_survives_the_round_trip() {
        assert_eq!(OptU64::new(None).get(), None);
        assert_eq!(OptU64::NONE.get(), None);

        for value in [
            0,                   // an empty file, and a folder modified at the epoch
            1,                   // the smallest non-empty file
            512,                 // one block
            1_700_000_000,       // a plausible mtime
            994_663_481_856,     // the largest file on David's disk (2026-08-06)
            u32::MAX as u64,     // where a 32-bit size would have wrapped
            u32::MAX as u64 + 1, // and just past it
            i64::MAX as u64,     // the largest value a SQLite INTEGER can hold
            u64::MAX - 1,        // and the largest this encoding can hold at all
        ] {
            assert_eq!(
                OptU64::new(Some(value)).get(),
                Some(value),
                "{value} should survive the round trip"
            );
        }
    }
}
