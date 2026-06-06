/**
 * Multi-window helpers for the Cmdr Playwright E2E tests.
 *
 * The viewer and settings UIs run in their own Tauri `WebviewWindow` in
 * production. These helpers open them via the same prod triggers and return a
 * scoped `TauriPage`, plus close them race-free. See the suite's CLAUDE.md
 * § "Multi-window testing".
 */

import type { TauriPage } from '@srsholmes/tauri-playwright'
import { pollUntil } from './core.js'

// ── Multi-window helpers ────────────────────────────────────────────────────

/**
 * Opens a file viewer window via the production trigger and returns a TauriPage
 * scoped to the new viewer window.
 *
 * Uses the `open-file-viewer` Tauri event with a `{ path }` payload (the same
 * path the MCP server uses), wired in `routes/(main)/+page.svelte` to
 * `openFileViewer(path)` (creates a `viewer-<timestamp>` WebviewWindow). Then
 * polls `listWindows()` for a label starting with `viewer-`.
 *
 * @param filePath - File path to view. Pass an empty string to exercise the
 *   "missing path" error branch in `routes/viewer/+page.svelte`.
 */
export async function openViewerWindow(tauriPage: TauriPage, filePath: string): Promise<TauriPage> {
  const before = new Set((await tauriPage.listWindows()).map((w) => w.label).filter((l) => l.startsWith('viewer-')))
  const pathJson = JSON.stringify(filePath)
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'open-file-viewer', payload: { path: ${pathJson} } });
    })()`)
  // Wait for a NEW viewer-* window (not one left open from a previous test).
  const viewer = await tauriPage.waitForWindow((w) => w.label.startsWith('viewer-') && !before.has(w.label), {
    timeout: 10000,
  })
  return viewer
}

/**
 * Opens the settings window via the production trigger and returns a TauriPage
 * scoped to it. Uses the `open-settings` Tauri event, which `(main)/+page.svelte`
 * forwards to `openSettingsWindow()`. The settings window has the stable label
 * `settings`.
 */
export async function openSettingsWindowViaProd(tauriPage: TauriPage): Promise<TauriPage> {
  await tauriPage.evaluate(`(function() {
        var invoke = window.__TAURI_INTERNALS__.invoke;
        invoke('plugin:event|emit', { event: 'open-settings' });
    })()`)
  return tauriPage.waitForWindow((w) => w.label === 'settings', { timeout: 10000 })
}

/**
 * Closes a scoped window (viewer or settings) and waits for it to disappear
 * from the window list. `mainPage` is needed for the post-close `listWindows()`
 * poll because the scoped page is gone once the window closes.
 *
 * Uses the Tauri window-close IPC directly instead of synthesizing Escape:
 * the viewer's Escape handler closes an open search bar first (one extra
 * Escape needed before the window-close path runs), and the settings window
 * may not have focus when afterEach kicks in. The window-close call has no
 * such gating and works regardless of in-page state.
 */
export async function closeScopedWindow(mainPage: TauriPage, scoped: TauriPage, label: string): Promise<void> {
  // Close the scoped window from the MAIN page, not the scoped page itself.
  // If we eval into the scoped window and call `plugin:window|close` there,
  // the window closes mid-script and never returns the pw_result IPC, so the
  // plugin times out waiting for the eval to finish (30 s) and blocks the
  // socket for the next test. Calling close from the main page is fire-and-
  // forget from the IPC plumbing's perspective. Main's response comes back
  // immediately, and the target window dies independently. (Touched arg
  // `scoped` is referenced to keep the API symmetrical with future helpers
  // that may need both pages.)
  void scoped
  const labelJson = JSON.stringify(label)
  try {
    await mainPage.evaluate(`window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: ${labelJson} })`)
  } catch {
    // The window may already be gone; fall through to the poll.
  }
  const gone = await pollUntil(
    mainPage,
    async () => {
      const labels = (await mainPage.listWindows()).map((w) => w.label)
      return !labels.includes(label)
    },
    5000,
  )
  if (!gone) {
    throw new Error(`closeScopedWindow: window '${label}' still present after 5s`)
  }
}
