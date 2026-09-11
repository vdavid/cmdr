/**
 * What KIND of capture run this is, and where its artifacts go.
 *
 * The i18n capture harness runs in three shapes off the same driver: the normal
 * English COUPLING pass, a pseudolocale OVERFLOW pass, and a WORST-CASE overflow
 * pass that additionally maxes the zoom and shrinks every window. Which one is
 * live comes from the environment the orchestrator (`scripts/i18n-capture.ts`)
 * sets, and nearly every module needs the answer. A fourth shape photographs
 * nothing at all: the STAGE-ONLY run the regular E2E lane drives (`isStageOnly`).
 *
 * Its own module because it's pure configuration: no page, no filesystem work, no
 * imports from the rest of the harness. That's what lets the framing, shutter,
 * and surface modules all read it without an import cycle.
 */

import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * Whether this run only STAGES each surface: open it, prove it's ready, close it,
 * and never enable the key sink, take the front position, or write a file.
 *
 * `i18n-capture-staging.spec.ts` turns it on so the routine E2E lane runs the
 * capture's own staging code and fails at the commit that breaks it, rather than
 * weeks later when someone next runs `pnpm i18n:shots`. It's a switch the spec
 * flips rather than an env var because that spec shares a lane, and a worker
 * process, with every other spec: nothing else there imports the capture modules,
 * so the switch can't leak into them.
 */
let stageOnly = false

/** Turns stage-only mode on or off for this worker. See `isStageOnly`. */
export function setStageOnly(on: boolean): void {
  stageOnly = on
}

/** True while a stage-only run is in flight: no sink recording, no focus grab, no image. */
export function isStageOnly(): boolean {
  return stageOnly
}

const here = dirname(fileURLToPath(import.meta.url))
const baseScreenshotsDir = join(here, '..', '..', 'src', 'lib', 'intl', 'messages', 'screenshots')

/**
 * The locale this run captures in for OVERFLOW review (the pseudolocale `en-XA`),
 * or empty for the normal English coupling capture. Set by the orchestrator via
 * `CMDR_I18N_OVERFLOW_LOCALE`. When set, the run is an overflow pass: screenshots
 * land in a SEPARATE `overflow/` dir (so they never overwrite the coupling
 * screenshots), the driver switches the app to this locale before capturing, and
 * each surface gets a DOM clip-overflow scan. An overflow pass never touches the
 * coupling artifacts (`capture-report.json` / `@key.screenshot`).
 */
export const overflowLocale = process.env.CMDR_I18N_OVERFLOW_LOCALE ?? ''
export const isOverflowPass = overflowLocale !== ''

/**
 * Worst-case overflow pass (overflow pass only): on top of the pseudolocale, the
 * driver maxes the UI zoom (`MAX_UI_ZOOM`) and resizes each captured window to
 * its minimum allowed size before the shot + clip scan, the maximal-overflow
 * scenario a translator must fit. Set by the orchestrator via
 * `CMDR_I18N_WORST_CASE`. No effect outside an overflow pass.
 */
export const isWorstCasePass = isOverflowPass && process.env.CMDR_I18N_WORST_CASE === '1'

/**
 * The largest UI zoom the app offers (the `appearance.textSize` percentage; the
 * `view.zoom.set150` preset is the ceiling). The worst-case pass drives the app
 * to this before capturing so layout is stressed at max zoom AND inflated text.
 */
export const MAX_UI_ZOOM = 150

/** The default UI zoom (`appearance.textSize`), and what a fitted window restores to. */
export const DEFAULT_UI_ZOOM = 100

/**
 * Where screenshots land this run: the coupling dir for a normal pass, a
 * dedicated `overflow/` subdir for an overflow pass, and a further
 * `overflow/worst-case/` subdir for the worst-case pass (all gitignored), so the
 * three never overwrite each other.
 */
export const screenshotsDir = isWorstCasePass
  ? join(baseScreenshotsDir, 'overflow', 'worst-case')
  : isOverflowPass
    ? join(baseScreenshotsDir, 'overflow')
    : baseScreenshotsDir
export const reportPath = join(screenshotsDir, 'capture-report.json')
/** Sibling list of surfaces that FAILED to capture this run (coverage honesty). */
export const failedPath = join(screenshotsDir, 'capture-failed.json')
/** Sibling list of surfaces deliberately SKIPPED (documented harness gaps). */
export const skippedPath = join(screenshotsDir, 'capture-skipped.json')
