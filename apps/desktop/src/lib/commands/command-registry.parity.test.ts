/**
 * Base-locale (en) parity net for the command registry's i18n migration.
 *
 * Every command's `name` (and `description`, where it has one) now resolves from
 * `messages/en/commands.json` through `t()`. This pins the EXACT English each
 * command rendered BEFORE the migration, so the reviewer can trust "current
 * users see no change" — the migration is readiness, not a copy edit. A drifted
 * catalog value (a typo, a dropped ellipsis, a mangled apostrophe) fails here.
 *
 * The license command (`app.licenseKey`) has two states; both are pinned via
 * `updateLicenseCommandName`. The three platform-conditional commands
 * (`file.showInFinder` / `file.getInfo` / `file.quickLook`) resolve their name
 * by `isMacOS()`; the test runs under jsdom (non-macOS), so it pins the
 * non-macOS labels and asserts the macOS catalog keys exist separately.
 */
import { describe, it, expect } from 'vitest'
import { commands, updateLicenseCommandName } from './command-registry'
import { tString } from '$lib/intl/messages.svelte'

/** id → exact pre-migration English `name`. */
const EXPECTED_NAMES: Record<string, string> = {
  'app.quit': 'Quit Cmdr',
  'app.hide': 'Hide Cmdr',
  'app.hideOthers': 'Hide others',
  'app.showAll': 'Show all',
  'app.about': 'About Cmdr',
  // 'app.licenseKey' is state-dependent; pinned separately below.
  'app.commandPalette': 'Open command palette',
  'app.settings': 'Open settings',
  'app.checkForUpdates': 'Check for updates…',
  'cmdr.openOnboarding': 'Onboarding…',
  'help.openShortcuts': 'Keyboard shortcuts',
  'queue.show': 'Show transfer queue',
  'help.sendErrorReport': 'Send error report…',
  'help.whatsNew': "What's new",
  'feedback.send': 'Send feedback',
  'log.operationLog': 'Operation log',
  'askCmdr.toggle': 'Ask Cmdr',
  'search.open': 'Search files',
  'nav.goToPath': 'Go to path…',
  'favorites.add': 'Add to favorites',
  'downloads.goToLatest': 'Go to latest download',
  'view.showHidden': 'Toggle hidden files',
  'view.briefMode': 'Switch to Brief view',
  'view.fullMode': 'Switch to Full view',
  'view.setMode': 'Set pane view mode',
  'view.zoom.set75': 'Zoom to 75%',
  'view.zoom.set100': 'Zoom to 100%',
  'view.zoom.set125': 'Zoom to 125%',
  'view.zoom.set150': 'Zoom to 150%',
  'view.zoom.in': 'Zoom in',
  'view.zoom.out': 'Zoom out',
  'sort.byName': 'Sort by name',
  'sort.byExtension': 'Sort by extension',
  'sort.byModified': 'Sort by date modified',
  'sort.bySize': 'Sort by size',
  'sort.byCreated': 'Sort by date created',
  'sort.ascending': 'Sort ascending',
  'sort.descending': 'Sort descending',
  'sort.toggleOrder': 'Toggle sort order',
  'sort.set': 'Set pane sort',
  'pane.switch': 'Switch pane',
  'pane.swap': 'Swap panes',
  'pane.leftVolumeChooser': 'Open left volume chooser',
  'pane.rightVolumeChooser': 'Open right volume chooser',
  'pane.copyPathLeftToRight': 'Copy path from left to right pane',
  'pane.copyPathRightToLeft': 'Copy path from right to left pane',
  'pane.refresh': 'Refresh pane',
  'tab.new': 'New tab',
  'tab.close': 'Close tab',
  'tab.reopen': 'Reopen closed tab',
  'tab.next': 'Next tab',
  'tab.prev': 'Previous tab',
  'tab.togglePin': 'Toggle pin tab',
  'tab.closeOthers': 'Close other tabs',
  'tab.mcpAction': 'Pane tab action',
  'nav.up': 'Select previous file',
  'nav.down': 'Select next file',
  'nav.open': 'Open file or folder',
  'nav.parent': 'Go to parent folder',
  'nav.home': 'Go to first file',
  'nav.end': 'Go to last file',
  'nav.pageUp': 'Page up',
  'nav.pageDown': 'Page down',
  'nav.back': 'Go back',
  'nav.forward': 'Go forward',
  'nav.openUnderCursor': 'Open item under cursor',
  'cursor.moveTo': 'Move pane cursor',
  'cursor.scrollTo': 'Scroll pane to index',
  'nav.left': 'Move to left column',
  'nav.right': 'Move to right column',
  'nav.firstInFull': 'Jump to first file',
  'nav.lastInFull': 'Jump to last file',
  'file.rename': 'Rename',
  'file.view': 'View',
  'file.edit': 'Edit in default editor',
  'file.copy': 'Copy',
  'file.move': 'Move',
  'file.compress': 'Compress',
  'edit.copy': 'Copy to clipboard',
  'edit.cut': 'Cut to clipboard',
  'edit.paste': 'Paste',
  'edit.pasteAsMove': 'Move here',
  'file.newFolder': 'New folder',
  'file.newFile': 'Create new file',
  'file.delete': 'Delete',
  'file.deletePermanently': 'Delete permanently',
  'dialog.confirm': 'Confirm open dialog',
  // Platform-conditional names resolve to the non-macOS label under jsdom.
  'file.showInFinder': 'Show in file manager',
  'file.copyPath': 'Copy path to clipboard',
  'file.copyCurrentDirectoryPath': 'Copy current directory path',
  'file.copyFilename': 'Copy filename',
  'file.getInfo': 'File properties',
  'file.quickLook': 'Preview',
  'file.contextMenu': 'Open context menu',
  'cloud.makeOffline': 'Make available offline',
  'cloud.removeDownload': 'Remove download',
  'tags.toggleGrey': 'Toggle gray tag',
  'tags.toggleGreen': 'Toggle green tag',
  'tags.togglePurple': 'Toggle purple tag',
  'tags.toggleBlue': 'Toggle blue tag',
  'tags.toggleYellow': 'Toggle yellow tag',
  'tags.toggleRed': 'Toggle red tag',
  'tags.toggleOrange': 'Toggle orange tag',
  'selection.toggle': 'Toggle selection',
  'selection.toggleAndDown': 'Toggle selection and move down',
  'selection.selectAll': 'Select all',
  'selection.deselectAll': 'Deselect all',
  'selection.selectFiles': 'Select files…',
  'selection.deselectFiles': 'Deselect files…',
  'selection.mcpSelect': 'Select range in pane',
  'selection.mcpSelectByNames': 'Select files by name in pane',
  'network.selectHost': 'Select network host',
  'network.refresh': 'Refresh network hosts',
  'share.back': 'Back to host list',
  'share.selectShare': 'Connect to share',
  'volume.select': 'Select volume',
  'volume.close': 'Close volume chooser',
  'volume.selectByName': 'Select pane volume by name',
  'about.openWebsite': 'Open website',
  'about.openUpgrade': 'Open upgrade page',
  'about.close': 'Close About window',
  'palette.up': 'Previous result',
  'palette.down': 'Next result',
  'palette.execute': 'Execute command',
  'palette.close': 'Close palette',
}

