/**
 * Complete registry of all commands in the application.
 *
 * This is the single source of truth for:
 * - Command palette entries
 * - Keyboard shortcut documentation
 * - Future MCP server commands
 * - Settings pane shortcut configuration
 *
 * Each entry is authored as a `CommandSource` holding i18n message KEYS
 * (`nameKey` / `descriptionKey`), not English. `resolveCommand` turns each source
 * into a `Command` whose `name` / `description` resolve the catalog string through
 * `t()` at read time, so the whole `command.name` consumer surface (palette,
 * fuzzy haystack, shortcuts list, menus) is unchanged while the copy lives in
 * `messages/en/commands.json`. The command IDS stay untouched.
 */

import type { Command, CommandSource } from './types'
import { getBadgeStatus } from '$lib/feature-status'
import { tString } from '$lib/intl/messages.svelte'
import { isMacOS } from '$lib/shortcuts/key-capture'

/**
 * The macOS-native commands: AppKit `PredefinedMenuItem`s own BOTH the behavior
 * and the accelerator (`terminate:`, `hide:`, `hideOtherApplications:`,
 * `unhideAllApplications:`). Cmdr can neither rebind nor intercept them, so the
 * shortcuts editor renders them read-only and the store refuses to customize
 * them. Single source of truth: the registry entries below carry
 * `nativeShortcut: true` for exactly these ids (pinned by `command-registry.test.ts`),
 * and `command-handlers/types.ts` sources its Family-1 dispatch-exempt list from here.
 */
export const NATIVE_SHORTCUT_COMMAND_IDS = ['app.quit', 'app.hide', 'app.hideOthers', 'app.showAll'] as const

/**
 * The fixed-key commands: their keys are hardcoded in the owning component's
 * keydown handler (FilePane arrows, palette navigation, modal Enter/Escape) and
 * never consult the shortcuts store, so a customization would be a no-op
 * illusion — the new key wouldn't fire and the built-in key wouldn't release.
 * The shortcuts editor renders them read-only ("Fixed" badge) and the store
 * refuses to customize them. Single source of truth: the registry entries carry
 * `fixedKey: true` for exactly these ids (pinned by `command-registry.test.ts`),
 * and `command-handlers/types.ts` sources its Family-2/3 dispatch-exempt lists
 * from here.
 */
export const FIXED_KEY_COMMAND_IDS = [
  // Family 2 — per-keystroke file-list navigation (FilePane keydown).
  'nav.up',
  'nav.down',
  'nav.left',
  'nav.right',
  'nav.firstInFull',
  'nav.lastInFull',
  // Family 3 — component-scoped modal / sub-view keys.
  'palette.up',
  'palette.down',
  'palette.execute',
  'palette.close',
  'volume.select',
  'volume.close',
  'network.selectHost',
  'share.back',
  'share.selectShare',
  'file.contextMenu',
] as const

/**
 * Whether the user already has a license, driving the `app.licenseKey` command's
 * name (`See license details` vs `Enter license key`). The label depends on
 * runtime license state, so it can't be a single static key. `updateLicenseCommandName`
 * flips this; the resolved command's `name` getter reads it live. Kept in sync
 * with the native menu's license item.
 */
let hasExistingLicense = true

