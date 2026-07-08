/**
 * Tests for `persistence-subscriber.svelte.ts`, the single nav-state persistence
 * subscriber (A5). They pin:
 * - a store mutation → exactly one `saveAppStatus` with the diffed (changed-only) snapshot,
 * - a no-op when nothing nav-relevant changed (the diff),
 * - per-pane isolation (P1: a left change doesn't re-persist the right pane),
 * - the order-only toggle re-persisting the tab set without an AppStatus field,
 * - the load-from-disk baseline NOT immediately re-persisting (the seed guard),
 * - layout persisting drag-end-only via the explicit hook (not per frame),
 * - last-used-path forwarded through the explicit hook (the volume-switch delta).
 *
 * Uses Svelte runes (`$effect.root` + `$state`), so the filename carries the
 * `.svelte.` infix: the subscriber creates its effects in a reactive root, and
 * the tests back the `deps` getters with `$state` so a mutation + `flushSync`
 * drives the effects exactly as the live store would.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { ViewMode } from '$lib/app-status-store'
import type { SortColumn, SortOrder } from '../types'

const { saveAppStatusSpy, saveLastUsedPathSpy, recordVisitSpy } = vi.hoisted(() => ({
  saveAppStatusSpy: vi.fn(),
  saveLastUsedPathSpy: vi.fn().mockResolvedValue(undefined),
  recordVisitSpy: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('$lib/app-status-store', () => ({
  saveAppStatus: saveAppStatusSpy,
  saveLastUsedPathForVolume: saveLastUsedPathSpy,
}))

vi.mock('$lib/tauri-commands', () => ({
  recordVisit: recordVisitSpy,
}))

import { initPersistenceSubscriber } from './persistence-subscriber.svelte'

interface PaneNavState {
  path: string
  volumeId: string
  viewMode: ViewMode
  sortBy: SortColumn
  sortOrder: SortOrder
}

/**
 * A reactive fake of the store the subscriber reads. Backs every getter with
 * `$state` so a mutation re-runs the matching effect on `flushSync`. Starts
 * uninitialized; `markInitialized()` flips the gate and seeds the baseline.
 */
function createFakeStore() {
  let initialized = $state(false)
  let focusedPane = $state<'left' | 'right'>('left')
  const panes = $state<Record<'left' | 'right', PaneNavState>>({
    left: { path: '/left', volumeId: 'root', viewMode: 'full', sortBy: 'name', sortOrder: 'ascending' },
    right: { path: '/right', volumeId: 'root', viewMode: 'full', sortBy: 'name', sortOrder: 'ascending' },
  })

  const saveTabsForPaneSide = vi.fn<(pane: 'left' | 'right') => void>()

  const deps = {
    getInitialized: () => initialized,
    getFocusedPane: () => focusedPane,
    getPanePath: (pane: 'left' | 'right') => panes[pane].path,
    getPaneVolumeId: (pane: 'left' | 'right') => panes[pane].volumeId,
    getPaneViewMode: (pane: 'left' | 'right') => panes[pane].viewMode,
    getPaneSortBy: (pane: 'left' | 'right') => panes[pane].sortBy,
    getPaneSortOrder: (pane: 'left' | 'right') => panes[pane].sortOrder,
    saveTabsForPaneSide,
  }

  return {
    deps,
    saveTabsForPaneSide,
    markInitialized() {
      initialized = true
    },
    setFocusedPane(pane: 'left' | 'right') {
      focusedPane = pane
    },
    mutatePane(pane: 'left' | 'right', patch: Partial<PaneNavState>) {
      Object.assign(panes[pane], patch)
    },
  }
}

