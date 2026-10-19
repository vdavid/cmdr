/**
 * The searchable non-setting rows of `AskCmdrSection.svelte`.
 *
 * Declaration only: it makes each row findable, and never decides what renders.
 * The section has no `SectionCard`s (it groups with `h3` titles), so no row here
 * carries a `cardKey`; each is gated on `shouldShow(id)` in the markup.
 */

import type { SearchableRow } from '../types'

export const askCmdrRows: SearchableRow[] = [
  {
    id: 'row:askCmdr.openMemoryFolder',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.memory.open',
    keywordsKey: 'settings.askCmdr.openMemoryFolder.keywords',
  },
  {
    // "Forget everything", under the "What Cmdr remembers" heading.
    id: 'row:askCmdr.forgetMemory',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'askCmdr.forget.confirm',
    keywordsKey: 'settings.askCmdr.forgetMemory.keywords',
  },
]
