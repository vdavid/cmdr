/**
 * E2E for an AI suggestion that files a file under a new name: a move whose op carries
 * `newName`. The dialog row reads `old → new`, and approving it lands the file renamed, in the
 * same one operation that moves the file that kept its name.
 *
 * The group is staged through the real `propose_suggestions` handler (`e2e_propose_suggestions`,
 * `playwright-e2e` only), so what the dialog shows is exactly what a model's call would have
 * staged. Contract: `src-tauri/src/agent/tools/suggestions/DETAILS.md` § Moving or copying under
 * a new name.
 *
 * Fixture layout (at $CMDR_E2E_START_PATH): `left/` and `right/`; this spec adds its two files
 * to `left/` and the move takes them to `right/`.
 */

import fs from 'fs'
import path from 'path'
import { waitBudget } from './wait-budget.js'
import { test, expect } from './fixtures.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import { recreateFixtures } from '../e2e-shared/fixtures.js'
import { drainOperations, ensureAppReady, escapeOverlayUntilGone, getFixtureRoot } from './helpers.js'

const DIALOG = '.modal-overlay'

test.beforeEach(() => {
  recreateFixtures(getFixtureRoot())
})

test.afterEach(async ({ tauriPage }) => {
  await drainOperations(tauriPage)
  restoreFixtureTree(getFixtureRoot())
})

test.describe('An AI suggestion that renames on the way in', () => {
  test('shows old → new, and approving it lands the file under its new name', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    const fixtureRoot = getFixtureRoot()
    const left = path.join(fixtureRoot, 'left')
    const right = path.join(fixtureRoot, 'right')
    fs.writeFileSync(path.join(left, 'Invoice-X.pdf'), 'invoice')
    fs.writeFileSync(path.join(left, 'receipt.pdf'), 'receipt')
    const displayName = `Invoices ${String(Date.now())}`
    // Short, so the cell shows it whole: a long name is middle-shortened on screen.
    const newName = 'OpenAI 2026-09.pdf'

    const report = await tauriPage.evaluate<{ readyForReview: boolean }>(
      `window.__TAURI_INTERNALS__.invoke('e2e_propose_suggestions', { params: ${JSON.stringify({
        groups: [
          {
            verb: 'move',
            sourceVolumeId: 'root',
            displayName,
            destination: { path: right },
            renames: [
              { path: path.join(left, 'Invoice-X.pdf'), newName },
              { path: path.join(left, 'receipt.pdf'), newName: 'receipt.pdf' },
            ],
          },
        ],
      })} })`,
    )
    expect(report.readyForReview).toBe(true)

    // The badge is the way in.
    await expect
      .poll(() => tauriPage.evaluate<boolean>(`document.querySelector('.status-corner .indicator') !== null`), {
        timeout: waitBudget(10000),
      })
      .toBe(true)
    await tauriPage.evaluate(`document.querySelector('.status-corner .indicator').click()`)
    try {
      // The renamed row reads old → new; the kept one draws no arrow.
      const groupSelector = `[...document.querySelectorAll('article.group')].find(g => g.querySelector('.group-title .title-text')?.textContent === ${JSON.stringify(displayName)})`
      await expect
        .poll(
          () =>
            tauriPage.evaluate<string[]>(`(function(){
              var group = ${groupSelector};
              if (!group) return [];
              return [...group.querySelectorAll('.name-cell')].map(function (cell) {
                var names = [...cell.querySelectorAll('.shortened')].map(function (n) { return n.textContent; });
                return names.join(cell.querySelector('.arrow') ? ' → ' : '');
              });
            })()`),
          { timeout: waitBudget(10000) },
        )
        .toEqual(expect.arrayContaining([`Invoice-X.pdf → ${newName}`, 'receipt.pdf']))

      // Approve: one move, both files.
      await expect
        .poll(
          () =>
            tauriPage.evaluate<boolean>(`(function(){
              var group = ${groupSelector};
              if (!group) return false;
              var button = [...group.querySelectorAll('button')].find(function (b) {
                return (b.textContent || '').trim() === 'Move 2 files' && !b.disabled;
              });
              if (!button) return false;
              button.click();
              return true;
            })()`),
          { timeout: waitBudget(10000) },
        )
        .toBe(true)

      await expect
        .poll(() => fs.existsSync(path.join(right, newName)) && fs.existsSync(path.join(right, 'receipt.pdf')), {
          timeout: waitBudget(10000),
        })
        .toBe(true)
      expect(fs.existsSync(path.join(right, 'Invoice-X.pdf'))).toBe(false)
      expect(fs.existsSync(path.join(left, 'Invoice-X.pdf'))).toBe(false)
    } finally {
      await escapeOverlayUntilGone(tauriPage, DIALOG)
    }
  })
})
