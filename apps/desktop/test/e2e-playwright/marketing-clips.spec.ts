/**
 * The marketing clips: short, silent, looping recordings of the real app for the website,
 * one test per clip and theme.
 *
 * Driven by `pnpm marketing:clips`, never by a bare Playwright run: the orchestrator
 * launches a prod-looking app on the persistent clips data dir (`scripts/marketing-launch.ts`),
 * compiles the recorder, and encodes what this spec records. It's a recorder driver, not a
 * pass/fail suite, so it has its own shard and never joins `all` / `mtp` / `non-mtp`.
 *
 * ❗ It runs with NO fixture tree, on `captureTest` (no leak guard), against the staged
 * demo tree in `marketing-clips-demo.ts`. The choreography is in `marketing-clips-scenes.ts`.
 */

import { waitBudget } from './wait-budget.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { captureTest as test, stopVideoRecording } from './fixtures.js'
import { removeDemoTree, stageDemoTree } from './marketing-clips-demo.js'
import {
  copyRollback,
  languageSwitch,
  multiRename,
  opsQueue,
  search,
  settingsPan,
  viewer,
} from './marketing-clips-scenes.js'
import { CLIP_SLUGS } from './marketing-clips-slugs.js'
import type { ClipSlug } from './marketing-clips-slugs.js'
import type { ClipTheme } from './marketing-clips-take.js'

/**
 * Every clip's scene, by the slug its files are named after. A `Record` over the slug
 * list, so a slug added there without a scene here fails the typecheck.
 * `pnpm marketing:clips --only <slug>` picks by it.
 */
const SCENES: Record<ClipSlug, (page: TauriPage, theme: ClipTheme) => Promise<void>> = {
  'copy-rollback': copyRollback,
  'ops-queue': opsQueue,
  'multi-rename': multiRename,
  search,
  'language-switch': languageSwitch,
  viewer,
  settings: settingsPan,
}

const THEMES: ClipTheme[] = ['dark', 'light']

// A take is a few UI round-trips plus ~10 s of choreography, but the first one may wait
// out a drive-index reconcile (`waitForIndexedSizes`). Generous on purpose: on timeout
// Playwright destroys the plugin socket, so every later take fails with `Not connected`.
test.setTimeout(waitBudget(1_200_000))

test.describe('marketing clips', () => {
  test.skip(process.platform !== 'darwin', 'The clips are ScreenCaptureKit recordings of macOS windows.')

  test.beforeAll(() => {
    stageDemoTree()
  })

  test.afterAll(() => {
    removeDemoTree()
  })

  test.beforeEach(async ({ tauriPage }) => {
    const page = tauriPage as TauriPage
    // `captureTest` carries no auto fixture, so the recorder upstream starts for every
    // test is still running here; it would burn CPU beside the very recording it degrades.
    await stopVideoRecording(page)
    await page.waitForSelector('.file-pane', 15000)
    await ensureMcpClient(page)
  })

  for (const slug of CLIP_SLUGS) {
    for (const theme of THEMES) {
      // The title's shape is what the orchestrator's `--grep` matches: `<slug> <theme>`.
      test(`${slug} ${theme}`, async ({ tauriPage }) => {
        await SCENES[slug](tauriPage as TauriPage, theme)
      })
    }
  }
})
