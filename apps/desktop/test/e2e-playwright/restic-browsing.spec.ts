/**
 * E2E for browsing a restic backup: the whole person-facing loop, end to end.
 *
 * A repo folder shows a `Browse snapshots` row next to its real files; entering
 * it asks for the password (a wrong one re-arms the prompt in place, saying so);
 * the right one lists the snapshots; `latest` holds the backed-up files; and a
 * copy OUT of the snapshot lands a byte-identical file in a real folder.
 *
 * The fixture is a real repo made with the restic CLI (`../e2e-shared/restic-fixtures/
 * backup-repo`, password `hunter2`, one snapshot of `readme.txt` and
 * `docs/notes.txt`). Each test copies it to a fresh, uniquely named folder: the
 * backend remembers a repo's password (in memory, and in the shard's file-backed
 * secret store) by its path, so a reused path would skip the prompt under test.
 *
 * The routing, guards, walkers, and the index lifecycle have Rust cells
 * (`apps/desktop/src-tauri/src/file_system/restic/`, `crates/cmdr-restic/`); this
 * spec is the proof that they meet in the running app.
 *
 * Requires `--features playwright-e2e`.
 */

import { waitBudget } from './wait-budget.js'
import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import {
  clickButtonByText,
  ensureAppReady,
  expectAndDismissToast,
  fileExistsInPane,
  getFixtureRoot,
  pollUntil,
} from './helpers.js'
import { ensureMcpClient, mcpCall, mcpCallRaw, mcpNavToPath, mcpConfirmedCopy } from '../e2e-shared/mcp-client.js'

type PageLike = TauriPage | BrowserPageAdapter

const FIXTURE_REPO = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
  'e2e-shared',
  'restic-fixtures',
  'backup-repo',
)
const PASSWORD = 'hunter2'
const WRONG_PASSWORD = 'not-the-password'
const VIEW_FOLDER = 'Browse snapshots'
const BACKED_UP_TEXT = 'restored from a backup\n'
const DIALOG = '[data-dialog-id="archive-password"]'

test.setTimeout(waitBudget(90_000))

let repoRoot = ''
let outDir = ''

test.beforeEach(async ({ tauriPage }) => {
  const fixtureRoot = getFixtureRoot()
  repoRoot = path.join(fixtureRoot, `restic-repo-${String(Date.now())}`)
  outDir = path.join(fixtureRoot, 'restic-copy-out')
  fs.cpSync(FIXTURE_REPO, repoRoot, { recursive: true })
  fs.rmSync(outDir, { recursive: true, force: true })
  fs.mkdirSync(outDir, { recursive: true })
  await ensureAppReady(tauriPage)
  await ensureMcpClient(tauriPage)
})

test.afterEach(async () => {
  // A prompt left up would block every later spec's file operations.
  await mcpCallRaw('dialog', { action: 'close', type: 'archive-password' })
  for (const dir of [repoRoot, outDir]) fs.rmSync(dir, { recursive: true, force: true })
})

async function paneHasFile(page: PageLike, paneIndex: number, name: string): Promise<boolean> {
  return pollUntil(page, async () => fileExistsInPane(page, name, paneIndex), waitBudget(15_000))
}

async function dialogText(page: PageLike): Promise<string> {
  return page.evaluate<string>(`document.querySelector('${DIALOG}')?.textContent ?? ''`)
}

/** Types into the prompt's password field and presses Unlock, as a person would. */
async function submitPassword(page: PageLike, password: string): Promise<void> {
  await page.fill(`${DIALOG} input`, password)
  await clickButtonByText(page, `${DIALOG} button`, 'Unlock')
}

test.describe('Restic browsing', () => {
  test('a repo shows its snapshots row, unlocks after a wrong password, and copies a file out', async ({
    tauriPage,
  }) => {
    const page = tauriPage

    // 1. The repo folder lists its real files plus the overlay row.
    await mcpNavToPath('left', repoRoot)
    expect(await paneHasFile(page, 0, VIEW_FOLDER)).toBe(true)
    expect(await paneHasFile(page, 0, 'config')).toBe(true)

    // 2. Entering the row asks for the password. `nav_to_path` raw: the landing
    //    waits on the prompt, which is what's under test.
    await mcpCallRaw('nav_to_path', { pane: 'left', path: path.join(repoRoot, VIEW_FOLDER) })
    await expect.poll(() => page.isVisible(DIALOG), { timeout: waitBudget(15_000) }).toBeTruthy()

    // 3. A wrong password is checked before the prompt closes, and re-arms it saying so.
    await submitPassword(page, WRONG_PASSWORD)
    await expect.poll(() => dialogText(page), { timeout: waitBudget(30_000) }).toContain('That didn’t work')

    // 4. The right one unlocks, and the pane lists the snapshots.
    await submitPassword(page, PASSWORD)
    await expect.poll(() => page.isVisible(DIALOG), { timeout: waitBudget(30_000) }).toBeFalsy()
    expect(await paneHasFile(page, 0, 'latest')).toBe(true)

    // 5. Inside a snapshot: the backed-up tree, read through the index.
    await mcpNavToPath('left', path.join(repoRoot, VIEW_FOLDER, 'latest', 'docs'))
    expect(await paneHasFile(page, 0, 'notes.txt')).toBe(true)

    // 6. Copy out into a real folder: the restore.
    await mcpNavToPath('right', outDir)
    await mcpCall('select', { pane: 'left', names: ['notes.txt'] })
    expect(await mcpConfirmedCopy('left')).toContain('OK')
    const landed = await mcpCall('await', {
      pane: 'right',
      condition: 'has_item',
      value: 'notes.txt',
      timeoutSeconds: 30,
    })
    expect(landed).toContain('OK')
    await expect
      .poll(
        () => fs.existsSync(path.join(outDir, 'notes.txt')) && fs.readFileSync(path.join(outDir, 'notes.txt'), 'utf8'),
        {
          timeout: waitBudget(20_000),
        },
      )
      .toBe(BACKED_UP_TEXT)
    await expectAndDismissToast(page, 'Copied 1 file.', { timeout: waitBudget(10_000) })
  })
})
