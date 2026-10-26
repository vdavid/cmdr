/**
 * Tests for the pure half of `capture-countdown.ts`: reading the stored estimates,
 * the calibration rule, and what the spec is told to show. The overlay itself lives
 * test-side (`test/e2e-playwright/capture-countdown-overlay.ts`, with its own tests).
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  COUNTDOWN_S,
  SEED_ESTIMATES_S,
  calibratedEstimate,
  countdownEnv,
  timedSeconds,
  describeEstimate,
  estimateFor,
  parseEstimates,
  readEstimates,
  writeEstimate,
} from './capture-countdown.ts'

describe('parseEstimates', () => {
  it('reads positive second counts per kind', () => {
    expect(parseEstimates('{"marketing": 31, "i18n": 240}')).toEqual({ marketing: 31, i18n: 240 })
  })

  it('drops kinds it does not know and values that are not positive finite numbers', () => {
    expect(parseEstimates('{"marketing": -4, "i18n": "fast", "bogus": 9, "i18n-overflow": 0}')).toEqual({})
  })

  it('answers empty for a missing or unreadable file rather than throwing', () => {
    expect(parseEstimates(null)).toEqual({})
    expect(parseEstimates('not json')).toEqual({})
    expect(parseEstimates('[1, 2]')).toEqual({})
  })
})

describe('estimateFor', () => {
  it('prefers the stored estimate', () => {
    expect(estimateFor({ marketing: 40 }, 'marketing')).toBe(40)
  })

  it('falls back to the seed when nothing is stored', () => {
    expect(estimateFor({}, 'marketing')).toBe(SEED_ESTIMATES_S.marketing)
    expect(estimateFor({}, 'i18n')).toBe(SEED_ESTIMATES_S.i18n)
  })
})

describe('calibratedEstimate', () => {
  it('keeps the estimate when the run landed within 20% of it', () => {
    expect(calibratedEstimate(25, 29)).toBeNull()
    expect(calibratedEstimate(25, 21)).toBeNull()
    expect(calibratedEstimate(25, 30)).toBeNull() // exactly 20% still counts as close enough
  })

  it('takes the actual duration, rounded, once it drifts more than 20%', () => {
    expect(calibratedEstimate(25, 31.4)).toBe(31)
    expect(calibratedEstimate(25, 19.6)).toBe(20)
    expect(calibratedEstimate(200, 412)).toBe(412)
  })

  it('never stores less than a second', () => {
    expect(calibratedEstimate(25, 0.2)).toBe(1)
  })
})

describe('describeEstimate', () => {
  it('rounds short runs to five seconds', () => {
    expect(describeEstimate(25)).toBe('This will take about 25 seconds.')
    expect(describeEstimate(27)).toBe('This will take about 25 seconds.')
    expect(describeEstimate(33)).toBe('This will take about 35 seconds.')
  })

  it('spells out one through nine', () => {
    expect(describeEstimate(4)).toBe('This will take about five seconds.')
    expect(describeEstimate(200)).toBe('This will take about three minutes.')
  })

  it('says a minute, then whole minutes', () => {
    expect(describeEstimate(58)).toBe('This will take about a minute.')
    expect(describeEstimate(80)).toBe('This will take about a minute.')
    expect(describeEstimate(95)).toBe('This will take about two minutes.')
    expect(describeEstimate(720)).toBe('This will take about 12 minutes.')
  })
})

describe('countdownEnv', () => {
  it('hands the spec the countdown length, the worded estimate, and where to stamp the countdown end', () => {
    expect(countdownEnv(25, '/tmp/ended')).toEqual({
      CMDR_CAPTURE_COUNTDOWN_S: String(COUNTDOWN_S),
      CMDR_CAPTURE_COUNTDOWN_LINE: 'This will take about 25 seconds.',
      CMDR_CAPTURE_COUNTDOWN_ENDED_FILE: '/tmp/ended',
    })
  })
})

describe('timedSeconds', () => {
  it('times from the end of the countdown, so no wait before it leaks into the estimate', () => {
    expect(timedSeconds('10000', 41_000)).toBe(31)
  })

  it('answers null when no countdown ended, so that run never calibrates', () => {
    expect(timedSeconds(null, 41_000)).toBeNull()
    expect(timedSeconds('garbage', 41_000)).toBeNull()
  })

  it('never goes below zero', () => {
    expect(timedSeconds('11000', 10_000)).toBe(0)
  })
})

describe('readEstimates and writeEstimate', () => {
  let dir: string
  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'capture-countdown-test-'))
  })
  afterEach(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  it('round-trips one kind without touching the others', () => {
    const path = join(dir, 'nested', 'estimates.json')
    expect(readEstimates(path)).toEqual({})
    writeEstimate(path, 'marketing', 31)
    writeEstimate(path, 'i18n', 240)
    writeEstimate(path, 'marketing', 28)
    expect(readEstimates(path)).toEqual({ marketing: 28, i18n: 240 })
  })

  it('replaces a corrupt file instead of failing the run', () => {
    const path = join(dir, 'estimates.json')
    writeFileSync(path, '{ half-writ')
    writeEstimate(path, 'marketing', 31)
    expect(JSON.parse(readFileSync(path, 'utf8'))).toEqual({ marketing: 31 })
  })
})
