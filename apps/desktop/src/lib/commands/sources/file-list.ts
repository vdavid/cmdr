/**
 * File list command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, concatShortcuts, sameOnAll } from '../platform-shortcuts'
import { getBadgeStatus } from '#lib/feature-status.js'
import { sameKindCommandLabel } from '#lib/file-explorer/pane/same-kind-target.svelte.js'
import { trashUndoCommandLabel } from '#lib/file-operations/delete/trash-undo.js'
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
    shortcuts: sameOnAll('↑'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.down',
    nameKey: 'commands.navDown.label',
    scope: 'Main window/File list',
    showInPalette: false,
    shortcuts: sameOnAll('↓'),
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
    shortcuts: concatShortcuts(sameOnAll('Enter'), cmdOrCtrl('⌘↓')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.parent',
    nameKey: 'commands.navParent.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: concatShortcuts(sameOnAll('Backspace'), cmdOrCtrl('⌘↑')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.home',
    nameKey: 'commands.navHome.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('⌥↑', 'Home'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.end',
    nameKey: 'commands.navEnd.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('⌥↓', 'End'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.pageUp',
    nameKey: 'commands.navPageUp.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('PageUp'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.pageDown',
    nameKey: 'commands.navPageDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('PageDown'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.back',
    nameKey: 'commands.navBack.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘['),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'nav.forward',
    nameKey: 'commands.navForward.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘]'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘/ because `/` is the root's own name.
    id: 'nav.goToRoot',
    nameKey: 'commands.navGoToRoot.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘/'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘⇧H, not ⌘H: macOS reserves ⌘H for "Hide Cmdr" (an AppKit predefined item
    // Cmdr can neither rebind nor intercept — see `NATIVE_SHORTCUT_COMMAND_IDS`).
    id: 'nav.goHome',
    nameKey: 'commands.navGoHome.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⇧H'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    // Windows names it "Go to user folder", so "home" stays findable for muscle memory.
    keywords: ['home'],
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
    shortcuts: cmdOrCtrl('⌘D'),
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
    shortcuts: sameOnAll('←'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.right',
    nameKey: 'commands.navRight.label',
    scope: 'Main window/Brief mode',
    showInPalette: false,
    shortcuts: sameOnAll('→'),
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
    shortcuts: sameOnAll('←'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'nav.lastInFull',
    nameKey: 'commands.navLastInFull.label',
    scope: 'Main window/Full mode',
    showInPalette: false,
    shortcuts: sameOnAll('→'),
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
    shortcuts: sameOnAll('F2', '⇧F6'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Total Commander's Ctrl+S. It stays out of plain typing's way: the Settings mode decides
    // what typing does, and this works on one pane for one folder (`quick-filter-session.svelte.ts`).
    id: 'pane.quickFilter',
    nameKey: 'commands.paneQuickFilter.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: { macos: ['⌃S'], windows: ['Ctrl+S'], linux: ['Ctrl+S'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneQuickFilter.description',
    keywords: ['narrow', 'type to jump', 'search names'],
  },
  {
    // Total Commander's Ctrl+M. ⌘M is macOS's Minimize, so it's ⌃M here.
    id: 'file.multiRename',
    nameKey: 'commands.fileMultiRename.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: { macos: ['⌃M'], windows: ['Ctrl+M'], linux: ['Ctrl+M'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileMultiRename.description',
    // Phrases, not loose words: the palette's uFuzzy matches a query's words in order and at
    // most three characters apart, so "batch rename" needs "batch" right before a "rename".
    keywords: ['batch rename', 'bulk rename', 'mass rename', 'rename multiple', 'rename many'],
  },
  {
    id: 'file.view',
    nameKey: 'commands.fileView.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('F3'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.edit',
    nameKey: 'commands.fileEdit.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('F4'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copy',
    nameKey: 'commands.fileCopy.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('F5'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.move',
    nameKey: 'commands.fileMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('F6'),
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
    shortcuts: cmdOrCtrl('⌘D'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileDuplicate.description',
  },
  {
    id: 'file.compress',
    nameKey: 'commands.fileCompress.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('⌥F5'),
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
    shortcuts: cmdOrCtrl('⌘C'),
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editCopy.description',
  },
  {
    id: 'edit.cut',
    nameKey: 'commands.editCut.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘X'),
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editCut.description',
  },
  {
    id: 'edit.paste',
    nameKey: 'commands.editPaste.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘V'),
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
    descriptionKey: 'commands.editPaste.description',
  },
  {
    id: 'edit.pasteAsMove',
    nameKey: 'commands.editPasteAsMove.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⌥V'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.editPasteAsMove.description',
  },
  {
    id: 'file.newFolder',
    nameKey: 'commands.fileNewFolder.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('F7'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.newFile',
    nameKey: 'commands.fileNewFile.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('⇧F4'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.delete',
    nameKey: 'commands.fileDelete.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // `⌘Backspace` (shown as ⌘⌫) mirrors Finder's "Move to Trash", and `Delete`
    // is the forward-delete key (⌦, or fn+⌫ on a laptop), the way Total Commander
    // and Windows users reach for it. The menu accelerator stays `F8` (first
    // shortcut); the other two dispatch purely via the document keydown handler,
    // which leaves them to a focused text field.
    shortcuts: concatShortcuts(sameOnAll('F8'), cmdOrCtrl('⌘Backspace'), sameOnAll('Delete')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.deletePermanently',
    nameKey: 'commands.fileDeletePermanently.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Each delete-to-trash key with Shift, Total Commander's way (`⇧F8`, `⇧Delete`),
    // plus Finder's "Delete Immediately" (`⌥⌘⌫`). The menu accelerator stays `⇧F8`.
    shortcuts: concatShortcuts(sameOnAll('⇧F8', '⇧Delete'), cmdOrCtrl('⌘⌥Backspace')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // The trash toast's Undo, kept reachable after the toast has gone and without a
    // pointer: Edit > Undo move to trash and the palette (`trash-undo.ts`). No default
    // shortcut: ⌘Z is macOS's text Undo, which the Edit menu's predefined item owns.
    id: 'edit.undoTrash',
    nameKey: 'commands.editUndoTrash.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.editUndoTrash.description',
    keywords: ['undo', 'trash', 'bin', 'recycle bin', 'restore', 'put back', 'recover', 'deleted'],
    // Offered only while there's a trash to undo, and named after it, the way the Edit menu
    // item greys out and relabels itself.
    paletteCondition: 'trashUndoAvailable',
    displayName: trashUndoCommandLabel,
  },
  {
    // No shortcut: it's the "where did that go?" follow-up, reached from the
    // palette or from the button on the trash toast, not from muscle memory.
    id: 'file.goToTrash',
    nameKey: 'commands.fileGoToTrash.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileGoToTrash.description',
    keywords: ['trash', 'bin', 'recycle bin', 'deleted', 'recover', 'restore', 'goto'],
  },
  {
    id: 'file.showInFinder',
    nameKey: 'commands.fileShowInFileManager.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⌥O'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Behind `openTerminalHere`: the known-terminals table and its launch recipes are a macOS
    // vocabulary, and there's no Linux module behind the command yet.
    // F9 first: it's the F-key bar's slot, so the bar chip, the palette, and the
    // menu all show it. ⌘⌥T stays as a second binding, written Command-then-Option
    // because that's the order `formatKeyCombo` emits.
    id: 'file.openTerminalHere',
    nameKey: 'commands.fileOpenTerminalHere.label',
    scope: 'Main window/File list',
    needsCapability: 'openTerminalHere',
    showInPalette: true,
    shortcuts: concatShortcuts(sameOnAll('F9'), cmdOrCtrl('⌘⌥T')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileOpenTerminalHere.description',
    keywords: ['terminal', 'shell', 'console', 'cd', 'iterm', 'ghostty', 'warp'],
  },
  {
    id: 'file.copyPath',
    nameKey: 'commands.fileCopyPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⌥C'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copyCurrentDirectoryPath',
    nameKey: 'commands.fileCopyCurrentDirectoryPath.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copyFilename',
    nameKey: 'commands.fileCopyFilename.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.copyShareLink',
    nameKey: 'commands.fileCopyShareLink.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Only where the focused pane's volume can mint one (S3 today), by capability.
    paletteCondition: 'focusedPaneSharesLinks',
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileCopyShareLink.description',
  },
  {
    id: 'file.copyShareLinkOneDay',
    nameKey: 'commands.fileCopyShareLinkOneDay.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Only where the focused pane's volume can mint one (S3 today), by capability.
    paletteCondition: 'focusedPaneSharesLinks',
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileCopyShareLinkOneDay.description',
  },
  {
    id: 'file.copyShareLinkOneHour',
    nameKey: 'commands.fileCopyShareLinkOneHour.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Only where the focused pane's volume can mint one (S3 today), by capability.
    paletteCondition: 'focusedPaneSharesLinks',
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileCopyShareLinkOneHour.description',
  },
  {
    id: 'file.getInfo',
    nameKey: 'commands.fileGetInfo.label',
    scope: 'Main window/File list',
    needsCapability: 'getInfo',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘I'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.quickLook',
    nameKey: 'commands.fileQuickLook.label',
    scope: 'Main window/File list',
    needsCapability: 'quickLook',
    showInPalette: true,
    // ⇧Space matches the menu accelerator. key-capture.ts maps `' '` → 'Space',
    // and registry shortcuts use that same `⇧Space` form (no separator) for the
    // Tier-1 dispatcher and the menu-accelerator sync to agree.
    shortcuts: sameOnAll('⇧Space'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'file.contextMenu',
    nameKey: 'commands.fileContextMenu.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Finder's own combo for the same thing. `⌃` is nearly unused here (only
    // `⌃Tab` / `⌃⇧Tab`), and `#lib/context-menu-ownership.ts` listens for the DOM
    // `contextmenu` event rather than a keydown, so nothing else wants it.
    shortcuts: { macos: ['⌃Enter'], windows: ['Ctrl+Enter'], linux: ['Ctrl+Enter'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.fileContextMenu.description',
  },
  {
    id: 'cloud.makeOffline',
    nameKey: 'commands.cloudMakeOffline.label',
    scope: 'Main window/File list',
    needsCapability: 'cloudFileActions',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudMakeOffline.description',
  },
  {
    id: 'cloud.removeDownload',
    nameKey: 'commands.cloudRemoveDownload.label',
    scope: 'Main window/File list',
    needsCapability: 'cloudFileActions',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudRemoveDownload.description',
  },
  {
    id: 'cloud.openInGoogleDrive',
    nameKey: 'commands.cloudOpenInGoogleDrive.label',
    scope: 'Main window/File list',
    needsCapability: 'cloudFileActions',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudOpenInGoogleDrive.description',
  },
  {
    id: 'cloud.copyGoogleDriveLink',
    nameKey: 'commands.cloudCopyGoogleDriveLink.label',
    scope: 'Main window/File list',
    needsCapability: 'cloudFileActions',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudCopyGoogleDriveLink.description',
  },
  {
    id: 'cloud.askGemini',
    nameKey: 'commands.cloudAskGemini.label',
    scope: 'Main window/File list',
    needsCapability: 'cloudFileActions',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.cloudAskGemini.description',
  },
  {
    // The way to drop a backup's remembered password (`#lib/file-explorer/restic/forget-password.ts`).
    id: 'restic.forgetPassword',
    nameKey: 'commands.resticForgetPassword.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.resticForgetPassword.description',
  },

  // ============================================================================
  // File list - Finder tag colors (macOS). Toggle a system color tag on the
  // focused selection. No default shortcut; the user binds one in Settings.
  // ============================================================================
  {
    id: 'tags.toggleGrey',
    nameKey: 'commands.tagsToggleGrey.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleGrey.description',
  },
  {
    id: 'tags.toggleGreen',
    nameKey: 'commands.tagsToggleGreen.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleGreen.description',
  },
  {
    id: 'tags.togglePurple',
    nameKey: 'commands.tagsTogglePurple.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsTogglePurple.description',
  },
  {
    id: 'tags.toggleBlue',
    nameKey: 'commands.tagsToggleBlue.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleBlue.description',
  },
  {
    id: 'tags.toggleYellow',
    nameKey: 'commands.tagsToggleYellow.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleYellow.description',
  },
  {
    id: 'tags.toggleRed',
    nameKey: 'commands.tagsToggleRed.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.tagsToggleRed.description',
  },
  {
    id: 'tags.toggleOrange',
    nameKey: 'commands.tagsToggleOrange.label',
    scope: 'Main window/File list',
    needsCapability: 'colorTags',
    showInPalette: true,
    shortcuts: sameOnAll(),
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
    shortcuts: sameOnAll('Space'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'selection.toggleAndDown',
    nameKey: 'commands.selectionToggleAndDown.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('Insert'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionToggleAndDown.description',
  },
  {
    id: 'selection.selectAll',
    nameKey: 'commands.selectionSelectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘A'),
    whileDialogOpen: IN_TEXT_INPUTS_ONLY,
  },
  {
    id: 'selection.deselectAll',
    nameKey: 'commands.selectionDeselectAll.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⇧A'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'selection.invert',
    nameKey: 'commands.selectionInvert.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Total Commander's invert key: whichever key types `*` on the user's layout
    // (US ⇧8, Swedish ⇧', the numpad, an AltGr-style ⌥ combo), since
    // `formatKeyCombo` names typed symbols by character.
    shortcuts: sameOnAll('*'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionInvert.description',
  },
  {
    id: 'selection.selectSameKind',
    nameKey: 'commands.selectionSelectSameKind.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Total Commander's `Alt+Num +`, both ways it's typed: the main-row key and
    // the numpad one. `⌥⇧=` is `±` on a US layout, matched by physical key
    // (`keyComboCandidates`) so it works wherever the combo types something
    // else. It stays first because that's the combo the
    // menu DISPLAYS; the file pane's keydown handler owns both, so neither is a
    // real menu accelerator (a bare-ish `⌥` combo would eat the character in
    // every text field).
    shortcuts: sameOnAll('⌥⇧=', '⌥+'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionSelectSameKind.description',
    // The palette alone reads this; every listing surface keeps the static name.
    displayName: sameKindCommandLabel,
  },
  {
    // Total Commander's ⇧F2: each pane marks the files the other lacks plus the
    // newer copies, so F5 afterwards brings the other side up to date.
    id: 'selection.compareDirectories',
    nameKey: 'commands.selectionCompareDirectories.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('⇧F2'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionCompareDirectories.description',
  },
  {
    id: 'selection.compareDirectoriesMissing',
    nameKey: 'commands.selectionCompareDirectoriesMissing.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionCompareDirectoriesMissing.description',
  },
  {
    id: 'selection.compareDirectoriesSize',
    nameKey: 'commands.selectionCompareDirectoriesSize.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.selectionCompareDirectoriesSize.description',
  },
  {
    id: 'selection.selectFiles',
    nameKey: 'commands.selectionSelectFiles.label',
    scope: 'Main window/File list',
    showInPalette: true,
    shortcuts: sameOnAll('+'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionSelectFiles.description',
  },
  {
    id: 'selection.deselectFiles',
    nameKey: 'commands.selectionDeselectFiles.label',
    scope: 'Main window/File list',
    showInPalette: true,
    // Total Commander's `-`: the main-row key and the numpad one, whatever Shift
    // the layout needs to type it (`formatKeyCombo` names typed symbols by
    // character). The menu only displays it: no ⌘ / ⌃ / ⌥, so no accelerator.
    shortcuts: sameOnAll('-'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionDeselectFiles.description',
  },
]
