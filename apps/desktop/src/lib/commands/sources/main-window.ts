/**
 * Main window command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'
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
    shortcuts: ['⌘F', '⌥F7'],
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
    shortcuts: ['⌘G'],
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
    shortcuts: [],
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
    shortcuts: ['⌃D'],
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
    shortcuts: ['⌘J'],
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
    shortcuts: ['⌘⇧.'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Total Commander's Alt+Shift+Enter: the size of every folder in the pane
    // whose exact size isn't known yet. Esc stops the calculation.
    id: 'view.calculateFolderSizes',
    nameKey: 'commands.viewCalculateFolderSizes.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌥⇧Enter'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.viewCalculateFolderSizes.description',
  },
  {
    id: 'view.briefMode',
    nameKey: 'commands.viewBriefMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘2'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'view.fullMode',
    nameKey: 'commands.viewFullMode.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘1'],
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
    shortcuts: [],
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
    shortcuts: [],
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set100',
    nameKey: 'commands.viewZoomSet100.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘0'],
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set125',
    nameKey: 'commands.viewZoomSet125.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.set150',
    nameKey: 'commands.viewZoomSet150.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.in',
    nameKey: 'commands.viewZoomIn.label',
    scope: 'Main window',
    // Both fire from the webview's own keydown: `+` is what Shift+= types, `=` the
    // unshifted key. The menu row spells it `Cmd+Equal` (`menu/menu_bar.rs`), because
    // muda parses an accelerator into a PHYSICAL key and has no `Plus` token, so
    // `Cmd+Plus` registered nothing at all and Zoom in showed no key.
    shortcuts: ['⌘+', '⌘='],
    showInPalette: true,
    whileDialogOpen: ZOOM_SCALES_APP,
  },
  {
    id: 'view.zoom.out',
    nameKey: 'commands.viewZoomOut.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘-'],
    whileDialogOpen: ZOOM_SCALES_APP,
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
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byExtension',
    nameKey: 'commands.sortByExtension.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘4', '⌘F4'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byModified',
    nameKey: 'commands.sortByModified.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘5', '⌘F5'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.bySize',
    nameKey: 'commands.sortBySize.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘6', '⌘F6'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.byCreated',
    nameKey: 'commands.sortByCreated.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.ascending',
    nameKey: 'commands.sortAscending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.descending',
    nameKey: 'commands.sortDescending.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'sort.toggleOrder',
    nameKey: 'commands.sortToggleOrder.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
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
    shortcuts: [],
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
    shortcuts: ['Tab'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.swap',
    nameKey: 'commands.paneSwap.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘U'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    // Total Commander's "Target = Source": the other pane opens the focused
    // pane's folder. Unlike ⌘→ / ⌘←, the cursor never refines it.
    id: 'pane.clone',
    nameKey: 'commands.paneClone.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘⇧C'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneClone.description',
  },
  {
    id: 'pane.leftVolumeChooser',
    nameKey: 'commands.paneLeftVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌥F1'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.rightVolumeChooser',
    nameKey: 'commands.paneRightVolumeChooser.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌥F2'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'pane.copyPathLeftToRight',
    nameKey: 'commands.paneCopyPathLeftToRight.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘→'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
    descriptionKey: 'commands.paneCopyPathLeftToRight.description',
  },
  {
    id: 'pane.copyPathRightToLeft',
    nameKey: 'commands.paneCopyPathRightToLeft.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘←'],
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
    shortcuts: ['⌘R'],
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
    shortcuts: ['⌘T'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.close',
    nameKey: 'commands.tabClose.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘W'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.reopen',
    nameKey: 'commands.tabReopen.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘⇧T'],
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
    shortcuts: ['⌃Tab', '⌘⌥→'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.prev',
    nameKey: 'commands.tabPrev.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌃⇧Tab', '⌘⌥←'],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.togglePin',
    nameKey: 'commands.tabTogglePin.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
  {
    id: 'tab.closeOthers',
    nameKey: 'commands.tabCloseOthers.label',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: [],
    whileDialogOpen: BLOCKED_BY_DIALOGS,
  },
]
