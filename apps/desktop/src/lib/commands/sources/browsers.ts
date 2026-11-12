/**
 * Network, share, and volume browsers (main window) command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, concatShortcuts, sameOnAll } from '../platform-shortcuts'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

export const browsersCommands: CommandSource[] = [
  // ============================================================================
  // Network browser
  // ============================================================================
  {
    id: 'network.selectHost',
    nameKey: 'commands.networkSelectHost.label',
    scope: 'Main window/Servers',
    showInPalette: false,
    shortcuts: sameOnAll('Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    // No combo of its own: ⌘R is `pane.refresh`, which re-scans hosts when the
    // focused pane shows the network browser. This entry stays for the palette,
    // where "Refresh network hosts" says what it does.
    id: 'network.refresh',
    nameKey: 'commands.networkRefresh.label',
    scope: 'Main window/Servers',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Share browser
  // ============================================================================
  {
    id: 'share.back',
    nameKey: 'commands.shareBack.label',
    scope: 'Main window/Places',
    showInPalette: true,
    // `⌘↑` mirrors the file list's `⌘↑` = parent; PlacesBrowser handles all three
    // keys (`handleBackToHostKey`). Display-only — `fixedKey` handling is in-component.
    shortcuts: concatShortcuts(sameOnAll('Backspace', 'Escape'), cmdOrCtrl('⌘↑')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'share.selectShare',
    nameKey: 'commands.shareSelectShare.label',
    scope: 'Main window/Places',
    showInPalette: true,
    shortcuts: sameOnAll('Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },

  // ============================================================================
  // Servers
  // ============================================================================
  {
    // Takes the focused pane to the servers hub, wherever it is. The one command
    // here that needs no server under the cursor.
    id: 'servers.show',
    nameKey: 'commands.serversShow.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'servers.togglePin',
    nameKey: 'commands.serversTogglePin.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'servers.disconnect',
    nameKey: 'commands.serversDisconnect.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'servers.forgetSecret',
    nameKey: 'commands.serversForgetSecret.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘K, Finder's binding for the same thing. ❗ Window-wide, ❌ not the hub's
    // scope: adding a server is about no server in particular, so it works from
    // a file list as readily as from the hub.
    id: 'servers.connect',
    nameKey: 'commands.serversConnect.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘K'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘E lives in the hub's own scope, a sibling of the file list's, so it can
    // never meet a file-list binding.
    id: 'servers.edit',
    nameKey: 'commands.serversEdit.label',
    scope: 'Main window/Servers',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘E'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Volume chooser
  // ============================================================================
  {
    id: 'volume.select',
    nameKey: 'commands.volumeSelect.label',
    scope: 'Main window/Volume chooser',
    showInPalette: false,
    shortcuts: sameOnAll('Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'volume.close',
    nameKey: 'commands.volumeClose.label',
    scope: 'Main window/Volume chooser',
    showInPalette: false,
    shortcuts: sameOnAll('Escape'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },

  // ============================================================================
  // Favorites menu
  // ============================================================================
  // The two digit rows the open menu answers itself, here so Settings > Keyboard shortcuts
  // and the Help window say the menu HAS them. `fixedKey`, like the chooser's Enter: the
  // house `Menu`'s accelerator column decides which digits exist from the caller's data, so
  // a rebind would be an illusion. The command that OPENS the menu is `favorites.open`
  // (`main-window.ts`), which is window-wide.
  {
    id: 'favorites.openByNumber',
    nameKey: 'commands.favoritesOpenByNumber.label',
    scope: 'Main window/Favorites menu',
    showInPalette: false,
    shortcuts: sameOnAll('1', '2', '3', '4', '5', '6', '7', '8', '9'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'favorites.addFromMenu',
    // The one `nameKey` outside `commands.*`, and deliberately so: this row doesn't name a
    // command, it quotes the menu's `0` row back to the reader. The row owns the wording
    // (`favorites-menu.svelte.ts` draws it from the same key), so a second copy under
    // `commands.*` was one English edit away from the two disagreeing, and five locales
    // already rendered them byte-identical.
    nameKey: 'fileExplorer.navigation.favoritesAddCurrent',
    scope: 'Main window/Favorites menu',
    showInPalette: false,
    shortcuts: sameOnAll('0'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
]
