/**
 * Tests for `tab-mcp-sync.svelte.ts`, the factory that mirrors each pane's tab
 * STRUCTURE (id / path / pinned / active) into the MCP backend, debounced ~100 ms.
 * They pin:
 * - `syncTabsToBackend` pushes BOTH panes after the debounce, projecting id / path /
 *   pinned and setting `active` from each manager's `activeTabId`,
 * - repeated calls inside the window COALESCE into a single push per pane,
 * - the `$effect` pushes on a structural change only once `getInitialized()` is true,
 *   and stays silent before init,
 * - `cleanup()` cancels a pending debounce so no push lands after teardown.
 *
 * Runes factory (owns an `$effect`), so the filename carries the `.svelte.` infix and
 * the sync is created inside `$effect.root`; the debounce is driven with fake timers.
 */
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest'
import { flushSync } from 'svelte'
import type { TabState } from '../tabs/tab-types'
import { createHistory } from '../navigation/navigation-history'
import { createTabManager, type TabManager } from '../tabs/tab-state-manager.svelte'

const { updatePaneTabs } = vi.hoisted<{ updatePaneTabs: Mock }>(() => ({
  updatePaneTabs: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('$lib/tauri-commands', () => ({ updatePaneTabs }))

import { initTabMcpSync, type TabMcpSyncDeps } from './tab-mcp-sync.svelte'

function makeTab(overrides: Partial<TabState> = {}): TabState {
  return {
    id: crypto.randomUUID(),
    path: '/Users/test',
    volumeId: 'root',
    history: createHistory('root', '/Users/test'),
    sortBy: 'name',
    sortOrder: 'ascending',
    viewMode: 'full',
    pinned: false,
    cursorFilename: null,
    unreachable: null,
    ...overrides,
  }
}

/** A manager with the given tabs, active on the first one. */
function managerWith(tabs: TabState[]): TabManager {
  const mgr = createTabManager(tabs[0])
  mgr.tabs = tabs
  mgr.activeTabId = tabs[0].id
  return mgr
}

function callArgsFor(side: 'left' | 'right'): unknown {
  const call = updatePaneTabs.mock.calls.find((c) => c[0] === side)
  return call?.[1]
}

describe('initTabMcpSync', () => {
  let dispose: (() => void) | undefined

  beforeEach(() => {
    vi.clearAllMocks()
    vi.useFakeTimers()
  })

  afterEach(() => {
    dispose?.()
    dispose = undefined
    vi.useRealTimers()
  })

  /**
   * Creates the sync inside an effect root. Defaults to NOT initialized so the mount
   * effect schedules nothing and direct `syncTabsToBackend` calls are observed in
   * isolation; pass `initialized: true` to exercise the reactive path.
   */
  function setup(opts: { left: TabManager; right: TabManager; initialized?: boolean }) {
    let initialized = $state(opts.initialized ?? false)
    const deps: TabMcpSyncDeps = {
      getLeftTabMgr: () => opts.left,
      getRightTabMgr: () => opts.right,
      getInitialized: () => initialized,
    }
    let sync!: ReturnType<typeof initTabMcpSync>
    dispose = $effect.root(() => {
      sync = initTabMcpSync(deps)
    })
    flushSync()
    return {
      sync,
      setInitialized: (v: boolean) => {
        initialized = v
        flushSync()
      },
    }
  }

  it('pushes both panes after the debounce, projecting fields and the active flag', () => {
    const leftActive = makeTab({ path: '/left/active', pinned: false })
    const leftPinned = makeTab({ path: '/left/pinned', pinned: true })
    const rightTab = makeTab({ path: '/right/only', pinned: false })
    const left = managerWith([leftActive, leftPinned])
    const right = managerWith([rightTab])

    const { sync } = setup({ left, right })
    sync.syncTabsToBackend()

    // Debounced: nothing pushed synchronously.
    expect(updatePaneTabs).not.toHaveBeenCalled()

    vi.advanceTimersByTime(100)

    expect(callArgsFor('left')).toEqual([
      { id: leftActive.id, path: '/left/active', pinned: false, active: true },
      { id: leftPinned.id, path: '/left/pinned', pinned: true, active: false },
    ])
    expect(callArgsFor('right')).toEqual([
      { id: rightTab.id, path: '/right/only', pinned: false, active: true },
    ])
  })

  it('coalesces repeated calls inside the window into a single push per pane', () => {
    const { sync } = setup({ left: managerWith([makeTab()]), right: managerWith([makeTab()]) })

    sync.syncTabsToBackend()
    vi.advanceTimersByTime(40)
    sync.syncTabsToBackend()
    vi.advanceTimersByTime(40)
    sync.syncTabsToBackend()
    vi.advanceTimersByTime(100)

    // One push for 'left' and one for 'right', not three each.
    expect(updatePaneTabs).toHaveBeenCalledTimes(2)
    expect(updatePaneTabs.mock.calls.filter((c) => c[0] === 'left')).toHaveLength(1)
    expect(updatePaneTabs.mock.calls.filter((c) => c[0] === 'right')).toHaveLength(1)
  })

  it('pushes on a structural change once initialized', () => {
    const first = makeTab({ path: '/a' })
    const second = makeTab({ path: '/b' })
    const left = managerWith([first, second])
    const right = managerWith([makeTab()])

    setup({ left, right, initialized: true })
    // Flush the initial mount push, then start clean.
    vi.advanceTimersByTime(100)
    updatePaneTabs.mockClear()

    // A structural change (active tab moves) re-runs the effect.
    left.activeTabId = second.id
    flushSync()
    vi.advanceTimersByTime(100)

    expect(updatePaneTabs).toHaveBeenCalledWith('left', expect.any(Array))
    expect(updatePaneTabs).toHaveBeenCalledWith('right', expect.any(Array))
    const leftArgs = callArgsFor('left') as Array<{ id: string; active: boolean }>
    expect(leftArgs.find((t) => t.id === second.id)?.active).toBe(true)
  })

  it('stays silent on a change before init', () => {
    const first = makeTab()
    const second = makeTab()
    const left = managerWith([first, second])

    const { sync: _sync } = setup({ left, right: managerWith([makeTab()]), initialized: false })

    left.activeTabId = second.id
    flushSync()
    vi.advanceTimersByTime(100)

    expect(updatePaneTabs).not.toHaveBeenCalled()
  })

  it('cleanup cancels a pending debounced push', () => {
    const { sync } = setup({ left: managerWith([makeTab()]), right: managerWith([makeTab()]) })

    sync.syncTabsToBackend()
    sync.cleanup()
    vi.advanceTimersByTime(100)

    expect(updatePaneTabs).not.toHaveBeenCalled()
  })
})
