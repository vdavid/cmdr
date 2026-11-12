/**
 * The operation queue window's own keys (`Operation queue window`). Pure data; the window's
 * keydown asks `eventMatchesCommand` for each (`lib/file-operations/queue/queue-keys.ts`).
 * Registered so Settings and the Help window list them. Fixed, because that window has no
 * shortcuts store to read a rebinding from (it holds no `store:` capability). See
 * `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, sameOnAll } from '../platform-shortcuts'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'
import { QUEUE_ROW_KEYS } from '#lib/file-operations/decision-keys.js'

/** Every entry shares these: the window answers them itself, never the dispatch spine. */
const queueWindowKey = {
  scope: 'Operation queue window',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
  fixedKey: true,
} as const

export const queueWindowCommands: CommandSource[] = [
  { id: 'queue.cursorUp', nameKey: 'commands.queueCursorUp.label', shortcuts: sameOnAll('↑'), ...queueWindowKey },
  { id: 'queue.cursorDown', nameKey: 'commands.queueCursorDown.label', shortcuts: sameOnAll('↓'), ...queueWindowKey },
  {
    id: 'queue.cursorFirst',
    nameKey: 'commands.queueCursorFirst.label',
    shortcuts: sameOnAll('Home'),
    ...queueWindowKey,
  },
  { id: 'queue.cursorLast', nameKey: 'commands.queueCursorLast.label', shortcuts: sameOnAll('End'), ...queueWindowKey },
  // ⇧ extends the "Cancel selected" set while the cursor moves, as in a macOS table.
  { id: 'queue.extendUp', nameKey: 'commands.queueExtendUp.label', shortcuts: sameOnAll('⇧↑'), ...queueWindowKey },
  { id: 'queue.extendDown', nameKey: 'commands.queueExtendDown.label', shortcuts: sameOnAll('⇧↓'), ...queueWindowKey },
  {
    id: 'queue.toggleSelected',
    nameKey: 'commands.queueToggleSelected.label',
    shortcuts: sameOnAll('Space'),
    ...queueWindowKey,
  },
  // The menu accelerator fires before the page sees ⌘A, so the native menu routes it to
  // the window too (`queue-select-all`); this entry is the keydown road and the listing.
  { id: 'queue.selectAll', nameKey: 'commands.queueSelectAll.label', shortcuts: cmdOrCtrl('⌘A'), ...queueWindowKey },
  // The letters are the row buttons' chips, read like the decision prompts' letters
  // (`decisionKeyOf`: bare, unrepeated, the physical key on a non-Latin layout).
  {
    id: 'queue.showInMainWindow',
    nameKey: 'commands.queueShowInMainWindow.label',
    shortcuts: sameOnAll('Enter', QUEUE_ROW_KEYS.show),
    ...queueWindowKey,
  },
  {
    id: 'queue.pauseOrResume',
    nameKey: 'commands.queuePauseOrResume.label',
    shortcuts: sameOnAll(QUEUE_ROW_KEYS.pauseOrResume),
    ...queueWindowKey,
  },
  {
    id: 'queue.cancelOrDismiss',
    nameKey: 'commands.queueCancelOrDismiss.label',
    shortcuts: sameOnAll('Backspace', 'Delete'),
    ...queueWindowKey,
  },
  {
    id: 'queue.cancel',
    nameKey: 'commands.queueCancel.label',
    shortcuts: sameOnAll(QUEUE_ROW_KEYS.cancel),
    ...queueWindowKey,
  },
  // Raises the row's rollback confirmation, as the button does; never rolls back by itself.
  {
    id: 'queue.rollback',
    nameKey: 'commands.queueRollback.label',
    shortcuts: sameOnAll(QUEUE_ROW_KEYS.rollback),
    ...queueWindowKey,
  },
  // ⌥↑ / ⌥↓ is the house reorder key (`lib/ui/menu-navigation.ts`) and the ARIA APG
  // rearrangeable-listbox key. ❌ Not ⇧-arrows (they extend the selection).
  {
    id: 'queue.moveEarlier',
    nameKey: 'commands.queueMoveEarlier.label',
    shortcuts: sameOnAll('⌥↑'),
    ...queueWindowKey,
  },
  { id: 'queue.moveLater', nameKey: 'commands.queueMoveLater.label', shortcuts: sameOnAll('⌥↓'), ...queueWindowKey },
  {
    id: 'queue.nextControl',
    nameKey: 'commands.queueNextControl.label',
    shortcuts: sameOnAll('→'),
    ...queueWindowKey,
  },
  {
    id: 'queue.previousControl',
    nameKey: 'commands.queuePreviousControl.label',
    shortcuts: sameOnAll('←'),
    ...queueWindowKey,
  },
  { id: 'queue.close', nameKey: 'commands.queueClose.label', shortcuts: sameOnAll('Escape'), ...queueWindowKey },
]
