//! Directory listing lifecycle, cache API, sorting, and statistics.
//!
//! This is the synchronous, frontend-facing API. Low-level disk I/O is in reading.rs,
//! async streaming is in streaming.rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;
use std::sync::atomic::Ordering;
#[cfg(test)]
use uuid::Uuid;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::Inner;
use crate::cached_listing::CachedListing;
#[cfg(test)]
use crate::pipeline::ReadBy;
use crate::sorting::{DirectorySortMode, SortColumn, SortOrder, sort_entries};
use crate::visible_rows::VisibleRows;
#[cfg(test)]
use cmdr_fs::benchmark;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::log_private::LogPrivate;

// ============================================================================
// Listing lifecycle
// ============================================================================

/// Result of starting a new directory listing synchronously.
#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct ListingStartResult {
    pub(crate) listing_id: String,
    pub(crate) total_count: usize,
}

impl Inner {
    /// Starts a directory listing on a volume and returns once it's cached: the
    /// synchronous twin of `list_directory_start_streaming`, which needs a Tauri
    /// runtime for its events. A test seam only; every pane lists through the
    /// streaming path.
    #[cfg(test)]
    pub(crate) async fn list_directory_start_with_volume(
        &self,
        volume_id: &str,
        path: &Path,
        include_hidden: bool,
        sort_by: SortColumn,
        sort_order: SortOrder,
        dir_sort_mode: DirectorySortMode,
    ) -> Result<ListingStartResult, std::io::Error> {
        // Reset benchmark epoch for this navigation
        benchmark::reset_epoch();
        benchmark::log_event_value("list_directory_start CALLED", path.display());

        // Resolve the volume, routing a `.zip`-crossing path to its read-only
        // `ArchiveVolume`. The cache keeps the FE-provided `volume_id` (parent drive);
        // the downstream re-read sites re-resolve the archive from `(volume_id, path)`.
        let resolved = self.volumes.resolve(volume_id, path).await;
        let is_routed = resolved.is_routed();
        let volume = resolved.volume.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Volume '{}' not found", volume_id),
            )
        })?;

        // Use the Volume trait to list the directory. A refused read is the error
        // and nothing else: the streaming path's `ListingEvents::finished` is where
        // the host hears about a failure (and moves a volume off a dead mount).
        let all_entries = volume.list_directory(path, None).await.map_err(std::io::Error::other)?;
        benchmark::log_event_value("volume.list_directory COMPLETE, entries", all_entries.len());

        // Generate listing ID
        let listing_id = Uuid::new_v4().to_string();

        // Enrich (before sorting, so sort-by-size works for directories) and fold
        // in the rows a PANE sees that the volume doesn't hold, as the streaming
        // path does (`pipeline.rs`). A routed volume has no drive index (its paths
        // aren't real FS paths), so enrich and verify are skipped.
        let read_by = ReadBy::Volume {
            volume: &volume,
            routed: is_routed,
        };
        let mut all_entries = self.prepare(read_by, volume_id, path, all_entries).await;
        if !is_routed {
            self.index.verify_directory(volume_id, path);
        }

        // Sort the entries
        all_entries.sort(&self.collator(), sort_by, sort_order, dir_sort_mode);

        // Cache the entries FIRST (watcher will read from here), then read the row
        // count back off the listing's own map so the number the frontend sizes its
        // scroller with comes from the same filter every later fetch goes through.
        let listing = CachedListing::new(
            self,
            volume_id.to_string(),
            path.to_path_buf(),
            all_entries,
            include_hidden,
            sort_by,
            sort_order,
            dir_sort_mode,
        );
        let total_count = listing.rows(include_hidden).len();
        let cached_path = listing.path.as_path().to_path_buf();
        self.cache.write_ignore_poison().insert(listing_id.clone(), listing);

        // Whatever a subsystem keeps alive while a pane shows this directory: in
        // Cmdr the change watch first, then the git portal's per-repo watcher.
        // `host/observers.rs`.
        self.listing_opened(&super::host::OpenedListing {
            id: &super::listing_id::ListingId::new(listing_id.as_str()),
            volume_id,
            volume: volume.as_ref(),
            path,
            stored_path: &cached_path,
            engine: self,
        });

        benchmark::log_event("list_directory_start RETURNING");
        Ok(ListingStartResult {
            listing_id,
            total_count,
        })
    }

    /// Ends a directory listing: the cache entry and its pending diffs, then
    /// every observer (the change watch among them).
    pub(crate) fn list_directory_end(&self, listing_id: &str) {
        // Remove and discard publication under the mutation lock (cache then queue).
        self.write_cache().remove(listing_id);

        // AFTER the cache removal, ❗ never before: an observer's own detached arm
        // reconciles against listing-cache membership, so releasing while the entry
        // is still there lets a racing arm re-take what this just gave back. The
        // change watch disarms here too, so an event landing in the gap finds no
        // listing to patch.
        self.listing_closed(&super::listing_id::ListingId::new(listing_id));
    }

    /// Commits visibility and remaps the cursor and selection under one lock.
    /// The response supersedes queued old-space transitions, including their counts.
    pub(crate) fn set_listing_include_hidden(
        &self,
        listing_id: &str,
        include_hidden: bool,
        expected_sequence: Option<u64>,
        cursor_filename: Option<&str>,
        selected_indices: Option<&[usize]>,
        all_selected: bool,
    ) -> Result<ResortResult, ListingLookupError> {
        let visibility = self.scratch_visibility();
        let mut cache = self.write_cache();
        let (listing, publish) = cache
            .get_mut(listing_id)
            .ok_or_else(|| ListingLookupError::gone(listing_id))?;
        listing.reconcile_scratch(listing_id, visibility, &publish);
        check_expected(listing_id, listing, expected_sequence)?;
        let old_rows = listing.pane_rows();
        let selected_paths = selected_indices.or(all_selected.then_some(&[])).map(|indices| {
            if all_selected {
                old_rows.iter().map(|e| e.path.clone()).collect::<Vec<_>>()
            } else {
                indices
                    .iter()
                    .filter_map(|&i| old_rows.get(i).map(|e| e.path.clone()))
                    .collect()
            }
        });
        let cursor_path =
            cursor_filename.and_then(|name| old_rows.iter().find(|e| e.name == name).map(|e| e.path.clone()));
        drop(old_rows);
        if listing.set_include_hidden(include_hidden) {
            listing.advance_sequence();
            publish.drop_pending(listing_id);
        }
        listing.touch();
        let rows = listing.pane_rows();
        Ok(ResortResult {
            sequence: listing.sequence.load(Ordering::Acquire),
            total_count: rows.len(),
            new_cursor_index: cursor_path.and_then(|path| rows.iter().position(|e| e.path == path)),
            new_selected_indices: selected_paths.map(|paths| {
                let paths: std::collections::HashSet<_> = paths.into_iter().collect();
                rows.iter()
                    .enumerate()
                    .filter(|(_, e)| paths.contains(&e.path))
                    .map(|(i, _)| i)
                    .collect()
            }),
        })
    }
}

