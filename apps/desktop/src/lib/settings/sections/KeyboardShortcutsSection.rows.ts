/**
 * The searchable non-setting rows of `KeyboardShortcutsSection.svelte`.
 *
 * That page is command-driven, not registry-driven: its search normally comes
 * from `searchAllCommands`, which knows nothing about the page's own footer
 * button. `SettingsContent` therefore ORs the command match with the
 * section-scoped match set for this path, so a row hit opens the page (with its
 * command list filtered to nothing and the footer button in place).
 */

import type { SearchableRow } from '../types'

export const keyboardShortcutsRows: SearchableRow[] = [
  {
    // "Reset all shortcuts to defaults", in the page footer (always rendered).
    id: 'row:shortcuts.resetAll',
    section: ['Keyboard shortcuts'],
    labelKey: 'shortcuts.section.resetAll',
    keywords: ['reset', 'shortcuts', 'defaults', 'restore', 'keyboard'],
  },
]
