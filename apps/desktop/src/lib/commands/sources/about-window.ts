/**
 * About window command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const aboutWindowCommands: CommandSource[] = [
  // ============================================================================
  // About window
  // ============================================================================
  {
    id: 'about.openWebsite',
    nameKey: 'commands.aboutOpenWebsite.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'about.openUpgrade',
    nameKey: 'commands.aboutOpenUpgrade.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'about.close',
    nameKey: 'commands.aboutClose.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: ['Escape'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
]