// ============================================================================
// On-demand virtual scrolling API (cache accessors)
// ============================================================================

/// Why a listing accessor couldn't answer: gone, or a guarded row space changed.
///
/// Typed so the frontend can tell a pane whose listing went away (and re-list it)
/// from stale row indices, without reading a message. The cache lock recovers
/// from poison rather than refusing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListingLookupError {
    /// Ended by its pane, never started, or reclaimed by the orphan reaper.
    Gone {
        /// The listing the caller asked for.
        listing_id: String,
    },
    /// The supplied row space is not the committed listing state.
    Changed {
        /// The listing whose rows changed.
        listing_id: String,
    },
}

impl ListingLookupError {
    pub(crate) fn changed(listing_id: &str) -> Self {
        Self::Changed {
            listing_id: listing_id.to_string(),
        }
    }

    pub(crate) fn gone(listing_id: &str) -> Self {
        Self::Gone {
            listing_id: listing_id.to_string(),
        }
    }
}

impl std::fmt::Display for ListingLookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gone { listing_id } => write!(f, "listing {listing_id} isn't cached"),
            Self::Changed { listing_id } => write!(f, "listing {listing_id} changed"),
        }
    }
}

impl Inner {
    /// Runs `f` against a cached listing, or reports that it's gone.
    ///
    /// Every read accessor below shares this preamble: take the cache read lock, find
    /// the listing, and stamp it so the six-hour orphan reaper knows a live pane is
    /// still behind it.
    pub(crate) fn with_listing<R>(
        &self,
        listing_id: &str,
        f: impl FnOnce(&CachedListing) -> R,
    ) -> Result<R, ListingLookupError> {
        let cache = self.reconciled_cache(&[listing_id]);
        let listing = cache
            .get(listing_id)
            .ok_or_else(|| ListingLookupError::gone(listing_id))?;

        listing.touch();

        Ok(f(listing))
    }

