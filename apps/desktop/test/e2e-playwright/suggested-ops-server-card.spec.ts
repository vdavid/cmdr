/**
 * E2E for an AI suggestion to add a server: the card shows what the click saves, with nothing to
 * type into, and approving it saves the server into the servers hub, NOT connected.
 *
 * The card is staged through the real `propose_server` handler (`e2e_propose_server`,
 * `playwright-e2e` only, as on a turn the user started), so the card is exactly what a model's
 * call would have staged. Contract: `src-tauri/src/agent/suggested_ops/DETAILS.md` § Approving a
 * server.
 *
 * ❗ The server points nowhere real: `.invalid` is reserved by RFC 2606 and never resolves, so a
 * save that dialed would have nothing to reach, and the test asserts the server stays `saved`
 * (never connected). The `finally` forgets it: the app is shared by every spec in the shard.
 */

import { waitBudget } from './wait-budget.js'
import os from 'node:os'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, escapeOverlayUntilGone } from './helpers.js'
import { initMcpClient, mcpCall, mcpReadResource } from '../e2e-shared/mcp-client.js'

type PageLike = TauriPage | BrowserPageAdapter

const DIALOG = '.modal-overlay'
const HOST = 'e2e-server-card.invalid'
/** "Macintosh HD" on macOS, "Root" on Linux: where the pane goes back to. */
const LOCAL_VOLUME_NAME = os.platform() === 'linux' ? 'Root' : 'Macintosh HD'

interface SavedServerRow {
  id: string
  displayName: string
  lastConnectedAt: string | null
}

async function savedServerNamed(tauriPage: PageLike, name: string): Promise<SavedServerRow | undefined> {
  const rows = await tauriPage.evaluate<SavedServerRow[]>(`window.__TAURI_INTERNALS__.invoke('list_saved_servers')`)
  return rows.find((row) => row.displayName === name)
}

test.describe('An AI suggestion to add a server', () => {
  test.beforeEach(async ({ tauriPage }) => {
    await initMcpClient(tauriPage)
    await ensureAppReady(tauriPage)
  })

  test('shows what it saves with no password field, and approving it saves the server unconnected', async ({
    tauriPage,
  }) => {
    const name = `Build box ${String(Date.now())}`
    const report = await tauriPage.evaluate<{ readyForReview: boolean }>(
      `window.__TAURI_INTERNALS__.invoke('e2e_propose_server', { params: ${JSON.stringify({
        protocol: 'sftp',
        host: HOST,
        username: 'ada',
        path: '/srv/builds',
        name,
        rationale: 'You open this box in the terminal every day.',
      })} })`,
    )
    expect(report.readyForReview).toBe(true)

    try {
      // The badge is the way in.
      await expect
        .poll(() => tauriPage.evaluate<boolean>(`document.querySelector('.status-corner .indicator') !== null`), {
          timeout: waitBudget(10000),
        })
        .toBe(true)
      await tauriPage.evaluate(`document.querySelector('.status-corner .indicator').click()`)

      const card = `[...document.querySelectorAll('article.group')].find(g => g.querySelector('.group-title .title-text')?.textContent === ${JSON.stringify(name)})`
      // The facts: the address, port, account, and folder the click saves, the address marked new,
      // and nothing to type into, a password least of all.
      await expect
        .poll(() => tauriPage.evaluate<string>(`(${card})?.querySelector('.facts')?.textContent ?? ''`), {
          timeout: waitBudget(10000),
        })
        .toContain(HOST)
      const facts = await tauriPage.evaluate<string>(`(${card}).querySelector('.facts').textContent`)
      for (const text of ['SFTP', '22', 'ada', '/srv/builds', 'A new address']) expect(facts).toContain(text)
      const inputs = await tauriPage.evaluate<number>(
        `(${card}).querySelectorAll('input, textarea, select, [contenteditable]').length`,
      )
      expect(inputs).toBe(0)

      // Approve.
      await expect
        .poll(
          () =>
            tauriPage.evaluate<boolean>(`(function(){
              var card = ${card};
              if (!card) return false;
              var button = [...card.querySelectorAll('button')].find(function (b) {
                return (b.textContent || '').trim() === 'Save server' && !b.disabled;
              });
              if (!button) return false;
              button.click();
              return true;
            })()`),
          { timeout: waitBudget(10000) },
        )
        .toBe(true)

      // Saved, and never connected: a save files no `lastConnectedAt`.
      await expect.poll(() => savedServerNamed(tauriPage, name), { timeout: waitBudget(10000) }).toBeDefined()
      expect((await savedServerNamed(tauriPage, name))?.lastConnectedAt).toBeFalsy()

      // In the servers hub, as a saved server with no session: `saved`, never `connected`.
      await escapeOverlayUntilGone(tauriPage, DIALOG)
      await mcpCall('select_volume', { pane: 'left', name: 'Servers' })
      await expect
        .poll(async () => (await mcpReadResource('cmdr://state')).includes(`${name}  protocol=sftp  status="saved"`), {
          timeout: waitBudget(15000),
        })
        .toBeTruthy()
    } finally {
      await escapeOverlayUntilGone(tauriPage, DIALOG)
      await mcpCall('select_volume', { pane: 'left', name: LOCAL_VOLUME_NAME })
      const saved = await savedServerNamed(tauriPage, name)
      if (saved)
        await tauriPage.evaluate(
          `window.__TAURI_INTERNALS__.invoke('forget_server', { id: ${JSON.stringify(saved.id)} })`,
        )
    }
  })
})
