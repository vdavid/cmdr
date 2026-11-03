/**
 * Tests for `quick-filter-session.svelte.ts`, what ⌃S (`pane.quickFilter`) does. They pin the
 * Filter → Jump → Filter cycle over one pattern, what Esc and an emptied badge do, and how a
 * view that can't filter (the network views) or can't do either (a load) answers.
 *
 * The two controllers are stand-ins with the same surface: their own tests pin the IPC.
 */
import { describe, it, expect, vi } from 'vitest'

vi.mock('#lib/intl/messages.svelte.js', () => ({ tString: (key: string) => key }))

import { createQuickFilterSession, type QuickFilterSessionDeps } from './quick-filter-session.svelte'

function setup({ canFilter = true, canMatch = true, pattern = '', buffer = '' } = {}) {
  const filter = {
    pattern,
    canFilter: () => canFilter,
    isActive: () => filter.pattern !== '',
    append: vi.fn((char: string) => {
      filter.pattern += char
    }),
    backspace: vi.fn(() => {
      filter.pattern = filter.pattern.slice(0, -1)
    }),
    clear: vi.fn(() => {
      filter.pattern = ''
    }),
    replace: vi.fn((next: string) => {
      filter.pattern = next
    }),
  }
  const jump = {
    buffer,
    canMatch: () => canMatch,
    hold: vi.fn((held: string) => {
      jump.buffer = held
    }),
    release: vi.fn(() => {
      jump.buffer = ''
    }),
    handleJumpKeystroke: vi.fn((char: string) => {
      jump.buffer += char
    }),
    backspaceHeld: vi.fn(() => {
      jump.buffer = jump.buffer.slice(0, -1)
    }),
    clearJumpState: vi.fn(() => {
      jump.buffer = ''
    }),
  }
  const announce = vi.fn()
  const onPlainFilterStarted = vi.fn()
  const typing = { touch: vi.fn(), end: vi.fn() }
  const deps: QuickFilterSessionDeps = { filter, jump, announce, onPlainFilterStarted, typing }
  return { session: createQuickFilterSession(deps), filter, jump, announce, onPlainFilterStarted, typing }
}

describe('the typing burst', () => {
  it('renews on every pattern edit and ends when the session closes', () => {
    const { session, typing } = setup()
    session.append('a')
    session.backspace()
    expect(typing.touch).toHaveBeenCalledTimes(2)
    expect(typing.end).not.toHaveBeenCalled()
    session.clear()
    expect(typing.end).toHaveBeenCalledOnce()
  })
})

describe('⌃S', () => {
  it('opens an empty Filter badge when there is no pattern', () => {
    const { session, filter } = setup()
    expect(session.kind).toBeNull()
    session.cycle()
    expect(session.kind).toBe('filter')
    expect(filter.replace).not.toHaveBeenCalled()
    expect(filter.pattern).toBe('')
  })

  it('opens onto the pattern already on, leaving it and its rows alone', () => {
    const { session, filter } = setup({ pattern: 're' })
    session.cycle()
    expect(session.kind).toBe('filter')
    expect(filter.pattern).toBe('re')
    expect(filter.clear).not.toHaveBeenCalled()
    expect(filter.replace).not.toHaveBeenCalled()
  })

  it('takes over a plain jump in progress, so what was typed stays the pattern', () => {
    const { session, filter, jump } = setup({ buffer: 'do' })
    session.cycle()
    expect(session.kind).toBe('filter')
    expect(filter.pattern).toBe('do')
    expect(jump.buffer).toBe('')
  })

  it('cycles the same pattern Filter, Jump, Filter', () => {
    const { session, filter, jump } = setup({ pattern: 're' })
    session.cycle()

    session.cycle()
    expect(session.kind).toBe('jump')
    expect(jump.buffer).toBe('re')
    expect(filter.pattern).toBe('')

    session.cycle()
    expect(session.kind).toBe('filter')
    expect(filter.pattern).toBe('re')
    expect(jump.buffer).toBe('')

    session.cycle()
    expect(session.kind).toBe('jump')
  })

  it('says which one it is on each press', () => {
    const { session, announce } = setup()
    session.cycle()
    session.cycle()
    session.cycle()
    expect(announce.mock.calls).toEqual([
      ['fileExplorer.quickFilter.modeFilter'],
      ['fileExplorer.typeToJump.modeJump'],
      ['fileExplorer.quickFilter.modeFilter'],
    ])
  })

  it('opens on Jump where there is nothing to filter, and stays there', () => {
    const { session, filter, jump, announce } = setup({ canFilter: false })
    session.cycle()
    expect(session.kind).toBe('jump')
    expect(jump.hold).toHaveBeenCalledWith('')

    session.cycle()
    expect(session.kind).toBe('jump')
    expect(filter.replace).not.toHaveBeenCalled()
    expect(announce).toHaveBeenCalledOnce()
  })

  it('does nothing where neither works (a load, search results)', () => {
    const { session, announce } = setup({ canFilter: false, canMatch: false })
    session.cycle()
    expect(session.kind).toBeNull()
    expect(announce).not.toHaveBeenCalled()
  })
})

