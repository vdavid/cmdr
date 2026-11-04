import { afterEach, describe, expect, it, vi } from 'vitest'
import { exposeUnanswered, noteUnanswered } from './unanswered.js'

afterEach(() => {
  vi.restoreAllMocks()
})

describe('unanswered commands', () => {
  it('counts each call and logs only the first per command', () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    exposeUnanswered()
    const seen = (window as unknown as { __cmdrDemo: { unanswered: Map<string, number> } }).__cmdrDemo.unanswered

    noteUnanswered('cancel_listing', { a: 1 })
    noteUnanswered('cancel_listing', { a: 2 })
    expect(() => noteUnanswered('mystery_command', {})).toThrow()

    expect(seen.get('cancel_listing')).toBe(2)
    expect(seen.get('mystery_command')).toBe(1)
    expect(debug).toHaveBeenCalledTimes(2)
  })

  it('resolve a void command, as its caller expects nothing back', () => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    // `Promise<void>` in the bindings, and a `Result<(), E>` one, whose success carries `null`.
    expect(noteUnanswered('cancel_listing', {})).toBeNull()
    expect(noteUnanswered('show_notification', {})).toBeNull()
  })

  it('reject a command that owes an answer, the way Tauri rejects one it lacks', () => {
    vi.spyOn(console, 'debug').mockImplementation(() => undefined)
    // A `null` here once reached the search dialog as a real answer and crashed it.
    expect(() => noteUnanswered('get_index_status', {})).toThrow(/get_index_status/)
    expect(() => noteUnanswered('not_a_command_at_all', {})).toThrow(/not_a_command_at_all/)
  })
})
