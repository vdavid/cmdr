/**
 * Main window command sources. Pure data (i18n message keys, not English); see
 * `../command-registry.ts` for how the scope arrays are concatenated into the
 * registry and resolved into `Command`s.
 */
import type { CommandSource } from '../types'
import { getBadgeStatus } from '$lib/feature-status'

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
]
