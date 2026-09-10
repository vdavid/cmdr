/**
 * File list command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { BLOCKED_BY_DIALOGS, IN_TEXT_INPUTS_ONLY } from '../while-dialog-open'

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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.down',
    nameKey: 'commands.navDown.label',
    scope: 'Main window/File list',
    showInPalette: false,
    shortcuts: ['↓'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.parent',
    nameKey: 'commands.navParent.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['Backspace', '⌘↑'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.home',
    nameKey: 'commands.navHome.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥↑', 'Home'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.end',
    nameKey: 'commands.navEnd.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥↓', 'End'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.pageUp',
    nameKey: 'commands.navPageUp.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['PageUp'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.pageDown',
    nameKey: 'commands.navPageDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['PageDown'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.back',
    nameKey: 'commands.navBack.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘['],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.forward',
    nameKey: 'commands.navForward.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘]'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘⇧H, not ⌘H: macOS reserves ⌘H for "Hide Cmdr" (an AppKit predefined item
    // Cmdr can neither rebind nor intercept — see `NATIVE_SHORTCUT_COMMAND_IDS`).
    id: 'nav.goHome',
    nameKey: 'commands.navGoHome.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⇧H'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ErrorPane owns this key outright: it registers a CAPTURE-phase document
    // listener while an error screen is showing, so ⌘D reaches the disclosure even
    // when the user has bound ⌘D to something else. Fixed for exactly that reason
    // — a rebind here would be a no-op illusion, and releasing the key would break
    // the "Technical details ⌘D" hint the screen advertises.
    id: 'errorPane.toggleTechnicalDetails',
    nameKey: 'commands.errorPaneToggleTechnicalDetails.label',
    scope: 'Main window/Error screen',
    showInPalette: false,
    shortcuts: ['⌘D'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.right',
    nameKey: 'commands.navRight.label',
    scope: 'Main window/Brief mode',
    showInPalette: false,
    shortcuts: ['→'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.lastInFull',
    nameKey: 'commands.navLastInFull.label',
    scope: 'Main window/Full mode',
    showInPalette: false,
    shortcuts: ['→'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.view',
    nameKey: 'commands.fileView.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F3'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.edit',
    nameKey: 'commands.fileEdit.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F4'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copy',
    nameKey: 'commands.fileCopy.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F5'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.move',
    nameKey: 'commands.fileMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F6'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘D is Finder's Duplicate. The error screen's ⌘D (a capture-phase listener,
    // `fixedKey` above) shows INSTEAD of the file list and shadows this one there
    // on purpose, which `scope-hierarchy.ts` documents as a non-conflict.
    id: 'file.duplicate',
    nameKey: 'commands.fileDuplicate.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘D'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileDuplicate.description',
  },
  {
    id: 'file.compress',
    nameKey: 'commands.fileCompress.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌥F5'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editCopy.description',
  },
  {
    id: 'edit.cut',
    nameKey: 'commands.editCut.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘X'],
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editCut.description',
  },
  {
    id: 'edit.paste',
    nameKey: 'commands.editPaste.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘V'],
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editPaste.description',
  },
  {
    id: 'edit.pasteAsMove',
    nameKey: 'commands.editPasteAsMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⌥V'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.editPasteAsMove.description',
  },
  {
    id: 'file.newFolder',
    nameKey: 'commands.fileNewFolder.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['F7'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.newFile',
    nameKey: 'commands.fileNewFile.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⇧F4'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.delete',
    nameKey: 'commands.fileDelete.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // `⌘Backspace` (shown as ⌘⌫) mirrors Finder's "Move to Trash". The menu
    // accelerator stays `F8` (first shortcut); ⌘⌫ dispatches purely via the
    // document keydown handler.
    shortcuts: ['F8', '⌘Backspace'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.deletePermanently',
    nameKey: 'commands.fileDeletePermanently.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⇧F8'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // No shortcut: it's the "where did that go?" follow-up, reached from the
    // palette or from the button on the trash toast, not from muscle memory.
    id: 'file.goToTrash',
    nameKey: 'commands.fileGoToTrash.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileGoToTrash.description',
    keywords: ['trash', 'bin', 'deleted', 'recover', 'restore', 'goto'],
  },
  {
    id: 'file.showInFinder',
    nameKey: isMacOS() ? 'commands.fileShowInFinder.mac.label' : 'commands.fileShowInFinder.other.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⌥O'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // macOS only: the known-terminals table and its launch recipes are a macOS
    // vocabulary, and there's no Linux module behind the command yet.
    // ⌘⌥T, Command-then-Option, because that's the order `formatKeyCombo` emits;
    // macOS still renders it ⌥⌘T in the native menu.
    id: 'file.openTerminalHere',
    nameKey: 'commands.fileOpenTerminalHere.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: isMacOS() ? ['⌘⌥T'] : [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileOpenTerminalHere.description',
    keywords: ['terminal', 'shell', 'console', 'cd', 'iterm', 'ghostty', 'warp'],
  },
  {
    id: 'file.copyPath',
    nameKey: 'commands.fileCopyPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⌥C'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copyCurrentDirectoryPath',
    nameKey: 'commands.fileCopyCurrentDirectoryPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copyFilename',
    nameKey: 'commands.fileCopyFilename.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.getInfo',
    nameKey: isMacOS() ? 'commands.fileGetInfo.mac.label' : 'commands.fileGetInfo.other.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: isMacOS() ? ['⌘I'] : [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.contextMenu',
    nameKey: 'commands.fileContextMenu.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
    descriptionKey: 'commands.fileContextMenu.description',
  },
  {
    id: 'cloud.makeOffline',
    nameKey: 'commands.cloudMakeOffline.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudMakeOffline.description',
  },
  {
    id: 'cloud.removeDownload',
    nameKey: 'commands.cloudRemoveDownload.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudRemoveDownload.description',
  },
  {
    id: 'cloud.openInGoogleDrive',
    nameKey: 'commands.cloudOpenInGoogleDrive.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudOpenInGoogleDrive.description',
  },
  {
    id: 'cloud.copyGoogleDriveLink',
    nameKey: 'commands.cloudCopyGoogleDriveLink.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudCopyGoogleDriveLink.description',
  },
  {
    id: 'cloud.askGemini',
    nameKey: 'commands.cloudAskGemini.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudAskGemini.description',
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleGrey.description',
  },
  {
    id: 'tags.toggleGreen',
    nameKey: 'commands.tagsToggleGreen.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleGreen.description',
  },
  {
    id: 'tags.togglePurple',
    nameKey: 'commands.tagsTogglePurple.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsTogglePurple.description',
  },
  {
    id: 'tags.toggleBlue',
    nameKey: 'commands.tagsToggleBlue.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleBlue.description',
  },
  {
    id: 'tags.toggleYellow',
    nameKey: 'commands.tagsToggleYellow.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleYellow.description',
  },
  {
    id: 'tags.toggleRed',
    nameKey: 'commands.tagsToggleRed.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleRed.description',
  },
  {
    id: 'tags.toggleOrange',
    nameKey: 'commands.tagsToggleOrange.label',
    scope: 'Main window/File list',
    showInPalette: isMacOS(),
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'selection.toggleAndDown',
    nameKey: 'commands.selectionToggleAndDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['Insert'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionToggleAndDown.description',
  },
  {
    id: 'selection.selectAll',
    nameKey: 'commands.selectionSelectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘A'],
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
  },
  {
    id: 'selection.deselectAll',
    nameKey: 'commands.selectionDeselectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['⌘⇧A'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'selection.invert',
    nameKey: 'commands.selectionInvert.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Total Commander's invert key, both ways it's typed. `⇧8` is `*` on a US
    // layout, matched by physical key too (`eventMatchesCommand`'s digit
    // fallback) so it works where Shift+8 types something else; bare `*` is the
    // numpad key, which reports `*` with no Shift on every layout. `⇧8` stays
    // first because a menu accelerator (pushed only on a rebind) reads
    // `shortcuts[0]`.
    shortcuts: ['⇧8', '*'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionInvert.description',
  },
  {
    id: 'selection.selectFiles',
    nameKey: 'commands.selectionSelectFiles.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: ['+'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionDeselectFiles.description',
  },
]
