import { afterEach, describe, expect, it, vi } from 'vitest'
import { exposeUnanswered, noteUnanswered } from './unanswered.js'

afterEach(() => {
  vi.restoreAllMocks()
})

describe('unanswered commands', () => {
  it('answers null, counts each call, and logs only the first per command', () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    exposeUnanswered()
    const seen = (window as unknown as { __cmdrDemo: { unanswered: Map<string, number> } }).__cmdrDemo.unanswered

    expect(noteUnanswered('mystery_command', { a: 1 })).toBeNull()
    expect(noteUnanswered('mystery_command', { a: 2 })).toBeNull()
    expect(noteUnanswered('other_command', {})).toBeNull()

    expect(seen.get('mystery_command')).toBe(2)
    expect(seen.get('other_command')).toBe(1)
    expect(debug).toHaveBeenCalledTimes(2)
  })
})
