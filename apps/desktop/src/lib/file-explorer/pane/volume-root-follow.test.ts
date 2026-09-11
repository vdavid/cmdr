/**
 * Panes, tabs, and remembered paths following an edit that moved a connected
 * place's root or start folder.
 *
 * ❗ The store patch lands BEFORE any pane moves: `navigate()` checks a server
 * target against the row's root, and the debounced republish that brings the new
 * root arrives after this event, so a widened root would otherwise be refused.
 */

import { describe, expect, it, vi } from 'vitest'
import type { VolumeRootChanged } from '$lib/ipc/bindings'

// The subscription wrapper reaches the real store, which carries a Svelte toast
// component; the cells here inject every dependency instead.
vi.mock('$lib/stores/volume-store.svelte', () => ({ applyVolumeRootChanged: vi.fn() }))
import { addTab, createTabManager, getActiveTab, type TabManager } from '../tabs/tab-state-manager.svelte'
import { createInitialTabState } from './tab-operations'
import type { NavigateIntent, NavigateResult } from './navigate'
import { followVolumeRootChange, type VolumeRootFollowDeps } from './volume-root-follow'

const NAS = 'sftp-nas-local-22-ada'
const OLD_ROOT = 'sftp://ada@nas.local:22/srv/data/tmp'
const NEW_ROOT = 'sftp://ada@nas.local:22/srv/data'

/** The prod case: a root narrowed to `/tmp` by mistake, widened back one level. */
const WIDENED: VolumeRootChanged = {
  volumeId: NAS,
  oldRoot: OLD_ROOT,
  newRoot: NEW_ROOT,
  oldLanding: OLD_ROOT,
  newLanding: NEW_ROOT,
}

interface HarnessOpts {
  /** Each pane's active tab, as `[volumeId, path]`. */
  left: [string, string]
  right: [string, string]
  remembered?: string
}

function harness(opts: HarnessOpts) {
  const managers: Record<'left' | 'right', TabManager> = {
    left: createTabManager(createInitialTabState(opts.left[1], opts.left[0])),
    right: createTabManager(createInitialTabState(opts.right[1], opts.right[0])),
  }
  const order: string[] = []
  const navigate = vi.fn((intent: NavigateIntent): NavigateResult => {
    order.push(`navigate:${intent.pane}`)
    return { status: 'started', settled: Promise.resolve() }
  })
  const saveTabs = vi.fn()
  const saveLastUsedPath = vi.fn(() => Promise.resolve())
  const deps: VolumeRootFollowDeps = {
    applyToVolumeList: () => {
      order.push('store')
    },
    getTabMgr: (pane) => managers[pane],
    navigate,
    saveTabs,
    getLastUsedPath: () => Promise.resolve(opts.remembered),
    saveLastUsedPath,
  }
  return { managers, deps, navigate, saveTabs, saveLastUsedPath, order }
}

describe('followVolumeRootChange: the panes', () => {
  it('moves a pane on the old root to the new root through navigate(), after the store patch', async () => {
    const h = harness({ left: [NAS, OLD_ROOT], right: ['root', '/Users/ada'] })

    await followVolumeRootChange(WIDENED, h.deps)

    // ❗ A terminal `'fallback'` commit with no history push: following an edit
    // isn't a step the person took, so it grows no Back target and forks no
    // pinned tab.
    expect(h.navigate).toHaveBeenCalledTimes(1)
    expect(h.navigate).toHaveBeenCalledWith({
      pane: 'left',
      to: { selectVolume: { volumeId: NAS, path: NEW_ROOT } },
      source: 'fallback',
      pushHistory: false,
    })
    expect(h.order).toEqual(['store', 'navigate:left'])
  })

  it('leaves a pane deeper inside the new root where it is', async () => {
    const h = harness({ left: [NAS, `${OLD_ROOT}/photos`], right: ['root', '/Users/ada'] })

    await followVolumeRootChange(WIDENED, h.deps)

    expect(h.navigate).not.toHaveBeenCalled()
    expect(h.order).toEqual(['store'])
  })

  it('moves both panes when both stand on the place', async () => {
    const h = harness({ left: [NAS, OLD_ROOT], right: [NAS, OLD_ROOT] })

    await followVolumeRootChange(WIDENED, h.deps)

    expect(h.navigate).toHaveBeenCalledTimes(2)
    expect(h.order).toEqual(['store', 'navigate:left', 'navigate:right'])
  })
})

describe('followVolumeRootChange: the tabs behind the panes', () => {
  it('moves an inactive tab without navigating, and saves that pane’s tabs', async () => {
    const h = harness({ left: ['root', '/Users/ada'], right: ['root', '/Users/ada'] })
    const left = h.managers.left
    const behind = createInitialTabState('sftp://ada@nas.local:22/srv/other', NAS)
    behind.cursorFilename = 'notes.txt'
    addTab(left, getActiveTab(left).id, behind)

    await followVolumeRootChange(WIDENED, h.deps)

    const moved = left.tabs.find((tab) => tab.id === behind.id)
    expect(moved?.path).toBe(NEW_ROOT)
    // The folder it pointed into is gone from the place, so the row it
    // remembered is too.
    expect(moved?.cursorFilename).toBeNull()
    expect(h.navigate).not.toHaveBeenCalled()
    expect(h.saveTabs).toHaveBeenCalledWith('left')
    expect(h.saveTabs).not.toHaveBeenCalledWith('right')
  })

  it('leaves a tab on another volume alone, even one whose path reads the same', async () => {
    const h = harness({ left: ['root', '/Users/ada'], right: ['root', '/Users/ada'] })
    const other = createInitialTabState(OLD_ROOT, 'sftp-other-host-22-ada')
    addTab(h.managers.right, getActiveTab(h.managers.right).id, other)

    await followVolumeRootChange(WIDENED, h.deps)

    expect(h.managers.right.tabs.find((tab) => tab.id === other.id)?.path).toBe(OLD_ROOT)
    expect(h.saveTabs).not.toHaveBeenCalled()
  })
})

describe('followVolumeRootChange: the remembered path', () => {
  it('moves the place’s remembered path, so the next switch there lands on the new root', async () => {
    const h = harness({ left: ['root', '/'], right: ['root', '/'], remembered: OLD_ROOT })

    await followVolumeRootChange(WIDENED, h.deps)

    expect(h.saveLastUsedPath).toHaveBeenCalledWith({ volumeId: NAS, path: NEW_ROOT })
  })

  it('leaves a remembered path still inside the new root alone, and writes nothing when there is none', async () => {
    const inside = harness({ left: ['root', '/'], right: ['root', '/'], remembered: `${OLD_ROOT}/photos` })
    await followVolumeRootChange(WIDENED, inside.deps)
    expect(inside.saveLastUsedPath).not.toHaveBeenCalled()

    const none = harness({ left: ['root', '/'], right: ['root', '/'] })
    await followVolumeRootChange(WIDENED, none.deps)
    expect(none.saveLastUsedPath).not.toHaveBeenCalled()
  })
})
