/**
 * The verdict's reducer: the answer comes once, the text only grows, nothing lands after the end, and each phase the
 * block draws comes from the events alone.
 */

import { describe, expect, it } from 'vitest'
import type { VerdictUnavailable } from '#lib/ipc/bindings.js'
import {
  applyVerdictUpdate,
  hintWhy,
  verdictPhase,
  verdictPlace,
  verdictProblem,
  type AboutVerdict,
  type VerdictUpdate,
} from './about-verdict'

function fold(updates: VerdictUpdate[]): AboutVerdict | null {
  return updates.reduce<AboutVerdict | null>(applyVerdictUpdate, null)
}

function planned(verdict: AboutVerdict | null): Extract<AboutVerdict, { status: 'planned' }> {
  if (verdict?.status !== 'planned') throw new Error('expected a planned verdict')
  return verdict
}

describe('applyVerdictUpdate', () => {
  it('streams: planned, given, deltas, then done', () => {
    const steps: VerdictUpdate[] = [
      { type: 'verdictPlanned', trust: 'trusted' },
      { type: 'verdictGiven', contents: 'firstPage' },
      { type: 'verdictDelta', text: 'A tax ' },
      { type: 'verdictDelta', text: 'return.' },
    ]
    const streaming = planned(fold(steps))
    expect(streaming).toMatchObject({ given: 'firstPage', text: 'A tax return.', ending: null })
    expect(verdictPhase(streaming)).toBe('streaming')

    const cost = { promptTokens: 1, completionTokens: 2, costMicros: 3, priced: true }
    const done = planned(applyVerdictUpdate(streaming, { type: 'verdictEnd', ending: { kind: 'done' }, cost }))
    expect(verdictPhase(done)).toBe('done')
    expect(done.cost).toEqual(cost)
  })

  it('waits with no text, and shows what it was given as soon as it goes out', () => {
    const waiting = planned(fold([{ type: 'verdictPlanned', trust: 'trusted' }]))
    expect(verdictPhase(waiting)).toBe('waiting')
    expect(waiting.given).toBeNull()
    const going = planned(applyVerdictUpdate(waiting, { type: 'verdictGiven', contents: 'none' }))
    expect(verdictPhase(going)).toBe('waiting')
    expect(going.given).toBe('none')
  })

  it('keeps the text so far when it stops', () => {
    const stopped = planned(
      fold([
        { type: 'verdictPlanned', trust: 'trusted' },
        { type: 'verdictDelta', text: 'Half' },
        { type: 'verdictEnd', ending: { kind: 'stopped' }, cost: null },
      ]),
    )
    expect(verdictPhase(stopped)).toBe('stopped')
    expect(stopped.text).toBe('Half')
  })

  it('takes a take from earlier whole, as finished', () => {
    const earlier = planned(
      fold([
        { type: 'verdictPlanned', trust: 'trusted' },
        { type: 'verdictFromEarlier', contents: 'entryNames', text: 'An archive.' },
      ]),
    )
    expect(verdictPhase(earlier)).toBe('fromEarlier')
    expect(earlier).toMatchObject({ given: 'entryNames', text: 'An archive.', fromEarlier: true })
  })

  it('names the problem of a failed one, and none otherwise', () => {
    const failed = planned(
      fold([
        { type: 'verdictPlanned', trust: 'trusted' },
        { type: 'verdictEnd', ending: { kind: 'failed', problem: 'noAnswer' }, cost: null },
      ]),
    )
    expect(verdictPhase(failed)).toBe('failed')
    expect(verdictProblem(failed)).toBe('noAnswer')
    expect(verdictProblem(planned(fold([{ type: 'verdictPlanned', trust: 'trusted' }])))).toBeNull()
  })

  it('drops what comes out of order: a second answer, anything unplanned, anything after the end', () => {
    const refused = fold([{ type: 'verdictUnavailable', why: { kind: 'noCloudConsent' } }])
    expect(refused).toEqual({ status: 'unavailable', why: { kind: 'noCloudConsent' } })
    for (const late of [
      { type: 'verdictPlanned', trust: 'trusted' },
      { type: 'verdictDelta', text: 'x' },
      { type: 'verdictGiven', contents: 'none' },
    ] satisfies VerdictUpdate[]) {
      expect(applyVerdictUpdate(refused, late)).toBe(refused)
    }

    expect(fold([{ type: 'verdictDelta', text: 'before the plan' }])).toBeNull()

    const once = fold([
      { type: 'verdictPlanned', trust: 'trusted' },
      { type: 'verdictGiven', contents: 'firstLines' },
    ])
    expect(applyVerdictUpdate(once, { type: 'verdictPlanned', trust: 'trusted' })).toBe(once)
    expect(applyVerdictUpdate(once, { type: 'verdictUnavailable', why: { kind: 'aiOff' } })).toBe(once)
    expect(applyVerdictUpdate(once, { type: 'verdictGiven', contents: 'protected' })).toBe(once)
    expect(applyVerdictUpdate(once, { type: 'verdictDelta', text: '' })).toBe(once)

    const ended = applyVerdictUpdate(once, { type: 'verdictEnd', ending: { kind: 'done' }, cost: null })
    expect(applyVerdictUpdate(ended, { type: 'verdictDelta', text: 'more' })).toBe(ended)
    expect(applyVerdictUpdate(ended, { type: 'verdictEnd', ending: { kind: 'stopped' }, cost: null })).toBe(ended)
    expect(applyVerdictUpdate(ended, { type: 'verdictFromEarlier', contents: 'none', text: 'x' })).toBe(ended)

    const writing = fold([
      { type: 'verdictPlanned', trust: 'trusted' },
      { type: 'verdictDelta', text: 'Live.' },
    ])
    expect(applyVerdictUpdate(writing, { type: 'verdictFromEarlier', contents: 'none', text: 'x' })).toBe(writing)
  })
})

describe('verdictPlace', () => {
  const unavailable = (why: VerdictUnavailable): AboutVerdict => ({ status: 'unavailable', why })

  it('gives a planned take, and AI on but not answering this time, the take’s own column', () => {
    expect(verdictPlace(fold([{ type: 'verdictPlanned', trust: 'trusted' }]))).toBe('column')
    const notThisTime: VerdictUnavailable[] = [
      { kind: 'notReady' },
      { kind: 'managed', refusal: 'cloudAiOff' },
      { kind: 'tooManyRequests' },
    ]
    for (const why of notThisTime) expect(verdictPlace(unavailable(why))).toBe('column')
  })

  it('gives the reasons a setting fixes the hint line, with their reason', () => {
    const settings: VerdictUnavailable[] = [{ kind: 'aiOff' }, { kind: 'noCloudConsent' }, { kind: 'notConfigured' }]
    for (const why of settings) {
      expect(verdictPlace(unavailable(why))).toBe('hint')
      expect(hintWhy(unavailable(why))).toEqual(why)
    }
  })

  it('says nothing about AI when the person turned the take off, or before the answer', () => {
    expect(verdictPlace(unavailable({ kind: 'turnedOff' }))).toBe('none')
    expect(hintWhy(unavailable({ kind: 'turnedOff' }))).toBeNull()
    expect(verdictPlace(null)).toBe('none')
  })
})