// `CommandSource.id` is the `CommandId` union derived from `COMMAND_IDS` in
// `command-ids.ts`. Adding an entry here whose id isn't in that tuple is a
// compile error; a tuple id with no entry here is caught by the set-equality
// test in `command-registry.test.ts`.
const commandSources: CommandSource[] = [
  // ============================================================================
  // App scope (work everywhere, regardless of window/modal state)
  // ============================================================================
  // Native-only: handled by PredefinedMenuItems via macOS selectors (hide:, hideOtherApplications:,
  // unhideAllApplications:, terminate:). showInPalette: false keeps them out of the JS shortcut
  // dispatch map; the native menu accelerators handle the keyboard shortcuts directly. `nativeShortcut`
  // makes the editor read-only and the store refuse to rebind them (NATIVE_SHORTCUT_COMMAND_IDS above).
  {
    id: 'app.quit',
    nameKey: 'commands.appQuit.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌘Q'],
    nativeShortcut: true,
  },
  {
    id: 'app.hide',
    nameKey: 'commands.appHide.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌘H'],
    nativeShortcut: true,
  },
  {
    id: 'app.hideOthers',
    nameKey: 'commands.appHideOthers.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: ['⌥⌘H'],
    nativeShortcut: true,
  },
  {
    id: 'app.showAll',
    nameKey: 'commands.appShowAll.label',
    scope: 'App',
    showInPalette: false,
    shortcuts: [],
    nativeShortcut: true,
  },
  { id: 'app.about', nameKey: 'commands.appAbout.label', scope: 'App', showInPalette: true, shortcuts: [] },
  // `app.licenseKey` resolves its name from one of two keys via the license-state
  // getter below (see `resolveCommand`), so it carries no `nameKey` here.
  {
    id: 'app.licenseKey',
    nameKey: 'commands.appLicenseKey.seeDetails.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'app.commandPalette',
    nameKey: 'commands.appCommandPalette.label',
    scope: 'App',
    showInPalette: false, // Don't show the palette in itself
    shortcuts: ['⌘⇧P'],
  },
  { id: 'app.settings', nameKey: 'commands.appSettings.label', scope: 'App', showInPalette: true, shortcuts: ['⌘,'] },
  {
    id: 'app.checkForUpdates',
    nameKey: 'commands.appCheckForUpdates.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.appCheckForUpdates.description',
  },
  {
    id: 'cmdr.openOnboarding',
    nameKey: 'commands.cmdrOpenOnboarding.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.cmdrOpenOnboarding.description',
  },
  {
    id: 'help.openShortcuts',
    nameKey: 'commands.helpOpenShortcuts.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpOpenShortcuts.description',
  },
  {
    id: 'queue.show',
    nameKey: 'commands.queueShow.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.queueShow.description',
  },
  {
    id: 'help.sendErrorReport',
    nameKey: 'commands.helpSendErrorReport.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpSendErrorReport.description',
  },
  {
    id: 'help.whatsNew',
    nameKey: 'commands.helpWhatsNew.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.helpWhatsNew.description',
  },
  {
    id: 'feedback.send',
    nameKey: 'commands.feedbackSend.label',
    scope: 'App',
    showInPalette: true,
    shortcuts: [],
    descriptionKey: 'commands.feedbackSend.description',
  },

  // ============================================================================
  // Main window - Search
  // ============================================================================
  {
    id: 'search.open',
    nameKey: 'commands.searchOpen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘F', '⌥F7'],
    status: getBadgeStatus('search'),
  },

  // ============================================================================
  // Main window - Navigation (Go to path)
  // ============================================================================
  {
    id: 'nav.goToPath',
    nameKey: 'commands.navGoToPath.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘G'],
    descriptionKey: 'commands.navGoToPath.description',
    keywords: ['jump', 'navigate', 'goto'],
  },

  // ============================================================================
  // Main window - Favorites
  // ============================================================================
  {
    id: 'favorites.add',
    nameKey: 'commands.favoritesAdd.label',
    scope: 'Main window',
    showInPalette: true,
    // No default shortcut: adding a favorite is infrequent, so it doesn't earn a global key by
    // default. Stays in the command palette and is assignable in Settings > Keyboard shortcuts.
    shortcuts: [],
    descriptionKey: 'commands.favoritesAdd.description',
    keywords: ['bookmark', 'favorite', 'pin', 'shortcut'],
  },

  // ============================================================================
  // Main window - Downloads
  // ============================================================================
  {
    id: 'downloads.goToLatest',
    nameKey: 'commands.downloadsGoToLatest.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘J'],
    descriptionKey: 'commands.downloadsGoToLatest.description',
    keywords: ['jump', 'navigate', 'goto'],
  },

  // ============================================================================
  // Main window - View commands
  // ============================================================================
  {
    id: 'view.showHidden',
    nameKey: 'commands.viewShowHidden.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘⇧.'],
  },
  {
    id: 'view.briefMode',
    nameKey: 'commands.viewBriefMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘2'],
  },
  {
    id: 'view.fullMode',
    nameKey: 'commands.viewFullMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘1'],
  },
  {
    // Per-pane view change carrying `{ pane, mode }` args, dispatched by the
    // native-menu `view-mode-changed` event (a click on the inactive pane's
    // Full/Brief item). Hidden from the palette: the focused-pane
    // `view.briefMode` / `view.fullMode` are the user-facing entries; this one
    // exists so an inactive-pane menu click sets that pane without stealing focus.
    id: 'view.setMode',
    nameKey: 'commands.viewSetMode.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },

  // ============================================================================
  // Main window - Zoom (text size) commands
  // ============================================================================
  {
    id: 'view.zoom.set75',
    nameKey: 'commands.viewZoomSet75.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'view.zoom.set100',
    nameKey: 'commands.viewZoomSet100.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘0'],
  },
  {
    id: 'view.zoom.set125',
    nameKey: 'commands.viewZoomSet125.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'view.zoom.set150',
    nameKey: 'commands.viewZoomSet150.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'view.zoom.in',
    nameKey: 'commands.viewZoomIn.label',
    scope: 'Main window',
    // ⌘+ is the native menu accelerator (Cmd+Plus on macOS = Cmd+Shift+=);
    // ⌘= is included so the unshifted `=` key fires zoom-in too.
    shortcuts: ['⌘+', '⌘='],
    showInPalette: true,
  },
  {
    id: 'view.zoom.out',
    nameKey: 'commands.viewZoomOut.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘-'],
  },

  // ============================================================================
  // Main window - Sort commands (also accessible via menu)
  // ============================================================================
  {
    id: 'sort.byName',
    nameKey: 'commands.sortByName.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘3', '⌘F3'],
  },
  {
    id: 'sort.byExtension',
    nameKey: 'commands.sortByExtension.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘4', '⌘F4'],
  },
  {
    id: 'sort.byModified',
    nameKey: 'commands.sortByModified.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘5', '⌘F5'],
  },
  {
    id: 'sort.bySize',
    nameKey: 'commands.sortBySize.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘6', '⌘F6'],
  },
  {
    id: 'sort.byCreated',
    nameKey: 'commands.sortByCreated.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'sort.ascending',
    nameKey: 'commands.sortAscending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'sort.descending',
    nameKey: 'commands.sortDescending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'sort.toggleOrder',
    nameKey: 'commands.sortToggleOrder.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  // Per-pane sort carrying `{ pane, column, order }`, dispatched by the MCP `sort`
  // tool. Hidden from the palette: the `sort.by*` commands are the user-facing
  // entries; this one targets a specific pane with an explicit order.
  { id: 'sort.set', nameKey: 'commands.sortSet.label', scope: 'Main window', showInPalette: false, shortcuts: [] },

  // ============================================================================
  // Main window - Pane commands
  // ============================================================================
  {
    id: 'pane.switch',
    nameKey: 'commands.paneSwitch.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['Tab'],
  },
  { id: 'pane.swap', nameKey: 'commands.paneSwap.label', scope: 'Main window', showInPalette: true, shortcuts: ['⌘U'] },
  {
    id: 'pane.leftVolumeChooser',
    nameKey: 'commands.paneLeftVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌥F1'],
  },
  {
    id: 'pane.rightVolumeChooser',
    nameKey: 'commands.paneRightVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌥F2'],
  },
  {
    id: 'pane.copyPathLeftToRight',
    nameKey: 'commands.paneCopyPathLeftToRight.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘→'],
    descriptionKey: 'commands.paneCopyPathLeftToRight.description',
  },
  {
    id: 'pane.copyPathRightToLeft',
    nameKey: 'commands.paneCopyPathRightToLeft.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘←'],
    descriptionKey: 'commands.paneCopyPathRightToLeft.description',
  },

  // ============================================================================
  // Main window - Tab commands
  // ============================================================================
  { id: 'tab.new', nameKey: 'commands.tabNew.label', scope: 'Main window', showInPalette: true, shortcuts: ['⌘T'] },
  { id: 'tab.close', nameKey: 'commands.tabClose.label', scope: 'Main window', showInPalette: true, shortcuts: ['⌘W'] },
  {
    id: 'tab.reopen',
    nameKey: 'commands.tabReopen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘⇧T'],
  },
  { id: 'tab.next', nameKey: 'commands.tabNext.label', scope: 'Main window', showInPalette: true, shortcuts: ['⌃Tab'] },
  {
    id: 'tab.prev',
    nameKey: 'commands.tabPrev.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌃⇧Tab'],
  },
  {
    id: 'tab.togglePin',
    nameKey: 'commands.tabTogglePin.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'tab.closeOthers',
    nameKey: 'commands.tabCloseOthers.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
  },

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
    shortcuts: ['Enter'],
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
    shortcuts: ['F8'],
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
    shortcuts: ['-'],
    status: getBadgeStatus('select-files'),
    descriptionKey: 'commands.selectionDeselectFiles.description',
  },

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
    shortcuts: ['Backspace', 'Escape'],
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

  // ============================================================================
  // MCP-only per-pane commands
  // ============================================================================
  // Carry per-pane / per-option payloads the focused-pane registry commands can't
  // express. Dispatched by the MCP server's tools through the command bus; all are
  // `showInPalette: false` (no user-facing palette entry) with no shortcut.
  {
    id: 'volume.selectByName',
    nameKey: 'commands.volumeSelectByName.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'selection.mcpSelect',
    nameKey: 'commands.selectionMcpSelect.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'selection.mcpSelectByNames',
    nameKey: 'commands.selectionMcpSelectByNames.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'cursor.moveTo',
    nameKey: 'commands.cursorMoveTo.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'cursor.scrollTo',
    nameKey: 'commands.cursorScrollTo.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'pane.refresh',
    nameKey: 'commands.paneRefresh.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'nav.openUnderCursor',
    nameKey: 'commands.navOpenUnderCursor.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'tab.mcpAction',
    nameKey: 'commands.tabMcpAction.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },
  {
    id: 'dialog.confirm',
    nameKey: 'commands.dialogConfirm.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: [],
  },

  // ============================================================================
  // About window
  // ============================================================================
  {
    id: 'about.openWebsite',
    nameKey: 'commands.aboutOpenWebsite.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'about.openUpgrade',
    nameKey: 'commands.aboutOpenUpgrade.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: [],
  },
  {
    id: 'about.close',
    nameKey: 'commands.aboutClose.label',
    scope: 'About window',
    showInPalette: true,
    shortcuts: ['Escape'],
  },

  // ============================================================================
  // Command palette modal
  // ============================================================================
  {
    id: 'palette.up',
    nameKey: 'commands.paletteUp.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['↑'],
    fixedKey: true,
  },
  {
    id: 'palette.down',
    nameKey: 'commands.paletteDown.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['↓'],
    fixedKey: true,
  },
  {
    id: 'palette.execute',
    nameKey: 'commands.paletteExecute.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['Enter'],
    fixedKey: true,
  },
  {
    id: 'palette.close',
    nameKey: 'commands.paletteClose.label',
    scope: 'Command palette',
    showInPalette: false,
    shortcuts: ['Escape'],
    fixedKey: true,
  },
]