describe('typing in the badge', () => {
  it('goes to the filter on Filter, and to the held jump on Jump', () => {
    const { session, filter, jump } = setup()
    session.cycle()
    session.append('a')
    expect(filter.pattern).toBe('a')
    session.cycle()
    session.append('b')
    expect(jump.buffer).toBe('ab')
    expect(filter.append).toHaveBeenCalledOnce()
  })

  it('edits the pattern with Backspace on either', () => {
    const { session, filter, jump } = setup({ pattern: 'abc' })
    session.cycle()
    session.backspace()
    expect(filter.pattern).toBe('ab')
    session.cycle()
    session.backspace()
    expect(jump.buffer).toBe('a')
  })

  it('closes an already empty badge on Backspace, so the next one goes to the parent folder', () => {
    const { session, jump } = setup()
    session.cycle()
    session.backspace()
    expect(session.kind).toBeNull()

    session.cycle()
    session.cycle()
    session.backspace()
    expect(session.kind).toBeNull()
    expect(jump.release).toHaveBeenCalled()
  })

  it('stays open when Backspace empties the pattern, so typing can go on', () => {
    const { session } = setup({ pattern: 'a' })
    session.cycle()
    session.backspace()
    expect(session.kind).toBe('filter')
  })

  it('raises the first-filter intro only for plain typing, since ⌃S was asked for', () => {
    const plain = setup()
    plain.session.append('a')
    plain.session.append('b')
    expect(plain.onPlainFilterStarted).toHaveBeenCalledOnce()

    const opened = setup()
    opened.session.cycle()
    opened.session.append('a')
    expect(opened.onPlainFilterStarted).not.toHaveBeenCalled()
  })
})

describe('leaving the session', () => {
  it('Esc clears the pattern and goes back to the Settings default', () => {
    const { session, filter, jump } = setup({ pattern: 're' })
    session.cycle()
    session.cycle()
    session.clear()
    expect(session.kind).toBeNull()
    expect(filter.pattern).toBe('')
    expect(jump.buffer).toBe('')
  })

  it('Esc on a plain filter clears it as ever', () => {
    const { session, filter } = setup({ pattern: 're' })
    session.clear()
    expect(filter.pattern).toBe('')
    expect(session.kind).toBeNull()
  })

  it('end() forgets the session without touching the backend filter, for a pane already on new rows', () => {
    const { session, filter, jump } = setup({ pattern: 're' })
    session.cycle()
    session.cycle()
    session.end()
    expect(session.kind).toBeNull()
    expect(jump.buffer).toBe('')
    expect(filter.clear).toHaveBeenCalledOnce() // The Jump press, not `end()`.
  })

  it('the next ⌃S after leaving starts over on Filter', () => {
    const { session } = setup()
    session.cycle()
    session.cycle()
    session.end()
    session.cycle()
    expect(session.kind).toBe('filter')
  })
})
