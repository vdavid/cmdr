/**
 * App-mode helper: distinguishes `prod` / `dev` / `e2e` / `capture` runs so the UI
 * can mark windows visibly (pink title bar in dev, blue in E2E, YELLOW in a
 * screenshot capture, plain in prod).
 *
 * Where each signal comes from: dev is read synchronously from Vite
 * (`import.meta.env.DEV`); E2E and capture both come from the launch environment,
 * which the backend reports through `getAutomatedRun()`: `CMDR_E2E_MODE=1` is an
 * E2E run, and `CMDR_I18N_CAPTURE=1` on top of it is a capture. The binary is the
 * same either way (the Playwright lane's E2E build), so the mode is a property of
 * the launch, never of the build.
 *
 * Precedence: capture > e2e > dev. A capture run is also an E2E run (it drives the
 * app through the Playwright plugin), and E2E typically runs against a dev build,
 * so the most specific marker has to win.
 *
 * Why capture gets its own loud color: the native screenshot reads the window's
 * last COMPOSITED frame, and macOS stops compositing a backgrounded window, so
 * anyone clicking into another app mid-run silently turns the remaining
 * screenshots blank. The yellow `SCREENSHOT` title bar is the at-a-glance signal
 * that this run must be left alone — distinct from an ordinary E2E run, which is
 * harmless to interrupt. ❗ It is a SIGNAL, not a grab: a capture run keeps the
 * E2E `Prohibited` activation policy and never steals focus on its own.
 */
import type { WebviewWindow } from '@tauri-apps/api/webviewWindow'

import { getAppLogger } from '$lib/logging/logger'
import { getAutomatedRun, orderWindowToBack } from '$lib/tauri-commands'

export type AppMode = 'prod' | 'dev' | 'e2e' | 'capture'

const log = getAppLogger('app-mode')

let cachedMode: AppMode | null = null
/** The in-flight resolution, so concurrent callers share one backend round trip. */
let pendingMode: Promise<AppMode> | null = null

/**
 * Resolves the app mode once per window and caches it. The root layout calls it for
 * every window; a page that needs the answer before acting awaits the same promise.
 */
export function initAppMode(): Promise<AppMode> {
  if (cachedMode != null) return Promise.resolve(cachedMode)
  pendingMode ??= getAutomatedRun().then((run) => {
    const mode: AppMode = run === 'capture' ? 'capture' : run === 'e2e' ? 'e2e' : import.meta.env.DEV ? 'dev' : 'prod'
    cachedMode = mode
    return mode
  })
  return pendingMode
}

/**
 * Returns the cached app mode. Before `initAppMode()` resolves, falls back to dev/prod
 * from the synchronous signal, so a window reads as unmarked until the backend says
 * otherwise. Nothing a run depends on happens that early: the main window renders its
 * explorer and opens child windows only after awaiting the resolution.
 */
export function getAppMode(): AppMode {
  if (cachedMode != null) return cachedMode
  return import.meta.env.DEV ? 'dev' : 'prod'
}

/**
 * Whether this is an automated run driven by the Playwright plugin: plain E2E OR
 * a screenshot capture.
 *
 * ❗ Use this, NOT `getAppMode() === 'e2e'`, for anything that changes BEHAVIOR
 * (suppressing a startup popup, listening for a harness-only event, keeping a new
 * window out of the way). `capture` is a REFINEMENT of `e2e`, not an alternative
 * to it: a capture run is an E2E run that also takes screenshots, and it drives
 * the app through the same harness events. Comparing to `'e2e'` alone silently
 * turns those behaviors off in a capture run, which breaks the capture run
 * itself (the gallery and whats-new surfaces need the E2E-only listeners, and the
 * onboarding suppression keeps a popup out of every screenshot).
 *
 * `getAppMode()` is for the VISUAL marker only, where the two must differ.
 */
export function isE2eRun(): boolean {
  const mode = getAppMode()
  return mode === 'e2e' || mode === 'capture'
}

/**
 * Decorates a child-window native title with the run marker in E2E and capture
 * modes. Dev mode leaves the title untouched: child windows are spawned from the
 * dev main window, whose pink stripe already provides the context.
 */
export function decorateChildWindowTitle(title: string): string {
  const mode = getAppMode()
  if (mode === 'capture') return `SCREENSHOT - ${title} - SCREENSHOT`
  return mode === 'e2e' ? `E2E - ${title} - E2E` : title
}

/**
 * The worktree/clone label for this dev session ("colorful-tags", "main", …), baked into the
 * frontend at dev-server start by `scripts/tauri-wrapper.js` → Vite `define`. Empty string in
 * prod, E2E, and unit tests.
 */
export function getWorktreeLabel(): string {
  return __CMDR_WORKTREE_LABEL__
}

/**
 * Decorates the MAIN window's title-bar text with the run-mode marker, and — in dev — the
 * worktree label so side-by-side worktree windows are tellable apart, e.g.
 * `(colorful-tags) DEV MODE - Cmdr - DEV MODE (colorful-tags)`. Prod returns the title
 * unchanged. The label wraps any marker but is empty outside a labeled dev session, so E2E
 * stays `E2E MODE - … - E2E MODE` and a capture run reads
 * `SCREENSHOT - Cmdr – Personal use only - SCREENSHOT`. Pure (mode + label injectable) so
 * it's unit-testable.
 *
 * The capture marker is `SCREENSHOT` rather than `CAPTURE MODE` because this title is
 * INSIDE every translator screenshot, and it doubles as the glanceable "don't touch the
 * machine, a screenshot run is in flight" signal.
 */
export function decorateMainWindowTitle(
  title: string,
  mode: AppMode = getAppMode(),
  label: string = getWorktreeLabel(),
): string {
  const marker = mode === 'capture' ? 'SCREENSHOT' : mode === 'dev' ? 'DEV MODE' : mode === 'e2e' ? 'E2E MODE' : null
  if (marker === null) return title
  const prefix = label ? `(${label}) ` : ''
  const suffix = label ? ` (${label})` : ''
  return `${prefix}${marker} - ${title} - ${marker}${suffix}`
}

/**
 * E2E-only (capture runs included): orders a freshly created child window behind
 * everything without focusing it, so a run's windows (Settings, file viewer,
 * shortcuts) don't pop in front of the developer's work. A no-op outside a run.
 *
 * A capture run keeps this: the yellow `SCREENSHOT` title bar makes a run VISIBLE,
 * which is not the same as letting it grab the screen. The capture harness fronts
 * a window deliberately, just before each shot.
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
  if (!isE2eRun()) return
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

/** Test-only: clears the cached and in-flight mode so each test sees a fresh resolution. */
export function _resetForTests(): void {
  cachedMode = null
  pendingMode = null
}
