import { describe, it, expect } from 'vitest'
import { errorMessage, limitNoticeText } from './ask-cmdr-labels'
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
})
