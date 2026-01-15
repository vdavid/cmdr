/**
 * Complete registry of all commands in the application.
 *
 * This is the single source of truth for:
 * - Command palette entries
 * - Keyboard shortcut documentation
 * - Future MCP server commands
 * - Settings pane shortcut configuration
 */

import type { Command } from './types'

export const commands: Command[] = [
    // ============================================================================
    // App scope (work everywhere, regardless of window/modal state)
    // ============================================================================
    { id: 'app.quit', name: 'Quit Cmdr', scope: 'App', showInPalette: true, shortcuts: ['⌘Q'] },
    { id: 'app.hide', name: 'Hide Cmdr', scope: 'App', showInPalette: true, shortcuts: ['⌘H'] },
    { id: 'app.hideOthers', name: 'Hide others', scope: 'App', showInPalette: true, shortcuts: ['⌥⌘H'] },
    { id: 'app.showAll', name: 'Show all', scope: 'App', showInPalette: true, shortcuts: [] },
    { id: 'app.about', name: 'About Cmdr', scope: 'App', showInPalette: true, shortcuts: [] },
    {
        id: 'app.commandPalette',
        name: 'Open command palette',
        scope: 'App',
        showInPalette: false, // Don't show the palette in itself
        shortcuts: ['⌘⇧P'],
    },

    // ============================================================================
    // Main window - View commands
    // ============================================================================
    {
        id: 'view.showHidden',
        name: 'Toggle hidden files',
        scope: 'Main window',
        showInPalette: true,
        shortcuts: ['⌘⇧.'],
    },
    {
        id: 'view.briefMode',
        name: 'Switch to Brief view',
        scope: 'Main window',
        showInPalette: true,
        shortcuts: ['⌘2'],
    },
    {
        id: 'view.fullMode',
        name: 'Switch to Full view',
        scope: 'Main window',
        showInPalette: true,
        shortcuts: ['⌘1'],
    },

    // ============================================================================
    // Main window - Sort commands (also accessible via menu)
    // ============================================================================
    { id: 'sort.byName', name: 'Sort by name', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.byExtension', name: 'Sort by extension', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.bySize', name: 'Sort by size', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.byModified', name: 'Sort by date modified', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.byCreated', name: 'Sort by date created', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.ascending', name: 'Sort ascending', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.descending', name: 'Sort descending', scope: 'Main window', showInPalette: true, shortcuts: [] },
    { id: 'sort.toggleOrder', name: 'Toggle sort order', scope: 'Main window', showInPalette: true, shortcuts: [] },

    // ============================================================================
    // Main window - Pane commands
    // ============================================================================
    { id: 'pane.switch', name: 'Switch pane', scope: 'Main window', showInPalette: true, shortcuts: ['Tab'] },
    {
        id: 'pane.leftVolumeChooser',
        name: 'Open left volume chooser',
        scope: 'Main window',
        showInPalette: true,
        shortcuts: ['F1'],
    },
    {
        id: 'pane.rightVolumeChooser',
        name: 'Open right volume chooser',
        scope: 'Main window',
        showInPalette: true,
        shortcuts: ['F2'],
    },

    // ============================================================================
    // File list - Navigation commands
    // ============================================================================
    {
        id: 'nav.up',
        name: 'Select previous file',
        scope: 'Main window/File list',
        showInPalette: false, // Too basic for palette
        shortcuts: ['↑'],
    },
    {
        id: 'nav.down',
        name: 'Select next file',
        scope: 'Main window/File list',
        showInPalette: false,
        shortcuts: ['↓'],
    },
    {
        id: 'nav.open',
        name: 'Open file or folder',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['Enter'],
    },
    {
        id: 'nav.parent',
        name: 'Go to parent folder',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['Backspace', '⌘↑'],
    },
    {
        id: 'nav.home',
        name: 'Go to first file',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['⌥↑', 'Home'],
    },
    {
        id: 'nav.end',
        name: 'Go to last file',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['⌥↓', 'End'],
    },
    {
        id: 'nav.pageUp',
        name: 'Page up',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['PageUp'],
    },
    {
        id: 'nav.pageDown',
        name: 'Page down',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['PageDown'],
    },
    { id: 'nav.back', name: 'Go back', scope: 'Main window/File list', showInPalette: true, shortcuts: ['⌘['] },
    { id: 'nav.forward', name: 'Go forward', scope: 'Main window/File list', showInPalette: true, shortcuts: ['⌘]'] },

    // ============================================================================
    // Brief mode specific
    // ============================================================================
    {
        id: 'nav.left',
        name: 'Move to left column',
        scope: 'Main window/Brief mode',
        showInPalette: false,
        shortcuts: ['←'],
    },
    {
        id: 'nav.right',
        name: 'Move to right column',
        scope: 'Main window/Brief mode',
        showInPalette: false,
        shortcuts: ['→'],
    },

    // ============================================================================
    // Full mode specific (left/right jump to first/last in full mode)
    // ============================================================================
    {
        id: 'nav.firstInFull',
        name: 'Jump to first file',
        scope: 'Main window/Full mode',
        showInPalette: false,
        shortcuts: ['←'],
    },
    {
        id: 'nav.lastInFull',
        name: 'Jump to last file',
        scope: 'Main window/Full mode',
        showInPalette: false,
        shortcuts: ['→'],
    },

    // ============================================================================
    // File list - File action commands
    // ============================================================================
    {
        id: 'file.showInFinder',
        name: 'Show in Finder',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['⌥⌘O'],
    },
    {
        id: 'file.copyPath',
        name: 'Copy path to clipboard',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['⌃⌘C'],
    },
    {
        id: 'file.copyFilename',
        name: 'Copy filename',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: [],
    },
    {
        id: 'file.getInfo',
        name: 'Get info',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: ['⌘I'],
    },
    {
        id: 'file.quickLook',
        name: 'Quick look',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: [],
    },
    {
        id: 'file.contextMenu',
        name: 'Open context menu',
        scope: 'Main window/File list',
        showInPalette: true,
        shortcuts: [],
        description: 'Opens the context menu for the file under the cursor',
    },

    // ============================================================================
    // Network browser
    // ============================================================================
    {
        id: 'network.selectHost',
        name: 'Select network host',
        scope: 'Main window/Network',
        showInPalette: false,
        shortcuts: ['Enter'],
    },
    {
        id: 'network.refresh',
        name: 'Refresh network hosts',
        scope: 'Main window/Network',
        showInPalette: true,
        shortcuts: [],
    },

    // ============================================================================
    // Share browser
    // ============================================================================
    {
        id: 'share.back',
        name: 'Back to host list',
        scope: 'Main window/Share browser',
        showInPalette: true,
        shortcuts: ['Backspace', 'Escape'],
    },
    {
        id: 'share.selectShare',
        name: 'Connect to share',
        scope: 'Main window/Share browser',
        showInPalette: true,
        shortcuts: ['Enter'],
    },

    // ============================================================================
    // Volume chooser
    // ============================================================================
    {
        id: 'volume.select',
        name: 'Select volume',
        scope: 'Main window/Volume chooser',
        showInPalette: false,
        shortcuts: ['Enter'],
    },
    {
        id: 'volume.close',
        name: 'Close volume chooser',
        scope: 'Main window/Volume chooser',
        showInPalette: false,
        shortcuts: ['Escape'],
    },

    // ============================================================================
    // About window
    // ============================================================================
    {
        id: 'about.openWebsite',
        name: 'Open website',
        scope: 'About window',
        showInPalette: true,
        shortcuts: [],
    },
    {
        id: 'about.openUpgrade',
        name: 'Open upgrade page',
        scope: 'About window',
        showInPalette: true,
        shortcuts: [],
    },
    {
        id: 'about.close',
        name: 'Close About window',
        scope: 'About window',
        showInPalette: true,
        shortcuts: ['Escape'],
    },

    // ============================================================================
    // Command palette modal
    // ============================================================================
    {
        id: 'palette.up',
        name: 'Previous result',
        scope: 'Command palette',
        showInPalette: false,
        shortcuts: ['↑'],
    },
    {
        id: 'palette.down',
        name: 'Next result',
        scope: 'Command palette',
        showInPalette: false,
        shortcuts: ['↓'],
    },
    {
        id: 'palette.execute',
        name: 'Execute command',
        scope: 'Command palette',
        showInPalette: false,
        shortcuts: ['Enter'],
    },
    {
        id: 'palette.close',
        name: 'Close palette',
        scope: 'Command palette',
        showInPalette: false,
        shortcuts: ['Escape'],
    },
]

/** Get all commands that should appear in the command palette */
export function getPaletteCommands(): Command[] {
    return commands.filter((c) => c.showInPalette)
}
