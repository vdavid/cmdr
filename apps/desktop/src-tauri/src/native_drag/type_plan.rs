//! Pure, locality-aware pasteboard composition for native drags.
//!
//! Pasteboard layout is policy, not incidental code: it's where the wry
//! constraint and the Finder interplay become visible and testable. This module
//! owns that policy as a pure function so the AppKit-touching code in the parent
//! module stays a thin executor of the plan.
//!
//! ## Locality is a property of the drag SESSION, not of individual items
//!
//! A single drag can never mix local and virtual items: selections are
//! single-pane and panes are single-volume. So locality is decided once, at the
//! boundary, and applies uniformly to every item. The current code's `i == 0`
//! per-item branching is exactly where a partial strip could hide; keying the
//! whole plan on one [`DragSessionLocality`] value makes that impossible by
//! construction.
//!
//! ## Local sessions: match Finder — files only, no path text
//!
//! - `public.file-url` on every item (the URL's `absoluteString`).
//! - `NSFilenamesPboardType` (legacy `NSArray<NSString>` of all paths) on the
//!   first item only. Required for stock wry's `collect_paths`.
//!
//! No `public.utf8-plain-text`. Finder and Forklift publish files only (verified:
//! a browser drop from Finder exposes `types: ["Files"]`, from Cmdr-with-text
//! `["text/plain", "Files"]`). The extra text item made some browser upload
//! widgets treat the drop as text instead of a file, so a file dragged from Cmdr
//! into a `<input type="file">` was ignored where the same file from Finder
//! worked (issue #28). Terminals (Warp, etc.) read the file URL / filenames and
//! insert the path themselves, exactly as they do for a Finder drag, so dropping
//! the text item costs nothing there.
//!
//! ## Virtual sessions: nothing external apps can materialize as garbage
//!
//! NO file-url, NO filenames — across EVERY item. A virtual path's `file://` URL
//! is bogus (the file doesn't exist locally) and an auto-derived (or explicit)
//! filenames entry is the textClipping junk Finder materializes. Promise-only
//! items still fire wry's drop event with an empty path vector (no panic), so the
//! in-app self-drag path keeps working via recorded identity. A virtual item's
//! pasteboard payload is empty here; the parent module attaches the
//! `NSFilePromiseProvider` writer that streams the real bytes on an external drop.

/// Whether a drag session's source volume is locally materialized (local FS or
/// an OS-mounted share, where a `file://` URL is real) or protocol-only /
/// virtual (MTP, direct SMB, search-results), where it isn't.
///
/// Decided once per session at the drag-start boundary, never per item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSessionLocality {
    /// Paths are real local filesystem paths. Keep the file-url + filenames layout.
    Local,
    /// Paths are volume-relative virtual paths with no local backing. Publish
    /// no legacy types; the parent module attaches a promise provider per item.
    Virtual,
}

/// The pasteboard representations a single dragging item should advertise.
///
/// Each `Option` is `None` when that representation must be omitted. A virtual
/// session yields every field `None` (an empty item); a local session fills the
/// fields per the layout above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteboardItemPlan {
    /// `public.file-url`: the item's `file://` URL absolute string.
    pub file_url: Option<String>,
    /// `NSFilenamesPboardType`: the full legacy path array (first item only).
    pub filenames: Option<Vec<String>>,
}

impl PasteboardItemPlan {
    /// An item that advertises nothing — the virtual-session payload.
    fn empty() -> Self {
        Self {
            file_url: None,
            filenames: None,
        }
    }

    /// Whether this item advertises no representations at all. Used by tests to
    /// assert the virtual-session payload strips everything; the executor in the
    /// parent module just reads the `Option` fields directly.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.file_url.is_none() && self.filenames.is_none()
    }
}

/// Builds the per-item pasteboard plan for a whole drag session.
///
/// `paths` are the paths as the source volume knows them (absolute local for a
/// local session; volume-relative for a virtual one). The returned vec has one
/// entry per input path, in order.
///
/// Pure: no AppKit, no I/O. The parent module turns each [`PasteboardItemPlan`]
/// into `NSPasteboardItem` representations (and attaches a promise provider for
/// virtual items).
pub fn plan_pasteboard_items(paths: &[String], locality: DragSessionLocality) -> Vec<PasteboardItemPlan> {
    match locality {
        DragSessionLocality::Virtual => {
            // No file-url, no filenames — across EVERY item.
            paths.iter().map(|_| PasteboardItemPlan::empty()).collect()
        }
        DragSessionLocality::Local => paths
            .iter()
            .enumerate()
            .map(|(i, path)| PasteboardItemPlan {
                file_url: Some(path.clone()),
                // The full filenames array rides only on the first item.
                filenames: if i == 0 { Some(paths.to_vec()) } else { None },
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // --- Local session: files only (file-url + filenames), no path text ---

    #[test]
    fn local_single_item_carries_url_and_filenames_but_no_text() {
        let plan = plan_pasteboard_items(&p(&["/Users/me/file.jpg"]), DragSessionLocality::Local);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].file_url.as_deref(), Some("/Users/me/file.jpg"));
        assert_eq!(
            plan[0].filenames.as_deref(),
            Some(["/Users/me/file.jpg".to_string()].as_slice())
        );
    }

    #[test]
    fn local_every_item_carries_its_own_file_url() {
        let paths = p(&["/a/one.jpg", "/has space/two.jpg", "/a/three.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Local);
        assert_eq!(plan[0].file_url.as_deref(), Some("/a/one.jpg"));
        assert_eq!(plan[1].file_url.as_deref(), Some("/has space/two.jpg"));
        assert_eq!(plan[2].file_url.as_deref(), Some("/a/three.jpg"));
    }

    #[test]
    fn local_filenames_ride_only_on_the_first_item() {
        let paths = p(&["/a/one.jpg", "/a/two.jpg", "/a/three.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Local);
        // The full path list rides on the first item; later items carry none.
        assert_eq!(plan[0].filenames.as_deref(), Some(paths.as_slice()));
        assert!(plan[1].filenames.is_none());
        assert!(plan[2].filenames.is_none());
    }

    // --- Virtual session: empty across every item (the textClipping fix) ---

    #[test]
    fn virtual_single_item_is_empty() {
        let plan = plan_pasteboard_items(&p(&["/photos/sunset.jpg"]), DragSessionLocality::Virtual);
        assert_eq!(plan.len(), 1);
        assert!(plan[0].is_empty());
        assert!(plan[0].file_url.is_none());
        assert!(plan[0].filenames.is_none());
    }

    #[test]
    fn virtual_strips_everything_across_every_item_not_just_first() {
        // The current `i == 0` branching is where a partial strip would hide.
        // Assert EVERY item is empty, including the first.
        let paths = p(&["/photos/a.jpg", "/photos/b.jpg", "/photos/c.jpg", "/photos/d.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Virtual);
        assert_eq!(plan.len(), 4);
        for (i, item) in plan.iter().enumerate() {
            assert!(item.is_empty(), "virtual item {i} must advertise nothing, got {item:?}");
        }
    }

    #[test]
    fn empty_paths_yield_empty_plan_for_both_localities() {
        assert!(plan_pasteboard_items(&[], DragSessionLocality::Local).is_empty());
        assert!(plan_pasteboard_items(&[], DragSessionLocality::Virtual).is_empty());
    }
}