    /// The panes' heartbeat: stamps every listing in `listing_ids` as still on screen,
    /// and returns the ids this cache no longer holds.
    ///
    /// A pane can sit on one folder for days with no reads and no FS events, so access
    /// alone can't tell the orphan reaper a listing is live. Each pane names its listing
    /// here well inside `ORPHAN_IDLE_WINDOW`; a listing whose pane leaked stops being
    /// named and is reaped as before. A returned id is a pane showing rows the backend
    /// can't serve, which the frontend answers by re-listing that folder.
    pub(crate) fn keep_listings_alive(&self, listing_ids: &[String]) -> Vec<String> {
        let cache = self.cache.read_ignore_poison();
        listing_ids
            .iter()
            .filter(|id| match cache.get(id.as_str()) {
                Some(listing) => {
                    listing.touch();
                    false
                }
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Gets a range of entries from a cached listing.
    pub(crate) fn get_file_range(
        &self,
        listing_id: &str,
        span: RowSpan,
        include_hidden: bool,
    ) -> Result<Vec<FileEntry>, ListingLookupError> {
        self.with_listing(listing_id, |listing| {
            let rows = listing.rows(include_hidden);
            let end = span.start.saturating_add(span.count).min(rows.len());
            (span.start..end)
                .filter_map(|row| rows.get(row).cloned())
                .collect::<Vec<FileEntry>>()
        })
    }

    /// Gets total count of entries in a cached listing.
    #[cfg(test)]
    pub(crate) fn get_total_count(&self, listing_id: &str, include_hidden: bool) -> Result<usize, ListingLookupError> {
        self.with_listing(listing_id, |listing| listing.rows(include_hidden).len())
    }

    /// Finds the index of a file by name in a cached listing, where the cursor lands.
    ///
    /// The name may come from outside this listing (a restored cursor, MCP, a reveal
    /// from Finder) and spell an accented name another way than the volume stores it,
    /// so a unique look-alike answers when the exact name doesn't (`row_of_any_spelling`).
    pub(crate) fn find_file_index(
        &self,
        listing_id: &str,
        name: &str,
        include_hidden: bool,
    ) -> Result<Option<usize>, ListingLookupError> {
        self.with_listing(listing_id, |listing| {
            listing.rows(include_hidden).row_of_any_spelling(name)
        })
    }
}

/// Which side of a named row to read: the one before it or the one after it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RowBeside {
    /// The row before it.
    Previous,
    /// The row after it.
    Next,
}

impl Inner {
    /// The entry sitting immediately before or after the one named `name`, or `None`
    /// when that name isn't in the listing or the row beside it doesn't exist.
    ///
    /// Resolving the anchor and reading its neighbour happen against ONE snapshot,
    /// which is the whole point: a caller doing it in two calls (`find_file_index`,
    /// then `get_file_at`) hands back a row that a rename landing between the two has
    /// already moved. Callers ask this instead of an index when they have a row they
    /// know and want the one beside it, because a name survives a re-sort that any
    /// index they hold does not.
    pub(crate) fn get_file_beside(
        &self,
        listing_id: &str,
        name: &str,
        side: RowBeside,
        include_hidden: bool,
    ) -> Result<Option<FileEntry>, ListingLookupError> {
        self.with_listing(listing_id, |listing| {
            let rows = listing.rows(include_hidden);
            let anchor = rows.row_of(name)?;
            let beside = match side {
                RowBeside::Previous => anchor.checked_sub(1)?,
                RowBeside::Next => anchor + 1,
            };
            rows.get(beside).cloned()
        })
    }

    /// Finds the indices of multiple files by name in a cached listing (batch version of
    /// `find_file_index`).
    ///
    /// Single pass over the listing's rows, O(rows + names). Returns only found names as keys.
    pub(crate) fn find_file_indices(
        &self,
        listing_id: &str,
        names: &[String],
        include_hidden: bool,
    ) -> Result<HashMap<String, usize>, ListingLookupError> {
        self.with_listing(listing_id, |listing| {
            let lookup: std::collections::HashSet<&str> = names.iter().map(|n| n.as_str()).collect();
            let mut result = HashMap::with_capacity(names.len());

            for (row, entry) in listing.rows(include_hidden).iter().enumerate() {
                if lookup.contains(entry.name.as_str()) {
                    result.insert(entry.name.clone(), row);
                }
            }

            result
        })
    }

    /// Gets a single file at the given index.
    pub(crate) fn get_file_at(
        &self,
        listing_id: &str,
        index: usize,
        include_hidden: bool,
    ) -> Result<Option<FileEntry>, ListingLookupError> {
        self.with_listing(listing_id, |listing| {
            let rows = listing.rows(include_hidden);
            let result = rows.get(index).cloned();
            if result.is_none() {
                // Out-of-bounds is expected briefly after a mutation: the FE iterates over a
                // cached `totalCount` that may lag the BE listing during the async refetch
                // window opened by a `directory-diff` event. The FE handles `None` gracefully
                // (skips the entry, breaks the loop). Logged at debug so we still have the
                // breadcrumb when investigating cursor/selection bugs without firing crash
                // reports for legitimate drift.
                log::debug!(
                    "get_file_at: index {} out of bounds (listing {} has {} entries at {}): likely FE/BE drift after async listing refresh",
                    index,
                    listing_id,
                    rows.len(),
                    LogPrivate::dir_path(listing.path.as_path())
                );
            }
            result
        })
    }
}

// ============================================================================
// Re-sorting
// ============================================================================

/// Where the pane's cursor and selection stood before a change that moves
/// rows, so the answer says where they landed.
// DEFAULT-OK: no cursor and nothing selected is a pane that marked nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaneMarks<'a> {
    /// The name under the cursor.
    pub cursor_filename: Option<&'a str>,
    /// The selected rows, as the pane numbered them before the change.
    pub selected: Option<&'a [usize]>,
    /// Every row is selected (`selected` is then ignored).
    pub all_selected: bool,
}

/// A run of a pane's rows: `count` rows from row `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowSpan {
    /// The first row.
    pub start: usize,
    /// How many rows, at most.
    pub count: usize,
}

