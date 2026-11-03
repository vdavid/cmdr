import { describe, expect, it } from 'vitest'
import { longChatLevel, longChatNudgeShows } from './ask-cmdr-long-chat'

function usage(estimatedTokens: number, budgetTokens: number) {
  return { estimatedTokens, budgetTokens, elidedResults: 0 }
}

describe('longChatLevel', () => {
  it('stays quiet while the chat has room, however many items it holds', () => {
    expect(longChatLevel(usage(1_800, 60_000), false)).toBeNull()
    expect(longChatLevel(usage(41_999, 60_000), false)).toBeNull()
  })

  it('turns soft at 70% of the budget', () => {
    expect(longChatLevel(usage(42_000, 60_000), false)).toBe('soft')
    expect(longChatLevel(usage(53_999, 60_000), false)).toBe('soft')
  })

  it('turns urgent at 90%, and over budget too', () => {
    expect(longChatLevel(usage(54_000, 60_000), false)).toBe('urgent')
    expect(longChatLevel(usage(70_000, 60_000), false)).toBe('urgent')
  })

  it('turns soft on a trim, whatever the fill', () => {
    expect(longChatLevel(usage(6_000, 60_000), true)).toBe('soft')
    expect(longChatLevel(null, true)).toBe('soft')
    expect(longChatLevel(usage(55_000, 60_000), true)).toBe('urgent')
  })

  it('reads an unmeasured thread as having room', () => {
    expect(longChatLevel(null, false)).toBeNull()
    expect(longChatLevel(usage(1_000, 0), false)).toBeNull()
  })
})

describe('longChatNudgeShows', () => {
  it('shows any level nobody dismissed', () => {
    expect(longChatNudgeShows('soft', null)).toBe(true)
    expect(longChatNudgeShows('urgent', null)).toBe(true)
    expect(longChatNudgeShows(null, null)).toBe(false)
  })

  it('comes back once at urgent after a soft dismissal', () => {
    expect(longChatNudgeShows('soft', 'soft')).toBe(false)
    expect(longChatNudgeShows('urgent', 'soft')).toBe(true)
  })

  it('stays gone after an urgent dismissal', () => {
    expect(longChatNudgeShows('urgent', 'urgent')).toBe(false)
  })
})
