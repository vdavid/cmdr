//! Pure, locality-aware pasteboard composition for native drags.
//!
//! Pasteboard layout is policy, not incidental code: it's where the wry
//! constraint, the Finder interplay, and the terminal-affordance removal become
//! visible and testable. This module owns that policy as a pure function so the
//! AppKit-touching code in the parent module stays a thin executor of the plan.
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
//! ## Local sessions: byte-identical to the legacy layout
//!
//! - `public.file-url` on every item (the URL's `absoluteString`).
//! - `public.utf8-plain-text` on every item: the first item carries all paths
//!   shell-escaped and space-joined (the "drop into terminal" gesture); later
//!   items carry just their own escaped path so item-iterating consumers don't
//!   see duplicates.
//! - `NSFilenamesPboardType` (legacy `NSArray<NSString>` of all paths) on the
//!   first item only. Required for stock wry's `collect_paths`.
//!
//! ## Virtual sessions: nothing external apps can materialize as garbage
//!
//! NO file-url, NO text, NO filenames — across EVERY item. A virtual path's
//! `file://` URL is bogus (the file doesn't exist locally), the text was a
//! meaningless volume-relative string outside Cmdr, and an auto-derived (or
//! explicit) filenames entry is the textClipping junk Finder materializes. The
//! M0 spike verified that promise-only items still fire wry's drop event with an
//! empty path vector (no panic), so the in-app self-drag path keeps working via
//! recorded identity. In M1 a virtual item's pasteboard payload is simply EMPTY;
//! the `NSFilePromiseProvider` writer arrives in M2.

/// Whether a drag session's source volume is locally materialized (local FS or
/// an OS-mounted share, where a `file://` URL is real) or protocol-only /
/// virtual (MTP, direct SMB, search-results), where it isn't.
///
/// Decided once per session at the drag-start boundary, never per item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragSessionLocality {
    /// Paths are real local filesystem paths. Keep the full legacy layout.
    Local,
    /// Paths are volume-relative virtual paths with no local backing. Publish
    /// nothing external apps can materialize (M2 adds the promise providers).
    Virtual,
}

/// The pasteboard representations a single dragging item should advertise.
///
/// Each `Option` is `None` when that representation must be omitted. A virtual
/// session yields every field `None` (an empty item); a local session fills the
/// fields per the legacy layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteboardItemPlan {
    /// `public.file-url`: the item's `file://` URL absolute string.
    pub file_url: Option<String>,
    /// `public.utf8-plain-text`: shell-escaped path text for terminal drops.
    pub text: Option<String>,
    /// `NSFilenamesPboardType`: the full legacy path array (first item only).
    pub filenames: Option<Vec<String>>,
}

impl PasteboardItemPlan {
    /// An item that advertises nothing — the virtual-session payload in M1.
    fn empty() -> Self {
        Self {
            file_url: None,
            text: None,
            filenames: None,
        }
    }

    /// Whether this item advertises no representations at all. Used by tests to
    /// assert the virtual-session payload strips everything; the executor in the
    /// parent module just reads the `Option` fields directly.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.file_url.is_none() && self.text.is_none() && self.filenames.is_none()
    }
}

/// Builds the per-item pasteboard plan for a whole drag session.
///
/// `paths` are the paths as the source volume knows them (absolute local for a
/// local session; volume-relative for a virtual one). The returned vec has one
/// entry per input path, in order.
///
/// Pure: no AppKit, no I/O. The parent module turns each [`PasteboardItemPlan`]
/// into `NSPasteboardItem` representations (and, in M2, attaches a promise
/// provider for virtual items).
pub fn plan_pasteboard_items(paths: &[String], locality: DragSessionLocality) -> Vec<PasteboardItemPlan> {
    match locality {
        DragSessionLocality::Virtual => {
            // No file-url, no text, no filenames — across EVERY item.
            paths.iter().map(|_| PasteboardItemPlan::empty()).collect()
        }
        DragSessionLocality::Local => {
            // Byte-identical to the legacy layout.
            let joined_text = paths.iter().map(|p| shell_escape(p)).collect::<Vec<_>>().join(" ");

            paths
                .iter()
                .enumerate()
                .map(|(i, path)| {
                    let text = if i == 0 {
                        joined_text.clone()
                    } else {
                        shell_escape(path)
                    };
                    PasteboardItemPlan {
                        file_url: Some(path.clone()),
                        text: Some(text),
                        // The full filenames array rides only on the first item.
                        filenames: if i == 0 { Some(paths.to_vec()) } else { None },
                    }
                })
                .collect()
        }
    }
}