/// Result of re-sorting a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResortResult {
    /// The revision the re-sorted rows are at.
    pub sequence: u64,
    /// How many rows the pane shows.
    pub total_count: usize,
    /// New index of the file that was at the cursor position before re-sorting.
    /// None if the filename wasn't provided or wasn't found.
    pub new_cursor_index: Option<usize>,
    /// New indices of previously selected files after re-sorting.
    /// None if no selected_indices were provided.
    pub new_selected_indices: Option<Vec<usize>>,
}

impl Inner {
    /// Re-sorts an existing cached listing in-place.
    ///
    /// More efficient than creating a new listing when you just want to change the sort order.
    #[allow(
        clippy::too_many_arguments,
        reason = "Resort requires sort params, cursor tracking, and selection state"
    )]
    pub(crate) fn resort_listing(
        &self,
        listing_id: &str,
        sort_by: SortColumn,
        sort_order: SortOrder,
        dir_sort_mode: DirectorySortMode,
        cursor_filename: Option<&str>,
        include_hidden: bool,
        selected_indices: Option<&[usize]>,
        all_selected: bool,
        expected_sequence: Option<u64>,
    ) -> Result<ResortResult, ListingLookupError> {
        // All three before the lock, since each asks the host. Sorting by size needs
        // the index's current folder sizes, asked with no engine lock held while the
        // index answers (`index_sizes.rs`).
        let visibility = self.scratch_visibility();
        let collator = self.collator();
        let asked = self.ask_index_sizes(listing_id);
        let mut cache = self.write_cache();

        let (listing, publish) = cache
            .get_mut(listing_id)
            .ok_or_else(|| ListingLookupError::gone(listing_id))?;

        listing.reconcile_scratch(listing_id, visibility, &publish);
        check_expected(listing_id, listing, expected_sequence)?;
        if expected_sequence.is_some() && listing.include_hidden() != include_hidden {
            return Err(ListingLookupError::changed(listing_id));
        }
        listing.touch();
        // Written silently: the resort drops every pending diff and the pane
        // re-fetches its rows, sizes included. A publish here would also move the
        // sequence the caller just checked.
        if let Some(asked) = asked {
            asked.write_to(listing);
        }

        // Collect filenames of selected files before re-sorting
        let selected_filenames: Option<Vec<String>> = if all_selected {
            // All files selected - we'll rebuild the full set after sort
            None
        } else {
            selected_indices.map(|indices| {
                let rows = listing.rows(include_hidden);
                indices
                    .iter()
                    .filter_map(|&idx| rows.get(idx).map(|e| e.name.clone()))
                    .collect()
            })
        };

        // Re-sort the entries
        sort_entries(&collator, listing.entries_mut(), sort_by, sort_order, dir_sort_mode);
        listing.sort_by = sort_by;
        listing.directory_sort_mode = dir_sort_mode;
        listing.sort_order = sort_order;
        listing.advance_sequence();
        publish.drop_pending(listing_id);

        let rows = listing.rows(include_hidden);

        // Find the new cursor position
        let new_cursor_index = cursor_filename.and_then(|name| rows.row_of(name));

        // Find new indices of selected files
        let new_selected_indices = if all_selected {
            Some((0..rows.len()).collect())
        } else {
            selected_filenames.map(|filenames| {
                let names_to_rows: HashMap<&str, usize> = rows
                    .iter()
                    .enumerate()
                    .map(|(row, entry)| (entry.name.as_str(), row))
                    .collect();
                filenames
                    .iter()
                    .filter_map(|name| names_to_rows.get(name.as_str()).copied())
                    .collect()
            })
        };

        Ok(ResortResult {
            sequence: listing.sequence.load(Ordering::Acquire),
            total_count: rows.len(),
            new_cursor_index,
            new_selected_indices,
        })
    }
}

