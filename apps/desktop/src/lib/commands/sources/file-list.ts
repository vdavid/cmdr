/**
 * File list command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'
import { isMacOS } from '$lib/shortcuts/key-capture'

export const fileListCommands: CommandSource[] = [
  // ============================================================================
  // File list - Navigation commands
  // ============================================================================
  {
    id: 'nav.up',
    nameKey: 'commands.navUp.label',
    scope: 'Main window/File list',
    showInPalette: false, // Too basic for palette
    shortcuts: ['↑'],
    fixedKey: true,
  },
  {
    id: 'nav.down',
    nameKey: 'commands.navDown.label',
    scope: 'Main window/File list',
    showInPalette: false,
    shortcuts: ['↓'],
    fixedKey: true,
  },
  {
    id: 'nav.open',
    nameKey: 'commands.navOpen.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // `Enter` and `⌘↓` are display entries; FilePane handles both keys directly in
    // `handleKeyDown` (mirroring `⌘↑` = parent), the palette/MCP path uses the handler.
    shortcuts: ['Enter', '⌘↓'],
  },
  {
    id: 'nav.parent',
    nameKey: 'commands.navParent.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['Backspace', '⌘↑'],
  },
  {
    id: 'nav.home',
    nameKey: 'commands.navHome.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥↑', 'Home'],
  },
  {
    id: 'nav.end',
    nameKey: 'commands.navEnd.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥↓', 'End'],
  },
  {
    id: 'nav.pageUp',
    nameKey: 'commands.navPageUp.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['PageUp'],
  },
  {
    id: 'nav.pageDown',
    nameKey: 'commands.navPageDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['PageDown'],
  },
  {
    id: 'nav.back',
    nameKey: 'commands.navBack.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘['],
  },
  {
    id: 'nav.forward',
    nameKey: 'commands.navForward.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘]'],
  },

  // ============================================================================
  // Brief mode specific
  // ============================================================================
  {
    id: 'nav.left',
    nameKey: 'commands.navLeft.label',
    scope: 'Main window/Brief mode',
    showInPalette: false,
    shortcuts: ['←'],
    fixedKey: true,
  },
  {
    id: 'nav.right',
    nameKey: 'commands.navRight.label',
    scope: 'Main window/Brief mode',
    showInPalette: false,
    shortcuts: ['→'],
    fixedKey: true,
  },

  // ============================================================================
  // Full mode specific (left/right jump to first/last in full mode)
  // ============================================================================
  {
    id: 'nav.firstInFull',
    nameKey: 'commands.navFirstInFull.label',
    scope: 'Main window/Full mode',
    showInPalette: false,
    shortcuts: ['←'],
    fixedKey: true,
  },
  {
    id: 'nav.lastInFull',
    nameKey: 'commands.navLastInFull.label',
    scope: 'Main window/Full mode',
    showInPalette: false,
    shortcuts: ['→'],
    fixedKey: true,
  },

  // ============================================================================
  // File list - File action commands
  // ============================================================================
  {
    id: 'file.rename',
    nameKey: 'commands.fileRename.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F2', '⇧F6'],
  },
  {
    id: 'file.view',
    nameKey: 'commands.fileView.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F3'],
  },
  {
    id: 'file.edit',
    nameKey: 'commands.fileEdit.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F4'],
  },
  {
    id: 'file.copy',
    nameKey: 'commands.fileCopy.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F5'],
  },
  {
    id: 'file.move',
    nameKey: 'commands.fileMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F6'],
  },
  {
    id: 'file.compress',
    nameKey: 'commands.fileCompress.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥F5'],
  },

  // ============================================================================
  // File list - Edit commands (clipboard operations)
  // ============================================================================
  {
    id: 'edit.copy',
    nameKey: 'commands.editCopy.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘C'],
    descriptionKey: 'commands.editCopy.description',
  },
  {
    id: 'edit.cut',
    nameKey: 'commands.editCut.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘X'],
    descriptionKey: 'commands.editCut.description',
  },
  {
    id: 'edit.paste',
    nameKey: 'commands.editPaste.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘V'],
    descriptionKey: 'commands.editPaste.description',
  },
  {
    id: 'edit.pasteAsMove',
    nameKey: 'commands.editPasteAsMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥⌘V'],
    descriptionKey: 'commands.editPasteAsMove.description',
  },
  {
    id: 'file.newFolder',
    nameKey: 'commands.fileNewFolder.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F7'],
  },
  {
    id: 'file.newFile',
    nameKey: 'commands.fileNewFile.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⇧F4'],
  },
  {
    id: 'file.delete',
    nameKey: 'commands.fileDelete.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // `⌘⌫` mirrors Finder's "Move to Trash". The menu accelerator stays `F8`
    // (first shortcut); `⌘⌫` dispatches purely via the document keydown handler.
    shortcuts: ['F8', '⌘⌫'],
  },
  {
    id: 'file.deletePermanently',
    nameKey: 'commands.fileDeletePermanently.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⇧F8'],
  },
  {
    id: 'file.showInFinder',
    nameKey: isMacOS() ? 'commands.fileShowInFinder.mac.label' : 'commands.fileShowInFinder.other.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥⌘O'],
  },
  {
    id: 'file.copyPath',
    nameKey: 'commands.fileCopyPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌃⌘C'],
  },
  {
    id: 'file.copyCurrentDirectoryPath',
    nameKey: 'commands.fileCopyCurrentDirectoryPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'file.copyFilename',
    nameKey: 'commands.fileCopyFilename.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'file.getInfo',
    nameKey: isMacOS() ? 'commands.fileGetInfo.mac.label' : 'commands.fileGetInfo.other.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: isMacOS() ? ['⌘I'] : [],
  },
  {
    id: 'file.quickLook',
    nameKey: isMacOS() ? 'commands.fileQuickLook.mac.label' : 'commands.fileQuickLook.other.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    // ⇧Space matches the menu accelerator. key-capture.ts maps `' '` → 'Space',
    // and registry shortcuts use that same `⇧Space` form (no separator) for the
    // Tier-1 dispatcher and the menu-accelerator sync to agree.
    shortcuts: isMacOS() ? ['⇧Space'] : [],
  },
  {
    id: 'file.contextMenu',
    nameKey: 'commands.fileContextMenu.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
    fixedKey: true,
    descriptionKey: 'commands.fileContextMenu.description',
  },
  {
    id: 'cloud.makeOffline',
    nameKey: 'commands.cloudMakeOffline.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.cloudMakeOffline.description',
  },
  {
    id: 'cloud.removeDownload',
    nameKey: 'commands.cloudRemoveDownload.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.cloudRemoveDownload.description',
  },

  // ============================================================================
  // File list - Finder tag colors (macOS). Toggle a system color tag on the
  // focused selection. No default shortcut; the user binds one in Settings.
  // ============================================================================
  {
    id: 'tags.toggleGrey',
    nameKey: 'commands.tagsToggleGrey.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleGrey.description',
  },
  {
    id: 'tags.toggleGreen',
    nameKey: 'commands.tagsToggleGreen.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleGreen.description',
  },
  {
    id: 'tags.togglePurple',
    nameKey: 'commands.tagsTogglePurple.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsTogglePurple.description',
  },
  {
    id: 'tags.toggleBlue',
    nameKey: 'commands.tagsToggleBlue.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleBlue.description',
  },
  {
    id: 'tags.toggleYellow',
    nameKey: 'commands.tagsToggleYellow.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleYellow.description',
  },
  {
    id: 'tags.toggleRed',
    nameKey: 'commands.tagsToggleRed.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleRed.description',
  },
  {
    id: 'tags.toggleOrange',
    nameKey: 'commands.tagsToggleOrange.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    descriptionKey: 'commands.tagsToggleOrange.description',
  },

  // ============================================================================
  // File list - Selection commands
  // ============================================================================
  {
    id: 'selection.toggle',
    nameKey: 'commands.selectionToggle.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['Space'],
  },
  {
    id: 'selection.toggleAndDown',
    nameKey: 'commands.selectionToggleAndDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['Insert'],
    descriptionKey: 'commands.selectionToggleAndDown.description',
  },
  {
    id: 'selection.selectAll',
    nameKey: 'commands.selectionSelectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘A'],
  },
  {
    id: 'selection.deselectAll',
    nameKey: 'commands.selectionDeselectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⇧A'],
  },
  {
    id: 'selection.selectFiles',
    nameKey: 'commands.selectionSelectFiles.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['+'],
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionSelectFiles.description',
  },
  {
    id: 'selection.deselectFiles',
    nameKey: 'commands.selectionDeselectFiles.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // `-` and `⇧-` both open "Deselect files…"; FilePane classifies the physical
    // Minus key (layout-independent) in `selection-dialog-keys.ts`. The menu
    // accelerator stays `-` (first shortcut).
    shortcuts: ['-', '⇧-'],
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionDeselectFiles.description',
  },
]
