/**
 * What a focused pane says about itself through its live region besides where it is: a slow
 * load, a folder that just went empty, the selection count, and the quick filter's match
 * count. Each is visible only (the status bar, the "Empty folder" text, the filter badge), so
 * without these a screen reader hears none of it.
 *
 * ❗ Chatty-safe: a burst (a held ↓ while marking, a filter typed letter by letter) says only
 * its end state, once the changes pause.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import {
  createPaneStatusAnnouncer,
  LOADING_ANNOUNCE_MS,
  STATUS_SETTLE_MS,
  type PaneStatusInput,
} from './pane-status-announcer.svelte'
import { getEffectiveShortcuts, toDisplayShortcut } from '#lib/shortcuts/index.js'

describe('createPaneStatusAnnouncer', () => {
  let dispose: (() => void) | null = null

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    dispose?.()
    dispose = null
    vi.useRealTimers()
  })

  function setup(initial: Partial<PaneStatusInput> = {}) {
    const state = $state<PaneStatusInput>({
      isFocused: true,
      location: '/a',
      loading: false,
      hasError: false,
      entryCount: 5,
      hiddenCount: 0,
      selectedCount: 0,
      filterPattern: null,
      ...initial,
    })
    const announce = vi.fn<(message: string) => void>()
    dispose = $effect.root(() => {
      createPaneStatusAnnouncer({ read: () => ({ ...state }), announce })
    })
    flushSync()
    return { state, announce }
  }

  function change(apply: () => void): void {
    apply()
    flushSync()
  }

  it('says nothing about the state a pane arrives in', () => {
    const { announce } = setup({ selectedCount: 3, entryCount: 0 })
    vi.advanceTimersByTime(5_000)
    expect(announce).not.toHaveBeenCalled()
  })

  describe('selection count', () => {
    it('says the count once marking pauses, not on every mark', () => {
      const { state, announce } = setup()
      for (let i = 1; i <= 4; i++) {
        change(() => {
          state.selectedCount = i
        })
        vi.advanceTimersByTime(STATUS_SETTLE_MS / 2)
      }
      expect(announce).not.toHaveBeenCalled()
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('4 selected')
    })

    it('says "Nothing selected" when the marks go', () => {
      const { state, announce } = setup({ selectedCount: 2 })
      change(() => {
        state.selectedCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Nothing selected')
    })

    it('formats a large count with thousands separators', () => {
      const { state, announce } = setup({ entryCount: 20_000 })
      change(() => {
        state.selectedCount = 12_345
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('12,345 selected')
    })

    it('says nothing when a change lands back where it started', () => {
      const { state, announce } = setup({ selectedCount: 1 })
      change(() => {
        state.selectedCount = 2
      })
      change(() => {
        state.selectedCount = 1
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()
    })

    it('stays quiet for the unfocused pane, and drops what was pending when focus leaves', () => {
      const { state, announce } = setup({ isFocused: false })
      change(() => {
        state.selectedCount = 3
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()

      change(() => {
        state.isFocused = true
      })
      change(() => {
        state.selectedCount = 4
      })
      change(() => {
        state.isFocused = false
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()
    })

    it('treats a new folder as a fresh start: its cleared selection says nothing', () => {
      const { state, announce } = setup({ selectedCount: 3 })
      change(() => {
        state.location = '/b'
        state.selectedCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()
    })
  })

  describe('quick filter', () => {
    it('says how many names match once typing pauses', () => {
      const { state, announce } = setup({ entryCount: 40 })
      change(() => {
        state.filterPattern = 'r'
        state.entryCount = 12
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS / 2)
      change(() => {
        state.filterPattern = 're'
        state.entryCount = 3
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('3 names contain “re”')
    })

    it('says when nothing matches, and wins over the selection the filter dropped', () => {
      const { state, announce } = setup({ entryCount: 40, selectedCount: 2 })
      change(() => {
        state.filterPattern = 'zz'
        state.entryCount = 0
        state.selectedCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('No names contain “zz”')
    })

    it('says the filter is gone, with the count it shows again, when it clears', () => {
      const { state, announce } = setup({ entryCount: 3, filterPattern: 're' })
      change(() => {
        state.filterPattern = null
        state.entryCount = 1_204
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Filter cleared, 1,204 names')
    })

    it('says the clearing, not the selection it brought back', () => {
      const { state, announce } = setup({ entryCount: 1, selectedCount: 0, filterPattern: 'b' })
      change(() => {
        state.filterPattern = null
        state.entryCount = 8
        state.selectedCount = 2
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Filter cleared, 8 names')
    })

    it('says nothing when a filter typed and cleared in one burst lands back where it started', () => {
      const { state, announce } = setup({ entryCount: 8 })
      change(() => {
        state.filterPattern = 'b'
        state.entryCount = 3
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS / 2)
      change(() => {
        state.filterPattern = null
        state.entryCount = 8
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()
    })
  })

  describe('empty folder', () => {
    it('says "Empty folder" when the last entry goes', () => {
      const { state, announce } = setup({ entryCount: 1, selectedCount: 1 })
      change(() => {
        state.entryCount = 0
        state.selectedCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Empty folder')
    })

    it('names the hidden items when the last visible entry goes', () => {
      const { state, announce } = setup({ entryCount: 1 })
      change(() => {
        state.entryCount = 0
        state.hiddenCount = 2
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      // The live binding, as this platform displays it (`⇧⌘.` on a Mac).
      const shortcut = toDisplayShortcut(getEffectiveShortcuts('view.showHidden')[0])
      expect(announce).toHaveBeenCalledExactlyOnceWith(
        `There are 2 hidden items in this folder. To see hidden items, press ${shortcut} or choose “View > Show hidden files”.`,
      )
    })

    it('says when a hidden item comes or goes in a folder that looks empty', () => {
      const { state, announce } = setup({ entryCount: 0, hiddenCount: 1 })
      change(() => {
        state.hiddenCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Empty folder')
    })

    it('never calls an unreadable folder empty', () => {
      const { state, announce } = setup({ entryCount: 3 })
      change(() => {
        state.hasError = true
        state.entryCount = 0
      })
      vi.advanceTimersByTime(STATUS_SETTLE_MS)
      expect(announce).not.toHaveBeenCalled()
    })
  })

  describe('loading', () => {
    it('says a load is under way only when it outlasts the grace period', () => {
      const { state, announce } = setup()
      change(() => {
        state.loading = true
        state.location = '/b'
      })
      vi.advanceTimersByTime(LOADING_ANNOUNCE_MS - 10)
      expect(announce).not.toHaveBeenCalled()
      vi.advanceTimersByTime(20)
      expect(announce).toHaveBeenCalledExactlyOnceWith('Loading…')
      // Once per load, however long it runs.
      vi.advanceTimersByTime(10 * LOADING_ANNOUNCE_MS)
      expect(announce).toHaveBeenCalledOnce()
    })

    it('says nothing for a fast load, nor about the folder it lands in', () => {
      const { state, announce } = setup({ selectedCount: 2 })
      change(() => {
        state.loading = true
        state.location = '/b'
      })
      vi.advanceTimersByTime(LOADING_ANNOUNCE_MS / 2)
      change(() => {
        state.loading = false
        state.entryCount = 0
        state.selectedCount = 0
      })
      vi.advanceTimersByTime(5_000)
      expect(announce).not.toHaveBeenCalled()
    })

    it('says nothing about a load in the unfocused pane', () => {
      const { state, announce } = setup({ isFocused: false })
      change(() => {
        state.loading = true
      })
      vi.advanceTimersByTime(5 * LOADING_ANNOUNCE_MS)
      expect(announce).not.toHaveBeenCalled()
    })
  })
})
