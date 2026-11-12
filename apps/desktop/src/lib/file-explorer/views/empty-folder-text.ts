/**
 * What a pane showing no rows says: "Empty folder", or, when the folder looks empty only
 * because hidden files are off, how many hidden items it holds and how to reveal them.
 * One source for the on-screen empty state (`EmptyFolderState.svelte`, which places the
 * `view.showHidden` key as a `ShortcutChip`) and what the announcers read aloud
 * (`emptyFolderSpokenText`, which speaks it as text), so the two can't drift.
 *
 * `hiddenCount` is the backend's (`CachedListing::hidden_count`): nonzero only while the
 * pane shows no rows and hides hidden files, so a quick filter matching nothing hidden,
 * or hidden files already on, falls back to "Empty folder" with no check here.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { joinSentences } from '#lib/intl/sentence-join.js'
import { formatInteger } from '#lib/intl/number-format.js'
import { shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'

/** The first paragraph for a folder with hidden items: how many it holds. */
export function hiddenItemsText(hiddenCount: number): string {
  return tString('fileExplorer.list.hiddenItems', { count: hiddenCount, countText: formatInteger(hiddenCount) })
}

/** The "View > Show hidden files" menu path, in the app's own menu labels. */
export function showHiddenMenuPath(): { menu: string; item: string } {
  return { menu: tString('menu.bar.view'), item: tString('menu.view.showHiddenFiles') }
}

/**
 * The empty state as one spoken string, for a live region. The hint names the live
 * `view.showHidden` binding, or only the menu path when the person unbound it.
 */
export function emptyFolderSpokenText(hiddenCount: number): string {
  if (hiddenCount <= 0) return tString('fileExplorer.list.empty')
  const shortcut = shortcutAsText('view.showHidden', 'announcement')
  const howTo =
    shortcut === undefined
      ? tString('fileExplorer.list.hiddenItemsHowToMenu', showHiddenMenuPath())
      : tString('fileExplorer.list.hiddenItemsHowToShortcut', { ...showHiddenMenuPath(), key: () => shortcut })
  return joinSentences([hiddenItemsText(hiddenCount), howTo])
}