pub(super) fn check_expected(
    listing_id: &str,
    listing: &CachedListing,
    expected: Option<u64>,
) -> Result<(), ListingLookupError> {
    if expected.is_some_and(|s| s != listing.sequence.load(Ordering::Acquire)) {
        return Err(ListingLookupError::changed(listing_id));
    }
    Ok(())
}

/// Exact paths and kinds consumed from one guarded backend row space.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSnapshot {
    /// Every selected row's full path, in row order.
    pub paths: Vec<String>,
    /// How many of them are files.
    pub file_count: usize,
    /// How many of them are folders.
    pub folder_count: usize,
}

impl Inner {
    /// Runs `read` over the pane's rows only while they are the committed ones:
    /// the revision the pane applied (`expected_sequence`) and its own
    /// hidden-files setting. Every selected-row read goes through here, because a
    /// watcher insert renumbers rows and the old row numbers would name other files.
    fn with_guarded_pane_rows<R>(
        &self,
        listing_id: &str,
        include_hidden: bool,
        expected_sequence: u64,
        read: impl FnOnce(&VisibleRows<'_>) -> Result<R, ListingLookupError>,
    ) -> Result<R, ListingLookupError> {
        let cache = self.reconciled_cache(&[listing_id]);
        let listing = cache
            .get(listing_id)
            .ok_or_else(|| ListingLookupError::gone(listing_id))?;
        check_expected(listing_id, listing, Some(expected_sequence))?;
        if listing.include_hidden() != include_hidden {
            return Err(ListingLookupError::changed(listing_id));
        }
        let result = read(&listing.pane_rows())?;
        listing.touch();
        Ok(result)
    }

    /// The entries on the selected rows, in selection order. A row past the end
    /// refuses the whole read: it can only mean the rows moved under the caller.
    fn guarded_selected_rows<'a>(
        listing_id: &str,
        rows: &VisibleRows<'a>,
        selected: &[usize],
    ) -> Result<Vec<&'a FileEntry>, ListingLookupError> {
        selected
            .iter()
            .map(|&row| rows.get(row).ok_or_else(|| ListingLookupError::changed(listing_id)))
            .collect()
    }

    pub(crate) fn get_selection_snapshot(
        &self,
        listing_id: &str,
        include_hidden: bool,
        selected_indices: &[usize],
        expected_sequence: u64,
    ) -> Result<SelectionSnapshot, ListingLookupError> {
        self.with_guarded_pane_rows(listing_id, include_hidden, expected_sequence, |rows| {
            let entries = Self::guarded_selected_rows(listing_id, rows, selected_indices)?;
            let folder_count = entries.iter().filter(|entry| entry.is_directory).count();
            Ok(SelectionSnapshot {
                file_count: entries.len() - folder_count,
                folder_count,
                paths: entries.into_iter().map(|entry| entry.path.clone()).collect(),
            })
        })
    }

    /// The full entries on the selected rows, under the same guard as
    /// [`get_selection_snapshot`](Self::get_selection_snapshot): the delete
    /// dialog's input, which shows names, sizes, and kinds.
    pub(crate) fn get_selection_entries(
        &self,
        listing_id: &str,
        include_hidden: bool,
        selected_indices: &[usize],
        expected_sequence: u64,
    ) -> Result<Vec<FileEntry>, ListingLookupError> {
        self.with_guarded_pane_rows(listing_id, include_hidden, expected_sequence, |rows| {
            Ok(Self::guarded_selected_rows(listing_id, rows, selected_indices)?
                .into_iter()
                .cloned()
                .collect())
        })
    }

    /// The names on the selected rows (every row with `None`), only while the
    /// pane's rows are the committed ones: the multi-rename sheet's input.
    pub(crate) fn get_selection_names(
        &self,
        listing_id: &str,
        include_hidden: bool,
        selected: Option<&[usize]>,
        expected_sequence: u64,
    ) -> Result<Vec<String>, ListingLookupError> {
        self.with_guarded_pane_rows(listing_id, include_hidden, expected_sequence, |rows| {
            Ok(match selected {
                Some(selected) => Self::guarded_selected_rows(listing_id, rows, selected)?
                    .into_iter()
                    .map(|entry| entry.name.clone())
                    .collect(),
                None => rows.iter().map(|entry| entry.name.clone()).collect(),
            })
        })
    }

    /// The distinct volume ids under `prefix` that have at least one cached listing.
    ///
    /// What a device backend asks when its event names an object the protocol alone
    /// can't place: "which of my storages is a pane showing?" Ids only, so nothing
    /// clones an open directory's entries to answer it.
    pub(crate) fn volume_ids_with_listings(&self, prefix: &str) -> Vec<String> {
        // Recover rather than answer empty: an empty answer reads as "no pane is
        // showing anything", which sends a device backend down the blanket-refresh
        // path for as long as the process lives.
        let cache = self.cache.read_ignore_poison();

        let mut ids: Vec<String> = cache
            .values()
            .filter(|listing| listing.volume_id.starts_with(prefix))
            .map(|listing| listing.volume_id.clone())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}