/// Single-quotes a path for paste into a POSIX shell. Returns the input
/// unchanged if it only contains characters that are universally safe outside
/// quoting.
pub fn shell_escape(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ',' | ':' | '@' | '%' | '=')
        });
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // --- Local session: byte-identical to the legacy layout ---

    #[test]
    fn local_single_item_carries_url_text_and_filenames() {
        let plan = plan_pasteboard_items(&p(&["/Users/me/file.jpg"]), DragSessionLocality::Local);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].file_url.as_deref(), Some("/Users/me/file.jpg"));
        assert_eq!(plan[0].text.as_deref(), Some("/Users/me/file.jpg"));
        assert_eq!(
            plan[0].filenames.as_deref(),
            Some(["/Users/me/file.jpg".to_string()].as_slice())
        );
    }

    #[test]
    fn local_first_item_text_is_joined_shell_escaped() {
        let paths = p(&["/a/one.jpg", "/has space/two.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Local);
        // First item's text joins all paths, shell-escaped, space-separated.
        assert_eq!(plan[0].text.as_deref(), Some("/a/one.jpg '/has space/two.jpg'"));
    }

    #[test]
    fn local_later_items_carry_only_own_escaped_path_no_filenames() {
        let paths = p(&["/a/one.jpg", "/has space/two.jpg", "/a/three.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Local);
        // Every item carries its own file-url.
        assert_eq!(plan[1].file_url.as_deref(), Some("/has space/two.jpg"));
        assert_eq!(plan[2].file_url.as_deref(), Some("/a/three.jpg"));
        // Later items carry just their own escaped path as text.
        assert_eq!(plan[1].text.as_deref(), Some("'/has space/two.jpg'"));
        assert_eq!(plan[2].text.as_deref(), Some("/a/three.jpg"));
        // Filenames ride only on the first item.
        assert!(plan[1].filenames.is_none());
        assert!(plan[2].filenames.is_none());
    }

    #[test]
    fn local_filenames_on_first_item_is_the_full_path_list() {
        let paths = p(&["/a/one.jpg", "/a/two.jpg", "/a/three.jpg"]);
        let plan = plan_pasteboard_items(&paths, DragSessionLocality::Local);
        assert_eq!(plan[0].filenames.as_deref(), Some(paths.as_slice()));
    }

    // --- Virtual session: empty across every item (the textClipping fix) ---

    #[test]
    fn virtual_single_item_is_empty() {
        let plan = plan_pasteboard_items(&p(&["/photos/sunset.jpg"]), DragSessionLocality::Virtual);
        assert_eq!(plan.len(), 1);
        assert!(plan[0].is_empty());
        assert!(plan[0].file_url.is_none());
        assert!(plan[0].text.is_none());
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

    // --- shell_escape (moved here from the parent module) ---

    #[test]
    fn shell_escape_safe_passthrough() {
        assert_eq!(shell_escape("/Users/me/file.jpg"), "/Users/me/file.jpg");
        assert_eq!(shell_escape("plain"), "plain");
    }

    #[test]
    fn shell_escape_quotes_spaces_and_unicode() {
        assert_eq!(shell_escape("/has space/x.jpg"), "'/has space/x.jpg'");
        assert_eq!(shell_escape("Anna fotók"), "'Anna fotók'");
    }

    #[test]
    fn shell_escape_handles_inner_single_quote() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn shell_escape_empty_is_quoted() {
        assert_eq!(shell_escape(""), "''");
    }
}
