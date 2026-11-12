/**
 * The Search and Select files dialogs' own keys (`Main window/Search dialog`): both are one
 * shared `QueryDialog`, whose keydown asks `eventMatchesCommand` for each
 * (`lib/query-ui/query-shortcuts.ts`). Fixed, and registered so Settings and the Help window
 * list them. The dialog's other keys (the mode chips' ⌥A / ⌥F / ⌥R, the filter popovers, ⌥⏎)
 * stay out of the registry: they're a per-platform table in `lib/query-ui/query-keys.ts`.
 * See `../command-registry.ts` for how the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl } from '../platform-shortcuts'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/** Every entry shares these: the dialog answers them itself, never the dispatch spine. */
const dialogKey = {
  scope: 'Main window/Search dialog',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
  fixedKey: true,
} as const

export const searchDialogCommands: CommandSource[] = [
  { id: 'search.newQuery', nameKey: 'commands.searchNewQuery.label', shortcuts: cmdOrCtrl('⌘N'), ...dialogKey },
  // Each OS's browser history key: Chrome's ⌘Y on macOS, where ⌘H is Hide, and Ctrl+H elsewhere.
  {
    id: 'search.toggleRecent',
    nameKey: 'commands.searchToggleRecent.label',
    shortcuts: { macos: ['⌘Y'], windows: ['Ctrl+H'], linux: ['Ctrl+H'] },
    ...dialogKey,
  },
  // One command for the three digits, like `favorites.openByNumber`: the Nth key picks the Nth
  // visible mode chip (`modeForShortcutNumber`), so a rebind would be an illusion.
  {
    id: 'search.modeByNumber',
    nameKey: 'commands.searchModeByNumber.label',
    shortcuts: cmdOrCtrl('⌘1', '⌘2', '⌘3'),
    ...dialogKey,
  },
]