/** id → exact pre-migration English `description` (only commands that had one). */
const EXPECTED_DESCRIPTIONS: Record<string, string | undefined> = {
  'app.checkForUpdates': 'Check whether a newer version of Cmdr is available, and download it if so',
  'cmdr.openOnboarding': 'Reopen the onboarding wizard to review or change first-launch setup options',
  'help.openShortcuts': 'Open a read-only window listing every keyboard shortcut, live-synced with your customizations',
  'queue.show': 'Open a window listing every running and waiting transfer, where you can pause, resume, or cancel them',
  'help.sendErrorReport': 'Send Cmdr logs to the team to help fix something that went wrong',
  'help.whatsNew': 'See what changed in the latest releases of Cmdr',
  'feedback.send': 'Tell the maker of Cmdr what you think: ideas, wishes, anything',
  'log.operationLog': 'See a history of your file operations, and roll them back',
  'askCmdr.toggle': 'Chat with an AI about your files, drives, and history',
  'nav.goToPath': 'Jump the focused pane to a typed, pasted, or recent path.',
  'favorites.add': "Add the focused pane's current folder to the switcher's Favorites.",
  'downloads.goToLatest': 'Open ~/Downloads and select the most recent file.',
  'pane.copyPathLeftToRight':
    'Open the left pane’s location on the right. When the left pane is focused and the cursor is on a folder, that folder opens on the right instead.',
  'pane.copyPathRightToLeft':
    'Open the right pane’s location on the left. When the right pane is focused and the cursor is on a folder, that folder opens on the left instead.',
  'edit.copy': 'Copy selected files to clipboard for pasting',
  'edit.cut': 'Cut selected files (paste will move them)',
  'edit.paste': 'Paste files from clipboard into current folder',
  'edit.pasteAsMove': 'Paste files from clipboard as a move',
  'file.contextMenu': 'Opens the context menu for the file under the cursor',
  'cloud.makeOffline': 'Downloads a cloud-stored file so it stays available without an internet connection',
  'cloud.removeDownload': 'Removes the local copy of a cloud file, leaving it available online only',
  'tags.toggleGrey': 'Adds or removes the gray Finder tag on the selected files',
  'tags.toggleGreen': 'Adds or removes the green Finder tag on the selected files',
  'tags.togglePurple': 'Adds or removes the purple Finder tag on the selected files',
  'tags.toggleBlue': 'Adds or removes the blue Finder tag on the selected files',
  'tags.toggleYellow': 'Adds or removes the yellow Finder tag on the selected files',
  'tags.toggleRed': 'Adds or removes the red Finder tag on the selected files',
  'tags.toggleOrange': 'Adds or removes the orange Finder tag on the selected files',
  'selection.toggleAndDown': 'Selects or deselects the file under the cursor, then moves down (Total Commander style)',
  'selection.selectFiles': 'Opens the Select files dialog to add matching files to the selection',
  'selection.deselectFiles': 'Opens the Deselect files dialog to remove matching files from the selection',
}