/**
 * Resolves an authored `CommandSource` into a `Command` whose `name` (and, where
 * present, `description`) are getters that read the catalog through `t()` at
 * access time, so palette/menu/shortcut consumers stay unchanged and reactivity
 * holds in markup. `app.licenseKey` resolves its name from one of two keys based
 * on the live `hasExistingLicense` flag (`updateLicenseCommandName` flips it).
 */
function resolveCommand(src: CommandSource): Command {
  const { nameKey, descriptionKey, ...rest } = src
  const cmd = {
    ...rest,
    get name(): string {
      if (rest.id === 'app.licenseKey') {
        return tString(
          hasExistingLicense ? 'commands.appLicenseKey.seeDetails.label' : 'commands.appLicenseKey.enterKey.label',
        )
      }
      return tString(nameKey)
    },
  } as Command
  if (descriptionKey !== undefined) {
    Object.defineProperty(cmd, 'description', { enumerable: true, get: () => tString(descriptionKey) })
  }
  return cmd
}

/**
 * Every command, with copy resolved through the catalog. A getter-backed
 * `Command[]` (not `as const`), so `getPaletteCommands()` and the shortcuts
 * conflict detector keep a mutable `Command[]`; the names themselves come from
 * the catalog, so there's nothing to mutate in place anymore (the license name
 * is driven by `hasExistingLicense` via `updateLicenseCommandName`).
 */
export const commands: Command[] = commandSources.map(resolveCommand)

/** Get all commands that should appear in the command palette */
export function getPaletteCommands(): Command[] {
  return commands.filter((c) => c.showInPalette)
}

/**
 * Update the license command name based on whether a license exists. Keeps the
 * command palette in sync with the native menu label. The `app.licenseKey`
 * command's `name` getter reads `hasExistingLicense` live, so flipping this flag
 * re-resolves the catalog label on the next read.
 */
export function updateLicenseCommandName(hasLicense: boolean): void {
  hasExistingLicense = hasLicense
}