describe('persistence-subscriber', () => {
  let dispose: (() => void) | undefined

  function create() {
    const store = createFakeStore()
    let sub!: ReturnType<typeof initPersistenceSubscriber>
    dispose = $effect.root(() => {
      sub = initPersistenceSubscriber(store.deps)
    })
    // The effects run once (uninitialized → early return). Now flip the gate and
    // flush so they seed their baselines from the loaded state, without saving.
    flushSync()
    store.markInitialized()
    flushSync()
    return { store, sub }
  }

  beforeEach(() => {
    vi.clearAllMocks()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
  })

  it('does not persist on load (the seed baseline writes nothing)', () => {
    create()
    expect(saveAppStatusSpy).not.toHaveBeenCalled()
    // no tab saves either
  })

  it('persists exactly one saveAppStatus with the diffed (changed-only) snapshot', () => {
    const { store } = create()
    store.mutatePane('left', { path: '/left/sub' })
    flushSync()
    expect(saveAppStatusSpy).toHaveBeenCalledTimes(1)
    expect(saveAppStatusSpy).toHaveBeenCalledWith({ leftPath: '/left/sub' })
  })

  it('emits only the changed fields (volumeId + path, not viewMode/sortBy)', () => {
    const { store } = create()
    store.mutatePane('left', { path: 'smb://host', volumeId: 'network' })
    flushSync()
    expect(saveAppStatusSpy).toHaveBeenCalledTimes(1)
    expect(saveAppStatusSpy).toHaveBeenCalledWith({ leftPath: 'smb://host', leftVolumeId: 'network' })
  })

  it('re-persists the pane tab set on a nav change', () => {
    const { store } = create()
    store.mutatePane('right', { path: '/right/deep' })
    flushSync()
    expect(store.saveTabsForPaneSide).toHaveBeenCalledTimes(1)
    expect(store.saveTabsForPaneSide).toHaveBeenCalledWith('right')
  })

  it('is a no-op when nothing nav-relevant changed', () => {
    const { store } = create()
    // Re-assign identical values (a mutation that nets no diff).
    store.mutatePane('left', { path: '/left', volumeId: 'root' })
    flushSync()
    expect(saveAppStatusSpy).not.toHaveBeenCalled()
    expect(store.saveTabsForPaneSide).not.toHaveBeenCalled()
  })

  it('isolates panes: a left change does not re-persist the right pane (P1)', () => {
    const { store } = create()
    store.mutatePane('left', { sortBy: 'size' })
    flushSync()
    expect(saveAppStatusSpy).toHaveBeenCalledTimes(1)
    expect(saveAppStatusSpy).toHaveBeenCalledWith({ leftSortBy: 'size' })
    expect(store.saveTabsForPaneSide).toHaveBeenCalledTimes(1)
    expect(store.saveTabsForPaneSide).toHaveBeenCalledWith('left')
    // The right pane's effect never ran.
    expect(store.saveTabsForPaneSide).not.toHaveBeenCalledWith('right')
  })

  it('order-only toggle re-persists the tab set without an AppStatus field', () => {
    const { store } = create()
    store.mutatePane('left', { sortOrder: 'descending' })
    flushSync()
    // sortOrder is tab-only: no saveAppStatus, but the tab set re-persists.
    expect(saveAppStatusSpy).not.toHaveBeenCalled()
    expect(store.saveTabsForPaneSide).toHaveBeenCalledTimes(1)
    expect(store.saveTabsForPaneSide).toHaveBeenCalledWith('left')
  })

  it('persists focusedPane on change', () => {
    const { store } = create()
    store.setFocusedPane('right')
    flushSync()
    expect(saveAppStatusSpy).toHaveBeenCalledTimes(1)
    expect(saveAppStatusSpy).toHaveBeenCalledWith({ focusedPane: 'right' })
  })

  it('does not persist focusedPane when it does not change', () => {
    const { store } = create()
    store.setFocusedPane('left') // already left
    flushSync()
    expect(saveAppStatusSpy).not.toHaveBeenCalled()
  })

  it('persistLayout writes leftPaneWidthPercent (drag-end hook, not reactive)', () => {
    const { sub } = create()
    sub.persistLayout(62)
    expect(saveAppStatusSpy).toHaveBeenCalledTimes(1)
    expect(saveAppStatusSpy).toHaveBeenCalledWith({ leftPaneWidthPercent: 62 })
  })

  it('persistLastUsedPath forwards to saveLastUsedPathForVolume', () => {
    const { sub } = create()
    sub.persistLastUsedPath({ volumeId: 'mtp-x:1', path: '/dcim' })
    expect(saveLastUsedPathSpy).toHaveBeenCalledTimes(1)
    expect(saveLastUsedPathSpy).toHaveBeenCalledWith('mtp-x:1', '/dcim')
  })

  it('persistLastUsedPath also feeds the importance visit signal (fire-and-forget)', () => {
    const { sub } = create()
    sub.persistLastUsedPath({ volumeId: 'root', path: '/Users/me/project' })
    expect(recordVisitSpy).toHaveBeenCalledTimes(1)
    expect(recordVisitSpy).toHaveBeenCalledWith('root', '/Users/me/project')
  })
})
