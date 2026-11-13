/** What a closed card reports: tokens only, and every way its AI take can end has its own word. */

import { describe, expect, it, vi } from 'vitest'
import type { AboutPlan, Derived } from '#lib/ipc/bindings.js'
import { aboutUsageProps, type AboutUsageContext } from './about-analytics'
import { emptyCard, type AboutCardData } from './about-card'
import type { AboutVerdict } from './about-verdict'

vi.mock('#lib/tauri-commands/index.js', () => ({ trackEvent: () => Promise.resolve() }))

const subject = {
  path: '/Users/test_user_1/Downloads/Secret plan.pdf',
  volumeId: 'root',
  name: 'Secret plan.pdf',
  iconId: null,
  isDirectory: false,
}
const plan: AboutPlan = { target: 'appBundle', sections: [{ section: 'removal', kinds: [] }], volumeNote: null }
const derived: Derived = {
  headline: { what: { type: 'file', kind: null, extension: null }, size: null, lead: { template: 'plain' } },
  removal: { shown: [{ type: 'cantTell' }], more: [] },
  actions: { shown: [], more: [] },
}
const context: AboutUsageContext = { trigger: 'keyboard', volumeKind: 'local' }

function card(verdict: AboutVerdict | null): AboutCardData {
  return { ...emptyCard('r', subject), plan, derived, settled: true, verdict }
}

function planned(over: Partial<Extract<AboutVerdict, { status: 'planned' }>>): AboutVerdict {
  return { status: 'planned', given: null, text: 'Some take.', ending: null, cost: null, fromEarlier: false, ...over }
}

describe('aboutUsageProps', () => {
  it('carries tokens only, never the item', () => {
    const props = aboutUsageProps(card(null), context, { type: 'action', action: 'continueInAskCmdr' })
    expect(props).toEqual({
      trigger: 'keyboard',
      target: 'app',
      volume_kind: 'local',
      unavailable: 'none',
      settled: true,
      verdict: 'none',
      verdict_why: 'none',
      action: 'continueInAskCmdr',
      continued: true,
    })
    expect(JSON.stringify(props)).not.toMatch(/Secret|Downloads|test_user/)
  })

  it.each([
    [planned({ ending: { kind: 'done' } }), 'shown', 'none'],
    [planned({ ending: { kind: 'done' }, fromEarlier: true }), 'cached', 'none'],
    [planned({ ending: { kind: 'stopped' } }), 'stopped', 'none'],
    [planned({ ending: { kind: 'failed', problem: 'noAnswer' } }), 'failed', 'noAnswer'],
    [planned({}), 'closed_early', 'none'],
    [planned({ text: '' }), 'closed_early', 'none'],
    [{ status: 'unavailable', why: { kind: 'modelOff' } } as const, 'unavailable', 'modelOff'],
  ])('words a take that ended as %j', (verdict, word, why) => {
    const props = aboutUsageProps(card(verdict), context, { type: 'closed' })
    expect(props.verdict).toBe(word)
    expect(props.verdict_why).toBe(why)
    expect(props.action).toBe('none')
  })

  it('says a card closed before its plan, and a row that took the pane somewhere', () => {
    const props = aboutUsageProps(emptyCard('r', subject), context, { type: 'goTo' })
    expect(props).toEqual(expect.objectContaining({ target: 'unknown', settled: false, action: 'goTo' }))
  })
})
