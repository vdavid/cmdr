/**
 * The open Multi-rename sheet's own keys (`Main window/Multi-rename`). Pure data; the sheet's
 * keydown asks `eventMatchesCommand` for each. Registered so Settings and the Help window list
 * them. See `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, sameOnAll } from '../platform-shortcuts'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const multiRenameCommands: CommandSource[] = [
  {
    // Total Commander's F2 in its Multi-Rename Tool: load/save settings.
    id: 'multiRename.openPresets',
    nameKey: 'commands.multiRenameOpenPresets.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: sameOnAll('F2'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.savePreset',
    nameKey: 'commands.multiRenameSavePreset.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: cmdOrCtrl('⌘S'),
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
    shortcuts: { macos: ['⌘⌥U'], windows: ['Alt+U'], linux: ['Alt+U'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.removeDiacritics',
    nameKey: 'commands.multiRenameRemoveDiacritics.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥N'], windows: ['Alt+N'], linux: ['Alt+N'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.greekToLatin',
    nameKey: 'commands.multiRenameGreekToLatin.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥G'], windows: ['Alt+G'], linux: ['Alt+G'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // P for precomposed, the form it renames to (N is Remove diacritics').
    id: 'multiRename.normalizeUnicode',
    nameKey: 'commands.multiRenameNormalizeUnicode.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥P'], windows: ['Alt+P'], linux: ['Alt+P'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.matchCase',
    nameKey: 'commands.multiRenameMatchCase.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥I'], windows: ['Alt+I'], linux: ['Alt+I'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.firstMatchOnly',
    nameKey: 'commands.multiRenameFirstMatchOnly.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥F'], windows: ['Alt+F'], linux: ['Alt+F'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.includeExtension',
    nameKey: 'commands.multiRenameIncludeExtension.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥E'], windows: ['Alt+E'], linux: ['Alt+E'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.regex',
    nameKey: 'commands.multiRenameRegex.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥R'], windows: ['Alt+R'], linux: ['Alt+R'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.replaceWholeName',
    nameKey: 'commands.multiRenameReplaceWholeName.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥W'], windows: ['Alt+W'], linux: ['Alt+W'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // The `media ▾` button's field picker. X as in EXIF; ⌘⌥M is macOS's "minimize all".
    id: 'multiRename.insertField',
    nameKey: 'commands.multiRenameInsertField.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥X'], windows: ['Alt+X'], linux: ['Alt+X'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // ⌘Z / ⇧⌘Z stay the fields' text undo and redo; ⌥ joins the sheet's option keys.
    id: 'multiRename.undoRename',
    nameKey: 'commands.multiRenameUndoRename.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: { macos: ['⌘⌥Z'], windows: ['Alt+Z'], linux: ['Alt+Z'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // TC's Results button (⌥R there, a menu mnemonic); ⌥Enter, beside Enter's Rename, as the PR had it.
    id: 'multiRename.results',
    nameKey: 'commands.multiRenameResults.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: sameOnAll('⌥Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // In a text field: TC's field history, as a combobox opens its list. With the caret inside a
    // mask's token, MaskInput claims ↓ first (into that token's editor).
    id: 'multiRename.fieldHistory',
    nameKey: 'commands.multiRenameFieldHistory.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: sameOnAll('↓'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // In the preview list: the row under the cursor moves in the rename order (what `[C]`
    // counts), the cursor with it. ⇧↑ / ⇧↓ are David's pick, ⌥↑ / ⌥↓ the house reorder key
    // (`lib/ui/menu-navigation.ts`, the queue window). In a text field both stay the field's own.
    id: 'multiRename.moveRowUp',
    nameKey: 'commands.multiRenameMoveRowUp.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: sameOnAll('⇧↑', '⌥↑'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'multiRename.moveRowDown',
    nameKey: 'commands.multiRenameMoveRowDown.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: sameOnAll('⇧↓', '⌥↓'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // The primary button's key, from anywhere in the sheet. Plain Enter in a text field renames too
    // (TC's Start!), outside the registry: it's the field's own key, and buttons keep theirs.
    id: 'multiRename.rename',
    nameKey: 'commands.multiRenameRename.label',
    scope: 'Main window/Multi-rename',
    showInPalette: false,
    shortcuts: cmdOrCtrl('⌘Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
]
