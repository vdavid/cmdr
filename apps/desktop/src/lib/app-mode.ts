/**
 * App-mode helper: distinguishes `prod` / `dev` / `e2e` runs so the UI can mark
 * windows visibly (pink title bar in dev, blue in E2E, plain in prod). Dev is
 * read synchronously from Vite (`import.meta.env.DEV`); E2E mode comes from
 * the `CMDR_E2E_MODE` env var the backend exposes via `isE2eMode()`. Resolve
 * once via `initAppMode()` at startup, then `getAppMode()` is sync everywhere.
 *
 * E2E wins over dev when both are true: E2E typically runs against a dev build
 * but we want the E2E indicator to take precedence.
 */
import type { WebviewWindow } from '@tauri-apps/api/webviewWindow'

import { getAppLogger } from '$lib/logging/logger'
import { isE2eMode, orderWindowToBack } from '$lib/tauri-commands'

export type AppMode = 'prod' | 'dev' | 'e2e'

const log = getAppLogger('app-mode')

let cachedMode: AppMode | null = null

/** Resolves the app mode once and caches it. Subsequent calls are no-ops. */
export async function initAppMode(): Promise<AppMode> {
  if (cachedMode != null) return cachedMode
  const e2e = await isE2eMode()
  cachedMode = e2e ? 'e2e' : import.meta.env.DEV ? 'dev' : 'prod'
  return cachedMode
}

/**
 * Returns the cached app mode. Before `initAppMode()` resolves, falls back to
 * dev/prod from `import.meta.env.DEV` so synchronous call sites (window
 * creation, title bar render on first frame) still get a sensible answer.
 */
export function getAppMode(): AppMode {
  return cachedMode ?? (import.meta.env.DEV ? 'dev' : 'prod')
}

/**
 * Decorates a child-window native title with the E2E marker when in E2E mode.
 * Dev mode leaves the title untouched: child windows are spawned from the dev
 * main window, whose pink stripe already provides the context.
 */
export function decorateChildWindowTitle(title: string): string {
  return getAppMode() === 'e2e' ? `E2E - ${title} - E2E` : title
}

/**
 * E2E-only: orders a freshly created child window behind everything without
 * focusing it, so a test run's windows (Settings, file viewer, shortcuts) don't
 * pop in front of the developer's work. A no-op outside E2E.
 *
 * Why this is needed on top of `focus: false`: macOS still raises a newly created
 * window to the front of its level even when it isn't made key, so `focus: false`
 * stops the *focus* theft but not the *visual* pop. This pushes the window to the
 * back. It pairs with the app-level `Prohibited` activation policy (set in the
 * Rust `setup`, see `test_mode::is_e2e_mode`), which is what actually stops the
 * app from ever becoming active; together they make a run unnoticeable.
 *
 * Best-effort and fire-and-forget safe: waits for the window's `tauri://created`
 * (so its NSWindow exists), then orders it back, logging instead of throwing.
 */
export async function orderChildWindowToBackInE2e(win: WebviewWindow): Promise<void> {
  if (getAppMode() !== 'e2e') return
  try {
    await new Promise<void>((resolve) => {
      void win.once('tauri://created', () => {
        resolve()
      })
    })
    await orderWindowToBack(win.label)
  } catch (e) {
    log.warn('Could not order child window {label} to back in E2E: {error}', { label: win.label, error: String(e) })
  }
}

/** Test-only: clears the cached mode so each test sees a fresh resolution. */
export function _resetForTests(): void {
  cachedMode = null
}
