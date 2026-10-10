/**
 * The open Multi-rename sheet's own keys (`Main window/Multi-rename`). Pure data; the sheet's
 * keydown asks `eventMatchesCommand` for each. Registered so Settings and the Help window list
 * them. See `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const multiRenameCommands: CommandSource[] = [
  {
    // Total Commander's F2 in its Multi-Rename Tool: load/save settings.
    id: 'multiRename.openPresets',
    nameKey: 'commands.multiRenameOpenPresets.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['F2'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.savePreset',
    nameKey: 'commands.multiRenameSavePreset.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘S'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  // The sheet's option keys: ⌘⌥ plus the option's letter, matched on the physical key, so
  // they work from a text field too. ⌘⌥ with C, A, H, L, O, Q, T, or V is an app command's.
  {
    id: 'multiRename.letterCase',
    nameKey: 'commands.multiRenameLetterCase.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥U'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.removeDiacritics',
    nameKey: 'commands.multiRenameRemoveDiacritics.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥N'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.greekToLatin',
    nameKey: 'commands.multiRenameGreekToLatin.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥G'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // P for precomposed, the form it renames to (N is Remove diacritics').
    id: 'multiRename.normalizeUnicode',
    nameKey: 'commands.multiRenameNormalizeUnicode.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥P'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.matchCase',
    nameKey: 'commands.multiRenameMatchCase.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥I'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.firstMatchOnly',
    nameKey: 'commands.multiRenameFirstMatchOnly.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥F'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.includeExtension',
    nameKey: 'commands.multiRenameIncludeExtension.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥E'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.regex',
    nameKey: 'commands.multiRenameRegex.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥R'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.replaceWholeName',
    nameKey: 'commands.multiRenameReplaceWholeName.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥W'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // ⌘Z / ⇧⌘Z stay the fields' text undo and redo; ⌥ joins the sheet's option keys.
    id: 'multiRename.undoRename',
    nameKey: 'commands.multiRenameUndoRename.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌘⌥Z'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // TC's Results button (⌥R there, a menu mnemonic); ⌥Enter, beside Enter's Rename, as the PR had it.
    id: 'multiRename.results',
    nameKey: 'commands.multiRenameResults.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['⌥Enter'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // In a text field: TC's field history, as a combobox opens its list. With the caret in a mask's
    // `[C…]` token, MaskInput claims ↓ first (into the counter's editor).
    id: 'multiRename.fieldHistory',
    nameKey: 'commands.multiRenameFieldHistory.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: ['↓'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
]
