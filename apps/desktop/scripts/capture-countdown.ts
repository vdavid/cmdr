/**
 * The orchestrator half of the pre-capture countdown: what the spec's in-window overlay
 * says, and the self-calibrating estimate behind it.
 *
 * A capture needs the machine left alone, and a stray keystroke reaching Cmdr's panes
 * corrupts the staged state. So right before a spec starts driving the app it covers the
 * Cmdr window in orange for three seconds, swallows input, and says how long the run
 * will take (`test/e2e-playwright/capture-countdown-overlay.ts`). The orchestrator owns
 * the wording and the timing, and hands the spec both through `countdownEnv`.
 *
 * The estimate is per capture kind and calibrates itself: a successful run that lands
 * more than 20% off the stored figure replaces it. It lives in the user's cache dir,
 * outside every worktree, so a fresh worktree doesn't start from the seed again; losing
 * it only means one run with the seed.
 *
 * Stdlib only, like `capture-runtime.ts`, and covered by `capture-countdown.test.ts`.
 */

import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { homedir, tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

/**
 * What's being timed. The i18n overflow passes run only the main pass, so they get
 * their own figures rather than dragging the full run's estimate around.
 */
export type CaptureKind = 'marketing' | 'i18n' | 'i18n-overflow' | 'i18n-overflow-worst-case'

/**
 * First-run estimates, in seconds. Marketing is the guide's "about 25 seconds"; i18n is
 * the last full run's PNG mtimes (~175 s for 178 shots, 2026-09-30) plus its seven app
 * launches; the overflow passes are the main pass alone. Calibration takes over after
 * the first run either way.
 */
export const SEED_ESTIMATES_S: Record<CaptureKind, number> = {
  marketing: 25,
  i18n: 200,
  'i18n-overflow': 150,
  'i18n-overflow-worst-case': 180,
}

/** A run within this fraction of the estimate leaves it alone. */
const CALIBRATION_TOLERANCE = 0.2

/** How long the overlay counts down, one second per number. */
export const COUNTDOWN_S = 3

/** Where the calibrated estimates live: machine-wide, untracked, survives worktrees. */
export const ESTIMATES_PATH = join(homedir(), 'Library', 'Caches', 'com.veszelovszki.cmdr-capture', 'estimates.json')

export type StoredEstimates = Partial<Record<CaptureKind, number>>

function isCaptureKind(key: string): key is CaptureKind {
  return Object.hasOwn(SEED_ESTIMATES_S, key)
}

/** The stored estimates, keeping only known kinds with a positive finite value. Never throws. */
export function parseEstimates(text: string | null): StoredEstimates {
  if (text === null) return {}
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    return {}
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {}
  const out: StoredEstimates = {}
  for (const [key, value] of Object.entries(parsed)) {
    if (isCaptureKind(key) && typeof value === 'number' && Number.isFinite(value) && value > 0) out[key] = value
  }
  return out
}

export function estimateFor(stored: StoredEstimates, kind: CaptureKind): number {
  return stored[kind] ?? SEED_ESTIMATES_S[kind]
}

/**
 * The new estimate after a successful run, or null when the run landed within 20% of
 * the old one. Taking the latest run outright (not an average) is deliberate: the
 * figure exists to set David's expectation for the NEXT run, which looks like the last.
 */
export function calibratedEstimate(estimateS: number, actualS: number): number | null {
  if (Math.abs(actualS - estimateS) <= estimateS * CALIBRATION_TOLERANCE) return null
  return Math.max(1, Math.round(actualS))
}

const SMALL_NUMBERS = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine']

function spellSmall(n: number): string {
  return SMALL_NUMBERS[n] ?? String(n)
}

/** The overlay's second line: five-second steps under a minute, whole minutes above. */
export function describeEstimate(seconds: number): string {
  if (seconds < 55) return `This will take about ${spellSmall(Math.max(5, Math.round(seconds / 5) * 5))} seconds.`
  if (seconds < 90) return 'This will take about a minute.'
  return `This will take about ${spellSmall(Math.round(seconds / 60))} minutes.`
}

export function readEstimates(path: string): StoredEstimates {
  try {
    return parseEstimates(readFileSync(path, 'utf8'))
  } catch {
    return parseEstimates(null)
  }
}

/** Stores one kind's estimate, keeping the others. Temp file plus rename, so a crash can't half-write it. */
export function writeEstimate(path: string, kind: CaptureKind, seconds: number): void {
  const next = { ...readEstimates(path), [kind]: seconds }
  mkdirSync(dirname(path), { recursive: true })
  const tmp = `${path}.${String(process.pid)}.tmp`
  writeFileSync(tmp, `${JSON.stringify(next, null, 2)}\n`)
  renameSync(tmp, path)
}

/**
 * The env that tells a capture spec to count down, what to say, and where to stamp the
 * moment the countdown ended. The spec reads it with `countdownFromEnv`.
 */
export function countdownEnv(estimateS: number, endedFile: string): Record<string, string> {
  return {
    CMDR_CAPTURE_COUNTDOWN_S: String(COUNTDOWN_S),
    CMDR_CAPTURE_COUNTDOWN_LINE: describeEstimate(estimateS),
    CMDR_CAPTURE_COUNTDOWN_ENDED_FILE: endedFile,
  }
}

/**
 * Seconds of capture work from the countdown's end (the stamp the spec wrote, as epoch
 * ms) to `endedAtMs`, or null when no countdown ended.
 *
 * ❗ Timed from the countdown's END, which the spec stamps, never from the orchestrator's
 * side of the spec run: everything the spec waits for before the countdown (the drive
 * index reconcile above all, about five minutes) would otherwise land in the estimate.
 * That's how one run taught it 324 s for a 25-second shoot (2026-10-26).
 */
export function timedSeconds(countdownEndedStamp: string | null, endedAtMs: number): number | null {
  const endedCountdownAt = Number(countdownEndedStamp)
  if (countdownEndedStamp === null || !Number.isFinite(endedCountdownAt) || endedCountdownAt <= 0) return null
  return Math.max(0, (endedAtMs - endedCountdownAt) / 1000)
}

/** Times the capture work the overlay's estimate describes. */
export interface CaptureStopwatch {
  /** Merge into the env of the Playwright run that should count down (only the first). */
  env: Record<string, string>
  /** Call once the run finished green; calibrates the stored estimate when it drifted. */
  recordSuccess(): void
}

/**
 * Arms the stopwatch for the spec run that shows the countdown. The clock itself starts
 * when the spec stamps the countdown's end (`timedSeconds` says why).
 */
export function startCaptureStopwatch(kind: CaptureKind, logPrefix: string): CaptureStopwatch {
  const estimateS = estimateFor(readEstimates(ESTIMATES_PATH), kind)
  console.log(`${logPrefix} hands off once the orange countdown appears. ${describeEstimate(estimateS)}`)
  const endedFile = join(tmpdir(), `cmdr-capture-countdown-${String(process.pid)}.ended`)
  rmSync(endedFile, { force: true })
  return {
    env: countdownEnv(estimateS, endedFile),
    recordSuccess(): void {
      let stamp: string | null = null
      try {
        stamp = readFileSync(endedFile, 'utf8')
        rmSync(endedFile, { force: true })
      } catch {
        /* no countdown ended (or none was asked for): nothing to calibrate */
      }
      const actualS = timedSeconds(stamp, Date.now())
      const next = actualS === null ? null : calibratedEstimate(estimateS, actualS)
      if (next === null) return
      try {
        writeEstimate(ESTIMATES_PATH, kind, next)
      } catch {
        /* best-effort: the next run shows the old figure */
      }
    },
  }
}
