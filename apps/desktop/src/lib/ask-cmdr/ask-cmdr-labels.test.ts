import { describe, it, expect } from 'vitest'
import { errorMessage, limitIsFixedCeiling, limitNoticeText } from './ask-cmdr-labels'
import { managedAiRefusalMessage } from '#lib/managed-policy/ai-refusal.js'

describe('limitNoticeText', () => {
  it('names the round limit and its stored value, and whether it still answered', () => {
    const answered = limitNoticeText({ kind: 'toolRounds', max: 25, answered: true })
    const unfinished = limitNoticeText({ kind: 'toolRounds', max: 25, answered: false })
    expect(answered).toContain('25 rounds')
    expect(unfinished).toContain('25 rounds')
    expect(answered).not.toBe(unfinished)
  })

  it('says wall time in whole minutes', () => {
    expect(limitNoticeText({ kind: 'wallTime', max: 600, answered: false })).toContain('10 minutes')
    expect(limitNoticeText({ kind: 'wallTime', max: 60, answered: false })).toContain('1 minute ')
  })

  it('names each loop-guard reason and a stall in its own words', () => {
    const texts = [
      limitNoticeText({ kind: 'repeatedCall', max: 3, answered: true }),
      limitNoticeText({ kind: 'repeatedCall', max: 3, answered: false }),
      limitNoticeText({ kind: 'noProgress', max: 3, answered: true }),
      limitNoticeText({ kind: 'noProgress', max: 3, answered: false }),
      limitNoticeText({ kind: 'stalled', max: 120, answered: false }),
    ]
    expect(new Set(texts).size).toBe(texts.length)
    expect(texts[2]).toContain('3 rounds')
    expect(texts[4]).toContain('120 seconds')
  })

  it('calls only the fixed ceilings fixed', () => {
    expect(limitIsFixedCeiling({ kind: 'toolRounds', max: 25, answered: true })).toBe(true)
    expect(limitIsFixedCeiling({ kind: 'wallTime', max: 600, answered: false })).toBe(true)
    expect(limitIsFixedCeiling({ kind: 'repeatedCall', max: 3, answered: true })).toBe(false)
    expect(limitIsFixedCeiling({ kind: 'noProgress', max: 3, answered: true })).toBe(false)
    expect(limitIsFixedCeiling({ kind: 'stalled', max: 120, answered: false })).toBe(false)
  })
})

describe('errorMessage', () => {
  it('words the organization’s refusal by the rule that refused, through the shared copy map', () => {
    expect(errorMessage('managedByOrganization', 'cloudAiOff')).toBe(managedAiRefusalMessage('cloudAiOff'))
    expect(errorMessage('managedByOrganization', 'aiOff')).toBe(managedAiRefusalMessage('aiOff'))
    expect(errorMessage('managedByOrganization', 'hostNotAllowed')).toBe(managedAiRefusalMessage('hostNotAllowed'))
  })

  it('keeps its own line when no reason came with it', () => {
    expect(errorMessage('managedByOrganization')).toBe(errorMessage('managedByOrganization', null))
    expect(errorMessage('timeout', null)).not.toBe('')
  })

  it('names the provider whose plan refused a request too big, so it can’t read as Cmdr’s problem', () => {
    const named = errorMessage('requestTooLarge', null, 'Groq')
    expect(named).toContain('Groq')
    expect(named).not.toBe(errorMessage('rateLimited'))
    // A custom endpoint or local server has no name to give, so the line says "your AI provider".
    const unnamed = errorMessage('requestTooLarge')
    expect(unnamed).not.toBe('')
    expect(unnamed).not.toBe(named)
  })
})
