// Appearance / system-environment wrappers: typed `on*` event listeners over the
// `tauri-specta` `events.*` helpers, plus the one-shot reads of the current OS
// appearance/accessibility state (accent color, reduce-transparency, text-size
// multiplier, localized system strings).

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type AccentColorChanged,
  type LocalizedSystemStrings,
  type ReduceTransparencyChanged,
  type SystemTextSizeChanged,
  type OsLocales,
  type OsLocalesChanged,
} from '$lib/ipc/bindings'

/** Reads the current OS accent color as `#rrggbb`. */
export function getAccentColor(): Promise<string> {
  return commands.getAccentColor()
}

/** Reads the current macOS "reduce transparency" (Accessibility > Display) state. */
export function getShouldReduceTransparency(): Promise<boolean> {
  return commands.getShouldReduceTransparency()
}

/** Reads the current macOS Accessibility text-size multiplier (1.0 = default). */
export function getSystemTextSizeMultiplier(): Promise<number> {
  return commands.getSystemTextSizeMultiplier()
}

/** Reads OS-localized system strings (folder names, menu labels) for the app locale. */
export function getLocalizedSystemStrings(): Promise<LocalizedSystemStrings> {
  return commands.getLocalizedSystemStrings()
}

/**
 * Reads both OS locale answers: the UI language the preference list points at
 * (the first catalog we ship that the user can read), and the tag whose
 * conventions dates and numbers follow, composed from the OS's language and
 * region. Either half is `null` when the OS has no answer, which is both halves
 * off macOS, where the webview's own default stands.
 */
export function getOsLocales(): Promise<OsLocales> {
  return commands.getOsLocales()
}

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

/**
 * Subscribes to a live OS locale change: the user reordered their macOS
 * language preferences, or moved their region, and one of the two answers
 * actually moved. The payload's `locales` is the fresh pair, the same value
 * `getOsLocales()` would return now. Backend-deduplicated, so every event
 * carries a real change (`src-tauri/src/intl/live_locale.rs`).
 */
export function onOsLocalesChanged(handler: (payload: OsLocalesChanged) => void): Promise<UnlistenFn> {
  return events.osLocalesChanged.listen((event) => {
    handler(event.payload)
  })
}

/**
 * Subscribes to macOS "reduce transparency" (Accessibility > Display) changes.
 * The payload's `reduce` is the new value. Drives the `reduce-transparency`
 * class on `<html>` (see `$lib/reduce-transparency`).
 */
export function onReduceTransparencyChanged(
  handler: (payload: ReduceTransparencyChanged) => void,
): Promise<UnlistenFn> {
  return events.reduceTransparencyChanged.listen((event) => {
    handler(event.payload)
  })
}
