/**
 * Frontend state and event wiring for native Quick Look (macOS).
 *
 * Two listeners, both attached once when the main window mounts:
 *
 * - `quick-look-closed`: backend tells us the panel left the screen. Flip
 *   `isOpen` back to `false` so the next Shift+Space opens (instead of trying
 *   to close again).
 * - `quick-look-key`: backend forwards a key event the panel didn't want to
 *   handle. We route it through the focused pane's navigation primitives via
 *   `explorerRef.routePanelKey(payload)`. The Quick Look key goes to the
 *   keyboard dispatcher instead (so the central dedup sees it), and plain
 *   Space closes the panel.
 * - Escape or plain Space in the main window: a capture listener closes the
 *   panel while the main window still has key focus (the panel is opening, or
 *   a click moved focus back), so Space never toggles the selection instead.
 *
 * The state object is a module-level singleton (`quickLookState`). The
 * command dispatcher reads `isOpen` to choose between `quickLookOpen` and
 * `quickLookClose`, and the close listener flips it. Nothing here needs
 * cross-pane fanout, so a single shared instance is fine.
 *
 * See `apps/desktop/src-tauri/src/quick_look/CLAUDE.md` for the native side.
 *
 * v1 shows the cursor item only; multi-selection "carousel" mode (Finder-style
 * arrow-keys-between-selected) is deliberately not implemented in v1 — see
 * the matching note at the `numberOfPreviewItemsInPreviewPanel:` site in
 * `src-tauri/src/quick_look/controller.rs`.
 */

import { type UnlistenFn } from '@tauri-apps/api/event'

import { onQuickLookClosed, onQuickLookKey, quickLookClose } from '$lib/tauri-commands'
import { eventMatchesCommand } from '$lib/shortcuts'
import { isTextInputFocused } from '$lib/utils/text-input-focus'

import type { ExplorerAPI } from '../../../routes/(main)/explorer-api'

export interface QuickLookKeyEventPayload {
  key: string
  code: string
  shiftKey: boolean
  metaKey: boolean
  altKey: boolean
  ctrlKey: boolean
}

/**
 * Reactive open-state. The dispatcher in `command-dispatch.ts` reads this to
 * decide whether Shift+Space should open or close, and the `quick-look-closed`
 * event listener flips it back to `false` when the panel goes away.
 */
export const quickLookState = $state({ isOpen: false })

/**
 * Close the panel because the focused pane went into an error state
 * (volume unmounted, listing failed, etc.). Sitting on a stale path while
 * the underlying volume is gone is worse than just dismissing the preview.
 *
 * Idempotent: no-op when already closed. Flips `isOpen` synchronously so any
 * subsequent dispatch sees the closed state, then fires the IPC. The
 * close-event observer in Rust will fire `quick-look-closed` once AppKit hides
 * the panel, but we don't depend on it: the synchronous flip is
 * what guarantees the next Shift+Space opens again instead of trying to
 * close-already-closed.
 */
function closeIfOpen(): boolean {
  if (!quickLookState.isOpen) return false
  quickLookState.isOpen = false
  void quickLookClose()
  return true
}

export function closeFromPaneError(): void {
  closeIfOpen()
}

/** Close from a main-window key (`shouldCloseFromMainWindowKey`). */
export function closeFromMainWindowKey(): boolean {
  return closeIfOpen()
}

/** Space with no modifier at all: the key Finder closes Quick Look with. */
function isPlainSpace(e: { code: string; shiftKey: boolean; metaKey: boolean; altKey: boolean; ctrlKey: boolean }) {
  return e.code === 'Space' && !e.shiftKey && !e.metaKey && !e.altKey && !e.ctrlKey
}

/**
 * Whether a main-window keydown closes an open Quick Look: plain Escape, or plain Space, which
 * closes like it does in Finder instead of toggling the selection behind the panel. The main
 * window sees these while the panel is still taking key focus, or after a click moved focus
 * back. A foreground dialog keeps both, and a text field keeps its Space.
 */
export function shouldCloseFromMainWindowKey(
  event: KeyboardEvent,
  dialogs: { dialogOpen: boolean; paletteOpen: boolean },
): boolean {
  if (!quickLookState.isOpen || dialogs.dialogOpen || dialogs.paletteOpen) return false
  if (event.key === 'Escape') return !event.metaKey && !event.ctrlKey && !event.altKey
  return isPlainSpace(event) && !isTextInputFocused()
}

/**
 * Whether this panel-forwarded keypress is the Quick Look gesture (⇧Space by
 * default). Resolved through the command registry, so it follows a rebind of
 * `file.quickLook` AND matches the whole combo: ⌥⇧Space and ⌘⇧Space are other
 * combos and must not dismiss the panel on their way elsewhere.
 *
 * The payload is a plain object from Rust, so we rebuild the `KeyboardEvent` the
 * matcher reads. `code === 'Space'` is the layout-independent fallback for the one
 * key whose `key` value is whitespace.
 */
function isQuickLookCloseKey(payload: QuickLookKeyEventPayload): boolean {
  const event = new KeyboardEvent('keydown', {
    key: payload.code === 'Space' ? ' ' : payload.key,
    code: payload.code,
    shiftKey: payload.shiftKey,
    metaKey: payload.metaKey,
    altKey: payload.altKey,
    ctrlKey: payload.ctrlKey,
  })
  return eventMatchesCommand(event, 'file.quickLook')
}

let attached = false

/**
 * Wire up the two Tauri event listeners. Idempotent — calling twice attaches
 * once. Pass a getter for the explorer ref (Svelte 5 component refs can be
 * `undefined` during construction, so we resolve lazily on each event).
 *
 * Returns an `UnlistenFn`-style cleanup that detaches both listeners.
 */
export async function initQuickLookListeners(
  getExplorer: () => ExplorerAPI | undefined,
  dispatchKeyboard: (commandId: 'file.quickLook') => Promise<void>,
): Promise<UnlistenFn> {
  if (attached) {
    // Belt and braces — the +page lifecycle should only call us once, but
    // returning a no-op keeps the API safe against double-attach during HMR.
    return () => {}
  }
  attached = true

  const unlistenClosed = await onQuickLookClosed(() => {
    quickLookState.isOpen = false
  })

  const unlistenKey = await onQuickLookKey((payload) => {
    // The Quick Look key toggles through the dispatch core like any keypress, so
    // the central keyboard+menu dedup drops the File menu's late duplicate of
    // the same press instead of letting it reopen the panel. The native monitor
    // consumes ⇧Space before it gets here, so this is the path for a rebound
    // Quick Look key.
    if (isQuickLookCloseKey(payload)) {
      void dispatchKeyboard('file.quickLook')
      return
    }
    // Plain Space closes like it does in Finder. It reaches us only if the panel
    // forwards it anyway (the native monitor wasn't installed), and must never
    // reach the pane, where it would toggle the selection.
    if (isPlainSpace(payload)) {
      closeIfOpen()
      return
    }
    // Everything else flows through the focused pane's existing navigation
    // primitives. The explorer ref encodes "which pane is focused" and "which
    // primitive handles ArrowDown vs PageUp"; we keep this listener narrow.
    const explorer = getExplorer()
    explorer?.routePanelKey(payload)
  })

  return () => {
    attached = false
    unlistenClosed()
    unlistenKey()
  }
}
