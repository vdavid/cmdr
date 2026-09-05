/**
 * Escape means two things once a search can run for minutes: the first press stops the
 * run, the second closes the dialog.
 *
 * The reflex for "stop that" is Escape, and closing on the first press would take the
 * results already on screen away along with the walk — the opposite of what somebody
 * pressing Escape at 40,000 folders wants. With no run in flight it stays what it always
 * was: close.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import QueryDialog from './QueryDialog.svelte'
import type { QueryDialogConfig } from './query-dialog-config'
import type { QueryStreamCallbacks, QueryStreamSource } from './query-stream'
import { makeQueryDialogConfig } from './test-helpers'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => (key === 'search.autoApply' ? true : undefined)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, getCachedCustomFolderIcon: () => undefined, iconCacheVersion: writable(0) }
})

interface Harness {
  overlay: Element
  config: QueryDialogConfig
  closes: () => number
  cancelled: string[]
  /** The callbacks the runner handed the source for the run in flight. */
  callbacks: () => QueryStreamCallbacks
  cleanup: () => void
}

function mountDialog(): Harness {
  let closes = 0
  const cancelled: string[] = []
  const started: { runId: string; callbacks: QueryStreamCallbacks }[] = []

  const streamingSource: QueryStreamSource = {
    start: (runId, callbacks) => {
      started.push({ runId, callbacks })
      return Promise.resolve(() => {})
    },
    cancel: (runId) => {
      cancelled.push(runId)
    },
  }

  const config = makeQueryDialogConfig({
    streamingSource,
    onClose: () => {
      closes += 1
    },
  })
  config.state.setQuery('report')

  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(QueryDialog, { target, props: { config } })
  const overlay = target.querySelector('.search-overlay')
  if (!overlay) throw new Error('overlay not found')

  return {
    overlay,
    config,
    closes: () => closes,
    cancelled,
    callbacks: () => {
      const last = started.at(-1)
      if (!last) throw new Error('no run started')
      return last.callbacks
    },
    cleanup: () => {
      void unmount(component)
      target.remove()
    },
  }
}

function pressEscape(overlay: Element): void {
  overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))
}

async function settle(): Promise<void> {
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
}

describe('Escape, while a live run is going', () => {
  it('stops the run on the first press and closes on the second', async () => {
    const h = mountDialog()
    h.overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }))
    await settle()

    pressEscape(h.overlay)
    expect(h.cancelled).toHaveLength(1)
    expect(h.closes()).toBe(0)

    // The run's own terminal word is what ends it; only then does Escape mean close.
    h.callbacks().onEnd({ matchCount: 3, incomplete: true, walked: true, capped: false })
    await settle()

    pressEscape(h.overlay)
    expect(h.cancelled).toHaveLength(1)
    expect(h.closes()).toBe(1)
    h.cleanup()
  })

  it('closes on the first press when nothing is running', async () => {
    const h = mountDialog()
    await settle()
    pressEscape(h.overlay)
    expect(h.closes()).toBe(1)
    expect(h.cancelled).toEqual([])
    h.cleanup()
  })
})
