/**
 * Arriving in a folder that only LOOKS empty, through both of a pane's announcers wired the
 * way `FilePane.svelte` wires them: the location announcer says where the pane is plus the
 * hidden-items text, and the status announcer (which speaks when an empty pane's hidden count
 * changes) stays quiet about the arrival, so the listener hears it once.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import { createPaneLocationAnnouncer, paneArrivalMessage } from './pane-location-announcer.svelte'
import { createPaneStatusAnnouncer, STATUS_SETTLE_MS } from './pane-status-announcer.svelte'
import { shortcutAsText } from '#lib/shortcuts/reactive-shortcuts.svelte.js'

describe('paneArrivalMessage', () => {
  it('is the location alone while the pane shows rows', () => {
    expect(paneArrivalMessage('Left pane, a', null)).toBe('Left pane, a')
  })

  it('adds "empty folder" for a folder with nothing in it', () => {
    expect(paneArrivalMessage('Left pane, a', 0)).toBe('Left pane, a, empty folder')
  })

  it('says the hidden items and how to see them for a folder that only looks empty', () => {
    const key = shortcutAsText('view.showHidden', 'announcement')
    expect(paneArrivalMessage('Left pane, a', 3)).toBe(
      `Left pane, a. There are 3 hidden items in this folder. To see hidden items, press ${String(key)} or choose “View > Show hidden files”.`,
    )
  })
})

describe('arriving in a folder that only looks empty', () => {
  let dispose: (() => void) | null = null

  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    dispose?.()
    dispose = null
    vi.useRealTimers()
  })

  function setupPane() {
    const pane = $state({ path: '/full', loading: false, entryCount: 4, hiddenCount: 0 })
    const announce = vi.fn<(message: string) => void>()
    dispose = $effect.root(() => {
      const label = (): string => `Left pane, ${pane.path.slice(1)}`
      const showsEmpty = (): boolean => !pane.loading && pane.entryCount === 0
      createPaneLocationAnnouncer({
        label,
        message: () => paneArrivalMessage(label(), showsEmpty() ? pane.hiddenCount : null),
        isFocused: () => true,
        isSettled: () => !pane.loading,
        announce,
      })
      createPaneStatusAnnouncer({
        read: () => ({
          isFocused: true,
          location: pane.path,
          loading: pane.loading,
          hasError: false,
          entryCount: pane.entryCount,
          hiddenCount: pane.hiddenCount,
          selectedCount: 0,
          filterPattern: null,
        }),
        announce,
      })
    })
    flushSync()
    vi.advanceTimersByTime(5_000)
    announce.mockClear()
    return { pane, announce }
  }

  /** A navigation as `listing-loader.ts` runs it: the count and hidden count land, then the load ends. */
  function navigate(pane: { path: string; loading: boolean; entryCount: number; hiddenCount: number }, to: string) {
    pane.loading = true
    pane.path = to
    flushSync()
    pane.entryCount = 0
    pane.hiddenCount = 3
    flushSync()
    pane.loading = false
    flushSync()
  }

  it('speaks once: where it is, and the hidden items', () => {
    const { pane, announce } = setupPane()
    navigate(pane, '/dotfiles')
    vi.advanceTimersByTime(5_000)
    expect(announce).toHaveBeenCalledTimes(1)
    expect(announce.mock.calls[0][0]).toMatch(/^Left pane, dotfiles\. There are 3 hidden items in this folder\./)
  })

  it('still speaks about a later change in place', () => {
    const { pane, announce } = setupPane()
    navigate(pane, '/dotfiles')
    vi.advanceTimersByTime(5_000)
    announce.mockClear()
    pane.hiddenCount = 0
    flushSync()
    vi.advanceTimersByTime(STATUS_SETTLE_MS)
    expect(announce).toHaveBeenCalledExactlyOnceWith('Empty folder')
  })
})
