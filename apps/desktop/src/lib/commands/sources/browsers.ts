/**
 * Network, share, and volume browsers (main window) command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'

export const browsersCommands: CommandSource[] = [
  // ============================================================================
  // Network browser
  // ============================================================================
  {
    id: 'network.selectHost',
    nameKey: 'commands.networkSelectHost.label',
    scope: 'Main window/Servers',
    showInPalette: false,
    shortcuts: ['Enter'],
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
    shortcuts: [],
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
    shortcuts: ['Backspace', 'Escape', '⌘↑'],
    fixedKey: true,
  },
  {
    id: 'share.selectShare',
    nameKey: 'commands.shareSelectShare.label',
    scope: 'Main window/Places',
    showInPalette: true,
    shortcuts: ['Enter'],
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
    shortcuts: [],
  },
  {
    id: 'servers.togglePin',
    nameKey: 'commands.serversTogglePin.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'servers.disconnect',
    nameKey: 'commands.serversDisconnect.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'servers.forgetSecret',
    nameKey: 'commands.serversForgetSecret.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    // ⌘E lives in the hub's own scope, a sibling of the file list's, so it can
    // never meet a file-list binding.
    id: 'servers.edit',
    nameKey: 'commands.serversEdit.label',
    scope: 'Main window/Servers',
    showInPalette: true,
    shortcuts: ['⌘E'],
  },

  // ============================================================================
  // Volume chooser
  // ============================================================================
  {
    id: 'volume.select',
    nameKey: 'commands.volumeSelect.label',
    scope: 'Main window/Volume chooser',
    showInPalette: false,
    shortcuts: ['Enter'],
    fixedKey: true,
  },
  {
    id: 'volume.close',
    nameKey: 'commands.volumeClose.label',
    scope: 'Main window/Volume chooser',
    showInPalette: false,
    shortcuts: ['Escape'],
    fixedKey: true,
  },
]
