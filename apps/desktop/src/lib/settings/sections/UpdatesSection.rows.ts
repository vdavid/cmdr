/**
 * The searchable non-setting rows of `UpdatesSection.svelte`.
 *
 * Declaration only: it makes the row findable and lets its card know to show,
 * and never decides what renders. The section hand-renders the markup and gates
 * it on `shouldShow(id)` with this id.
 */

import type { SearchableRow } from '../types'

export const updatesRows: SearchableRow[] = [
  {
    // The "Check for updates" button leading the Updates card.
    id: 'row:updates.checkForUpdates',
    section: ['Updates & privacy'],
    labelKey: 'settings.updates.checkForUpdates',
    cardKey: 'settings.updates.card.updates',
    keywordsKey: 'settings.updates.checkForUpdates.keywords',
  },
  {
    // The "Show in Finder" button under the security events switch.
    id: 'row:updates.showSecurityLog',
    section: ['Updates & privacy'],
    labelKey: 'settings.securityLog.showInFinder',
    cardKey: 'settings.updates.card.securityEvents',
    keywordsKey: 'settings.updates.showSecurityLog.keywords',
    terms: ['finder'],
  },
  {
    // The "Delete all Cmdr data" button, alone in its card.
    id: 'row:updates.deleteAllData',
    section: ['Updates & privacy'],
    labelKey: 'settings.dataWipe.button',
    cardKey: 'settings.dataWipe.card',
    keywordsKey: 'settings.updates.deleteAllData.keywords',
  },
]