describe('command registry en parity', () => {
  it('every command name is byte-identical to the pre-migration English', () => {
    for (const command of commands) {
      if (command.id === 'app.licenseKey') continue // state-dependent, checked below
      expect(command.name, command.id).toBe(EXPECTED_NAMES[command.id])
    }
  })

  it('every command description is byte-identical to the pre-migration English', () => {
    for (const command of commands) {
      const expected = EXPECTED_DESCRIPTIONS[command.id]
      if (expected === undefined) {
        // Commands with no description before migration must still have none.
        expect(command.description, `${command.id} should have no description`).toBeUndefined()
      } else {
        expect(command.description, command.id).toBe(expected)
      }
    }
  })

  it('the license command name matches its license state (both branches)', () => {
    updateLicenseCommandName(true)
    expect(commands.find((c) => c.id === 'app.licenseKey')?.name).toBe('See license details')
    updateLicenseCommandName(false)
    expect(commands.find((c) => c.id === 'app.licenseKey')?.name).toBe('Enter license key')
    updateLicenseCommandName(true) // restore default
  })

  it('the macOS platform-conditional labels exist in the catalog with the right English', () => {
    // jsdom is non-macOS, so the registry resolves the `.other` labels above.
    // Pin the macOS variants directly so a catalog edit can't drift them.
    expect(tString('commands.fileShowInFinder.mac.label')).toBe('Show in Finder')
    expect(tString('commands.fileGetInfo.mac.label')).toBe('Get info')
    expect(tString('commands.fileQuickLook.mac.label')).toBe('Quick look')
  })
})
