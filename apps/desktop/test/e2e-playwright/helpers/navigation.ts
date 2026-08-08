/**
 * Route and command-palette navigation helpers for the Cmdr Playwright E2E tests.
 */

import { expect } from '@playwright/test'
import { mcpReadResource } from '../../e2e-shared/mcp-client.js'
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
  await tauriPage.fill('.palette-overlay input.text-field-control', query)
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

/**
 * Reads the focused pane's active-tab path from the MCP `cmdr://state` resource.
 *
 * The `[active]` tab line carries the path in parentheses
 * (`- i:N id:... [active] ... (<path>)`) and is synced independently of the
 * sometimes-stale `volume:` field. Inside an archive it reads the transparent
 * `…/sample.zip[/inner]` path.
 */
export async function getFocusedPaneActiveTabPath(): Promise<string | null> {
  const state = await mcpReadResource('cmdr://state?compact=true')
  const focusedMatch = /^focused:\s*(left|right)/m.exec(state)
  if (focusedMatch === null) return null
  const pane = focusedMatch[1]
  const marker = `\n${pane}:\n`
  const idx = state.indexOf(marker)
  if (idx === -1) return null
  // The pane block runs until the next top-level YAML key (no leading spaces).
  const block = state.slice(idx + marker.length)
  const endIdx = block.search(/\n[a-z]/)
  const scoped = endIdx === -1 ? block : block.slice(0, endIdx)
  const m = /^\s+- i:\d+ id:\S+ \[active\][^\n]*\(([^)\n]+)\)\s*$/m.exec(scoped)
  return m?.[1] ?? null
}

/**
 * Waits until the LEFT pane is both focused and showing `targetPath`, re-requesting
 * left focus on every pass.
 *
 * Navigating a pane shifts focus to it on ITS listing-complete, and `ensureAppReady`
 * navigates the right pane too, so the right pane's shift can land after a spec's
 * left-pane nav and leave the wrong pane focused — which reads as "the nav went
 * somewhere else". Re-clicking each pass outlasts that late shift, the way
 * `ensureAppReady`'s own focus loop does.
 */
export async function settleFocusedPaneOnLeft(tauriPage: PageLike, targetPath: string): Promise<void> {
  await expect
    .poll(
      async () => {
        await tauriPage.evaluate(`(function() {
            var left = document.querySelectorAll('.file-pane')[0];
            if (left && !left.classList.contains('is-focused')) left.click();
        })()`)
        return getFocusedPaneActiveTabPath()
      },
      { timeout: 5000 },
    )
    .toBe(targetPath)
}
