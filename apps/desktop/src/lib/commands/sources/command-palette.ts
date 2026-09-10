/**
 * Command palette modal command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const commandPaletteCommands: CommandSource[] = [
  // ============================================================================
  // Command palette modal
  // ============================================================================
  {
    id: 'palette.up',
    nameKey: 'commands.paletteUp.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['↑'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'palette.down',
    nameKey: 'commands.paletteDown.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['↓'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'palette.execute',
    nameKey: 'commands.paletteExecute.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['Enter'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'palette.close',
    nameKey: 'commands.paletteClose.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['Escape'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
]
