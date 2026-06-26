/**
 * Classifies a double-click inside a file pane as "empty list background" or not.
 *
 * Double-clicking the empty area of the file list (below/around the rows) is the
 * gesture for "go up one folder" (Directory Opus-style), gated by the
 * `behavior.doubleClickPaneNavigatesToParent` setting. A double-click on an
 * actual row keeps its own behavior (open the item), so we exclude `.file-entry`;
 * the Full view's sticky column header (`.header-row`) sorts, so we exclude it too.
 *
 * The positive test is the scroll SURFACE (`[data-file-list-surface]`), present on
 * both the Brief (`.brief-list`) and Full (`.full-list`) scroll containers and
 * filling the pane. We can't key off `[role="listbox"]`: in Full view the listbox
 * spans only the rows, so the empty space below a short listing falls outside it
 * (the Full-mode bug), while the surface still covers it. Only the two list views
 * carry the surface, so error / network / search-results / loading panes never
 * trigger parent navigation.
 */
export function isFileListBackgroundClick(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false
  // Must be inside a file-list scroll surface (Brief or Full)...
  if (!target.closest('[data-file-list-surface]')) return false
  // ...but not on an actual row (rows open the item themselves)...
  if (target.closest('.file-entry')) return false
  // ...and not on the Full view's sticky column header (it sorts).
  if (target.closest('.header-row')) return false
  return true
}
