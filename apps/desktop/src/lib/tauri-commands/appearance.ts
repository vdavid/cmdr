// Appearance / system event listeners. Typed `on*` wrappers over the
// `tauri-specta` `events.accentColorChanged` / `events.systemTextSizeChanged`
// helpers.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events, type AccentColorChanged, type SystemTextSizeChanged } from '$lib/ipc/bindings'

/**
 * Subscribes to OS accent-color (or light/dark appearance) changes. The
 * payload's `hex` is the new accent color as `#rrggbb`.
 */
export function onAccentColorChanged(handler: (payload: AccentColorChanged) => void): Promise<UnlistenFn> {
  return events.accentColorChanged.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to macOS Accessibility text-size changes. The payload's
 * `multiplier` is the new system text-size multiplier (1.0 = default).
 */
export function onSystemTextSizeChanged(handler: (payload: SystemTextSizeChanged) => void): Promise<UnlistenFn> {
  return events.systemTextSizeChanged.listen((event) => {
    handler(event.payload)
  })
}
