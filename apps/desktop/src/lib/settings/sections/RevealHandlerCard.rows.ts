/**
 * The searchable non-setting row of `RevealHandlerCard.svelte`, the "Show in Finder"
 * card in Behavior › Navigation & file ops.
 *
 * Declaration only: it makes the row findable and never decides what renders. The
 * card gates itself on `shouldShow(id)` with this id. macOS-only, like the card: off
 * macOS there's no row for a hit to land on.
 */

import type { SearchableRow } from '../types'

export const revealHandlerRows: SearchableRow[] = [
  {
    id: 'row:behavior.revealHandler',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.revealHandler.label',
    cardKey: 'settings.navigationAndFileOps.card.showInFinder',
    // How people look for it: the Finder command by name (plus the "Find in Finder"
    // misremembering), and the apps whose downloads list runs that command.
    keywords: [
      'finder',
      'show in finder',
      'find in finder',
      'reveal',
      'reveal in finder',
      'default file manager',
      'google',
      'chrome',
      'safari',
      'firefox',
      'slack',
      'browser',
      'downloads',
    ],
    macOSOnly: true,
  },
]
