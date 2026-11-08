/**
 * The open Operation log dialog's own keys (`Main window/Operation log`). Pure data; the
 * dialog's panes ask `eventMatchesCommand` for each (`lib/operation-log/DETAILS.md` § Keyboard).
 * Registered so Settings and the Help window list them. Fixed, as the Multi-rename sheet's
 * are: the dialog reads the keys itself, so a rebinding would be an illusion. ↑ ↓ Home End
 * PgUp PgDn are the lists' own (`list-cursor.ts`), and ← → in the tree are `tree-cursor.ts`'s.
 * See `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/** Every entry shares these: the dialog answers them itself, never the dispatch spine. */
const operationLogKey = {
  scope: 'Main window/Operation log',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
  fixedKey: true,
} as const

export const operationLogCommands: CommandSource[] = [
  // In the operation list: into the picked operation's items. ← on a top row of the tree is
  // the way back (`tree-cursor.ts`'s `leave`).
  {
    id: 'operationLog.goToItems',
    nameKey: 'commands.operationLogGoToItems.label',
    shortcuts: ['→', 'Enter'],
    ...operationLogKey,
  },
  // In the tree. Space is the checkbox's own key; on a folder, everything inside follows.
  {
    id: 'operationLog.toggleItem',
    nameKey: 'commands.operationLogToggleItem.label',
    shortcuts: ['Space'],
    ...operationLogKey,
  },
  // The panes' `selection.toggleAndDown` key, as a twin: a command lives in one scope.
  {
    id: 'operationLog.toggleItemAndDown',
    nameKey: 'commands.operationLogToggleItemAndDown.label',
    shortcuts: ['Insert'],
    ...operationLogKey,
  },
  // ⌘A and ⌘⇧A are also Edit > Select all / Deselect all's accelerators, which fire before
  // the page sees the key, so the dialog claims those two menu commands too
  // (`#lib/commands/menu-claims.ts`); these entries are the keydown road and the listing.
  {
    id: 'operationLog.selectAll',
    nameKey: 'commands.operationLogSelectAll.label',
    shortcuts: ['⌘A'],
    ...operationLogKey,
  },
  {
    id: 'operationLog.deselectAll',
    nameKey: 'commands.operationLogDeselectAll.label',
    shortcuts: ['⌘⇧A'],
    ...operationLogKey,
  },
  // The action bar's key, from anywhere in the dialog. It asks first, as the button does.
  {
    id: 'operationLog.rollBack',
    nameKey: 'commands.operationLogRollBack.label',
    shortcuts: ['⌘Enter'],
    ...operationLogKey,
  },
]
