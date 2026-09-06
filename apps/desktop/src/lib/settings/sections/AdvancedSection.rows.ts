/**
 * The searchable non-setting rows of `AdvancedSection.svelte`.
 *
 * Declaration only: it makes each row findable and lets its card know to show,
 * and never decides what renders. The section hand-renders the markup and gates
 * it on `shouldShow(id)` with these ids.
 */

import type { SearchableRow } from '../types'

export const advancedRows: SearchableRow[] = [
  {
    // The page-header "Reset all to defaults" button. It sits above the cards,
    // so it carries no `cardKey`.
    id: 'row:advanced.resetAll',
    section: ['Advanced'],
    labelKey: 'settings.advanced.resetAll',
    keywords: ['reset', 'defaults', 'restore', 'factory'],
  },
  {
    // The two action buttons trailing the "Logging and diagnostics" card. Their
    // `cardKey` is that card's title key, which the card renders from the
    // `developer.verboseLogging` setting's own `cardKey`.
    id: 'row:advanced.openLogFile',
    section: ['Advanced'],
    labelKey: 'settings.logging.openLogFile',
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    keywords: ['log', 'logs', 'file', 'folder', 'reveal', 'finder'],
  },
  {
    id: 'row:advanced.copyDiagnostics',
    section: ['Advanced'],
    labelKey: 'settings.logging.copyDiagnostics',
    cardKey: 'settings.advanced.card.loggingAndDiagnostics',
    keywords: ['diagnostics', 'copy', 'version', 'support', 'clipboard'],
  },
]
