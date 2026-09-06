/**
 * The searchable non-setting rows of `DriveIndexingSection.svelte`.
 *
 * Declaration only: it makes each row findable and lets the card know to show,
 * and never decides what renders. The section hand-renders the markup and gates
 * it on `shouldShow(id)` with these ids.
 */

import type { SearchableRow } from '../types'

export const driveIndexingRows: SearchableRow[] = [
  {
    // "Index size" plus the Clear index button. Reuses the label key the row renders.
    id: 'row:indexing.indexSize',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.fileSystemWatching.indexSize',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['clear index', 'index database', 'disk space', 'delete'],
  },
  {
    // "Re-enable notifications for all drives", the button that clears
    // `indexing.silencedDrives`. It sits under the per-drive prompt toggle and
    // shows whenever that row does.
    id: 'row:indexing.reEnableNotifications',
    section: ['Indexing', 'Drive indexing'],
    labelKey: 'settings.indexing.reEnableNotifications.label',
    cardKey: 'settings.indexing.enabled.label',
    keywords: ['re-enable', 'notifications', 'drives', 'silenced', 'ask again'],
  },
]
