/**
 * Per-volume capability flags for the search-results virtual volume.
 *
 * The search-results pane (`volumeId === 'search-results'`, path
 * `search-results://<snapshot-id>`) is a read-only view of a snapshot, not a
 * real directory. Some pane actions don't make sense there:
 *
 * - Paste into the pane (`⌘V`): there's no destination folder, only a
 *   synthetic snapshot.
 * - Make a new folder / file (`F7`, etc.): same.
 * - Rename (`F2`, click-to-rename): a snapshot row's underlying file CAN be
 *   renamed in principle, but doing it inside the snapshot view is confusing
 *   because the rename happens on disk while the snapshot stays as-is. The
 *   user can navigate to the real folder and rename there.
 *
 * The flags returned here drive disablement at the source (F-key bar, context
 * menu, dialog routing). Per the plan's principle from `docs/design-principles.md`,
 * "disabled is better than 'you did the wrong thing' toasts": menus and
 * F-keys read these flags and render visibly disabled. Keyboard shortcuts
 * (which bypass menus) fall back to a friendly toast so the action isn't
 * silently swallowed.
 *
 * Source-side flags (copy / move source, drag out, delete) stay `true`:
 * the underlying paths are real, and acting on them is the snapshot view's
 * primary point. Delete in particular runs through the existing confirmation
 * dialog; on success, the deleted entry is removed from every snapshot that
 * contains it (see `snapshot-store.svelte.ts::removeEntryFromAllSnapshots`).
 */

export interface SearchResultsCapabilities {
  /** Can files be pasted INTO this pane? Always false for search-results. */
  canPasteInto: false
  /** Can a new folder be created here? Always false. */
  canMkdir: false
  /** Can a new file be created here? Always false. */
  canMkfile: false
  /** Can the cursor row be renamed in-place here? Always false. */
  canRename: false
  /** Can this pane act as the SOURCE for copy / move / delete? Always true. */
  isSourceOK: true
}

/** Returns the capability flag set for the search-results virtual volume. */
export function searchResultsVolumeCapabilities(): SearchResultsCapabilities {
  return {
    canPasteInto: false,
    canMkdir: false,
    canMkfile: false,
    canRename: false,
    isSourceOK: true,
  }
}

/**
 * Returns the user-facing toast text shown when a keyboard shortcut tries to
 * do something the search-results pane doesn't support (paste, mkdir, rename).
 * Kept here so the wording stays consistent between the dispatcher and tests.
 */
export const SEARCH_RESULTS_NOT_A_FOLDER_TOAST = "Search results aren't a folder. Paste into a real folder instead."
