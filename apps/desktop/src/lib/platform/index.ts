/**
 * Which OS this window runs on, as the backend says, read synchronously.
 *
 * The backend injects it into every webview before any script runs (`tauri_builder.rs`'s
 * `platform_plugin`), so a module may read it at load time. A context with no backend to ask (a
 * plain browser, Vitest) gets {@link FALLBACK_PLATFORM}. Mechanics and the browser demo's answer:
 * `DETAILS.md`.
 */

export type Platform = 'macos' | 'linux' | 'windows'

/** The global the backend's init script defines. Read-only in the app; tests set it via `withPlatform()`. */
export const PLATFORM_GLOBAL = '__CMDR_PLATFORM__'

/** What a context without a backend runs as. Not macOS, so no Mac-only surface shows by accident. */
export const FALLBACK_PLATFORM: Platform = 'linux'

function isPlatform(value: unknown): value is Platform {
  return value === 'macos' || value === 'linux' || value === 'windows'
}

/** The OS this window runs on. */
export function getPlatform(): Platform {
  const injected = (globalThis as Record<string, unknown>)[PLATFORM_GLOBAL]
  return isPlatform(injected) ? injected : FALLBACK_PLATFORM
}

export function isMacOS(): boolean {
  return getPlatform() === 'macos'
}

/**
 * Writes `data-platform` on `<html>`, so CSS can follow the platform (`app.css` § Window chrome).
 * Called once per window, before the first route renders (`hooks.client.ts`).
 */
export function applyPlatformAttribute(root: HTMLElement = document.documentElement): void {
  root.dataset.platform = getPlatform()
}
