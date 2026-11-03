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
import { ensureDemoTree } from './marketing-clips-demo.js'
import {
  copyRollback,
  languageSwitch,
  multiRename,
  opsQueue,
  search,
  settingsPan,
  viewer,
} from './marketing-clips-scenes.js'
import { reachDevices, reachNetwork } from './marketing-clips-reach.js'
import { CLIP_SLUGS, VIDEO2_BEATS, VIDEO_BEATS, VIDEO_SETS, VIDEO_TITLE_PREFIX } from './marketing-clips-slugs.js'
import type { ClipSlug, Video2Beat, VideoBeat } from './marketing-clips-slugs.js'
import { ai, care, hero, reach, rename, speed, trust } from './marketing-video-scenes.js'
import { archives, hero2, phone, s3, smb, transfers, undo } from './marketing-video2-scenes.js'

/** The second demo video's scenes, a `Record` for the same reason as `SCENES`. */
const VIDEO2_SCENES: Record<Video2Beat, (page: TauriPage) => Promise<void>> = {
  smb,
  s3,
  phone,
  archives,
  transfers,
  undo,
  hero2,
}
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
  'reach-network': reachNetwork,
  'reach-devices': reachDevices,
  settings: settingsPan,
}

/** Every video beat's scene, a `Record` for the same reason as `SCENES`. */
const VIDEO_SCENES: Record<VideoBeat, (page: TauriPage) => Promise<void>> = {
  speed,
  trust,
  rename,
  reach,
  ai,
  hero,
  care,
}

const THEMES: ClipTheme[] = ['dark', 'light']

// A take is a few UI round-trips plus ~10 s of choreography, but the first one may wait
// out a drive-index reconcile (`waitForIndexedSizes`). Generous on purpose: on timeout
// Playwright destroys the plugin socket, so every later take fails with `Not connected`.
test.setTimeout(waitBudget(1_200_000))

test.describe('marketing clips', () => {
  test.skip(process.platform !== 'darwin', 'The clips are ScreenCaptureKit recordings of macOS windows.')

  // The orchestrator already ensured it before the launch; this resets the mutable
  // folders when a worker restarts after a failure, and stages it for a hand run.
  test.beforeAll(() => {
    ensureDemoTree()
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

  // The demo video's long-form beats, dark only (`marketing-video-scenes.ts`). Recorded only
  // by `pnpm marketing:clips --video`, whose `--grep` names them by their `video-` prefix.
  for (const beat of VIDEO_BEATS) {
    test(`${VIDEO_TITLE_PREFIX}${beat} dark`, async ({ tauriPage }) => {
      await VIDEO_SCENES[beat](tauriPage as TauriPage)
    })
  }

  // The second demo video's beats (`marketing-video2-scenes.ts`), by `--video2`.
  for (const beat of VIDEO2_BEATS) {
    test(`${VIDEO_SETS.video2.prefix}${beat} dark`, async ({ tauriPage }) => {
      await VIDEO2_SCENES[beat](tauriPage as TauriPage)
    })
  }
})
