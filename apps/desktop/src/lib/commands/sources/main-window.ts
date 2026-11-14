/**
 * Main window command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, concatShortcuts, sameOnAll } from '../platform-shortcuts'
import { getBadgeStatus } from '#lib/feature-status.js'
import { BLOCKED_BY_DIALOGS, runsOverDialogs } from '../while-dialog-open'

// Zoom scales the whole app, dialog included, and touches no pane.
const ZOOM_SCALES_APP = runsOverDialogs(
  'Changes the text size app-wide: it scales the dialog too, and touches no pane.',
)

export const mainWindowCommands: CommandSource[] = [
  // ============================================================================
  // Main window - Search
  // ============================================================================
  {
    id: 'search.open',
    nameKey: 'commands.searchOpen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: concatShortcuts(cmdOrCtrl('⌘F'), sameOnAll('⌥F7')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    // ⌘⇧G is Finder's "Go to Folder…"; ⌘G stays first, so it's the one the menu shows.
    shortcuts: cmdOrCtrl('⌘G', '⌘⇧G'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.favoritesAdd.description',
    keywords: ['bookmark', 'favorite', 'pin', 'shortcut'],
  },
  {
    // ⌃D, what Total Commander and Double Commander bind for the same list. It's free:
    // ⌃Tab / ⌃⇧Tab are the only other Control defaults in the registry, so Duplicate keeps
    // ⌘D. The keys the menu itself answers live in the `Main window/Favorites menu` scope
    // (`browsers.ts`), which is where the digit rows are documented.
    id: 'favorites.open',
    nameKey: 'commands.favoritesOpen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃D'], windows: ['Ctrl+D'], linux: ['Ctrl+D'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.favoritesOpen.description',
    keywords: ['bookmark', 'hotlist', 'favorite'],
  },

  // ============================================================================
  // Main window - Downloads
  // ============================================================================
  {
    id: 'downloads.goToLatest',
    nameKey: 'commands.downloadsGoToLatest.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘J'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    shortcuts: cmdOrCtrl('⌘⇧.'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Total Commander's Alt+Shift+Enter: the size of every folder in the pane
    // whose exact size isn't known yet. Esc stops the calculation.
    id: 'view.calculateFolderSizes',
    nameKey: 'commands.viewCalculateFolderSizes.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll('⌥⇧Enter'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.viewCalculateFolderSizes.description',
  },
  {
    id: 'view.briefMode',
    nameKey: 'commands.viewBriefMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌘2'], windows: ['Ctrl+F1', 'Ctrl+2'], linux: ['Ctrl+2', 'Ctrl+F1'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'view.fullMode',
    nameKey: 'commands.viewFullMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌘1'], windows: ['Ctrl+F2', 'Ctrl+1'], linux: ['Ctrl+1', 'Ctrl+F2'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
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
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Main window - Zoom (text size) commands
  // ============================================================================
  {
    id: 'view.zoom.set75',
    nameKey: 'commands.viewZoomSet75.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set100',
    nameKey: 'commands.viewZoomSet100.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘0'),
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set125',
    nameKey: 'commands.viewZoomSet125.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set150',
    nameKey: 'commands.viewZoomSet150.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.in',
    nameKey: 'commands.viewZoomIn.label',
    scope: 'Main window',
    // Both fire from the webview's own keydown: `+` is what Shift+= types, `=` the
    // unshifted key. The menu row spells it `Cmd+Equal` (`crates/cmdr-menu/src/bar.rs`), because
    // muda parses an accelerator into a PHYSICAL key and has no `Plus` token, so
    // `Cmd+Plus` registered nothing at all and Zoom in showed no key.
    shortcuts: cmdOrCtrl('⌘+', '⌘='),
    showInPalette: true,
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.out',
    nameKey: 'commands.viewZoomOut.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘-'),
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.toggleFullScreen',
    nameKey: 'commands.viewToggleFullScreen.label',
    scope: 'Main window',
    showInPalette: true,
    // `F11` is Explorer's and every browser's key. macOS binds nothing: AppKit's own View menu item
    // keeps its key there (`docs/notes/windows-port/keymap-decisions.md`).
    shortcuts: { macos: [], windows: ['F11'], linux: ['F11'] },
    whileDialogOpen: runsOverDialogs('Changes the window, not anything in it, so the dialog stays up and untouched.'),
    needsCapability: 'fullScreen',
  },

  // ============================================================================
  // Main window - Sort commands (also accessible via menu)
  // ============================================================================
  {
    id: 'sort.byName',
    nameKey: 'commands.sortByName.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘3', '⌘F3'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byExtension',
    nameKey: 'commands.sortByExtension.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘4', '⌘F4'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byModified',
    nameKey: 'commands.sortByModified.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘5', '⌘F5'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.bySize',
    nameKey: 'commands.sortBySize.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘6', '⌘F6'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byCreated',
    nameKey: 'commands.sortByCreated.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byAdded',
    nameKey: 'commands.sortByAdded.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.ascending',
    nameKey: 'commands.sortAscending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.descending',
    nameKey: 'commands.sortDescending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.toggleOrder',
    nameKey: 'commands.sortToggleOrder.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  // Per-pane sort carrying `{ pane, column, order }`, dispatched by the MCP `sort`
  // tool. Hidden from the palette: the `sort.by*` commands are the user-facing
  // entries; this one targets a specific pane with an explicit order.
  {
    id: 'sort.set',
    nameKey: 'commands.sortSet.label',
    scope: 'Main window',
    showInPalette: false,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Main window - Pane commands
  // ============================================================================
  {
    id: 'pane.switch',
    nameKey: 'commands.paneSwitch.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['Tab'], windows: ['Tab', 'Ctrl+I'], linux: ['Tab', 'Ctrl+I'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.swap',
    nameKey: 'commands.paneSwap.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘U'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Total Commander's "Target = Source": the other pane opens the focused
    // pane's folder. Unlike ⌘→ / ⌘←, the cursor never refines it.
    id: 'pane.clone',
    nameKey: 'commands.paneClone.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌘⇧C'], windows: ['Ctrl+Shift+M'], linux: ['Ctrl+Shift+M'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneClone.description',
  },
  {
    // The keyboard way into pane chrome (`pane/pane-chrome.ts`) without giving up `Tab`,
    // which stays the pane switch. ⌃F5 mirrors macOS's "Move focus to the window toolbar".
    // Off macOS, ⌃F5 would be Ctrl+F5, which is ⌘F5's key there (sort by modified): Ctrl+Shift+F5.
    id: 'pane.focusControls',
    nameKey: 'commands.paneFocusControls.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃F5'], windows: ['Ctrl+Shift+F5'], linux: ['Ctrl+Shift+F5'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneFocusControls.description',
  },
  {
    // The keyboard way to the toasts (`#lib/ui/toast/toast-focus.ts`), whose buttons (Undo trash)
    // Tab can't reach, since Tab is the pane switch. ⌃F6 mirrors macOS's "Move focus to the
    // floating window"; off macOS, ⌃F6 would be ⌘F6's Ctrl+F6, so it takes Ctrl+Shift+F6 like `pane.focusControls`.
    id: 'app.focusNotifications',
    nameKey: 'commands.appFocusNotifications.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃F6'], windows: ['Ctrl+Shift+F6'], linux: ['Ctrl+Shift+F6'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.appFocusNotifications.description',
    keywords: ['toast', 'notification', 'notifications', 'undo', 'keyboard', 'focus'],
  },
  {
    id: 'pane.leftVolumeChooser',
    nameKey: 'commands.paneLeftVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌥F1'], windows: ['Alt+F1'], linux: ['Alt+F1', 'Ctrl+Shift+F1', 'Alt+1'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.rightVolumeChooser',
    nameKey: 'commands.paneRightVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌥F2'], windows: ['Alt+F2'], linux: ['Alt+F2', 'Ctrl+Shift+F2', 'Alt+2'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.copyPathLeftToRight',
    nameKey: 'commands.paneCopyPathLeftToRight.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘→'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneCopyPathLeftToRight.description',
  },
  {
    id: 'pane.copyPathRightToLeft',
    nameKey: 'commands.paneCopyPathRightToLeft.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘←'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneCopyPathRightToLeft.description',
  },
  {
    // The only key that re-reads a directory. Scoped to the whole main window, not
    // the file list: a pane shows either the file list or the network browser, and
    // ⌘R refreshes whichever is up (`refreshPane` in `pane-commands.ts` routes it).
    // That's also why `network.refresh` carries no ⌘R of its own: one combo, one winner.
    id: 'pane.refresh',
    nameKey: 'commands.paneRefresh.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘R'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Held or tapped quickly, the step grows; heading through the middle, it stops
    // at 50% until a fresh press (`pane-split.ts`). ⌃← / ⌃→ alone are macOS Spaces. Off macOS,
    // ⌃⌥← would be the tab keys' Ctrl+Alt+←, so it takes Ctrl+Alt+Shift there.
    id: 'pane.moveDividerLeft',
    nameKey: 'commands.paneMoveDividerLeft.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃⌥←'], windows: ['Alt+Shift+←'], linux: ['Alt+Shift+←'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.moveDividerRight',
    nameKey: 'commands.paneMoveDividerRight.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃⌥→'], windows: ['Alt+Shift+→'], linux: ['Alt+Shift+→'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // The divider's double-click, from the keyboard.
    id: 'pane.resetDivider',
    nameKey: 'commands.paneResetDivider.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Main window - Tab commands
  // ============================================================================
  {
    id: 'tab.new',
    nameKey: 'commands.tabNew.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘T'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.close',
    nameKey: 'commands.tabClose.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘W'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.reopen',
    nameKey: 'commands.tabReopen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘⇧T'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // ⌘⌥→ rides along with ⌃Tab: it's what Chrome, Safari, and VS Code bind, and it's
    // a one-hand reach for a key people press all day. ⌘→ alone is taken by
    // `pane.copyPathLeftToRight`. ⌃Tab stays FIRST: the menu shows `shortcuts[0]`.
    id: 'tab.next',
    nameKey: 'commands.tabNext.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌃Tab', '⌘⌥→'], windows: ['Ctrl+Tab', 'Ctrl+Alt+→'], linux: ['Ctrl+Tab'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.prev',
    nameKey: 'commands.tabPrev.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: {
      macos: ['⌃⇧Tab', '⌘⌥←'],
      windows: ['Ctrl+Shift+Tab', 'Ctrl+Alt+←'],
      linux: ['Ctrl+Shift+Tab'],
    },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.togglePin',
    nameKey: 'commands.tabTogglePin.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.closeOthers',
    nameKey: 'commands.tabCloseOthers.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // The keyboard's way to do what a tab drag does. ⌘⌃⇧← / → keeps the arrows `tab.prev` /
    // `tab.next` use for walking the bar (⌘⌥← / →), and nothing else in the registry or macOS holds it.
    id: 'tab.moveLeft',
    nameKey: 'commands.tabMoveLeft.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌘⌃⇧←'], windows: ['Ctrl+Shift+PageUp'], linux: ['Ctrl+Shift+PageUp'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.moveRight',
    nameKey: 'commands.tabMoveRight.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: { macos: ['⌘⌃⇧→'], windows: ['Ctrl+Shift+PageDown'], linux: ['Ctrl+Shift+PageDown'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // No default key: no free combo says "the other pane" the way the arrows say "along the
    // bar". The palette reaches it, and Settings can bind one.
    id: 'tab.moveToOtherPane',
    nameKey: 'commands.tabMoveToOtherPane.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: sameOnAll(),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },

  // ============================================================================
  // Main window - Ask Cmdr
  // ============================================================================
  {
    // ⌘N, the chat apps' "New chat". Free here: Cmdr has one main window, so macOS's "New
    // window" meaning has nothing to claim, and new tabs are ⌘T. The search dialog's own ⌘N
    // ("start fresh") only runs while that dialog is up, and this one is blocked by dialogs.
    // Opens the rail when it's closed; `openNewChat` puts the cursor in the prompt box.
    id: 'askCmdr.newChat',
    nameKey: 'commands.askCmdrNewChat.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: cmdOrCtrl('⌘N'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.askCmdrNewChat.description',
    status: getBadgeStatus('ask-cmdr'),
  },

  // ============================================================================
  // Main window/Ask Cmdr - in-chat find (the rail's own keys, while it has focus)
  // ============================================================================
  // Fixed like the viewer's ⌘F: the rail's keydown asks `eventMatchesCommand` for these
  // (`lib/ask-cmdr/ask-cmdr-find.svelte.ts`) and claims the key, so ⌘F and ⌘G keep meaning
  // Search files and Go to path everywhere else. Enter / ⇧Enter step only from the find field.
  {
    id: 'askCmdr.find',
    nameKey: 'commands.askCmdrFind.label',
    scope: 'Main window/Ask Cmdr',
    showInPalette: false,
    shortcuts: cmdOrCtrl('⌘F'),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'askCmdr.findNext',
    nameKey: 'commands.askCmdrFindNext.label',
    scope: 'Main window/Ask Cmdr',
    showInPalette: false,
    shortcuts: concatShortcuts(cmdOrCtrl('⌘G'), sameOnAll('Enter')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
  {
    id: 'askCmdr.findPrevious',
    nameKey: 'commands.askCmdrFindPrevious.label',
    scope: 'Main window/Ask Cmdr',
    showInPalette: false,
    shortcuts: concatShortcuts(cmdOrCtrl('⌘⇧G'), sameOnAll('⇧Enter')),
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },

  // ============================================================================
  // The "Tell me about this" card's Stop, fixed: the card's keydown asks `eventMatchesCommand`
  // for it (`lib/about-this/AboutThisDialog.svelte`) and claims it whenever the card is up, so ⌥S
  // never types `ß` (nor Alt+S reaches a menu mnemonic: the dialog masks them on Linux). It stops
  // only a take that's still writing.
  {
    id: 'aboutThis.stopVerdict',
    nameKey: 'commands.aboutThisStopVerdict.label',
    scope: 'Main window/Tell me about this',
    showInPalette: false,
    shortcuts: { macos: ['⌥S'], windows: ['Alt+S'], linux: ['Alt+S'] },
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    fixedKey: true,
  },
]
