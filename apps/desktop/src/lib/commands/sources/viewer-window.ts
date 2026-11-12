/**
 * The file viewer window's single-key commands (`Viewer window`). Pure data; the viewer's
 * keydown asks `eventMatchesCommand` for each (`routes/viewer/viewer-keyboard.ts`,
 * `viewer-view-mode.ts`, `MediaImageView.svelte`). Unlike the operation queue's keys the single
 * keys are NOT fixed: they're bare letters and digits, so WCAG 2.1.4 wants them remappable or
 * able to be turned off. The search bar's three chords are fixed. The viewer has no store access, so it reads rebindings through the typed
 * `get_restricted_window_shortcuts` snapshot plus the `shortcuts:changed` event
 * (`initializeShortcuts({ restrictedWindow: true })`). See `../command-registry.ts` for how
 * the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { cmdOrCtrl, sameOnAll } from '../platform-shortcuts'
import type { CommandId } from '../command-ids'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/**
 * Every remappable viewer key command. They never ride the main window's dispatch spine (the
 * viewer is a window of its own), so `command-handlers/types.ts` exempts exactly this list. The
 * search bar's fixed chords are exempt through `FIXED_KEY_COMMAND_IDS` instead.
 */
export const VIEWER_KEY_COMMAND_IDS = [
  'viewer.toggleWordWrap',
  'viewer.toggleTail',
  'viewer.viewAsMedia',
  'viewer.viewAsText',
  'viewer.viewAsBinary',
  'viewer.viewAsHex',
  'viewer.zoomIn',
  'viewer.zoomOut',
  'viewer.toggleImageZoom',
  'viewer.goBack',
  'viewer.goForward',
] as const satisfies readonly CommandId[]

/** Every entry shares these: the viewer answers them itself, never the dispatch spine. */
const viewerKey = {
  scope: 'Viewer window',
  showInPalette: false,
  whileDialogOpen: BLOCKED_BY_DIALOGS,
} as const

export const viewerWindowCommands: CommandSource[] = [
  {
    id: 'viewer.toggleWordWrap',
    nameKey: 'commands.viewerToggleWordWrap.label',
    shortcuts: sameOnAll('W'),
    ...viewerKey,
  },
  { id: 'viewer.toggleTail', nameKey: 'commands.viewerToggleTail.label', shortcuts: sameOnAll('F'), ...viewerKey },
  // The view modes on Total Commander's Lister keys. 0 also fits a zoomed image back to the
  // window: it's the "media" key, and that's the view it already shows.
  { id: 'viewer.viewAsMedia', nameKey: 'commands.viewerViewAsMedia.label', shortcuts: sameOnAll('0'), ...viewerKey },
  { id: 'viewer.viewAsText', nameKey: 'commands.viewerViewAsText.label', shortcuts: sameOnAll('1'), ...viewerKey },
  { id: 'viewer.viewAsBinary', nameKey: 'commands.viewerViewAsBinary.label', shortcuts: sameOnAll('2'), ...viewerKey },
  { id: 'viewer.viewAsHex', nameKey: 'commands.viewerViewAsHex.label', shortcuts: sameOnAll('3'), ...viewerKey },
  // `+` and `=` share a key on US layouts, so both zoom in, Shift held or not.
  { id: 'viewer.zoomIn', nameKey: 'commands.viewerZoomIn.label', shortcuts: sameOnAll('+', '='), ...viewerKey },
  { id: 'viewer.zoomOut', nameKey: 'commands.viewerZoomOut.label', shortcuts: sameOnAll('-'), ...viewerKey },
  {
    id: 'viewer.toggleImageZoom',
    nameKey: 'commands.viewerToggleImageZoom.label',
    shortcuts: sameOnAll('Enter', 'Space'),
    ...viewerKey,
  },
  // Back / Forward through the documents a Markdown link led to, on the browser's and the
  // main window's keys. The viewer bar's Go menu reaches the same two steps.
  {
    id: 'viewer.goBack',
    nameKey: 'commands.viewerGoBack.label',
    shortcuts: { macos: ['⌘[', '⌥←'], windows: ['Alt+←'], linux: ['Alt+←'] },
    ...viewerKey,
  },
  {
    id: 'viewer.goForward',
    nameKey: 'commands.viewerGoForward.label',
    shortcuts: { macos: ['⌘]', '⌥→'], windows: ['Alt+→'], linux: ['Alt+→'] },
    ...viewerKey,
  },
  // The search bar's chords: fixed (`FIXED_KEY_COMMAND_IDS`), and answered while the search
  // input has focus. The toggles are VS Code's find-widget keys: ⌘⌥R / ⌘⌥C on macOS, Alt+R /
  // Alt+C elsewhere, since `Ctrl+Alt+<letter>` is AltGr typing there (`&` on Hungarian AltGr+C).
  {
    id: 'viewer.search',
    nameKey: 'commands.viewerSearch.label',
    shortcuts: cmdOrCtrl('⌘F'),
    ...viewerKey,
    fixedKey: true,
  },
  {
    id: 'viewer.toggleSearchRegex',
    nameKey: 'commands.viewerToggleSearchRegex.label',
    shortcuts: { macos: ['⌘⌥R'], windows: ['Alt+R'], linux: ['Alt+R'] },
    ...viewerKey,
    fixedKey: true,
  },
  {
    id: 'viewer.toggleSearchCase',
    nameKey: 'commands.viewerToggleSearchCase.label',
    shortcuts: { macos: ['⌘⌥C'], windows: ['Alt+C'], linux: ['Alt+C'] },
    ...viewerKey,
    fixedKey: true,
  },
]
