/**
 * What a pane showing no rows says, as paragraphs: "Empty folder", or, when the folder
 * looks empty only because hidden files are off, how many hidden items it holds and how
 * to reveal them. One source for the on-screen empty state (`EmptyFolderState.svelte`)
 * and what the pane's status announcer reads aloud, so the two can't drift.
 *
 * `hiddenCount` is the backend's (`CachedListing::hidden_count`): nonzero only while the
 * pane shows no rows and hides hidden files, so a quick filter matching nothing hidden,
 * or hidden files already on, falls back to "Empty folder" with no check here.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'

/**
 * The empty state's paragraphs. `shortcut` is the live display binding of
 * `view.showHidden`, or `undefined` when the person unbound it: the hint then names only
 * the menu path.
 */
export function emptyFolderParagraphs(hiddenCount: number, shortcut: string | undefined): string[] {
  if (hiddenCount <= 0) return [tString('fileExplorer.list.empty')]
  const menu = tString('menu.bar.view')
  const item = tString('menu.view.showHiddenFiles')
  return [
    tString('fileExplorer.list.hiddenItems', { count: hiddenCount, countText: formatInteger(hiddenCount) }),
    shortcut === undefined
      ? tString('fileExplorer.list.hiddenItemsHowToMenu', { menu, item })
      : tString('fileExplorer.list.hiddenItemsHowToShortcut', { shortcut, menu, item }),
  ]
}
