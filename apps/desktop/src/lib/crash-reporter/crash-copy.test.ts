/**
 * Which sentence the crash dialog opens with, per report.
 *
 * The whole point of `appFate` is that the dialog never says something the report
 * can't back up, so each case here is really an assertion about a claim: "the app
 * quit unexpectedly" needs evidence, and the absence of evidence has to fall back
 * to a sentence that stays true either way.
 */

import { describe, it, expect } from 'vitest'
import { crashDialogBodyKey } from './crash-copy'

describe('crashDialogBodyKey', () => {
  it('says the app quit for a report whose fate is settled as ended', () => {
    expect(crashDialogBodyKey({ appFate: 'ended' })).toBe('crashReporter.dialog.body.ended')
  })

  it('says the app kept running when survival was confirmed', () => {
    expect(crashDialogBodyKey({ appFate: 'keptRunning' })).toBe('crashReporter.dialog.body.keptRunning')
  })

  it('claims nothing about the app for a report that carries no fate', () => {
    expect(crashDialogBodyKey({ appFate: 'unknown' })).toBe('crashReporter.dialog.body.unknown')
  })

  it('claims nothing for a crash file written before the field existed', () => {
    // `appFate` is `#[serde(default)]` backend-side, so an older file arrives with the
    // property absent rather than set. Nothing may be inferred from that.
    expect(crashDialogBodyKey({})).toBe('crashReporter.dialog.body.unknown')
  })

  it('claims nothing for an unresolved fate', () => {
    // `unconfirmed` is resolved to `ended` at the next launch, so it shouldn't reach
    // here. If it ever does, the safe reading is the one that asserts least: the
    // opposite default would tell a user their app crashed on no evidence.
    expect(crashDialogBodyKey({ appFate: 'unconfirmed' })).toBe('crashReporter.dialog.body.unknown')
  })
})
