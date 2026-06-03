/**
 * Acceptance test for "Go to path" (⌘G): open the dialog via the command
 * system, type a path, confirm with Enter, and verify the focused pane
 * navigated. Covers the two outcomes worth exercising end-to-end:
 *   1. Existing directory → the focused pane navigates into it.
 *   2. Non-existent path → the pane lands on the nearest existing ancestor and
 *      an INFO toast appears.
 *
 * The clipboard-prefill and digit-jump flows are covered by unit tests
 * (`lib/go-to-path/*.test.ts`) and the manual smoke list — clipboard perms make
 * them brittle in E2E. See `lib/go-to-path/CLAUDE.md` for the full contract.
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady, dispatchMenuCommand, pressKey, getFixtureRoot, expectAndDismissToast } from './helpers.js'
import { ensureMcpClient, mcpReadResource } from '../e2e-shared/mcp-client.js'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'

type PageLike = TauriPage | BrowserPageAdapter

const GO_TO_PATH_DIALOG = '[data-dialog-id="go-to-path"]'
const GO_TO_PATH_INPUT = `${GO_TO_PATH_DIALOG} input[aria-label="Path to go to"]`

/** Opens the Go-to-path dialog through the same command path ⌘G and the menu use. */
async function openGoToPathDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'nav.goToPath')
  await tauriPage.waitForSelector(GO_TO_PATH_INPUT, 3000)
}

/**
 * Reads the focused pane's active-tab path from the MCP `cmdr://state`
 * resource. We first read the top-level `focused:` field, then parse that
 * pane's `[active]` tab line, which carries the path in parentheses:
 * `- i:N id:... [active] ... (<path>)`. This is the same active-tab line
 * `search-open-in-pane.spec.ts` parses; it's synced independently of the
 * (sometimes-stale) `volume:` field.
 */
async function getFocusedPaneActiveTabPath(): Promise<string | null> {
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

/** Sets the dialog input value and fires `input` so the bound state updates. */
async function typeIntoGoToPath(tauriPage: PageLike, value: string): Promise<void> {
  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${JSON.stringify(GO_TO_PATH_INPUT)});
        if (!el) return;
        el.focus();
        el.value = ${JSON.stringify(value)};
        el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
}

test.describe('Go to path (⌘G)', () => {
  test('typing an existing directory navigates the focused pane into it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    // `ensureAppReady` leaves the left pane focused on `<fixtureRoot>/left`.
    // `left/sub-dir` is a real directory in the fixture tree.
    const targetDir = `${getFixtureRoot()}/left/sub-dir`

    await openGoToPathDialog(tauriPage)
    await typeIntoGoToPath(tauriPage, targetDir)
    // Enter on the input confirms the jump and closes the dialog.
    await pressKey(tauriPage, 'Enter')

    // The dialog closes on a successful (non-invalid) jump.
    await expect.poll(async () => (await tauriPage.count(GO_TO_PATH_DIALOG)) === 0, { timeout: 3000 }).toBeTruthy()
    // The focused pane is now inside the typed directory.
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 3000 }).toBe(targetDir)
  })

  test('a non-existent path lands on the nearest ancestor and shows an INFO toast', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)

    const ancestor = `${getFixtureRoot()}/left`
    const nonExistent = `${ancestor}/nope-go-to-path-e2e/deeper/x.txt`

    await openGoToPathDialog(tauriPage)
    await typeIntoGoToPath(tauriPage, nonExistent)
    await pressKey(tauriPage, 'Enter')

    await expect.poll(async () => (await tauriPage.count(GO_TO_PATH_DIALOG)) === 0, { timeout: 3000 }).toBeTruthy()
    // The pane jumps to the nearest existing ancestor (the fixture's `left/`).
    await expect.poll(async () => getFocusedPaneActiveTabPath(), { timeout: 3000 }).toBe(ancestor)
    // The nearest-ancestor INFO toast appears. The wording is the user-facing
    // contract (see `GoToPathAncestorToastContent.svelte`).
    await expectAndDismissToast(tauriPage, 'so we took you to')
  })
})
