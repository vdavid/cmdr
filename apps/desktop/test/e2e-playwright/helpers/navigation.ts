/**
 * Route and command-palette navigation helpers for the Cmdr Playwright E2E tests.
 */

import { type PageLike, CTRL_OR_META, pollUntil } from './core.js'

// ── Navigation helpers ──────────────────────────────────────────────────────

/**
 * Navigate to a SvelteKit route via link-click interception.
 * browser.url() doesn't work in Tauri, so we create a temporary `<a>` element
 * and click it to trigger SvelteKit's client-side routing.
 */
export async function navigateToRoute(tauriPage: PageLike, path: string): Promise<void> {
  await tauriPage.evaluate(`(function() {
        var a = document.createElement('a');
        a.href = ${JSON.stringify(path)};
        document.body.appendChild(a);
        a.click();
        a.remove();
    })()`)
}

// ── Command palette ─────────────────────────────────────────────────────────

/**
 * Executes a command via the command palette. Opens the palette, types the
 * query, and clicks the first matching result.
 */
export async function executeViaCommandPalette(tauriPage: PageLike, query: string): Promise<void> {
  await tauriPage.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', ctrlKey: ${String(CTRL_OR_META === 'Control')}, metaKey: ${String(CTRL_OR_META === 'Meta')}, shiftKey: true, bubbles: true
    }))`)
  await tauriPage.waitForSelector('.palette-overlay', 5000)
  await tauriPage.fill('.palette-overlay .search-input', query)
  // Wait for filtered results to appear
  await tauriPage.waitForSelector('.palette-overlay .result-item', 3000)
  await tauriPage.evaluate(`(function() {
        var item = document.querySelector('.palette-overlay .result-item');
        if (item) item.click();
    })()`)
  // Wait for palette to close after executing the command
  const paletteClosed = await pollUntil(tauriPage, async () => !(await tauriPage.isVisible('.palette-overlay')), 3000)
  if (!paletteClosed) {
    throw new Error('executeViaCommandPalette: palette did not close within 3s after clicking a result')
  }
}
