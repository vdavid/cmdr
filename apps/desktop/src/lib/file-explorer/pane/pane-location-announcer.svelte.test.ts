/**
 * "Left pane, Documents" through the pane's polite live region: when the pane takes focus,
 * and when a focused pane settles on another folder. VoiceOver never speaks the listbox's
 * own name (2026-10-15 VoiceOver spike), so this is how a listener learns where they are.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { createPaneLocationAnnouncer, paneLocationLabel } from './pane-location-announcer.svelte'

describe('paneLocationLabel', () => {
  const base = { side: 'left' as const, isVolumeRoot: false, isSearchResults: false, volumeName: 'Macintosh HD' }

  it('says the side and the folder’s own name', () => {
    expect(paneLocationLabel({ ...base, currentPath: '/Users/me/vo-test' })).toBe('Left pane, vo-test')
    expect(paneLocationLabel({ ...base, side: 'right', currentPath: '/Users/me/vo-target/' })).toBe(
      'Right pane, vo-target',
    )
  })

  it('says the volume’s name at its root', () => {
    expect(paneLocationLabel({ ...base, currentPath: '/', isVolumeRoot: true })).toBe('Left pane, Macintosh HD')
    expect(
      paneLocationLabel({ ...base, currentPath: '/Volumes/testshare', isVolumeRoot: true, volumeName: 'testshare' }),
    ).toBe('Left pane, testshare')
  })

  it('says "Search results" when results fill the pane', () => {
    expect(paneLocationLabel({ ...base, currentPath: 'search-results://sr-1', isSearchResults: true })).toBe(
      'Left pane, Search results',
    )
  })
})

describe('createPaneLocationAnnouncer', () => {
  let dispose: (() => void) | null = null

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    dispose?.()
    dispose = null
    vi.useRealTimers()
  })

  function setup(initial: { focused: boolean; label?: string; settled?: boolean; message?: string }) {
    const state = $state({
      focused: initial.focused,
      label: initial.label ?? 'Left pane, a',
      settled: initial.settled ?? true,
      message: initial.message,
    })
    const announce = vi.fn<(message: string) => void>()
    let announcer!: ReturnType<typeof createPaneLocationAnnouncer>
    dispose = $effect.root(() => {
      announcer = createPaneLocationAnnouncer({
        label: () => state.label,
        message: initial.message === undefined ? undefined : () => state.message ?? state.label,
        isFocused: () => state.focused,
        isSettled: () => state.settled,
        announce,
      })
    })
    flushSync()
    return { state, announce, announcer }
  }

  // The label names the folder (a change of it is an arrival); what's said on arrival can
  // carry more, like "empty folder", read when it goes out so it describes the settled list.
  it('says the arrival message, read when it goes out, while the label stays the folder’s identity', () => {
    const { state, announce, announcer } = setup({ focused: true, message: 'Left pane, a' })
    state.message = 'Left pane, a, empty folder'
    flushSync()
    announcer.cursorRowLanded()
    expect(announce).toHaveBeenCalledExactlyOnceWith('Left pane, a, empty folder')

    // The message changing on its own is no arrival.
    announce.mockClear()
    state.message = 'Left pane, a'
    flushSync()
    vi.advanceTimersByTime(1000)
    expect(announce).not.toHaveBeenCalled()
  })

  // Row first: the announcement waits for the list's cursor row to land as the active
  // descendant, so a polite message queues behind the row instead of racing it.
  it('announces a focused pane once its cursor row lands, not before', () => {
    const { announce, announcer } = setup({ focused: true })
    expect(announce).not.toHaveBeenCalled()
    announcer.cursorRowLanded()
    expect(announce).toHaveBeenCalledExactlyOnceWith('Left pane, a')
  })

  it('announces anyway when no cursor row lands (an empty folder at a drive’s root)', () => {
    const { announce } = setup({ focused: true })
    vi.advanceTimersByTime(1000)
    expect(announce).toHaveBeenCalledExactlyOnceWith('Left pane, a')
  })

  it('says nothing for an unfocused pane, and announces it when it takes focus', () => {
    const { state, announce, announcer } = setup({ focused: false })
    state.label = 'Left pane, b'
    flushSync()
    vi.advanceTimersByTime(1000)
    expect(announce).not.toHaveBeenCalled()

    state.focused = true
    flushSync()
    announcer.cursorRowLanded()
    expect(announce).toHaveBeenCalledExactlyOnceWith('Left pane, b')
  })

  it('announces every focus gain, even back to the same folder', () => {
    const { state, announce, announcer } = setup({ focused: true })
    announcer.cursorRowLanded()
    state.focused = false
    flushSync()
    state.focused = true
    flushSync()
    announcer.cursorRowLanded()
    expect(announce.mock.calls).toEqual([['Left pane, a'], ['Left pane, a']])
  })

  it('announces a folder change once the new listing settles', () => {
    const { state, announce, announcer } = setup({ focused: true })
    announcer.cursorRowLanded()
    announce.mockClear()

    state.settled = false
    state.label = 'Left pane, sub'
    flushSync()
    vi.advanceTimersByTime(1000)
    announcer.cursorRowLanded()
    expect(announce).not.toHaveBeenCalled()

    state.settled = true
    flushSync()
    announcer.cursorRowLanded()
    expect(announce).toHaveBeenCalledExactlyOnceWith('Left pane, sub')
  })

  it('says nothing when a listing batch lands in the same folder', () => {
    const { state, announce, announcer } = setup({ focused: true })
    announcer.cursorRowLanded()
    announce.mockClear()

    state.settled = false
    flushSync()
    state.settled = true
    flushSync()
    announcer.cursorRowLanded()
    vi.advanceTimersByTime(1000)
    expect(announce).not.toHaveBeenCalled()
  })

  it('drops a pending announcement when the pane loses focus', () => {
    const { state, announce } = setup({ focused: true })
    state.focused = false
    flushSync()
    vi.advanceTimersByTime(1000)
    expect(announce).not.toHaveBeenCalled()
  })
})
