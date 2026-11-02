/**
 * The searchable non-setting row of `FinderExtensionCard.svelte`. Declaration only: the
 * card gates itself on `shouldShow(id)` with this id. macOS-only, like the card.
 */

import type { SearchableRow } from '../types'

export const finderExtensionRows: SearchableRow[] = [
  {
    id: 'row:behavior.finderExtension',
    section: ['Behavior', 'Navigation & file ops'],
    labelKey: 'settings.finderExtension.label',
    cardKey: 'settings.finderExtension.card',
    keywordsKey: 'settings.behavior.finderExtension.keywords',
    terms: ['finder', 'context menu', 'right-click', 'extension'],
    macOSOnly: true,
  },
]
