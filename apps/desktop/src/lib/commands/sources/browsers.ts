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
    scope: 'Main window/Network',
    showInPalette: false,
    shortcuts: ['Enter'],
    fixedKey: true,
  },
  {
    id: 'network.refresh',
    nameKey: 'commands.networkRefresh.label',
    scope: 'Main window/Network',
    showInPalette: true,
    shortcuts: ['⌘R'],
  },

  // ============================================================================
  // Share browser
  // ============================================================================
  {
    id: 'share.back',
    nameKey: 'commands.shareBack.label',
    scope: 'Main window/Share browser',
    showInPalette: true,
    // `⌘↑` mirrors the file list's `⌘↑` = parent; ShareBrowser handles all three
    // keys (`handleBackToHostKey`). Display-only — `fixedKey` handling is in-component.
    shortcuts: ['Backspace', 'Escape', '⌘↑'],
    fixedKey: true,
  },
  {
    id: 'share.selectShare',
    nameKey: 'commands.shareSelectShare.label',
    scope: 'Main window/Share browser',
    showInPalette: true,
    shortcuts: ['Enter'],
    fixedKey: true,
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
