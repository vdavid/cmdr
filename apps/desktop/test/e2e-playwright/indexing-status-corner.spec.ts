/**
 * The corner hourglass reacting to a drive-indexing run.
 *
 * What only an E2E can cover here is the WIRE: the app registers its index
 * listeners at startup, `StatusCorner` mounts the indicator, and an
 * `index-*` event named in `bindings.ts` reaches that indicator and lights it.
 * The state machine behind it, the checklist, and the phase copy are all
 * unit-tested (`index-state.svelte.test.ts`, `indexing-steps.test.ts`); none of
 * those would notice a renamed event, a listener that never registered, or an
 * indicator the corner stopped mounting.
 *
 * The run is synthetic (`emitBackendEvent`), which is what makes this fast and
 * deterministic: waiting on a REAL index and hoping to catch it mid-flight is a
 * race, and the fixture tree finishes indexing in well under the window a
 * timing-based assertion would need.
 *
 * ⚠️ Everything here is scoped to a volume id no real drive has, so it can't
 * collide with the app's own indexing, and the terminal phase event clears it
 * (`live` drops activity, aggregation, phase, walked ground, and the run-shape
 * facts in one go). The app is shared by every spec in the shard; a leaked
 * synthetic drive would sit in the next spec's corner tooltip.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { test, expect } from './fixtures.js'
import { ensureAppReady, emitBackendEvent } from './helpers.js'

type PageLike = TauriPage | BrowserPageAdapter

/** The corner icon itself (`IndexingStatusIndicator`). */
const CORNER_HOURGLASS = '.indexing-status'
/** The tooltip body, which only exists while the indicator is visible. */
const CORNER_TOOLTIP = '.indexing-status ~ div .tooltip-content'

/** A drive id nothing real can claim, so this spec disturbs no live volume. */
const SYNTHETIC_VOLUME = 'e2e-synthetic-drive'

/** Whether the corner names our synthetic drive right now. The row renders only
 *  inside the indicator's `{#if visible}`, so a hit also proves the corner is up. */
async function cornerNamesSyntheticDrive(tauriPage: PageLike): Promise<boolean> {
  const text = await tauriPage.evaluate<string>(
    `(document.querySelector(${JSON.stringify(CORNER_TOOLTIP)})?.textContent ?? '')`,
  )
  return text.includes(SYNTHETIC_VOLUME)
}

/** Announces a phased first index on the synthetic drive, as the backend would. */
async function announcePhasedRun(tauriPage: PageLike): Promise<void> {
  await emitBackendEvent(tauriPage, 'index-scan-started', {
    volumeId: SYNTHETIC_VOLUME,
    scanRunKind: 'first_scan',
    priorTotalEntries: null,
    priorScanDurationMs: null,
    volumeUsedBytes: null,
    coveredInPhases: true,
  })
  await emitBackendEvent(tauriPage, 'index-coverage-phase-started', {
    volumeId: SYNTHETIC_VOLUME,
    phase: 'priorityRoot',
  })
  await emitBackendEvent(tauriPage, 'index-phase-changed', { volumeId: SYNTHETIC_VOLUME, phase: 'scanning' })
}

/** The terminal phase: the volume left the pipeline, so every live fact expires. */
async function endRun(tauriPage: PageLike): Promise<void> {
  await emitBackendEvent(tauriPage, 'index-phase-changed', { volumeId: SYNTHETIC_VOLUME, phase: 'live' })
}

// Belt and braces: the test ends the run itself, so this only matters when it
// fails partway. Leaving the synthetic drive behind would put it in every later
// spec's corner tooltip.
test.afterEach(async ({ tauriPage }) => {
  await endRun(tauriPage)
})

test.describe('Indexing status corner', () => {
  test('lights up while a drive is indexing and goes out when the run ends', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)

    // Baseline: no synthetic drive. (The corner itself may legitimately be up
    // for the app's own indexing, so the assertions below are scoped to ours.)
    expect(await cornerNamesSyntheticDrive(tauriPage)).toBe(false)

    await announcePhasedRun(tauriPage)

    await expect.poll(async () => cornerNamesSyntheticDrive(tauriPage), { timeout: 5000 }).toBe(true)
    expect(await tauriPage.isVisible(CORNER_HOURGLASS)).toBe(true)

    await endRun(tauriPage)

    await expect.poll(async () => cornerNamesSyntheticDrive(tauriPage), { timeout: 5000 }).toBe(false)
  })
})
