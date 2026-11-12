/**
 * The file viewer window's single-key commands (`Viewer window`). Pure data; the viewer's
 * keydown asks `eventMatchesCommand` for each (`routes/viewer/viewer-keyboard.ts`,
 * `viewer-view-mode.ts`, `MediaImageView.svelte`). Unlike the operation queue's keys these are
 * NOT fixed: they're bare letters and digits, so WCAG 2.1.4 wants them remappable or able to
 * be turned off. The viewer has no store access, so it reads rebindings through the typed
 * `get_restricted_window_shortcuts` snapshot plus the `shortcuts:changed` event
 * (`initializeShortcuts({ restrictedWindow: true })`). See `../command-registry.ts` for how
 * the scope arrays are concatenated.
 */
import type { CommandSource } from '../types'
import { sameOnAll } from '../platform-shortcuts'
import type { CommandId } from '../command-ids'
import { BLOCKED_BY_DIALOGS } from '../while-dialog-open'

/**
 * Every viewer key command. They never ride the main window's dispatch spine (the viewer is a
 * window of its own), so `command-handlers/types.ts` exempts exactly this list.
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
]
