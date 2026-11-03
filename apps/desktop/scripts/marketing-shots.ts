/**
 * Reshoots the brand masters in `brand/screenshots/` and re-measures the website
 * hero's pane rectangles, in one command.
 *
 * ```bash
 * pnpm marketing:shots            # reshoot, using the warm shots data dir
 * pnpm marketing:shots --out /tmp/shots   # write somewhere else, leave brand/ alone
 * ```
 *
 * The binary comes from `ensureE2eBinary`: the Playwright lane's E2E binary, rebuilt
 * first only when the tree changed, so a master can never be shot from a stale build.
 * The launch itself (data dir, suppressions, warm index, env) is `marketing-launch.ts`,
 * shared with `pnpm marketing:clips`; read its header for why `CMDR_E2E_MODE` and
 * `CMDR_E2E_START_PATH` stay unset.
 *
 * ❗ The machine must be left alone while it shoots. macOS draws the wide window shadow
 * only for the KEY window, so every shot takes the front position first, and clicking
 * into another app mid-run costs retries. The spec fronts Cmdr and covers its window
 * with a three-second orange countdown before the shoot (`capture-countdown.ts`), so
 * nobody has to step away in advance. Unlike `pnpm i18n:shots`, this does NOT refuse to
 * start behind another app: it claims the front through System Events, which works
 * across apps, and then proves it in the pixels.
 */

import { spawnSync } from 'node:child_process'
import { mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { startCaptureStopwatch } from './capture-countdown.ts'
import { ensureE2eBinary, frontmostApp, warnIfForeignCmdr } from './capture-runtime.ts'
import { captureInstance, launchCaptureApp, prepareCaptureInstance, runCaptureShard } from './marketing-launch.ts'

const LOG = '[marketing-shots]'

const here = dirname(fileURLToPath(import.meta.url))
const repoRoot = join(here, '..', '..', '..')

const outIdx = process.argv.indexOf('--out')
const outDir = outIdx >= 0 ? (process.argv.at(outIdx + 1) ?? '') : join(repoRoot, 'brand', 'screenshots')

const instance = captureInstance('shots')

/** Stops the launched app; set once it's up, so every exit path can reach it. */
let stopApp: () => void = () => {}

/**
 * Fails before the app launches if ImageMagick is missing.
 *
 * The masters are written as lossless WebP (a fifth of the PNG's bytes, pixel-identical),
 * and the conversion happens per shot inside the spec. Discovering the tool is missing
 * there would mean a launched app, a staged window, and a failure 30 seconds in that
 * reads like a capture bug.
 */
function requireImageMagick(): void {
  const res = spawnSync('magick', ['-version'], { encoding: 'utf8' })
  if (res.error !== undefined || res.status !== 0) {
    throw new Error('The masters are written as WebP, which needs ImageMagick. Install it: `brew install imagemagick`.')
  }
}

async function main(): Promise<void> {
  if (process.platform !== 'darwin') {
    throw new Error('The marketing masters are macOS window shots (traffic lights, a real system shadow), macOS only.')
  }
  if (outIdx >= 0 && outDir === '') throw new Error('--out needs a directory')

  warnIfForeignCmdr(LOG)
  // ❗ Deliberately NOT `waitForFrontPositionToClear` (which `i18n-capture.ts` uses and
  // needs). This pipeline claims the front position per shot through System Events,
  // which DOES work against another app, so refusing to start behind one would refuse
  // every run: the person starting it is by definition in a terminal that holds the
  // front. Instead the front grab is attempted and then VERIFIED after each shutter:
  // `shootWindow` asks System Events whether Cmdr was still frontmost and rejects the
  // shot by name if not. A warning is still worth it, because clicking into another app
  // mid-run is a real way to spend three retries.
  const front = frontmostApp()
  if (front !== null && front !== 'Cmdr') {
    console.warn(`${LOG} '${front}' holds the front position. Leave the machine alone; each shot takes it back.`)
  }

  requireImageMagick()

  const binary = ensureE2eBinary(LOG)

  mkdirSync(outDir, { recursive: true })
  // `askCmdr.enabled` on, so the chat shot's rail opens past its "Ask Cmdr is off" gate.
  await prepareCaptureInstance(instance, LOG, { 'askCmdr.enabled': true })

  const app = await launchCaptureApp({ binary, instance, logPrefix: LOG, extraEnv: { CMDR_SHOTS_OUT_DIR: outDir } })
  stopApp = app.stop
  console.log(`${LOG} socket ready; staging and shooting…`)

  // The spec waits for the drive index BEFORE its countdown, and the stopwatch times
  // from the countdown's end, so a reconcile never lands in the estimate.
  const stopwatch = startCaptureStopwatch('marketing', LOG)
  try {
    runCaptureShard('marketing-shots', {
      ...app.sharedEnv,
      CMDR_SHOTS_PID: String(app.pid),
      // The chat test seeds its thread into this instance's `main.db`.
      CMDR_SHOTS_DATA_DIR: instance.dataDir,
      ...stopwatch.env,
    })
  } finally {
    stopApp()
  }
  stopwatch.recordSuccess()

  console.log(`${LOG} masters written to ${outDir}.`)
  console.log(`${LOG} next: apps/website/scripts/regenerate-hero.sh, then refresh the listings in brand/listings/.`)
}

main()
  .then(() => {
    stopApp()
    process.exit(0)
  })
  .catch((e: unknown) => {
    console.error(`${LOG} ${e instanceof Error ? e.message : String(e)}`)
    stopApp()
    process.exit(1)
  })
