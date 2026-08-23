/**
 * Which sentence the crash dialog opens with, per report.
 *
 * The whole point of `appFate` is that the dialog never says something the report
 * can't back up, so each case here is really an assertion about a claim: "the app
 * quit unexpectedly" needs evidence, and the absence of evidence has to fall back
 * to a sentence that stays true either way.
 */

import { describe, it, expect } from 'vitest'
import { crashDialogBodyKey, crashDialogTitleKey, crashSentToastKey } from './crash-copy'

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

describe('crashDialogTitleKey', () => {
  it('names the artifact a crash report only when the app went down with it', () => {
    expect(crashDialogTitleKey({ appFate: 'ended' })).toBe('crashReporter.dialog.title.crash')
  })

  it('stays neutral for a survived panic and for a report with no fate', () => {
    // Two fates, one title: they want identical wording, so a third key would be duplication.
    expect(crashDialogTitleKey({ appFate: 'keptRunning' })).toBe('crashReporter.dialog.title.report')
    expect(crashDialogTitleKey({ appFate: 'unknown' })).toBe('crashReporter.dialog.title.report')
    expect(crashDialogTitleKey({})).toBe('crashReporter.dialog.title.report')
  })
})

describe('crashSentToastKey', () => {
  it('splits the same way as the title, and for the same reason', () => {
    expect(crashSentToastKey({ appFate: 'ended' })).toBe('crashReporter.sentToast.message.crash')
    expect(crashSentToastKey({ appFate: 'keptRunning' })).toBe('crashReporter.sentToast.message.report')
    expect(crashSentToastKey({})).toBe('crashReporter.sentToast.message.report')
  })
})
