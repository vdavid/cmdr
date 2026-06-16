/**
 * Resolves a "current folder" that the Search dialog's `Search in → Use current folder`
 * action can act on. Round-2 D12: the focused pane may be a `search-results://<id>` snapshot,
 * whose path isn't a real folder the index can search inside. In that case, walk the pane's
 * navigation history backward for the most recent non-snapshot entry; if none, surface a
 * disabled state with a tooltip the dialog renders.
 *
 * Kept pure so the dialog plumbing stays testable: takes the focused-pane path and a list of
 * history paths (newest first or arbitrary order — we filter, no ordering assumption beyond
 * "most recent at the end" matches `NavigationHistory.stack`).
 */
import { tString } from '$lib/intl/messages.svelte'

const SEARCH_RESULTS_PREFIX = 'search-results://'

export interface SearchableFolderInput {
  /** Current path of the focused pane (may be `search-results://<id>`). */
  currentPath: string
  /**
   * Stack of recent paths from the focused pane's navigation history, ordered oldest first
   * (matches `NavigationHistory.stack`). The current path is typically the last entry, but
   * we don't depend on that — we just skip every `search-results://` entry when scanning
   * backward.
   */
  history: string[]
}

export interface SearchableFolderResult {
  /** Path the dialog should pass to "Use current folder", or `null` when none is available. */
  path: string | null
  /**
   * `true` when the dialog should render the button disabled with the fallback tooltip.
   * This is exactly the case where `currentPath` is a search-results URL and we couldn't
   * find any real-folder history entry to fall back to.
   */
  disabled: boolean
  /** User-facing tooltip when `disabled` is true; empty string otherwise. */
  disabledReason: string
}

/**
 * Picks the best "current folder" path for the dialog. Three cases:
 *   1. The focused pane is on a real folder: use it as-is. Enabled.
 *   2. The focused pane is on `search-results://...` AND its history contains a real-folder
 *      entry: use the most recent such entry. Enabled.
 *   3. The focused pane is on `search-results://...` AND there's no real-folder entry:
 *      disabled with the canonical tooltip.
 */
export function resolveSearchableFolder({ currentPath, history }: SearchableFolderInput): SearchableFolderResult {
  if (!currentPath.startsWith(SEARCH_RESULTS_PREFIX)) {
    return { path: currentPath, disabled: false, disabledReason: '' }
  }
  // Walk backward through history for the newest non-snapshot path.
  for (let i = history.length - 1; i >= 0; i--) {
    const entry = history[i]
    if (!entry.startsWith(SEARCH_RESULTS_PREFIX)) {
      return { path: entry, disabled: false, disabledReason: '' }
    }
  }
  return { path: null, disabled: true, disabledReason: tString('search.searchableFolder.disabledTooltip') }
}
