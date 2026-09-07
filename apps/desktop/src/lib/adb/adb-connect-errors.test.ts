import { describe, expect, it, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import type { AdbConnectOutcomeError } from '$lib/ipc/bindings'
import { readAdbConnectOutcome } from './adb-connect-errors'

const SERIAL = 'R58M12345'

/** Every variant the backend can answer, with the fields each one carries. */
const EVERY_VARIANT: AdbConnectOutcomeError[] = [
  { type: 'adbNotInstalled' },
  { type: 'serverUnreachable' },
  { type: 'deviceGone', serial: SERIAL },
  { type: 'unauthorized', serial: SERIAL },
  { type: 'deviceTooOld', serial: SERIAL },
  { type: 'timedOut' },
  { type: 'cancelled' },
  { type: 'transport' },
]

describe('readAdbConnectOutcome', () => {
  beforeAll(() => {
    _setLocaleForTests('en-US')
  })
  afterAll(() => {
    _setLocaleForTests(null)
  })

  it('says nothing at all about a dial the user called off', () => {
    expect(readAdbConnectOutcome({ type: 'cancelled' })).toEqual({ kind: 'silent' })
  })

  it('sends a phone still showing its prompt to the waiting state, never to a refusal', () => {
    const outcome = readAdbConnectOutcome({ type: 'unauthorized', serial: SERIAL })
    expect(outcome.kind).toBe('waiting')
    if (outcome.kind !== 'waiting') return
    expect(outcome.reason).toBe('Check your phone and tap Allow.')
    expect(outcome.hint).toBe('Cmdr opens your phone as soon as you do.')
  })

  it('routes a missing toolchain to Settings, which is the only place that fixes it', () => {
    expect(readAdbConnectOutcome({ type: 'adbNotInstalled' })).toEqual({
      kind: 'refused',
      sentence: "Cmdr couldn't find the Android platform tools.",
      recovery: 'open_settings',
    })
  })

  // The three that a second attempt can genuinely clear: nothing about the
  // machine or the phone has to change first.
  it.each<[string, AdbConnectOutcomeError]>([
    ['serverUnreachable', { type: 'serverUnreachable' }],
    ['timedOut', { type: 'timedOut' }],
    ['transport', { type: 'transport' }],
  ])('offers %s a retry', (_name, error) => {
    const outcome = readAdbConnectOutcome(error)
    expect(outcome.kind === 'refused' && outcome.recovery).toBe('retry')
  })

  // ❗ No button, deliberately: an unplugged phone is fixed by the cable and an
  // Android 6 phone by nothing at all. A "Try again" that can only fail again is
  // the inert affordance the pane refuses to show.
  it.each<[string, AdbConnectOutcomeError]>([
    ['deviceGone', { type: 'deviceGone', serial: SERIAL }],
    ['deviceTooOld', { type: 'deviceTooOld', serial: SERIAL }],
  ])('leaves %s terminal', (_name, error) => {
    const outcome = readAdbConnectOutcome(error)
    expect(outcome.kind === 'refused' && outcome.recovery).toBe('none')
  })

  it('has a finished sentence for every variant the backend can answer', () => {
    for (const error of EVERY_VARIANT) {
      const outcome = readAdbConnectOutcome(error)
      if (outcome.kind === 'silent') continue
      const sentence = outcome.kind === 'waiting' ? outcome.reason : outcome.sentence
      // A missing key renders as the key itself, which is what this catches.
      expect(sentence, `${error.type} has no sentence`).not.toContain('adb.connect.')
      expect(sentence.length, `${error.type} says nothing`).toBeGreaterThan(0)
    }
  })

  it('never leaks a serial, a diagnostic, or the word "transport" into what a person reads', () => {
    for (const error of EVERY_VARIANT) {
      const outcome = readAdbConnectOutcome(error)
      if (outcome.kind === 'silent') continue
      const sentence = outcome.kind === 'waiting' ? outcome.reason : outcome.sentence
      expect(sentence).not.toContain(SERIAL)
      expect(sentence.toLowerCase()).not.toContain('transport')
      expect(sentence.toLowerCase()).not.toContain('adb server')
    }
  })
})
