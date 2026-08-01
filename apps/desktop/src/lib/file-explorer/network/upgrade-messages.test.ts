import { describe, expect, it } from 'vitest'
import type { UpgradeFailure } from '$lib/ipc/bindings'
import { directConnectionUnavailableMessage } from './upgrade-messages'

const ALL_REASONS: UpgradeFailure[] = ['unreachable', 'tooSlow', 'unexpected']

describe('directConnectionUnavailableMessage', () => {
  it('names the server so the user knows which one to check', () => {
    for (const reason of ALL_REASONS) {
      expect(directConnectionUnavailableMessage(reason, 'Naspolya')).toContain('Naspolya')
    }
  })

  it('gives each reason its own words', () => {
    const rendered = ALL_REASONS.map((reason) => directConnectionUnavailableMessage(reason, 'Naspolya'))
    expect(new Set(rendered).size).toBe(ALL_REASONS.length)
  })

  it('never tells the user something failed', () => {
    // `docs/style-guide.md`: error messages stay conversational and never use
    // the words "error" or "failed". The old copy led with "Direct connection
    // failed:" and then pasted a raw errno after it.
    for (const reason of ALL_REASONS) {
      const message = directConnectionUnavailableMessage(reason, 'Naspolya').toLowerCase()
      expect(message).not.toMatch(/\berrors?\b|\bfail(ed|ure)?\b/)
    }
  })

  it('says the share still works, because nothing actually broke', () => {
    // "system connection" is the app's existing name for the OS-mount path
    // (`fileExplorer.navigation.connectionTooltipSystem`); reusing it keeps the
    // vocabulary consistent and tells the user nothing was lost.
    for (const reason of ALL_REASONS) {
      expect(directConnectionUnavailableMessage(reason, 'Naspolya')).toContain('system connection')
    }
  })
})
