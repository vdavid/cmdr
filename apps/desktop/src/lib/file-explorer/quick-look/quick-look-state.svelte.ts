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
 *   `explorerRef.routePanelKey(payload)`. Shift+Space is a special case — we
 *   close the panel directly instead of routing, because the menu accelerator
 *   path isn't reliable while the panel is key (it may consume the keydown
 *   before AppKit's menu dispatcher sees it).
 *
 * The state object is a module-level singleton (`quickLookState`). The
 * command dispatcher reads `isOpen` to choose between `quickLookOpen` and
 * `quickLookClose`, and the close listener flips it. Nothing in the M1
 * surface needs cross-pane fanout, so a single shared instance is fine.
 *
 * See `apps/desktop/src-tauri/src/quick_look/CLAUDE.md` (will be added in M4)
 * for the native side.
 *
 * v1 shows the cursor item only; multi-selection "carousel" mode (Finder-style
 * arrow-keys-between-selected) is deliberately not implemented in v1 — see
 * the matching note at the `numberOfPreviewItemsInPreviewPanel:` site in
 * `src-tauri/src/quick_look/controller.rs`.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import { quickLookClose } from '$lib/tauri-commands'

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
 * Timestamp of the last `file.quickLook` dispatch (`performance.now()`).
 *
 * Every Shift+Space keypress fires `file.quickLook` *twice*: once via the
 * AppKit menu accelerator (`on_menu_event` → `execute-command` Tauri event)
 * and once via WKWebView's keydown → centralized JS shortcut dispatch. The
 * second fire would re-toggle the panel, so the dispatcher arms this guard on
 * entry and swallows any second fire inside the 200 ms window. The window is
 * comfortably below human "second press" cadence (~250 ms) and above any
 * plausible AppKit→IPC round-trip.
 *
 * The Shift+Space-from-panel close path (in the `quick-look-key` listener
 * below) also arms this guard, for the same reason: when the panel is key,
 * AppKit can still leak a delayed menu-accelerator fire of the same keystroke
 * to the dispatcher, which would re-open the just-closed panel.
 */
let lastQuickLookDispatchAt = Number.NEGATIVE_INFINITY
const QUICK_LOOK_DISPATCH_GRACE_MS = 200

export function quickLookDispatchGuardJustFired(): boolean {
  return performance.now() - lastQuickLookDispatchAt < QUICK_LOOK_DISPATCH_GRACE_MS
}

export function armQuickLookDispatchGuard(): void {
  lastQuickLookDispatchAt = performance.now()
}

/**
 * M3: close the panel because the focused pane went into an error state
 * (volume unmounted, listing failed, etc.). Sitting on a stale path while
 * the underlying volume is gone is worse than just dismissing the preview.
 *
 * Idempotent: no-op when already closed. Flips `isOpen` synchronously so any
 * subsequent dispatch sees the closed state, then fires the IPC. The
 * close-event observer in Rust will fire `quick-look-closed` once AppKit has
 * animated the panel out, but we don't depend on it: the synchronous flip is
 * what guarantees the next Shift+Space opens again instead of trying to
 * close-already-closed.
 */
export function closeFromPaneError(): void {
  if (!quickLookState.isOpen) return
  quickLookState.isOpen = false
  void quickLookClose()
}

let attached = false

/**
 * Wire up the two Tauri event listeners. Idempotent — calling twice attaches
 * once. Pass a getter for the explorer ref (Svelte 5 component refs can be
 * `undefined` during construction, so we resolve lazily on each event).
 *
 * Returns an `UnlistenFn`-style cleanup that detaches both listeners.
 */
export async function initQuickLookListeners(getExplorer: () => ExplorerAPI | undefined): Promise<UnlistenFn> {
  if (attached) {
    // Belt and braces — the +page lifecycle should only call us once, but
    // returning a no-op keeps the API safe against double-attach during HMR.
    return () => {}
  }
  attached = true

  const unlistenClosed = await listen<null>('quick-look-closed', () => {
    quickLookState.isOpen = false
  })

  const unlistenKey = await listen<QuickLookKeyEventPayload>('quick-look-key', (event) => {
    const payload = event.payload
    // Shift+Space closes — the panel is key, so the AppKit menu accelerator
    // can't be relied on. We close synchronously and let the close-event
    // listener flip `isOpen` back when the panel finishes animating out.
    if (payload.shiftKey && (payload.key === ' ' || payload.code === 'Space')) {
      armQuickLookDispatchGuard()
      // Flip `isOpen` immediately so any synchronous follow-up dispatch
      // (rare AppKit menu-accelerator race) sees the closed state.
      quickLookState.isOpen = false
      void quickLookClose()
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
