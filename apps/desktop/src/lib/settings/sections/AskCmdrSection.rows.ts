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
    // The consent button. Its label is one of three ("Turn on Ask Cmdr" / "Turn
    // off Ask Cmdr" / "Turn back on"); the row names the on-state key and carries
    // the other wordings as keywords, so a search finds the row whichever state
    // the button is in.
    id: 'row:askCmdr.consent',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.turnOn',
    keywords: ['turn off', 'turn back on', 'enable', 'disable', 'consent', 'opt in', 'opt out'],
  },
  {
    id: 'row:askCmdr.openMemoryFolder',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'settings.askCmdr.memory.open',
    keywords: ['memory', 'folder', 'notes', 'remember', 'open'],
  },
  {
    // "Forget everything", under the "What Ask Cmdr remembers" heading.
    id: 'row:askCmdr.forgetMemory',
    section: ['AI', 'Ask Cmdr'],
    labelKey: 'askCmdr.forget.confirm',
    keywords: ['forget', 'memory', 'erase', 'delete', 'remembers', 'privacy'],
  },
]
