/**
 * Tests for `quick-filter-controller.svelte.ts`. They pin:
 * - a keystroke ships the pattern with the cursor's file and the BACKEND
 *   selection, and applies the answer with the `..` offset,
 * - a cursor the filter leaves out lands on the first match,
 * - keystrokes that land mid-flight collapse into one follow-up call carrying
 *   the latest pattern (one call in flight, latest wins),
 * - a keystroke the backend refuses (nothing would match) is dropped,
 * - Backspace edits (never refused), `clear` sends `null`, and `reset` forgets without IPC,
 * - an answer for a listing the pane has left is dropped.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

const { ipc } = vi.hoisted(() => ({ ipc: { setListingNameFilter: vi.fn() } }))

vi.mock('$lib/tauri-commands', () => ({ setListingNameFilter: ipc.setListingNameFilter }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createQuickFilterController, type QuickFilterControllerDeps } from './quick-filter-controller.svelte'
import { createListingUpdateQueue } from './listing-update-queue'

function setup(over: Partial<QuickFilterControllerDeps> = {}) {
  const apply = vi.fn()
  const deps: QuickFilterControllerDeps = {
    runListingUpdate: createListingUpdateQueue(),
    getListingId: () => 'listing-1',
    getLoading: () => false,
    getHasBackendListing: () => true,
    getIncludeHidden: () => false,
    getHasParent: () => true,
    getCursorFilename: () => 'charlie.pdf',
    getSelectedIndices: () => [0, 2, 3],
    apply,
    ...over,
  }
  return { ctl: createQuickFilterController(deps), apply }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('createQuickFilterController', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    ipc.setListingNameFilter.mockResolvedValue({
      accepted: true,
      totalCount: 2,
      newCursorIndex: 1,
      newSelectedIndices: [0],
      sequence: 7,
    })
  })

  it('ships the pattern with backend indices and applies the answer with the parent offset', async () => {
    const { ctl, apply } = setup()
    ctl.append('p')
    await settle()

    // Frontend [0 (..), 2, 3] → backend [1, 2].
    expect(ipc.setListingNameFilter).toHaveBeenCalledWith('listing-1', 'p', false, 'charlie.pdf', [1, 2], true)
    expect(apply).toHaveBeenCalledWith({ totalCount: 2, cursorIndex: 2, selectedIndices: [1], sequence: 7 })
    expect(ctl.pattern).toBe('p')
    expect(ctl.isActive()).toBe(true)
  })

  it('puts a cursor the filter leaves out on the first match', async () => {
    ipc.setListingNameFilter.mockResolvedValue({
      accepted: true,
      totalCount: 3,
      newCursorIndex: null,
      newSelectedIndices: [],
    })
    const { ctl, apply } = setup()
    ctl.append('x')
    await settle()
    expect(apply).toHaveBeenCalledWith({ totalCount: 3, cursorIndex: 1, selectedIndices: [] })
  })

  it('puts the cursor on `..` when nothing matches', async () => {
    ipc.setListingNameFilter.mockResolvedValue({
      accepted: true,
      totalCount: 0,
      newCursorIndex: null,
      newSelectedIndices: [],
    })
    const { ctl, apply } = setup()
    ctl.append('z')
    await settle()
    expect(apply).toHaveBeenCalledWith({ totalCount: 0, cursorIndex: 0, selectedIndices: [] })
  })

  it('collapses keystrokes typed mid-flight into one call with the latest pattern', async () => {
    let release: () => void = () => {}
    ipc.setListingNameFilter.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = () => {
            resolve({ accepted: true, totalCount: 5, newCursorIndex: null, newSelectedIndices: [], sequence: 1 })
          }
        }),
    )
    const { ctl } = setup()
    ctl.append('r')
    ctl.append('e')
    ctl.append('p')
    expect(ipc.setListingNameFilter).toHaveBeenCalledTimes(1)
    release()
    await settle()
    await settle()
    expect(ipc.setListingNameFilter).toHaveBeenCalledTimes(2)
    expect(ipc.setListingNameFilter.mock.calls[1][1]).toBe('rep')
  })

  it('drops a keystroke that would match nothing and keeps the rows', async () => {
    const { ctl, apply } = setup()
    ctl.append('t')
    ctl.append('s')
    await settle()
    await settle()
    expect(ctl.pattern).toBe('ts')
    apply.mockClear()

    ipc.setListingNameFilter.mockResolvedValueOnce({
      accepted: false,
      totalCount: 1,
      newCursorIndex: 0,
      newSelectedIndices: [],
    })
    ctl.append('t')
    await settle()
    expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[5]).toBe(true)
    expect(ctl.pattern).toBe('ts')
    expect(apply).not.toHaveBeenCalled()
  })

  it.each(['etst', 'zetst'])('preserves valid characters in a rejected batch %s', async (suffix) => {
    const { ctl } = setup()
    ctl.append('o')
    await settle()
    const answers: Array<() => void> = []
    ipc.setListingNameFilter.mockImplementation(
      (_id: string, next: string) =>
        new Promise((resolve) =>
          answers.push(() => {
            resolve({
              accepted: ['one.txt', 'onet.txt', 'onets.txt'].some((name) => name.includes(next)),
              totalCount: 1,
              newCursorIndex: 0,
              newSelectedIndices: [],
              sequence: 1,
            })
          }),
        ),
    )
    ctl.append('n')
    for (const char of suffix) ctl.append(char)
    for (let i = 0; i < 10 && answers.length > 0; i++) {
      answers.shift()?.()
      await settle()
    }
    expect(ctl.pattern).toBe('onets')
    expect(ipc.setListingNameFilter.mock.calls[2][1]).toBe('on' + suffix)
  })

  it.each(['clear', 'backspace', 'reset', 'navigate', 'append'] as const)(
    'respects a newer %s during rejected-batch replay',
    async (action) => {
      let listingId = 'listing-1'
      const { ctl, apply } = setup({ getListingId: () => listingId })
      ctl.append('o')
      await settle()
      const answers: Array<() => void> = []
      ipc.setListingNameFilter.mockImplementation(
        (_id: string, next: string | null) =>
          new Promise((resolve) =>
            answers.push(() => {
              resolve({
                accepted: next === null || 'onets.txt'.includes(next),
                totalCount: 1,
                newCursorIndex: 0,
                newSelectedIndices: [],
                sequence: 1,
              })
            }),
          ),
      )
      ctl.append('n')
      for (const char of 'etst') ctl.append(char)
      answers.shift()?.() // accept "on"
      await settle()
      answers.shift()?.() // refuse the batch, begin replaying "one"
      await settle()
      expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[1]).toBe('one')
      apply.mockClear()
      if (action === 'append') ctl.append('.')
      else if (action === 'navigate') {
        listingId = 'listing-2'
        ctl.reset()
        ctl.append('t')
      } else ctl[action]()
      answers.shift()?.() // the stale replay answer
      await settle()
      if (action === 'reset' || action === 'navigate') expect(apply).not.toHaveBeenCalled()
      for (let i = 0; i < 10 && answers.length > 0; i++) {
        answers.shift()?.()
        await settle()
      }
      const expected = { clear: '', backspace: 'onets', reset: '', navigate: 't', append: 'onets.' }
      expect(ctl.pattern).toBe(expected[action])
      if (action === 'navigate') expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[0]).toBe('listing-2')
    },
  )

  it('never refuses a shrinking pattern', async () => {
    const { ctl } = setup()
    ctl.append('a')
    await settle()
    ctl.backspace()
    await settle()
    expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[5]).toBe(false)
  })

  it('edits with backspace and clears with null', async () => {
    const { ctl } = setup()
    ctl.append('a')
    await settle()
    ctl.append('b')
    await settle()
    ctl.backspace()
    await settle()
    expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[1]).toBe('a')
    ctl.clear()
    await settle()
    expect(ipc.setListingNameFilter.mock.calls.at(-1)?.[1]).toBeNull()
    expect(ctl.isActive()).toBe(false)
  })

  it('does nothing while loading or without a backend listing', async () => {
    setup({ getLoading: () => true }).ctl.append('a')
    setup({ getHasBackendListing: () => false }).ctl.append('a')
    await settle()
    expect(ipc.setListingNameFilter).not.toHaveBeenCalled()
  })

  it('drops an answer for a listing the pane has left', async () => {
    let listingId = 'listing-1'
    const { ctl, apply } = setup({ getListingId: () => listingId })
    ctl.append('a')
    listingId = 'listing-2'
    await settle()
    expect(apply).not.toHaveBeenCalled()
  })

  it('forgets the pattern on reset without IPC', () => {
    const { ctl } = setup()
    ctl.reset()
    expect(ctl.isActive()).toBe(false)
    expect(ipc.setListingNameFilter).not.toHaveBeenCalled()
  })

  it('keeps every character typed while a slow answer is on its way', async () => {
    // The review's 50k-file folder: each answer takes a while, the user types on.
    const answers: Array<(v: unknown) => void> = []
    ipc.setListingNameFilter.mockImplementation(() => new Promise((resolve) => answers.push(resolve)))
    const { ctl } = setup()
    for (const ch of '2030-') ctl.append(ch)
    expect(ctl.pattern).toBe('2030-')

    const ok = { accepted: true, totalCount: 9, newCursorIndex: 0, newSelectedIndices: [], sequence: 1 }
    answers[0](ok)
    await settle()
    expect(ipc.setListingNameFilter).toHaveBeenCalledTimes(2)
    expect(ipc.setListingNameFilter.mock.calls[1][1]).toBe('2030-')
    answers[1](ok)
    await settle()
    expect(ctl.pattern).toBe('2030-')
  })

  it('never lets a late refusal undo a newer clear', async () => {
    const { ctl } = setup()
    ctl.append('a')
    await settle() // "a" is the listing's filter now
    let refuse: (v: unknown) => void = () => {}
    ipc.setListingNameFilter.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          refuse = resolve
        }),
    )
    ctl.append('z') // "az" matches nothing, but the answer is slow
    ctl.clear() // Esc while it's on its way
    refuse({ accepted: false, totalCount: 2, newCursorIndex: 0, newSelectedIndices: [], sequence: null })
    await settle()

    // The refusal of "az" must not bring "a" back: the clear goes out instead.
    expect(ctl.pattern).toBe('')
    expect(ipc.setListingNameFilter).toHaveBeenLastCalledWith('listing-1', null, false, 'charlie.pdf', [1, 2], false)
  })
})
