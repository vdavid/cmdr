//! What a pane changes about how it shows a listing, and the engine-wide
//! scratch visibility.

use crate::host::ScratchVisibility;
use crate::listing_id::ListingId;
use crate::operations::PaneMarks;
use crate::quick_filter::NameFilterResult;
use crate::quick_filter::OnNoMatch;
use crate::{ListingLookupError, ResortResult};
use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

use super::Listings;

impl Listings {
    /// Re-sorts `id` in place, asking the index for fresh folder sizes first.
    /// With `expected_sequence`, refuses when the pane's rows aren't the
    /// committed ones.
    pub fn resort(
        &self,
        id: &ListingId,
        order: (SortColumn, SortOrder, DirectorySortMode),
        include_hidden: bool,
        marks: PaneMarks<'_>,
        expected_sequence: Option<u64>,
    ) -> Result<ResortResult, ListingLookupError> {
        let (sort_by, sort_order, dir_sort_mode) = order;
        self.inner.resort_listing(
            id.as_str(),
            sort_by,
            sort_order,
            dir_sort_mode,
            marks.cursor_filename,
            include_hidden,
            marks.selected,
            marks.all_selected,
            expected_sequence,
        )
    }

    /// The pane showing `id` now shows (or hides) hidden files, so its diffs
    /// speak its new rows.
    pub fn set_include_hidden(
        &self,
        id: &ListingId,
        include_hidden: bool,
        expected_sequence: Option<u64>,
        marks: PaneMarks<'_>,
    ) -> Result<ResortResult, ListingLookupError> {
        self.inner.set_listing_include_hidden(
            id.as_str(),
            include_hidden,
            expected_sequence,
            marks.cursor_filename,
            marks.selected,
            marks.all_selected,
        )
    }

    /// Sets (or, with `None` or an empty pattern, clears) the pane's quick
    /// filter, and says where `marks` landed. `on_no_match` says whether a
    /// pattern matching nothing is refused; with `expected_sequence`, so is a
    /// request whose rows aren't the committed ones.
    pub fn set_name_filter_guarded(
        &self,
        id: &ListingId,
        pattern: Option<&str>,
        include_hidden: bool,
        marks: PaneMarks<'_>,
        on_no_match: OnNoMatch,
        expected_sequence: Option<u64>,
    ) -> Result<NameFilterResult, ListingLookupError> {
        self.inner.set_listing_name_filter_guarded(
            id.as_str(),
            pattern,
            include_hidden,
            marks,
            on_no_match,
            expected_sequence,
        )
    }

    /// Which scratch files the user asked to see.
    pub fn scratch_visibility(&self) -> ScratchVisibility {
        self.inner.scratch_visibility()
    }

    /// Applies the user's scratch-visibility settings to every listing from its
    /// next read on; a listing already cached reconciles lazily, at its next read
    /// or mutation (`visible_rows.rs`).
    pub fn set_scratch_visibility(&self, visibility: ScratchVisibility) {
        self.inner.scratch.set(visibility);
    }
}
