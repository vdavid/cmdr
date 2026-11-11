/**
 * Which OS this window runs on, and what that OS lets Cmdr offer, as the backend says, read
 * synchronously.
 *
 * The backend injects both into every webview before any script runs (the Rust `platform`
 * module's `frontend.rs`), so a module may read them at load time. A context with no backend to
 * ask (a plain browser, Vitest) gets {@link FALLBACK_PLATFORM} and {@link NO_CAPABILITIES}.
 * Mechanics and the browser demo's answer: `DETAILS.md`.
 */
import type { PlatformCapabilities } from '#lib/ipc/bindings.js'

export type { PlatformCapabilities }

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

/** One thing this OS lets Cmdr offer, named after the UI that asks (`PlatformCapabilities`). */
export type Capability = keyof PlatformCapabilities

/** The global the backend's init script defines beside the platform. Tests set it via `withCapabilities()`. */
export const CAPABILITIES_GLOBAL = '__CMDR_CAPABILITIES__'

/**
 * What a context without a backend can offer: nothing. Like {@link FALLBACK_PLATFORM}, so no
 * OS-backed surface shows where nothing can answer it.
 */
export const NO_CAPABILITIES: Readonly<PlatformCapabilities> = Object.freeze({
  quickLook: false,
  getInfo: false,
  openTerminalHere: false,
  textEditorChoice: false,
  cloudFileActions: false,
  colorTags: false,
  servicesMenu: false,
  fullDiskAccess: false,
  finderExtension: false,
  revealHandler: false,
  dockPin: false,
  cliCommand: false,
  overlayTitleBar: false,
  imageIndexing: false,
})

/**
 * What this OS lets Cmdr offer, as the backend says. Each flag reads `true` only where the injected
 * value says exactly `true`, so a missing global or a missing field hides its surface.
 */
export function platformCapabilities(): PlatformCapabilities {
  const injected = (globalThis as Record<string, unknown>)[CAPABILITIES_GLOBAL]
  if (typeof injected !== 'object' || injected === null) return { ...NO_CAPABILITIES }
  const answered = injected as Partial<Record<Capability, unknown>>
  const capabilities = { ...NO_CAPABILITIES }
  for (const capability of Object.keys(NO_CAPABILITIES) as Capability[]) {
    capabilities[capability] = answered[capability] === true
  }
  return capabilities
}

/** Whether this OS lets Cmdr offer `capability`. Ask this, ❌ not `isMacOS()`, to show or hide a feature. */
export function hasCapability(capability: Capability): boolean {
  return platformCapabilities()[capability]
}

/**
 * Writes `data-platform` on `<html>`, and `data-overlay-title-bar` where the page draws the title
 * bar, so CSS can follow both (`app.css` § Window chrome). Called once per window, before the first
 * route renders (`hooks.client.ts`).
 */
export function applyPlatformAttributes(root: HTMLElement = document.documentElement): void {
  root.dataset.platform = getPlatform()
  if (hasCapability('overlayTitleBar')) root.dataset.overlayTitleBar = ''
  else delete root.dataset.overlayTitleBar
}
