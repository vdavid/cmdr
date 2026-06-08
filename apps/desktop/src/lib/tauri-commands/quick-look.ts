// Quick Look event listeners. Typed `on*` wrappers over the `tauri-specta`
// `events.quickLookKey` / `events.quickLookClosed` helpers. The panel forwards
// keys it didn't want via `quick-look-key`; it fires `quick-look-closed` when
// it leaves the screen.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type QuickLookKeyEvent } from '$lib/ipc/bindings'

/**
 * A keyboard event the Quick Look panel didn't want. The payload mirrors a DOM
 * `KeyboardEvent` (`key`, `code`, `shiftKey`, `metaKey`, `altKey`, `ctrlKey`)
 * so the FE can re-dispatch through the focused pane's navigation primitives.
 */
export function onQuickLookKey(handler: (payload: QuickLookKeyEvent) => void): Promise<UnlistenFn> {
  return events.quickLookKey.listen((event) => {
    handler(event.payload)
  })
}

/** The Quick Look panel left the screen (our `orderOut:`, the ✕ button, or Esc). */
export function onQuickLookClosed(handler: () => void): Promise<UnlistenFn> {
  return events.quickLookClosed.listen(() => {
    handler()
  })
}
